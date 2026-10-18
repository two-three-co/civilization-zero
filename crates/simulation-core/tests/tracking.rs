//! Per-agent material in the tick's report (ADR-0015 D4, 5-2):
//! `TickReport.gatherers` lists every successful gatherer in phase 3's
//! cell order and, within a cell, id order; `encroachers` lists every
//! (gatherer, first occupant) pair in the order `Violated` records them.
//! Both come in every world, behind no setting, and touch no state (the
//! three goldens of `determinism.rs` and `cz-web/tests` stand).

mod common;

use common::*;
use simulation_core::{
    Action, AgentId, Dir, Event, Intervention, Norm, NormKind, PolicyKind, Pos, Sanction,
    TickReport, World, WorldConfig,
};

fn rule() -> Intervention {
    Intervention::Enact {
        norm: Norm {
            kind: NormKind::Encroach,
            standing_ticks: 32,
            sanction: Sanction::HARM,
            except_sanction: false,
        },
    }
}

/// The `Violated` events of kind `Encroach`, as (gatherer, first occupant).
fn violated(r: &TickReport) -> Vec<(AgentId, AgentId)> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Violated {
                norm: NormKind::Encroach,
                agent,
                against,
                ..
            } => Some((*agent, *against)),
            _ => None,
        })
        .collect()
}

fn ids(v: &[u64]) -> Vec<AgentId> {
    v.iter().map(|&i| AgentId(i)).collect()
}

fn pair(a: u64, b: u64) -> (AgentId, AgentId) {
    (AgentId(a), AgentId(b))
}

/// Founders on three stocked cells — (0,0), (2,2) and (5,5), cell indices
/// 0, 18 and 45 — two agents one step east of a stocked cell, and one on
/// bare ground whose gathers always fail.
fn scripted(cfg: WorldConfig) -> World {
    let mut w = empty_world(cfg);
    let p = w.config.initial_params.clone();
    let mut add = |pos, kind| add_agent_with(&mut w, pos, 50_000, 0, kind, p.clone());
    add(Pos::new(2, 2), PolicyKind::Forager); // 0
    add(Pos::new(2, 2), PolicyKind::Learner); // 1
    add(Pos::new(6, 5), PolicyKind::Enforcer); // 2: steps west onto (5,5)
    add(Pos::new(0, 0), PolicyKind::Forager); // 3
    add(Pos::new(5, 5), PolicyKind::Forager); // 4
    add(Pos::new(3, 2), PolicyKind::Learner); // 5: steps west onto (2,2)
    add(Pos::new(7, 7), PolicyKind::Forager); // 6: bare ground
    set_food(&mut w, Pos::new(2, 2), 40);
    set_food(&mut w, Pos::new(5, 5), 40);
    set_food(&mut w, Pos::new(0, 0), 40);
    w
}

/// Everyone gathers; the arrivals step onto the stocked cells; everyone
/// gathers again. Returns the three reports.
fn script(w: &mut World) -> [TickReport; 3] {
    use Action::{Gather, Move, Rest};
    let t0 = w.apply(intents(&[Gather; 7]));
    let t1 = w.apply(intents(&[
        Rest,
        Rest,
        Move(Dir::West),
        Rest,
        Rest,
        Move(Dir::West),
        Rest,
    ]));
    assert_eq!(w.agents[2].pos, Pos::new(5, 5));
    assert_eq!(w.agents[5].pos, Pos::new(2, 2));
    let t2 = w.apply(intents(&[Gather; 7]));
    [t0, t1, t2]
}

fn sums_agree(r: &TickReport) {
    assert_eq!(
        r.gatherers.len() as u32,
        r.gathers.iter().sum::<u32>(),
        "tick {}: gatherers vs gathers",
        r.tick
    );
    assert_eq!(
        r.encroachers.len() as u32,
        r.encroachments.iter().sum::<u32>(),
        "tick {}: encroachers vs encroachments",
        r.tick
    );
}

#[test]
fn gatherers_come_in_cell_then_id_order_and_encroachers_in_the_order_of_violated() {
    let mut w = scripted(WorldConfig {
        nomos: true,
        record_violations: true,
        ..cfg()
    });
    w.intervene(rule()).unwrap();
    let [t0, t1, t2] = script(&mut w);

    // Tick 0: only the stocked cells yield; founders encroach on nobody.
    // Cell order (0,0) < (2,2) < (5,5), ids ascending within (2,2).
    assert_eq!(t0.gatherers, ids(&[3, 0, 1, 4]));
    assert_eq!(t0.gathers, [3, 0, 0, 1, 0]);
    assert!(t0.encroachers.is_empty());
    assert!(violated(&t0).is_empty());
    // Tick 1: nobody gathered.
    assert!(t1.gatherers.is_empty() && t1.encroachers.is_empty());
    // Tick 2: the late arrivals gather beside the founders. 5 (arrived at
    // tick 2) encroached on 0 (a founder, the smaller id of the two on the
    // cell); 2 on 4. Cell order puts (2,2) before (5,5), so the encroacher
    // with the larger id comes first — exactly as `Violated` has it.
    assert_eq!(t2.gatherers, ids(&[3, 0, 1, 5, 2, 4]));
    assert_eq!(t2.gathers, [3, 0, 0, 2, 1]);
    assert_eq!(t2.encroachers, vec![pair(5, 0), pair(2, 4)]);
    assert_eq!(violated(&t2), t2.encroachers);
    assert_eq!(t2.encroachments, [0, 0, 0, 1, 1]);
    for r in [&t0, &t1, &t2] {
        sums_agree(r);
    }
    // Agent 6 never gathered anything: bare ground fails as `NoResource`.
    assert!(!t0.gatherers.contains(&AgentId(6)) && !t2.gatherers.contains(&AgentId(6)));
    assert_eq!(w.ledger.gathers, 10);
    assert_eq!(w.ledger.encroachments, 2);
}

