//! Interventions (ADR-0010 2a-1): applied between ticks, booked in the
//! ledger, recorded as events, rejected without side effects when invalid.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::{
    Action, AgentId, Event, Intervention, InterventionError, ParamsMode, PolicyKind, Pos, Who,
    World, WorldConfig,
};

#[test]
fn drought_replaces_regrowth_and_is_recorded() {
    let mut w = empty_world(WorldConfig {
        regrow_interval: 1,
        regrow_amount: 5,
        ..cfg()
    });
    set_food(&mut w, Pos::new(0, 0), 10);
    w.resources.food[0] = 2;
    w.ledger.food_initial -= 8;

    let e = w
        .intervene(Intervention::Drought {
            regrow_amount: 0,
            regrow_interval: 3,
        })
        .unwrap();
    assert_eq!(
        e,
        Event::Intervened {
            tick: 0,
            intervention: Intervention::Drought {
                regrow_amount: 0,
                regrow_interval: 3
            }
        }
    );
    assert_eq!(e.tick(), 0);
    w.run(6);
    assert_eq!(w.resources.food_at(Pos::new(0, 0)), 2, "nothing regrows");
    assert_eq!(w.config.regrow_interval, 3);
    assert_eq!(w.ledger.interventions, 1);
    w.check_invariants().unwrap();

    // Ending the drought: regrowth resumes at the new interval.
    w.intervene(Intervention::Drought {
        regrow_amount: 5,
        regrow_interval: 3,
    })
    .unwrap();
    w.run(3);
    assert_eq!(w.resources.food_at(Pos::new(0, 0)), 7);
    w.check_invariants().unwrap();
}

#[test]
fn scale_food_rounds_down_and_books_the_difference() {
    let mut w = empty_world(cfg());
    set_food(&mut w, Pos::new(1, 1), 3);
    set_food(&mut w, Pos::new(2, 2), 10);
    add_agent(&mut w, Pos::new(0, 0), 50_000, 4); // carried food is untouched

    w.intervene(Intervention::ScaleFood { permille: 500 })
        .unwrap();
    assert_eq!(w.resources.food_at(Pos::new(1, 1)), 1);
    assert_eq!(w.resources.food_at(Pos::new(2, 2)), 5);
    assert_eq!(w.agents[0].inventory, 4);
    assert_eq!(w.ledger.food_intervened, -7);
    w.check_invariants().unwrap();

    w.intervene(Intervention::ScaleFood { permille: 3_000 })
        .unwrap();
    assert_eq!(w.resources.food_at(Pos::new(2, 2)), 15);
    assert_eq!(w.ledger.food_intervened, -7 + 2 + 10);
    w.check_invariants().unwrap();
}

#[test]
fn spawn_agents_take_the_next_ids_and_act_next_tick() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(0, 0), 50_000, 0); // id 0
    w.apply(intents(&[Action::Rest]));

    let pos = Pos::new(3, 4);
    w.intervene(Intervention::SpawnAgents {
        n: 3,
        pos,
        policy: PolicyKind::Random,
    })
    .unwrap();
    let ids: Vec<u64> = w.agents.iter().map(|a| a.id.0).collect();
    assert_eq!(ids, vec![0, 1, 2, 3]);
    assert_eq!(w.next_agent_id, 4);
    for a in &w.agents[1..] {
        assert_eq!(
            (a.pos, a.energy, a.health, a.age),
            (pos, c.initial_energy, c.max_health, 0)
        );
        assert_eq!(a.policy, PolicyKind::Random);
        assert_eq!(a.params, c.initial_params);
    }
    assert_eq!(w.ledger.energy_intervened, 3 * c.initial_energy);
    assert_eq!(w.ledger.births, 0, "spawning is not birth");
    w.check_invariants().unwrap();

    let r = w.step();
    assert_eq!(r.actions.iter().sum::<u32>(), 4, "all four decided");
    w.check_invariants().unwrap();
}

