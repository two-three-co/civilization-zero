//! 2a-4: restoring any tick from snapshot + ledger equals running directly.

use multiverse::{replay, BranchId, Multiverse, MultiverseError, ReplayError};
use simulation_core::{
    Event, EventLog, Intervention, ParamsMode, PolicyKind, PolicyTable, Pos, Who, World,
    WorldConfig,
};

fn root() -> World {
    World::new(WorldConfig {
        seed: 23,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..WorldConfig::default()
    })
    .unwrap()
}

const SCHEDULE: [(u64, Intervention); 3] = [
    (120, Intervention::ScaleFood { permille: 600 }),
    (
        350,
        Intervention::Drought {
            regrow_amount: 0,
            regrow_interval: 10,
        },
    ),
    (
        640,
        Intervention::SpawnAgents {
            n: 20,
            pos: Pos::new(10, 10),
            policy: PolicyKind::Forager,
        },
    ),
];

/// Runs the schedule by hand, keeping the state some ticks *start* with.
fn direct(to: u64, keep: &[u64]) -> (World, Vec<(u64, World)>) {
    let mut w = root();
    let mut kept = Vec::new();
    while w.tick < to {
        if keep.contains(&w.tick) {
            kept.push((w.tick, w.clone()));
        }
        for (t, iv) in SCHEDULE {
            if t == w.tick {
                w.intervene(iv).unwrap();
            }
        }
        w.step();
    }
    if keep.contains(&w.tick) {
        kept.push((w.tick, w.clone()));
    }
    (w, kept)
}

fn driven(to: u64, every: u64) -> Multiverse {
    let mut mv = Multiverse::new(root(), every);
    let r = BranchId(0);
    for tick in 0..to {
        for (t, iv) in SCHEDULE {
            if t == tick {
                mv.intervene(r, iv).unwrap();
            }
        }
        mv.step(r, 1).unwrap();
    }
    mv
}

#[test]
fn state_at_equals_the_direct_run_at_every_requested_tick() {
    let mv = driven(800, 100);
    let r = BranchId(0);
    let keep = [0, 50, 100, 120, 121, 350, 499, 500, 640, 641, 799, 800];
    let (direct_head, kept) = direct(800, &keep);
    assert_eq!(
        mv.branch(r).unwrap().head().state_hash(),
        direct_head.state_hash()
    );
    assert_eq!(kept.len(), keep.len());
    for (t, expected) in kept {
        let restored = mv.state_at(r, t).unwrap();
        assert_eq!(restored.state_hash(), expected.state_hash(), "tick {t}");
        assert_eq!(restored, expected, "tick {t}");
    }
    let ticks: Vec<u64> = mv.branch(r).unwrap().snapshot_ticks().collect();
    assert_eq!(ticks, (0..=800).step_by(100).collect::<Vec<_>>());
}

#[test]
fn replay_counts_interventions_and_refuses_to_rewind() {
    let mv = driven(400, 1_000); // only the origin snapshot
    let b = mv.branch(BranchId(0)).unwrap();
    let mut w = root();
    let stats = replay(&mut w, b.log(), 400).unwrap();
    assert_eq!((stats.from, stats.to, stats.interventions), (0, 400, 2));
    assert_eq!(stats.events_checked, b.log().len() - 2);
    assert_eq!(w.state_hash(), b.head().state_hash());
    assert_eq!(
        replay(&mut w, b.log(), 300),
        Err(ReplayError::TargetBehind { head: 400, to: 300 })
    );
    assert_eq!(
        mv.state_at(BranchId(0), 401),
        Err(MultiverseError::BeyondHead {
            tick: 401,
            head: 400
        })
    );
    assert_eq!(
        mv.state_at(BranchId(7), 0),
        Err(MultiverseError::UnknownBranch(BranchId(7)))
    );
}