#[test]
fn the_material_needs_no_rule_and_no_record_and_the_encroachers_need_nomos() {
    // The same world with the tracking on but no rule: encroachments are
    // facts (ADR-0014 D3), so `encroachers` is the same list while nothing
    // is `Violated`.
    let mut w = scripted(WorldConfig {
        nomos: true,
        ..cfg()
    });
    let [t0, _, t2] = script(&mut w);
    assert_eq!(t0.gatherers, ids(&[3, 0, 1, 4]));
    assert_eq!(t2.gatherers, ids(&[3, 0, 1, 5, 2, 4]));
    assert_eq!(t2.encroachers, vec![pair(5, 0), pair(2, 4)]);
    assert!(violated(&t2).is_empty());
    assert!(w.nomos.rules.is_empty());

    // Tracking off: the gatherers are still listed (physics, every
    // world); there is no first occupant, so no encroacher.
    let mut w = scripted(cfg());
    let [t0, t1, t2] = script(&mut w);
    assert_eq!(t0.gatherers, ids(&[3, 0, 1, 4]));
    assert_eq!(t2.gatherers, ids(&[3, 0, 1, 5, 2, 4]));
    for r in [&t0, &t1, &t2] {
        assert!(r.encroachers.is_empty());
        assert_eq!(r.encroachments, [0; PolicyKind::COUNT]);
        sums_agree(r);
    }
    assert_eq!(w.ledger.encroachments, 0);
}

/// A generated institution world, 300 ticks: every tick the gatherers are
/// strictly ordered by (cell at S(t), id), their kinds sum to `gathers`,
/// every encroacher gathered on the first occupant's cell, the encroachers'
/// kinds sum to `encroachments`, and the list is `Violated`'s.
#[test]
fn in_a_generated_institution_world_the_material_agrees_with_the_counts_every_tick() {
    let mut w = World::generate(
        WorldConfig {
            seed: 11,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            record_violations: true,
            ..WorldConfig::institution_preset()
        },
        |i| {
            if i % 10 == 0 {
                PolicyKind::Enforcer
            } else {
                PolicyKind::Learner
            }
        },
    )
    .unwrap();
    w.intervene(rule()).unwrap();
    let bounds = w.bounds();
    let (mut gathers, mut encroachments, mut ticks_with_encroachers) = (0u64, 0u64, 0u32);
    for _ in 0..300 {
        // Where everyone stood at S(t), by id (the agents are id-sorted).
        let before: Vec<(AgentId, usize, PolicyKind)> = w
            .agents
            .iter()
            .map(|a| (a.id, bounds.index(a.pos), a.policy))
            .collect();
        let r = w.step();
        let at = |id: AgentId| before[before.binary_search_by_key(&id, |x| x.0).unwrap()];
        let keys: Vec<(usize, AgentId)> = r.gatherers.iter().map(|&g| (at(g).1, g)).collect();
        assert!(
            keys.windows(2).all(|p| p[0] < p[1]),
            "tick {}: gatherers not in (cell, id) order: {keys:?}",
            r.tick
        );
        let mut by_kind = [0u32; PolicyKind::COUNT];
        for &g in &r.gatherers {
            by_kind[at(g).2.index()] += 1;
        }
        assert_eq!(by_kind, r.gathers, "tick {}", r.tick);
        let mut enc_by_kind = [0u32; PolicyKind::COUNT];
        for &(g, occupant) in &r.encroachers {
            enc_by_kind[at(g).2.index()] += 1;
            assert_ne!(g, occupant);
            assert_eq!(
                at(g).1,
                at(occupant).1,
                "tick {}: not the same cell",
                r.tick
            );
            assert!(
                r.gatherers.contains(&g),
                "tick {}: an encroacher gathered",
                r.tick
            );
        }
        assert_eq!(enc_by_kind, r.encroachments, "tick {}", r.tick);
        assert_eq!(violated(&r), r.encroachers, "tick {}", r.tick);
        sums_agree(&r);
        gathers += r.gatherers.len() as u64;
        encroachments += r.encroachers.len() as u64;
        ticks_with_encroachers += u32::from(!r.encroachers.is_empty());
    }
    assert_eq!(gathers, w.ledger.gathers);
    assert_eq!(encroachments, w.ledger.encroachments);
    assert!(ticks_with_encroachers > 50, "{ticks_with_encroachers}");
    assert!(w.ledger.sanctions.iter().sum::<u64>() > 0);
}
