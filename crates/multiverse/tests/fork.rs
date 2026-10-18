//! 2a-5: forking at a past tick with interventions, lineage, reproducibility.

use multiverse::{BranchId, Multiverse, MultiverseError};
use simulation_core::{
    Charter, Event, Intervention, InterventionError, Norm, NormKind, ParamsMode, PolicyKind,
    PolicyTable, Sanction, Who, World, WorldConfig,
};

fn root() -> World {
    World::new(WorldConfig {
        seed: 31,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..WorldConfig::default()
    })
    .unwrap()
}

const DROUGHT: [Intervention; 2] = [
    Intervention::ScaleFood { permille: 500 },
    Intervention::Drought {
        regrow_amount: 0,
        regrow_interval: 10,
    },
];

#[test]
fn the_same_fork_twice_gives_identical_branches() {
    let mut mv = Multiverse::new(root(), 100);
    let r = BranchId(0);
    mv.step(r, 600).unwrap();

    let a = mv.fork(r, 300, &DROUGHT).unwrap();
    let b = mv.fork(r, 300, &DROUGHT).unwrap();
    assert_ne!(a, b);
    assert_eq!(mv.branches().len(), 3);
    for id in [a, b] {
        assert_eq!(mv.branch(id).unwrap().head().tick, 300);
        mv.step(id, 300).unwrap();
    }
    let (ha, hb) = (
        mv.branch(a).unwrap().head().state_hash(),
        mv.branch(b).unwrap().head().state_hash(),
    );
    assert_eq!(ha, hb, "same intervention at the same tick, same history");
    assert_ne!(
        ha,
        mv.branch(r).unwrap().head().state_hash(),
        "the drought changed history"
    );
    assert_eq!(
        mv.state_at(a, 450).unwrap().state_hash(),
        mv.state_at(b, 450).unwrap().state_hash()
    );

    let ba = mv.branch(a).unwrap();
    assert_eq!(ba.parent(), Some(r));
    assert_eq!(ba.forked_at(), Some(300));
    assert_eq!(ba.origin_tick(), 300);
    assert_eq!(
        ba.log().range(0..300),
        mv.branch(r).unwrap().log().range(0..300)
    );
    let at_fork = ba.log().at(300);
    assert!(matches!(
        at_fork[0],
        Event::Intervened {
            tick: 300,
            intervention: Intervention::ScaleFood { permille: 500 }
        }
    ));
    assert!(matches!(
        at_fork[1],
        Event::Intervened {
            tick: 300,
            intervention: Intervention::Drought { .. }
        }
    ));
    // Every branch replays on its own: the child's state at its origin is
    // the parent's state there, before the child's interventions.
    assert_eq!(
        mv.state_at(a, 300).unwrap().state_hash(),
        mv.state_at(r, 300).unwrap().state_hash()
    );
}

