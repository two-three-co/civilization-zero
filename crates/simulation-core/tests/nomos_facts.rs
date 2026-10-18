//! Social facts (ADR-0014 D3, task 4-1): first occupancy by arrival order,
//! encroachment, the audit of accepted deals, the health term of the
//! learner's reward, and the contract that tracking changes no decision
//! and adds no byte to a world that has it switched off.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::policy::{feature, PolicyTable, REFLEX_UNIT};
use simulation_core::world::SNAPSHOT_FORMAT;
use simulation_core::{
    Action, AgentId, Dir, Interaction, InteractionKind, Intervention, Message, OpenDeal,
    PolicyKind, PolicyParams, Pos, SnapshotError, TickReport, World, WorldConfig,
};

const K: usize = Action::KIND_COUNT;
const FORAGER: usize = 0;

/// The hand-built world of `common::cfg()` with the tracking on.
fn ncfg() -> WorldConfig {
    WorldConfig {
        nomos: true,
        ..cfg()
    }
}

fn propose(to: u64, give: u8, want: u8, by: u16) -> Action {
    Action::Signal(Message::Propose {
        to: AgentId(to),
        give,
        want,
        by,
    })
}

fn accept(to: u64, offer_tick: u64) -> Action {
    Action::Signal(Message::Accept {
        to: AgentId(to),
        offer_tick,
    })
}

fn transfer(to: u64, n: u32) -> Action {
    Action::Transfer { to: AgentId(to), n }
}

fn encroached(tick: u64, by: u64, value: i64) -> Interaction {
    Interaction {
        tick,
        other: AgentId(by),
        kind: InteractionKind::EncroachedBy,
        value,
    }
}

fn by_kind(n: u32) -> [u32; PolicyKind::COUNT] {
    let mut a = [0; PolicyKind::COUNT];
    a[FORAGER] = n;
    a
}

/// The physical part of an agent: everything the tracking must not touch.
fn body(w: &World) -> Vec<(AgentId, Pos, i64, i64, u32, u64, PolicyParams)> {
    w.agents
        .iter()
        .map(|a| {
            (
                a.id,
                a.pos,
                a.energy,
                a.health,
                a.inventory,
                a.age,
                a.params.clone(),
            )
        })
        .collect()
}