/// ADR-0013 3-7 acceptance: a learning world replays bit-exactly. Q tables
/// and pending steps are state, so a replay that lost or re-derived either
/// differently would diverge from the direct run — and the switch of a
/// seed-keyed fraction to learners mid-way is replayed from the ledger.
#[test]
fn learner_worlds_replay_bit_exactly_from_snapshot_and_ledger() {
    let base = World::generate(
        WorldConfig {
            seed: 23,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            ..WorldConfig::default()
        },
        |i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Reflex,
            _ => PolicyKind::Learner,
        },
    )
    .unwrap();
    let schedule = [
        (120u64, Intervention::ScaleFood { permille: 600 }),
        (
            300,
            Intervention::SetPolicy {
                who: Who::Fraction { permille: 500 },
                kind: PolicyKind::Learner,
                params: ParamsMode::Keep,
            },
        ),
        (
            450,
            Intervention::SetPolicy {
                who: Who::All,
                kind: PolicyKind::Learner,
                params: ParamsMode::Default,
            },
        ),
    ];
    let keep = [0u64, 99, 100, 300, 301, 449, 450, 451, 600];
    let mut mv = Multiverse::new(base.clone(), 100);
    let r = BranchId(0);
    let mut direct = base.clone();
    let mut kept = Vec::new();
    for tick in 0..600u64 {
        if keep.contains(&tick) {
            kept.push((tick, direct.clone()));
        }
        for (t, iv) in schedule {
            if t == tick {
                mv.intervene(r, iv).unwrap();
                direct.intervene(iv).unwrap();
            }
        }
        mv.step(r, 1).unwrap();
        direct.step();
    }
    kept.push((600, direct.clone()));
    let head = mv.branch(r).unwrap().head();
    assert_eq!(head, &direct);
    assert!(!head.agents.is_empty());
    assert!(head.agents.iter().all(|a| a.policy == PolicyKind::Learner
        && a.last_step.is_some() == (a.age > 0)
        && matches!(&a.params.table, Some(PolicyTable::Learner { q }) if q.iter().any(|&v| v != 0))));
    for (t, expected) in kept {
        let restored = mv.state_at(r, t).unwrap();
        assert_eq!(restored, expected, "tick {t}");
    }
    // From the tick-300 snapshot (taken before that tick's switch) through
    // both switches to the head, every lifecycle event verified.
    let mut w = mv.state_at(r, 300).unwrap();
    let stats = replay(&mut w, mv.branch(r).unwrap().log(), 600).unwrap();
    assert_eq!((stats.from, stats.to, stats.interventions), (300, 600, 2));
    assert_eq!(&w, head);
    // The same world built again from bytes replays the same.
    let mut w = World::from_bytes(&mv.state_at(r, 100).unwrap().to_bytes()).unwrap();
    replay(&mut w, mv.branch(r).unwrap().log(), 600).unwrap();
    assert_eq!(w.state_hash(), head.state_hash());
}

#[test]
fn replay_detects_a_tampered_or_foreign_ledger() {
    let mv = driven(300, 100);
    let b = mv.branch(BranchId(0)).unwrap();
    // Change one death record: the kernel will not reproduce it.
    let mut tampered = EventLog::new();
    let mut victim_tick = None;
    for e in b.log().events() {
        let mut e = *e;
        if victim_tick.is_none() && e.tick() > 150 {
            if let Event::Died {
                energy_written_off,
                tick,
                ..
            } = &mut e
            {
                *energy_written_off += 1;
                victim_tick = Some(*tick);
            }
        }
        tampered.push(e).unwrap();
    }
    let victim_tick = victim_tick.expect("a death after tick 150");
    let mut w = root();
    match replay(&mut w, &tampered, 300) {
        Err(ReplayError::Diverged { tick, .. }) => assert_eq!(tick, victim_tick),
        other => panic!("expected divergence, got {other:?}"),
    }
    // A ledger from a different seed is foreign: it diverges at its first
    // lifecycle event.
    let other = driven(50, 100);
    let mut w = World::new(WorldConfig {
        seed: 99,
        ..root().config
    })
    .unwrap();
    assert!(matches!(
        replay(&mut w, other.branch(BranchId(0)).unwrap().log(), 50),
        Err(ReplayError::Diverged { .. })
    ));
}