/// ADR-0013 3-5 acceptance: "fixed vs learning" is a fork plus `SetPolicy`,
/// and doing it twice at the same tick gives the same branch — including the
/// seed-keyed `Fraction` draw, which the ledger replays.
#[test]
fn switching_policies_at_a_fork_is_reproducible() {
    let mut mv = Multiverse::new(root(), 100);
    let r = BranchId(0);
    mv.step(r, 600).unwrap();
    for switch in [
        Intervention::SetPolicy {
            who: Who::All,
            kind: PolicyKind::Learner,
            params: ParamsMode::Default,
        },
        Intervention::SetPolicy {
            who: Who::Fraction { permille: 300 },
            kind: PolicyKind::Reflex,
            params: ParamsMode::Keep,
        },
    ] {
        let a = mv.fork(r, 300, &[switch]).unwrap();
        let b = mv.fork(r, 300, &[switch]).unwrap();
        mv.step(a, 300).unwrap();
        mv.step(b, 300).unwrap();
        let (ha, hb) = (
            mv.branch(a).unwrap().head().state_hash(),
            mv.branch(b).unwrap().head().state_hash(),
        );
        assert_eq!(ha, hb, "{switch:?}");
        assert_ne!(
            ha,
            mv.branch(r).unwrap().head().state_hash(),
            "the kinds (and their tables) are state"
        );
        // Restoring a past tick replays the switch from the ledger.
        assert_eq!(
            mv.state_at(a, 450).unwrap().state_hash(),
            mv.state_at(b, 450).unwrap().state_hash()
        );
        // The switch is the first record of the fork tick; what follows it
        // are the social events of the freshly switched agents (a learner
        // with a blank table tries every kind, 3-7).
        let at_300 = mv.branch(a).unwrap().log().at(300);
        assert_eq!(
            at_300[0],
            Event::Intervened {
                tick: 300,
                intervention: switch
            }
        );
        assert!(at_300[1..]
            .iter()
            .all(|e| !matches!(e, Event::Intervened { .. })));
        let head = mv.branch(a).unwrap().head();
        head.check_invariants().unwrap();
        match switch {
            Intervention::SetPolicy { who: Who::All, .. } => {
                assert!(head.agents.iter().all(|ag| ag.policy == PolicyKind::Learner
                    && matches!(ag.params.table, Some(PolicyTable::Learner { .. }))));
            }
            _ => {
                let reflex = head
                    .agents
                    .iter()
                    .filter(|ag| ag.policy == PolicyKind::Reflex)
                    .count();
                assert!(
                    reflex > 0 && reflex < head.agents.len(),
                    "{reflex} of {} switched",
                    head.agents.len()
                );
            }
        }
    }
    // Both pairs are distinct branches of the same root.
    assert_eq!(mv.branches().len(), 5);
}

/// ADR-0014 4-2 acceptance: "with vs without rule" is a fork plus `Enact`
/// (or `Repeal`), and doing it twice at the same tick gives the same
/// branch — rules, episodes and sanctions replay from the ledger.
#[test]
fn enacting_a_rule_at_a_fork_is_reproducible() {
    let root = World::generate(
        WorldConfig {
            seed: 31,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            nomos: true,
            record_violations: true,
            ..WorldConfig::default()
        },
        |i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Random,
            _ => PolicyKind::Learner,
        },
    )
    .unwrap();
    let enact = Intervention::Enact {
        norm: Norm {
            kind: NormKind::Encroach,
            standing_ticks: 32,
            sanction: Sanction::ALL,
            except_sanction: false,
        },
    };
    let mut mv = Multiverse::new(root, 100);
    let r = BranchId(0);
    mv.step(r, 450).unwrap();
    let a = mv.fork(r, 150, &[enact]).unwrap();
    let b = mv.fork(r, 150, &[enact]).unwrap();
    mv.step(a, 300).unwrap();
    mv.step(b, 300).unwrap();
    let (ha, hb, hr) = (
        mv.branch(a).unwrap().head().state_hash(),
        mv.branch(b).unwrap().head().state_hash(),
        mv.branch(r).unwrap().head().state_hash(),
    );
    assert_eq!(ha, hb, "same decree at the same tick, same history");
    assert_ne!(hr, ha, "the rule and its episodes are state");
    assert_eq!(
        mv.state_at(a, 300).unwrap().state_hash(),
        mv.state_at(b, 300).unwrap().state_hash()
    );
    let head = mv.branch(a).unwrap().head();
    head.check_invariants().unwrap();
    assert_eq!(head.nomos.rules.len(), 1);
    assert!(head.ledger.violations[0] > 0, "the crowd encroaches");
    assert!(
        head.ledger.sanctions[0] > 0,
        "random walkers and learners harm the accused now and then"
    );
    let log = mv.branch(a).unwrap().log();
    assert_eq!(
        log.at(150)[0],
        Event::Intervened {
            tick: 150,
            intervention: enact
        }
    );
    assert!(log
        .range(150..450)
        .iter()
        .any(|e| matches!(e, Event::Sanctioned { .. })));
    assert!(log
        .range(150..450)
        .iter()
        .any(|e| matches!(e, Event::Violated { .. })));
    assert!(!log
        .range(0..150)
        .iter()
        .any(|e| matches!(e, Event::Violated { .. } | Event::Sanctioned { .. })));
    // The root never had the rule: nothing to repeal there; the child has
    // it as rule 0, so a fork of the child that repeals it is "without".
    assert!(matches!(
        mv.fork(r, 150, &[Intervention::Repeal { rule: 0 }]),
        Err(MultiverseError::Intervention(InterventionError::Invalid(_)))
    ));
    let c = mv
        .fork(a, 300, &[Intervention::Repeal { rule: 0 }])
        .unwrap();
    let d = mv
        .fork(a, 300, &[Intervention::Repeal { rule: 0 }])
        .unwrap();
    mv.step(c, 100).unwrap();
    mv.step(d, 100).unwrap();
    assert_eq!(
        mv.branch(c).unwrap().head().state_hash(),
        mv.branch(d).unwrap().head().state_hash()
    );
    assert!(mv.branch(c).unwrap().head().nomos.rules.is_empty());
    assert!(mv.branch(c).unwrap().head().nomos.standing.is_empty());
    assert_ne!(
        mv.branch(c).unwrap().head().state_hash(),
        mv.state_at(a, 400).unwrap().state_hash()
    );
    // A world without the switch refuses the decree, and no branch is left.
    let mut plain = Multiverse::new(root_plain(), 100);
    plain.step(BranchId(0), 50).unwrap();
    assert!(matches!(
        plain.fork(BranchId(0), 50, &[enact]),
        Err(MultiverseError::Intervention(InterventionError::Invalid(_)))
    ));
    assert_eq!(plain.branches().len(), 1);
}