#[test]
fn first_occupancy_is_arrival_order_and_lapses_with_a_move_or_a_death() {
    use Action::{Gather, Move, Rest};
    let mut w = empty_world(ncfg());
    let a = add_agent(&mut w, Pos::new(2, 2), 50_000, 0); // 0: founder
    let b = add_agent(&mut w, Pos::new(2, 2), 50_000, 0); // 1: founder, same cell
    let c = add_agent(&mut w, Pos::new(3, 2), 50_000, 0); // 2: founder, one cell east
    let d = add_agent(&mut w, Pos::new(0, 0), 50_000, 0); // 3: founder in the corner
    set_food(&mut w, Pos::new(2, 2), 40);
    assert!(w.agents.iter().all(|x| x.arrived.is_none()));

    // Tick 0: two founders gather side by side: nobody arrived after anybody.
    let r = w.apply(intents(&[Gather, Gather, Rest, Rest]));
    assert_eq!(r.gathers, by_kind(2));
    assert_eq!(r.encroachments, [0; PolicyKind::COUNT]);
    assert_eq!((w.ledger.gathers, w.ledger.encroachments), (2, 0));
    assert!(w.agents.iter().all(|x| x.recent.is_empty()));

    // Tick 1: C steps onto the cell (arrived = 2); D walks into the wall
    // and keeps its founding claim.
    w.apply(intents(&[Rest, Rest, Move(Dir::West), Move(Dir::North)]));
    assert_eq!(w.agents[c].pos, Pos::new(2, 2));
    assert_eq!(w.agents[c].arrived, Some(2));
    assert_eq!(w.agents[d].arrived, None, "a failed move changes nothing");
    assert_eq!(w.agents[a].arrived, None);

    // Tick 2: A and C gather. The first occupant is A (a founder, the
    // smaller id of the two); C arrived later, so C encroached on A — and
    // only A remembers it, with the amount taken.
    let r = w.apply(intents(&[Gather, Rest, Gather, Rest]));
    assert_eq!(r.gathers, by_kind(2));
    assert_eq!(r.encroachments, by_kind(1));
    assert_eq!(w.ledger.encroachments, 1);
    assert_eq!(w.agents[a].recent, vec![encroached(2, 2, 2)]);
    assert!(w.agents[b].recent.is_empty());
    assert!(w.agents[c].recent.is_empty());

    // Ticks 3-4: A leaves and comes back: its claim is now the youngest.
    w.apply(intents(&[Move(Dir::East), Rest, Rest, Rest]));
    assert_eq!(w.agents[a].arrived, Some(4));
    w.apply(intents(&[Move(Dir::West), Rest, Rest, Rest]));
    assert_eq!(
        (w.agents[a].pos, w.agents[a].arrived),
        (Pos::new(2, 2), Some(5))
    );
    // Tick 5: A gathers where B (founder) is now the first occupant.
    let r = w.apply(intents(&[Gather, Rest, Rest, Rest]));
    assert_eq!(r.encroachments, by_kind(1));
    assert_eq!(w.agents[b].recent, vec![encroached(5, 0, 2)]);

    // B leaves the world: the next earliest arrival, C (tick 2), is the
    // first occupant; A (tick 5) encroaches on C, C on nobody.
    w.intervene(Intervention::RemoveAgent { id: AgentId(1) })
        .unwrap();
    assert_eq!(w.agents.len(), 3);
    let r = w.apply(intents(&[Gather, Gather, Rest])); // A, C, D
    assert_eq!(r.gathers, by_kind(2));
    assert_eq!(r.encroachments, by_kind(1));
    assert_eq!(w.agents[1].id, AgentId(2));
    assert_eq!(w.agents[1].recent, vec![encroached(6, 0, 2)]);
    assert_eq!(w.agents[0].recent, vec![encroached(2, 2, 2)]);
    assert_eq!(w.ledger.encroachments, 3);
    assert_eq!(w.ledger.gathers, 7);
    w.check_invariants().unwrap();
}

#[test]
fn same_tick_arrivals_do_not_encroach_and_a_tie_goes_to_the_smaller_id() {
    use Action::{Gather, Move, Rest};
    let mut w = empty_world(ncfg());
    let d = add_agent(&mut w, Pos::new(4, 5), 50_000, 0); // 0
    let e = add_agent(&mut w, Pos::new(5, 4), 50_000, 0); // 1
    let f = add_agent(&mut w, Pos::new(5, 6), 50_000, 0); // 2
    set_food(&mut w, Pos::new(5, 5), 40);

    // Tick 0: D and E step onto (5, 5) together.
    w.apply(intents(&[Move(Dir::East), Move(Dir::South), Rest]));
    assert_eq!(
        (w.agents[d].pos, w.agents[e].pos),
        (Pos::new(5, 5), Pos::new(5, 5))
    );
    assert_eq!(
        (w.agents[d].arrived, w.agents[e].arrived),
        (Some(1), Some(1))
    );

    // Tick 1: both gather; F arrives (tick 2).
    let r = w.apply(intents(&[Gather, Gather, Move(Dir::North)]));
    assert_eq!(r.gathers, by_kind(2));
    assert_eq!(r.encroachments, [0; PolicyKind::COUNT]);
    assert_eq!(w.agents[f].arrived, Some(2));

    // Tick 2: all three gather. D and E tie on arrival; the smaller id (D)
    // is the first occupant, E does not encroach on it, F does.
    let r = w.apply(intents(&[Gather, Gather, Gather]));
    assert_eq!(r.gathers, by_kind(3));
    assert_eq!(r.encroachments, by_kind(1));
    assert_eq!(w.agents[d].recent, vec![encroached(2, 2, 2)]);
    assert!(w.agents[e].recent.is_empty());
    assert!(w.agents[f].recent.is_empty());
    assert_eq!(w.ledger.encroachments, 1);
    w.check_invariants().unwrap();
}