#[test]
fn remove_agent_drops_food_and_books_its_energy_without_a_death() {
    let mut w = empty_world(cfg());
    let p = Pos::new(5, 5);
    add_agent(&mut w, Pos::new(0, 0), 10_000, 0); // id 0
    add_agent(&mut w, p, 30_000, 6); // id 1
    add_agent(&mut w, Pos::new(1, 1), 20_000, 0); // id 2

    let e = w
        .intervene(Intervention::RemoveAgent { id: AgentId(1) })
        .unwrap();
    assert!(matches!(e, Event::Intervened { tick: 0, .. }));
    let ids: Vec<u64> = w.agents.iter().map(|a| a.id.0).collect();
    assert_eq!(ids, vec![0, 2], "order kept");
    assert_eq!(w.resources.food_at(p), 6);
    assert_eq!(w.ledger.energy_intervened, -30_000);
    assert_eq!(w.ledger.deaths, 0);
    assert_eq!(w.ledger.energy_written_off, 0);
    w.check_invariants().unwrap();

    let before = w.state_hash();
    assert_eq!(
        w.intervene(Intervention::RemoveAgent { id: AgentId(1) }),
        Err(InterventionError::NoSuchAgent(AgentId(1)))
    );
    assert_eq!(
        w.state_hash(),
        before,
        "rejected intervention changes nothing"
    );
}