fn root_plain() -> World {
    root()
}

/// ADR-0014 4-5 acceptance: forking while a motion is open is
/// reproducible — the floor and its voters are state, the votes and the
/// closing replay from the ledger — and a decree at the fork tick that
/// changes what the motion can do replays too.
#[test]
fn forking_while_a_motion_is_open_is_reproducible() {
    let root = World::generate(
        WorldConfig {
            seed: 31,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            record_votes: true,
            charter: Charter {
                quorum_permille: 50,
                threshold_permille: 500,
                vote_ticks: 12,
                amendable: true,
            },
            ..WorldConfig::nomos_preset()
        },
        |i| match i % 3 {
            0 => PolicyKind::Random,
            1 => PolicyKind::Enforcer,
            _ => PolicyKind::Learner,
        },
    )
    .unwrap();
    let mut mv = Multiverse::new(root, 50);
    let r = BranchId(0);
    mv.step(r, 400).unwrap();
    // A tick at which a motion is open with at least one vote on it.
    let at = (100..400u64)
        .find(|&t| {
            mv.state_at(r, t)
                .unwrap()
                .nomos
                .motion
                .as_ref()
                .is_some_and(|m| !m.ayes.is_empty() || !m.nays.is_empty())
        })
        .expect("random walkers and enforcers legislate");
    let open = mv.state_at(r, at).unwrap();
    let motion_id = open.nomos.motion.as_ref().unwrap().id;
    // A shorter voting period, decreed at the fork tick: the open motion
    // closes at the next phase 3c under the new charter.
    let shorten = Intervention::Charter {
        charter: Charter {
            quorum_permille: 0,
            threshold_permille: 500,
            vote_ticks: 1,
            amendable: true,
        },
    };
    let a = mv.fork(r, at, &[shorten]).unwrap();
    let b = mv.fork(r, at, &[shorten]).unwrap();
    mv.step(a, 100).unwrap();
    mv.step(b, 100).unwrap();
    let (ha, hb) = (
        mv.branch(a).unwrap().head().state_hash(),
        mv.branch(b).unwrap().head().state_hash(),
    );
    assert_eq!(ha, hb, "same decree on the same open floor, same history");
    assert_ne!(
        ha,
        mv.state_at(r, at + 100).unwrap().state_hash(),
        "the charter is state and changed the closing"
    );
    assert_eq!(
        mv.state_at(a, at + 50).unwrap().state_hash(),
        mv.state_at(b, at + 50).unwrap().state_hash()
    );
    let log = mv.branch(a).unwrap().log();
    assert_eq!(
        log.at(at)[0],
        Event::Intervened {
            tick: at,
            intervention: shorten
        }
    );
    // The motion open at the fork closed in the fork's first tick (its
    // period had long passed under the new charter), with the votes the
    // parent had already counted.
    let closing = log
        .at(at)
        .iter()
        .find(|e| matches!(e, Event::MotionClosed { .. }))
        .copied()
        .expect("the open motion closes at once");
    if let Event::MotionClosed {
        motion, ayes, nays, ..
    } = closing
    {
        assert_eq!(motion, motion_id);
        let m = open.nomos.motion.as_ref().unwrap();
        assert!(ayes as usize >= m.ayes.len() && nays as usize >= m.nays.len());
    }
    // The parent's motion closed later, under the old period.
    let parent_closing = mv
        .branch(r)
        .unwrap()
        .log()
        .range(at..400)
        .iter()
        .find(|e| matches!(e, Event::MotionClosed { .. }))
        .copied();
    assert!(
        parent_closing.is_some_and(|e| e.tick() > at),
        "{parent_closing:?}"
    );
    mv.branch(a).unwrap().head().check_invariants().unwrap();
    assert_eq!(mv.branches().len(), 3);
}