#[test]
fn children_share_the_claim_spawned_agents_arrive_at_the_intervention_and_off_tracks_nothing() {
    use Action::{Gather, Move, Rest};
    let c = WorldConfig {
        birth_threshold: Some(60_000),
        child_energy: 30_000,
        ..ncfg()
    };
    let script: Vec<(Option<Intervention>, Vec<Action>)> = vec![
        // Tick 0: P rests, Q steps onto (5, 5); both give birth.
        (None, vec![Rest, Move(Dir::South)]),
        // Tick 1: each family gathers on its own cell.
        (None, vec![Gather, Gather, Gather, Gather]),
        // Tick 2: two agents are spawned onto P's cell, then everybody
        // there gathers.
        (
            Some(Intervention::SpawnAgents {
                n: 2,
                pos: Pos::new(2, 2),
                policy: PolicyKind::Forager,
            }),
            vec![Gather, Rest, Gather, Rest, Gather, Gather],
        ),
    ];
    let run = |cfg: WorldConfig| -> (World, Vec<TickReport>) {
        let mut w = empty_world(cfg);
        add_agent(&mut w, Pos::new(2, 2), 70_000, 0); // 0: P, founder
        add_agent(&mut w, Pos::new(5, 4), 70_000, 0); // 1: Q, founder, moves first
        set_food(&mut w, Pos::new(2, 2), 40);
        set_food(&mut w, Pos::new(5, 5), 40);
        let mut reports = Vec::new();
        for (iv, actions) in &script {
            if let Some(iv) = iv {
                w.intervene(*iv).unwrap();
            }
            reports.push(w.apply(intents(actions)));
        }
        (w, reports)
    };

    let (on, reports) = run(c.clone());
    assert_eq!(reports[0].births, 2);
    assert_eq!(on.agents[2].arrived, None, "a founder's child is a founder");
    assert_eq!(
        on.agents[3].arrived,
        Some(1),
        "a mover's child arrived when the mover did"
    );
    assert_eq!(reports[1].gathers, by_kind(4));
    assert_eq!(
        reports[1].encroachments,
        [0; PolicyKind::COUNT],
        "a family gathers together"
    );
    assert!(on.agents[4..].iter().all(|x| x.arrived == Some(2)));
    // P and its child are founders; the two spawned agents arrived at tick
    // 2 and both encroached on P, in id order. Neither encroached on the
    // other (same tick), nor on the child.
    assert_eq!(reports[2].gathers, by_kind(4));
    assert_eq!(reports[2].encroachments, by_kind(2));
    assert_eq!(
        on.agents[0].recent,
        vec![encroached(2, 4, 2), encroached(2, 5, 2)]
    );
    assert!(on.agents[2].recent.is_empty());
    assert_eq!(on.ledger.encroachments, 2);
    on.check_invariants().unwrap();

    // The same script with the switch off: identical bodies, resources,
    // events and gathers; no claim, no encroachment, no memory of one.
    let (off, off_reports) = run(WorldConfig { nomos: false, ..c });
    assert_eq!(body(&off), body(&on));
    assert_eq!(off.resources, on.resources);
    assert_eq!(off.ledger.gathers, on.ledger.gathers);
    assert_eq!(off.ledger.encroachments, 0);
    assert!(off.agents.iter().all(|x| x.arrived.is_none()));
    assert!(off.agents.iter().all(|x| x.recent.is_empty()));
    assert!(off.nomos.is_empty());
    for (a, b) in off_reports.iter().zip(&reports) {
        assert_eq!(a.events, b.events);
        assert_eq!(a.gathers, b.gathers);
        assert_eq!(a.encroachments, [0; PolicyKind::COUNT]);
    }
    off.check_invariants().unwrap();
}