#[test]
fn invalid_interventions_are_rejected_without_mutation() {
    let mut w = World::new(WorldConfig {
        seed: 4,
        width: 16,
        height: 16,
        initial_agents: 20,
        patch_count: 2,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(5);
    let before = w.clone();
    let bad = [
        Intervention::Drought {
            regrow_amount: 1,
            regrow_interval: 0,
        },
        Intervention::ScaleFood {
            permille: Intervention::MAX_PERMILLE + 1,
        },
        Intervention::SpawnAgents {
            n: 0,
            pos: Pos::new(0, 0),
            policy: PolicyKind::Forager,
        },
        Intervention::SpawnAgents {
            n: 1,
            pos: Pos::new(16, 0),
            policy: PolicyKind::Forager,
        },
        Intervention::RemoveAgent {
            id: AgentId(10_000),
        },
    ];
    for iv in bad {
        assert!(w.intervene(iv).is_err(), "{iv:?} should be rejected");
        assert_eq!(w, before, "{iv:?} must not mutate");
    }
    assert_eq!(w.ledger.interventions, 0);
}

#[test]
fn same_intervention_sequence_replays_to_the_same_hash() {
    let base = World::new(WorldConfig {
        seed: 21,
        width: 32,
        height: 32,
        initial_agents: 60,
        patch_count: 3,
        ..WorldConfig::default()
    })
    .unwrap();
    let sequence = [
        (50u64, Intervention::ScaleFood { permille: 300 }),
        (
            80,
            Intervention::Drought {
                regrow_amount: 0,
                regrow_interval: 7,
            },
        ),
        (
            120,
            Intervention::SpawnAgents {
                n: 10,
                pos: Pos::new(16, 16),
                policy: PolicyKind::Forager,
            },
        ),
        // Placeholder id: the run closure removes the first living agent.
        (150, Intervention::RemoveAgent { id: AgentId(0) }),
    ];
    let run = |mut w: World, apply: bool| {
        let mut events = Vec::new();
        for (at, iv) in sequence {
            while w.tick < at {
                w.step();
            }
            if apply {
                let iv = match iv {
                    Intervention::RemoveAgent { .. } => {
                        Intervention::RemoveAgent { id: w.agents[0].id }
                    }
                    other => other,
                };
                events.push(w.intervene(iv).unwrap());
            }
        }
        w.run(100);
        w.check_invariants().unwrap();
        (w.state_hash(), events)
    };
    let (h1, e1) = run(base.clone(), true);
    let (h2, e2) = run(base.clone(), true);
    let (h0, _) = run(base, false);
    assert_eq!(h1, h2);
    assert_eq!(e1, e2);
    assert_ne!(h1, h0, "interventions changed history");
    assert_eq!(e1.len(), 4);
    assert_eq!(e1[1].tick(), 80);
}

#[test]
fn snapshot_roundtrip_keeps_intervention_ledger() {
    let mut w = World::new(WorldConfig {
        seed: 8,
        width: 24,
        height: 24,
        initial_agents: 30,
        patch_count: 2,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(20);
    w.intervene(Intervention::ScaleFood { permille: 250 })
        .unwrap();
    w.intervene(Intervention::SpawnAgents {
        n: 2,
        pos: Pos::new(1, 1),
        policy: PolicyKind::Forager,
    })
    .unwrap();
    let restored = World::from_snapshot(&w.to_snapshot()).unwrap();
    assert_eq!(restored, w);
    assert_eq!(restored.ledger.interventions, 2);
}

#[test]
fn intervened_event_serialises_as_tagged_record() {
    let e = Event::Intervened {
        tick: 5,
        intervention: Intervention::Drought {
            regrow_amount: 0,
            regrow_interval: 10,
        },
    };
    let json = serde_json::to_string(&e).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"intervened","tick":5,"intervention":{"op":"drought","regrow_amount":0,"regrow_interval":10}}"#
    );
    let back: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e);
    let spawn = serde_json::to_string(&Intervention::SpawnAgents {
        n: 2,
        pos: Pos::new(1, 2),
        policy: PolicyKind::Random,
    })
    .unwrap();
    assert_eq!(
        spawn,
        r#"{"op":"spawn_agents","n":2,"pos":{"x":1,"y":2},"policy":"Random"}"#
    );
}

fn intervention_strategy() -> impl Strategy<Value = Intervention> {
    prop_oneof![
        (0u32..3, 1u64..20).prop_map(|(a, i)| Intervention::Drought {
            regrow_amount: a,
            regrow_interval: i
        }),
        (0u32..=Intervention::MAX_PERMILLE)
            .prop_map(|permille| Intervention::ScaleFood { permille }),
        (1u32..6, 0u16..16, 0u16..16, any::<bool>()).prop_map(|(n, x, y, r)| {
            Intervention::SpawnAgents {
                n,
                pos: Pos::new(x, y),
                policy: if r {
                    PolicyKind::Random
                } else {
                    PolicyKind::Forager
                },
            }
        }),
        (0u64..60).prop_map(|id| Intervention::RemoveAgent { id: AgentId(id) }),
        (
            prop_oneof![
                Just(Who::All),
                (0u64..60).prop_map(|id| Who::Id(AgentId(id))),
                (0u32..=1100).prop_map(|permille| Who::Fraction { permille }),
            ],
            0usize..PolicyKind::ALL.len(),
            any::<bool>(),
        )
            .prop_map(|(who, k, keep)| Intervention::SetPolicy {
                who,
                kind: PolicyKind::ALL[k],
                params: if keep {
                    ParamsMode::Keep
                } else {
                    ParamsMode::Default
                },
            }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

    /// Random valid-or-invalid interventions interleaved with ticks keep every
    /// invariant, and a twin receiving the same sequence stays identical.
    #[test]
    fn interventions_keep_invariants_and_determinism(
        seed in any::<u64>(),
        sequence in prop::collection::vec((intervention_strategy(), 1u64..8), 1..10),
    ) {
        let cfg = WorldConfig {
            seed, width: 16, height: 16, initial_agents: 30, patch_count: 2,
            ..WorldConfig::default()
        };
        let mut w = World::new(cfg.clone()).unwrap();
        let mut twin = World::new(cfg).unwrap();
        for (iv, ticks) in sequence {
            let r1 = w.intervene(iv);
            let r2 = twin.intervene(iv);
            prop_assert_eq!(r1, r2);
            prop_assert!(w.check_invariants().is_ok(), "{:?}", w.check_invariants());
            for _ in 0..ticks {
                w.step();
                twin.step();
                prop_assert!(w.check_invariants().is_ok(), "{:?}", w.check_invariants());
            }
        }
        prop_assert_eq!(w.state_hash(), twin.state_hash());
    }
}