#[test]
fn a_fork_of_a_fork_keeps_its_lineage() {
    let mut mv = Multiverse::new(root(), 50);
    let r = BranchId(0);
    mv.step(r, 300).unwrap();
    let a = mv.fork(r, 200, &DROUGHT[..1]).unwrap();
    mv.step(a, 200).unwrap();
    let c = mv
        .fork(a, 350, &[Intervention::ScaleFood { permille: 2_000 }])
        .unwrap();
    let bc = mv.branch(c).unwrap();
    assert_eq!(
        (bc.parent(), bc.forked_at(), bc.origin_tick()),
        (Some(a), Some(350), 350)
    );
    assert_eq!(
        mv.state_at(c, 350).unwrap().state_hash(),
        mv.state_at(a, 350).unwrap().state_hash()
    );
    // c's ledger carries a's drought at 200 (shared past) and its own boom at 350.
    assert!(matches!(
        bc.log().at(200)[0],
        Event::Intervened {
            intervention: Intervention::ScaleFood { permille: 500 },
            ..
        }
    ));
    assert!(matches!(
        bc.log().at(350)[0],
        Event::Intervened {
            intervention: Intervention::ScaleFood { permille: 2_000 },
            ..
        }
    ));
    mv.step(c, 100).unwrap();
    assert_eq!(mv.branch(c).unwrap().head().tick, 450);
    // a's head is 400: compare there, after the boom at 350 has acted.
    assert_ne!(
        mv.state_at(c, 400).unwrap().state_hash(),
        mv.state_at(a, 400).unwrap().state_hash()
    );
}

#[test]
fn a_fork_without_interventions_is_a_copy_and_invalid_forks_are_rejected() {
    let mut mv = Multiverse::new(root(), 100);
    let r = BranchId(0);
    mv.step(r, 250).unwrap();
    let twin = mv.fork(r, 250, &[]).unwrap();
    assert_eq!(
        mv.branch(twin).unwrap().head().state_hash(),
        mv.branch(r).unwrap().head().state_hash()
    );
    mv.step(r, 100).unwrap();
    mv.step(twin, 100).unwrap();
    assert_eq!(
        mv.branch(twin).unwrap().head().state_hash(),
        mv.branch(r).unwrap().head().state_hash()
    );

    assert_eq!(
        mv.fork(r, 351, &[]),
        Err(MultiverseError::BeyondHead {
            tick: 351,
            head: 350
        })
    );
    assert_eq!(
        mv.fork(BranchId(9), 0, &[]),
        Err(MultiverseError::UnknownBranch(BranchId(9)))
    );
    // A fork before the child's origin is not possible from the child.
    assert_eq!(
        mv.fork(twin, 100, &[]),
        Err(MultiverseError::BeforeOrigin {
            tick: 100,
            origin: 250
        })
    );
    // Invalid interventions leave no branch behind.
    let before = mv.branches().len();
    assert!(mv
        .fork(
            r,
            300,
            &[Intervention::Drought {
                regrow_amount: 1,
                regrow_interval: 0
            }]
        )
        .is_err());
    assert_eq!(mv.branches().len(), before);
}