#[test]
fn deals_are_audited_from_the_acceptance_and_settled_at_the_deadline() {
    use Action::Rest;
    let deal = |offer_tick: u64, accepted_tick: u64, by: u16, give: u8, want: u8| OpenDeal {
        proposer: AgentId(0),
        acceptor: AgentId(1),
        offer_tick,
        accepted_tick,
        deadline: offer_tick + by as u64,
        give,
        want,
        given: 0,
        repaid: 0,
    };
    // A (0) and B (1), adjacent, five food each. A always proposes, B
    // always accepts; what follows differs per deal.
    let script: Vec<[Action; 2]> = vec![
        // 0-4: kept — A gives 2 at tick 2, B repays 3 at tick 3, due at 4.
        [propose(1, 2, 3, 4), Rest],
        [Rest, accept(0, 0)],
        [transfer(1, 2), Rest],
        [Rest, transfer(0, 3)],
        [Rest, Rest],
        // 5-8: broken by the acceptor — A gives, B never repays, due at 8.
        [propose(1, 1, 2, 3), Rest],
        [Rest, accept(0, 5)],
        [transfer(1, 1), Rest],
        [Rest, Rest],
        // 9-11: a transfer in the acceptance tick counts although the
        // giver acts before the acceptor; B repays at the deadline itself.
        [propose(1, 1, 1, 2), Rest],
        [transfer(1, 1), accept(0, 9)],
        [Rest, transfer(0, 1)],
        // 12-14: a late acceptance (deadline 13, accepted 14) settles at
        // once: nothing could be delivered in time.
        [propose(1, 0, 1, 1), Rest],
        [Rest, Rest],
        [Rest, accept(0, 12)],
        // 15-16: a deal that asks nothing is kept at its deadline, which
        // is the acceptance tick.
        [propose(1, 0, 0, 1), Rest],
        [Rest, accept(0, 15)],
        // 17-19: both short.
        [propose(1, 2, 2, 2), Rest],
        [Rest, accept(0, 17)],
        [Rest, Rest],
        // 20-25: two overlapping deals; the transfers of tick 24 count for
        // both (ADR-0013 D5 addendum).
        [propose(1, 1, 1, 4), Rest],
        [Rest, accept(0, 20)],
        [propose(1, 1, 1, 3), Rest],
        [Rest, accept(0, 22)],
        [transfer(1, 1), transfer(0, 1)],
        [Rest, Rest],
    ];
    let run = |nomos: bool| -> (World, Vec<TickReport>) {
        let mut w = empty_world(WorldConfig { nomos, ..cfg() });
        add_agent(&mut w, Pos::new(2, 2), 50_000, 5);
        add_agent(&mut w, Pos::new(3, 2), 50_000, 5);
        let reports = script.iter().map(|s| w.apply(intents(s))).collect();
        (w, reports)
    };
    let (w, r) = run(true);
    assert!(
        r.iter().all(|t| t.failures.iter().sum::<u32>() == 0),
        "every message was delivered and every transfer moved: {:?}",
        r.iter().map(|t| t.failures).collect::<Vec<_>>()
    );
    let settled: Vec<(u64, Vec<OpenDeal>)> = r
        .iter()
        .filter(|t| !t.promises_settled.is_empty())
        .map(|t| (t.tick, t.promises_settled.clone()))
        .collect();
    assert_eq!(
        settled,
        vec![
            (
                4,
                vec![OpenDeal {
                    given: 2,
                    repaid: 3,
                    ..deal(0, 1, 4, 2, 3)
                }]
            ),
            (
                8,
                vec![OpenDeal {
                    given: 1,
                    ..deal(5, 6, 3, 1, 2)
                }]
            ),
            (
                11,
                vec![OpenDeal {
                    given: 1,
                    repaid: 1,
                    ..deal(9, 10, 2, 1, 1)
                }]
            ),
            (14, vec![deal(12, 14, 1, 0, 1)]),
            (16, vec![deal(15, 16, 1, 0, 0)]),
            (19, vec![deal(17, 18, 2, 2, 2)]),
            (
                24,
                vec![OpenDeal {
                    given: 1,
                    repaid: 1,
                    ..deal(20, 21, 4, 1, 1)
                }]
            ),
            (
                25,
                vec![OpenDeal {
                    given: 1,
                    repaid: 1,
                    ..deal(22, 23, 3, 1, 1)
                }]
            ),
        ]
    );
    let verdicts: Vec<(bool, bool, bool)> = settled
        .iter()
        .flat_map(|(_, ds)| ds.iter())
        .map(|d| (d.kept(), d.proposer_short(), d.acceptor_short()))
        .collect();
    assert_eq!(
        verdicts,
        vec![
            (true, false, false),
            (false, false, true),
            (true, false, false),
            (false, false, true),
            (true, false, false),
            (false, true, true),
            (true, false, false),
            (true, false, false),
        ]
    );
    assert!(w.nomos.deals.is_empty(), "nothing left open");
    assert_eq!((w.ledger.promises_due, w.ledger.promises_broken), (8, 3));
    assert_eq!(w.ledger.acceptances, 8);
    // The deal is open, with its terms, between acceptance and deadline.
    let mut mid = empty_world(ncfg());
    add_agent(&mut mid, Pos::new(2, 2), 50_000, 5);
    add_agent(&mut mid, Pos::new(3, 2), 50_000, 5);
    for s in &script[..3] {
        mid.apply(intents(s));
    }
    assert_eq!(
        mid.nomos.deals,
        vec![OpenDeal {
            given: 2,
            ..deal(0, 1, 4, 2, 3)
        }]
    );
    mid.check_invariants().unwrap();
    w.check_invariants().unwrap();

    // Counted, not enforced: the switch changes no body, no inventory, no
    // event and no memory — only the audit exists.
    let (off, off_r) = run(false);
    assert_eq!(body(&off), body(&w));
    assert_eq!(
        off.agents.iter().map(|a| &a.recent).collect::<Vec<_>>(),
        w.agents.iter().map(|a| &a.recent).collect::<Vec<_>>()
    );
    for (a, b) in off_r.iter().zip(&r) {
        assert_eq!(a.events, b.events);
        assert!(a.promises_settled.is_empty());
    }
    assert!(off.nomos.is_empty());
    assert_eq!(
        (off.ledger.promises_due, off.ledger.promises_broken),
        (0, 0)
    );
    assert_eq!(off.ledger.acceptances, 8);
}

#[test]
fn the_health_term_prices_harm_taken_and_rest_healing() {
    for weight in [0u16, 1, 3] {
        let c = WorldConfig {
            learner_health_weight: weight,
            learner_epsilon_permille: 0,
            ..cfg()
        };
        let mut w = empty_world(c);
        let params = w.config.initial_params.clone();
        let l = add_agent_with(
            &mut w,
            Pos::new(2, 2),
            50_000,
            0,
            PolicyKind::Learner,
            params.clone(),
        );
        let h = add_agent_with(
            &mut w,
            Pos::new(3, 2),
            50_000,
            0,
            PolicyKind::Reflex,
            params,
        );
        // The learner's greedy choice in every row: Consume (fails on an
        // empty pack and costs nothing), later Rest.
        let force = |w: &mut World, kind: usize| match w.agents[l].params.table.as_mut() {
            Some(PolicyTable::Learner { q }) => {
                for (k, v) in q.iter_mut().enumerate() {
                    *v = if k % K == kind { 100 } else { -100 };
                }
            }
            other => panic!("not a learner table: {other:?}"),
        };
        force(&mut w, 2);
        // The reflex agent always harms its nearest neighbour: the learner.
        match w.agents[h].params.table.as_mut() {
            Some(PolicyTable::Reflex { weights }) => {
                weights.fill(0);
                for k in 0..K {
                    weights[feature::BIAS * K + k] =
                        if k == 6 { 1000 } else { -1000 } * REFLEX_UNIT;
                }
            }
            other => panic!("not a reflex table: {other:?}"),
        }
        w.check_invariants().unwrap();

        // Tick 0: metabolism −500 (reward −5) and 20 health taken.
        let r = w.step();
        assert_eq!(r.harms, 1);
        assert_eq!(w.agents[l].health, 80);
        assert_eq!(w.agents[l].energy, 49_500);
        let reward = w.agents[l].last_step.expect("the learner decided").reward;
        assert_eq!(reward, -5 - 20 * weight as i16, "weight {weight}");

        // Tick 1: the attacker is gone; the learner rests and heals one.
        w.intervene(Intervention::RemoveAgent { id: AgentId(1) })
            .unwrap();
        force(&mut w, 3);
        let r = w.step();
        assert_eq!(r.actions[3], 1);
        assert_eq!(w.agents[0].health, 81);
        let reward = w.agents[0].last_step.unwrap().reward;
        assert_eq!(reward, -5 + weight as i16, "weight {weight}");
        w.check_invariants().unwrap();
    }
}

#[test]
fn tracking_changes_no_decision_in_forager_and_learner_worlds() {
    for kind in [PolicyKind::Forager, PolicyKind::Learner] {
        let base = WorldConfig {
            seed: 7,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            ..WorldConfig::default()
        };
        let mut off = World::generate(base.clone(), |_| kind).unwrap();
        let mut on = World::generate(
            WorldConfig {
                nomos: true,
                ..base
            },
            |_| kind,
        )
        .unwrap();
        for _ in 0..300 {
            let a = off.step();
            let b = on.step();
            assert_eq!(a.tick, b.tick);
            assert_eq!(a.actions, b.actions, "tick {}", a.tick);
            assert_eq!(a.failures, b.failures);
            assert_eq!(
                (a.food_gathered, a.food_consumed, a.births, a.deaths),
                (b.food_gathered, b.food_consumed, b.births, b.deaths)
            );
            assert_eq!(a.events, b.events);
            assert_eq!(a.gathers, b.gathers);
            assert_eq!(a.learner_rows, b.learner_rows);
            assert_eq!(a.encroachments, [0; PolicyKind::COUNT]);
            assert!(b.promises_settled.is_empty(), "nobody here proposes");
        }
        assert_eq!(body(&off), body(&on), "{kind:?}");
        assert_eq!(off.resources, on.resources);
        assert_eq!(
            off.agents.iter().map(|a| a.last_step).collect::<Vec<_>>(),
            on.agents.iter().map(|a| a.last_step).collect::<Vec<_>>()
        );
        assert_eq!(off.ledger.gathers, on.ledger.gathers);
        assert!(off.ledger.gathers > 0);
        assert_eq!(off.ledger.encroachments, 0);
        assert!(off.agents.iter().all(|a| a.arrived.is_none()));
        assert!(on.agents.iter().any(|a| a.arrived.is_some()));
        assert!(
            on.ledger.encroachments > 0,
            "{kind:?}: a crowded world has gathers after someone else"
        );
        assert!(on.ledger.encroachments <= on.ledger.gathers);
        assert!(on.agents.iter().any(|a| a
            .recent
            .iter()
            .any(|e| e.kind == InteractionKind::EncroachedBy)));
        assert_ne!(
            off.dynamics_hash(),
            on.dynamics_hash(),
            "the tracked state is state"
        );
        off.check_invariants().unwrap();
        on.check_invariants().unwrap();
    }
}

#[test]
fn reports_count_gathers_by_kind_and_learner_row_visits() {
    let c = WorldConfig {
        seed: 3,
        width: 32,
        height: 32,
        initial_agents: 120,
        patch_count: 4,
        learner_states: 8,
        nomos: true,
        ..WorldConfig::default()
    };
    let mut w = World::generate(c, |i| match i % 3 {
        0 => PolicyKind::Forager,
        1 => PolicyKind::Random,
        _ => PolicyKind::Learner,
    })
    .unwrap();
    let mut seen_learner = false;
    for _ in 0..60 {
        let learners = w
            .agents
            .iter()
            .filter(|a| a.policy == PolicyKind::Learner)
            .count() as u32;
        let before = w.ledger.clone();
        let r = w.step();
        assert_eq!(
            r.gathers.iter().map(|&g| g as u64).sum::<u64>(),
            w.ledger.gathers - before.gathers
        );
        assert_eq!(
            r.encroachments.iter().map(|&g| g as u64).sum::<u64>(),
            w.ledger.encroachments - before.encroachments
        );
        for k in 0..PolicyKind::COUNT {
            assert!(r.encroachments[k] <= r.gathers[k]);
        }
        assert_eq!(
            r.promises_settled.len() as u64,
            w.ledger.promises_due - before.promises_due
        );
        assert_eq!(
            r.promises_settled.iter().filter(|d| !d.kept()).count() as u64,
            w.ledger.promises_broken - before.promises_broken
        );
        if learners > 0 {
            assert_eq!(r.learner_rows.len(), 8);
            assert_eq!(r.learner_rows.iter().sum::<u32>(), learners);
            seen_learner = true;
        } else {
            assert!(r.learner_rows.is_empty());
        }
    }
    assert!(seen_learner);
    assert!(w.ledger.gathers > 0);
    w.check_invariants().unwrap();

    // A forager-only world reports no rows and, with the switch off, no
    // encroachment — but it still counts its gathers by kind.
    let mut f = World::new(WorldConfig {
        seed: 3,
        width: 32,
        height: 32,
        initial_agents: 120,
        patch_count: 4,
        ..WorldConfig::default()
    })
    .unwrap();
    for _ in 0..20 {
        let r = f.step();
        assert!(r.learner_rows.is_empty());
        assert_eq!(r.encroachments, [0; PolicyKind::COUNT]);
        assert_eq!(r.gathers[1..], [0; PolicyKind::COUNT - 1]);
    }
    assert!(f.ledger.gathers > 0);
    assert!(f.nomos.is_empty());
}

#[test]
fn snapshots_carry_claims_and_open_deals_and_invariants_bind_them_to_the_switch() {
    // A crowded world with random walkers: proposals are accepted every
    // few ticks, so deals are open when the snapshot is taken.
    let c = WorldConfig {
        seed: 11,
        width: 32,
        height: 32,
        initial_agents: 200,
        patch_count: 3,
        nomos: true,
        ..WorldConfig::default()
    };
    let mut w = World::generate(c, |i| {
        if i % 2 == 0 {
            PolicyKind::Forager
        } else {
            PolicyKind::Random
        }
    })
    .unwrap();
    let mut settled = 0u64;
    for _ in 0..120 {
        let r = w.step();
        settled += r.promises_settled.len() as u64;
        if !w.nomos.deals.is_empty() && settled > 0 && w.tick >= 60 {
            break;
        }
    }
    assert!(!w.nomos.deals.is_empty(), "a deal is open");
    assert_eq!(w.ledger.promises_due, settled);
    assert!(w.ledger.promises_due > 0);
    assert!(w.agents.iter().any(|a| a.arrived.is_some()));
    w.check_invariants().unwrap();

    let bin = w.to_bytes();
    let json = w.to_snapshot();
    assert_eq!(World::from_bytes(&bin).unwrap(), w);
    assert_eq!(World::from_snapshot(&json).unwrap(), w);
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(v["format"], SNAPSHOT_FORMAT);
    assert_eq!(SNAPSHOT_FORMAT, 12);
    assert!(v["world"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["arrived"].is_u64()));
    assert_eq!(
        v["world"]["nomos"]["deals"].as_array().unwrap().len(),
        w.nomos.deals.len()
    );
    assert_eq!(v["world"]["config"]["nomos"], true);
    assert!(v["world"]["ledger"]["encroachments"].is_u64());
    let mut old = v.clone();
    old["format"] = serde_json::json!(11);
    assert_eq!(
        World::from_snapshot(&serde_json::to_vec(&old).unwrap()).err(),
        Some(SnapshotError::Format(11))
    );

    // The tracked state is part of the dynamics: forgetting a deal or a
    // claim changes the hash.
    let h = w.dynamics_hash();
    let mut forgetful = w.clone();
    forgetful.nomos.deals.pop();
    assert_ne!(forgetful.dynamics_hash(), h);
    let mut newcomer = w.clone();
    let i = newcomer
        .agents
        .iter()
        .position(|a| a.arrived.is_some())
        .unwrap();
    newcomer.agents[i].arrived = None;
    assert_ne!(newcomer.dynamics_hash(), h);

    // Off, the same bytes would be an error: claims and deals exist only
    // where the switch says they are tracked.
    let mut off = w.clone();
    off.config.nomos = false;
    let errs = off.check_invariants().unwrap_err();
    assert!(errs
        .iter()
        .any(|e| e.contains("arrived set while nomos is off")));
    assert!(errs.iter().any(|e| e.contains("nomos state present")));
    assert!(matches!(
        World::from_snapshot(&off.to_snapshot()),
        Err(SnapshotError::Invalid(_))
    ));
    // Nor may a claim lie in the future or a deal stay open past its due.
    let mut future = w.clone();
    future.agents[i].arrived = Some(future.tick + 1);
    assert!(future.check_invariants().is_err());
    let mut overdue = w.clone();
    overdue.nomos.deals[0].deadline = overdue.tick - 1;
    assert!(overdue.check_invariants().is_err());
    let mut unordered = w.clone();
    if unordered.nomos.deals.len() >= 2 {
        unordered.nomos.deals.swap(0, 1);
        let (a, b) = (unordered.nomos.deals[0], unordered.nomos.deals[1]);
        if a.accepted_tick != b.accepted_tick {
            assert!(unordered.check_invariants().is_err());
        }
    }

    // A world that never tracked anything writes `null` claims, no deals,
    // and keeps every byte of its dynamics hash across the switch's
    // existence (the golden tests pin the value itself).
    let mut plain = World::new(WorldConfig {
        seed: 11,
        width: 32,
        height: 32,
        initial_agents: 100,
        patch_count: 3,
        ..WorldConfig::default()
    })
    .unwrap();
    plain.run(50);
    let v: serde_json::Value = serde_json::from_slice(&plain.to_snapshot()).unwrap();
    assert!(v["world"]["agents"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["arrived"].is_null()));
    assert_eq!(v["world"]["nomos"]["deals"].as_array().unwrap().len(), 0);
    assert_eq!(World::from_bytes(&plain.to_bytes()).unwrap(), plain);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// Worlds that track social facts are as deterministic as the rest:
    /// serial and parallel decisions agree down to the report, a twin
    /// replays to the same hash, the invariants hold, snapshots round-trip,
    /// and the ledger's counters agree with the reports.
    #[test]
    fn tracking_worlds_are_deterministic(
        seed in any::<u64>(),
        weight in 0u16..=2,
        slots in prop_oneof![Just(0u8), Just(2), Just(8)],
        ticks in 1u64..=80,
    ) {
        let cfg = WorldConfig {
            seed,
            width: 32,
            height: 32,
            initial_agents: 120,
            patch_count: 4,
            birth_threshold: Some(60_000),
            child_energy: 30_000,
            memory_slots: slots,
            learner_health_weight: weight,
            nomos: true,
            ..WorldConfig::default()
        };
        let mix = |i: u32| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Random,
            _ => PolicyKind::Learner,
        };
        let mut a = World::generate(cfg.clone(), mix).unwrap();
        let mut b = World::generate(cfg.clone(), mix).unwrap();
        let mut twin = a.clone();
        let (mut gathers, mut encroachments, mut due, mut broken) = (0u64, 0u64, 0u64, 0u64);
        for _ in 0..ticks {
            let ra = a.step();
            #[cfg(feature = "parallel")]
            let rb = b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            let rb = b.step();
            prop_assert_eq!(&ra, &rb);
            twin.step();
            gathers += ra.gathers.iter().map(|&g| g as u64).sum::<u64>();
            encroachments += ra.encroachments.iter().map(|&g| g as u64).sum::<u64>();
            due += ra.promises_settled.len() as u64;
            broken += ra.promises_settled.iter().filter(|d| !d.kept()).count() as u64;
            prop_assert!(ra.promises_settled.iter().all(|d| d.deadline <= ra.tick && d.accepted_tick <= ra.tick));
        }
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.state_hash(), twin.state_hash());
        prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
        prop_assert_eq!(a.ledger.gathers, gathers);
        prop_assert_eq!(a.ledger.encroachments, encroachments);
        prop_assert!(encroachments <= gathers);
        prop_assert_eq!((a.ledger.promises_due, a.ledger.promises_broken), (due, broken));
        prop_assert!(a.agents.iter().all(|x| x.arrived.is_none_or(|t| t <= a.tick)));
        prop_assert!(a.nomos.deals.iter().all(|d| d.deadline >= a.tick));
        if slots == 0 {
            prop_assert_eq!(a.ledger.acceptances, 0, "no memory, no acceptance, no deal");
            prop_assert!(a.nomos.deals.is_empty());
        }
        prop_assert_eq!(World::from_bytes(&a.to_bytes()).unwrap(), a.clone());
        prop_assert_eq!(World::from_snapshot(&a.to_snapshot()).unwrap(), a);
    }
}
