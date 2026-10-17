//! The frame for learning policies (ADR-0013 3-5): `PolicyKind::{Reflex,
//! Learner}` with their tables, the `SetPolicy` intervention and snapshot
//! format 7. The decision rules are 3-6 / 3-7; here both kinds still decide
//! like the forager, and that is asserted so the placeholder can never be
//! mistaken for an implementation.

mod common;

use common::*;
use simulation_core::policy::forager_weights;
use simulation_core::world::SNAPSHOT_FORMAT;
use simulation_core::{
    Action, Agent, AgentId, Event, Intervention, InterventionError, ParamsMode, PolicyKind,
    PolicyParams, PolicyTable, Pos, SnapshotError, Who, World, WorldConfig, REFLEX_FEATURES,
};

fn small(seed: u64) -> WorldConfig {
    WorldConfig {
        seed,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..WorldConfig::default()
    }
}

fn set(who: Who, kind: PolicyKind, params: ParamsMode) -> Intervention {
    Intervention::SetPolicy { who, kind, params }
}

/// Everything about an agent except its policy and params.
fn body(a: &Agent) -> (AgentId, Pos, i64, i64, u32, u64) {
    (a.id, a.pos, a.energy, a.health, a.inventory, a.age)
}

fn bodies(w: &World) -> Vec<(AgentId, Pos, i64, i64, u32, u64)> {
    w.agents.iter().map(body).collect()
}

#[test]
fn learning_kinds_carry_blank_tables_and_for_now_decide_like_the_forager() {
    let cfg = small(5);
    let mut forager = World::new(cfg.clone()).unwrap();
    let mut reflex = World::generate(cfg.clone(), |_| PolicyKind::Reflex).unwrap();
    let mut learner = World::generate(cfg.clone(), |_| PolicyKind::Learner).unwrap();
    for a in &reflex.agents {
        assert_eq!(
            a.params.table,
            Some(PolicyTable::Reflex {
                weights: forager_weights()
            })
        );
        assert_eq!(
            forager_weights().len(),
            REFLEX_FEATURES * Action::KIND_COUNT
        );
        assert_eq!(a.params.hunger_permille, cfg.initial_params.hunger_permille);
    }
    for a in &learner.agents {
        match &a.params.table {
            Some(PolicyTable::Learner { q }) => assert_eq!(q.len(), 32 * Action::KIND_COUNT),
            other => panic!("learner without a Q table: {other:?}"),
        }
    }
    assert!(forager.agents.iter().all(|a| a.params.table.is_none()));

    for _ in 0..600 {
        forager.step();
        reflex.step();
        learner.step();
    }
    assert!(learner.ledger.births > 0, "generations passed");
    // Reflex: by construction of `forager_weights()` (3-6); Learner: the
    // 3-5 placeholder. Same bodies, same food, same ledger — only the kind
    // and its table differ, and those are state the hash sees.
    assert_eq!(bodies(&forager), bodies(&reflex));
    assert_eq!(bodies(&forager), bodies(&learner));
    assert_eq!(forager.resources.food, learner.resources.food);
    assert_eq!(forager.ledger, learner.ledger);
    assert_ne!(forager.dynamics_hash(), reflex.dynamics_hash());
    assert_ne!(reflex.dynamics_hash(), learner.dynamics_hash());
    assert!(reflex
        .agents
        .iter()
        .all(|a| a.policy == PolicyKind::Reflex && a.params.fits(a.policy, &cfg)));
    assert!(learner
        .agents
        .iter()
        .all(|a| a.policy == PolicyKind::Learner && a.params.fits(a.policy, &cfg)));
    reflex.check_invariants().unwrap();
    learner.check_invariants().unwrap();
}

#[test]
fn set_policy_names_its_agents_and_params_modes_keep_or_reset() {
    let cfg = small(7);
    let mut w = World::generate(cfg.clone(), |i| {
        if i % 2 == 0 {
            PolicyKind::Forager
        } else {
            PolicyKind::Random
        }
    })
    .unwrap();
    w.run(20);
    let n = w.agents.len();
    let id = w.agents[3].id;
    w.agents[3].params.hunger_permille = 777;

    // Id + Default: one agent, config scalars, blank table.
    let iv = set(Who::Id(id), PolicyKind::Learner, ParamsMode::Default);
    let e = w.intervene(iv).unwrap();
    assert_eq!(
        e,
        Event::Intervened {
            tick: 20,
            intervention: iv
        }
    );
    assert_eq!(w.agents[3].policy, PolicyKind::Learner);
    assert_eq!(
        w.agents[3].params,
        PolicyParams::for_kind(PolicyKind::Learner, &cfg)
    );
    assert_eq!(w.agents[3].params.hunger_permille, 500);
    assert!(w
        .agents
        .iter()
        .enumerate()
        .all(|(i, a)| i == 3 || a.policy != PolicyKind::Learner));

    // Keep: what fits stays (a learner keeps what it learned), the rest is
    // refit — blank table for a new learning kind, none for a fixed one.
    if let Some(PolicyTable::Learner { q }) = w.agents[3].params.table.as_mut() {
        q[5] = -42;
    }
    w.agents[3].params.hunger_permille = 777;
    w.intervene(set(Who::Id(id), PolicyKind::Learner, ParamsMode::Keep))
        .unwrap();
    assert!(matches!(
        &w.agents[3].params.table,
        Some(PolicyTable::Learner { q }) if q[5] == -42
    ));
    assert_eq!(w.agents[3].params.hunger_permille, 777);
    w.intervene(set(Who::Id(id), PolicyKind::Reflex, ParamsMode::Keep))
        .unwrap();
    assert_eq!(
        w.agents[3].params.table,
        PolicyTable::blank(PolicyKind::Reflex, &cfg)
    );
    assert_eq!(w.agents[3].params.hunger_permille, 777);
    w.intervene(set(Who::Id(id), PolicyKind::Forager, ParamsMode::Keep))
        .unwrap();
    assert_eq!(w.agents[3].params.table, None);
    assert_eq!(w.agents[3].params.hunger_permille, 777);
    w.check_invariants().unwrap();

    // All + Default.
    let (energy, food) = (w.total_energy(), w.resources.total_food());
    let before = bodies(&w);
    w.intervene(set(Who::All, PolicyKind::Reflex, ParamsMode::Default))
        .unwrap();
    assert!(w.agents.iter().all(|a| a.policy == PolicyKind::Reflex
        && a.params == PolicyParams::for_kind(PolicyKind::Reflex, &cfg)));
    assert_eq!(bodies(&w), before, "bodies are untouched");
    assert_eq!((w.total_energy(), w.resources.total_food()), (energy, food));

    // Fraction: seed-keyed, deterministic, roughly the asked share.
    let base = w.clone();
    w.intervene(set(
        Who::Fraction { permille: 500 },
        PolicyKind::Learner,
        ParamsMode::Default,
    ))
    .unwrap();
    let learners = w
        .agents
        .iter()
        .filter(|a| a.policy == PolicyKind::Learner)
        .count();
    assert!(
        (n / 4..=3 * n / 4).contains(&learners),
        "{learners} of {n} became learners"
    );
    let mut twin = base.clone();
    twin.intervene(set(
        Who::Fraction { permille: 500 },
        PolicyKind::Learner,
        ParamsMode::Default,
    ))
    .unwrap();
    assert_eq!(twin, w, "the draw is a function of seed, tick and id");
    let mut none = base.clone();
    none.intervene(set(
        Who::Fraction { permille: 0 },
        PolicyKind::Learner,
        ParamsMode::Default,
    ))
    .unwrap();
    assert!(none.agents.iter().all(|a| a.policy == PolicyKind::Reflex));
    let mut all = base.clone();
    all.intervene(set(
        Who::Fraction { permille: 1000 },
        PolicyKind::Learner,
        ParamsMode::Default,
    ))
    .unwrap();
    assert!(all.agents.iter().all(|a| a.policy == PolicyKind::Learner));
    assert_eq!(w.ledger.interventions, 6);
    w.check_invariants().unwrap();
    // The switched world keeps running (and, in the 3-5 frame, like foragers).
    w.run(50);
    w.check_invariants().unwrap();
}

#[test]
fn invalid_set_policy_is_rejected_without_mutation() {
    let mut w = World::new(small(4)).unwrap();
    w.run(5);
    let before = w.clone();
    assert_eq!(
        w.intervene(set(
            Who::Fraction { permille: 1001 },
            PolicyKind::Learner,
            ParamsMode::Default
        )),
        Err(InterventionError::Invalid("permille must be <= 1000"))
    );
    assert_eq!(
        w.intervene(set(
            Who::Id(AgentId(10_000)),
            PolicyKind::Learner,
            ParamsMode::Default
        )),
        Err(InterventionError::NoSuchAgent(AgentId(10_000)))
    );
    assert_eq!(w, before);
    assert_eq!(w.ledger.interventions, 0);
}

#[test]
fn set_policy_serialises_as_a_tagged_record() {
    let cases = [
        (
            set(Who::All, PolicyKind::Learner, ParamsMode::Default),
            r#"{"op":"set_policy","who":"all","kind":"Learner","params":"default"}"#,
        ),
        (
            set(
                Who::Fraction { permille: 500 },
                PolicyKind::Reflex,
                ParamsMode::Keep,
            ),
            r#"{"op":"set_policy","who":{"fraction":{"permille":500}},"kind":"Reflex","params":"keep"}"#,
        ),
        (
            set(Who::Id(AgentId(7)), PolicyKind::Forager, ParamsMode::Keep),
            r#"{"op":"set_policy","who":{"id":7},"kind":"Forager","params":"keep"}"#,
        ),
    ];
    for (iv, json) in cases {
        assert_eq!(serde_json::to_string(&iv).unwrap(), json);
        assert_eq!(serde_json::from_str::<Intervention>(json).unwrap(), iv);
    }
}

#[test]
fn learner_children_inherit_the_table_or_start_blank_and_reflex_weights_are_inherited() {
    let base = WorldConfig {
        birth_threshold: Some(60_000),
        child_energy: 30_000,
        mutation_enabled: true,
        ..cfg()
    };
    for inherit in [true, false] {
        let c = WorldConfig {
            learner_inherit_q: inherit,
            ..base.clone()
        };
        let mut w = empty_world(c.clone());
        add_agent_with(
            &mut w,
            Pos::new(2, 2),
            90_000,
            0,
            PolicyKind::Learner,
            c.initial_params.clone(),
        );
        if let Some(PolicyTable::Learner { q }) = w.agents[0].params.table.as_mut() {
            q[1] = 9;
        }
        let r = w.apply(intents(&[Action::Rest]));
        assert_eq!(r.births, 1);
        let (parent, child) = (&w.agents[0], &w.agents[1]);
        assert_eq!(child.policy, PolicyKind::Learner);
        let expected = if inherit {
            parent.params.table.clone()
        } else {
            PolicyTable::blank(PolicyKind::Learner, &c)
        };
        assert_eq!(child.params.table, expected, "inherit = {inherit}");
        assert!(matches!(&parent.params.table, Some(PolicyTable::Learner { q }) if q[1] == 9));
        w.check_invariants().unwrap();
    }
    // Reflex weights travel to the child mutated (3-6): every weight within
    // ±mutation_step_permille of the parent's, and not all of them equal.
    let mut w = empty_world(base.clone());
    add_agent_with(
        &mut w,
        Pos::new(2, 2),
        90_000,
        0,
        PolicyKind::Reflex,
        base.initial_params.clone(),
    );
    if let Some(PolicyTable::Reflex { weights }) = w.agents[0].params.table.as_mut() {
        weights[0] = 1024;
        weights[127] = -3;
    }
    w.apply(intents(&[Action::Rest]));
    assert_eq!(w.agents[1].policy, PolicyKind::Reflex);
    let (
        Some(PolicyTable::Reflex { weights: parent }),
        Some(PolicyTable::Reflex { weights: child }),
    ) = (&w.agents[0].params.table, &w.agents[1].params.table)
    else {
        panic!("reflex tables");
    };
    let step = base.mutation_step_permille as i32;
    assert!(parent.iter().zip(child).all(|(p, c)| (p - c).abs() <= step));
    assert_ne!(parent, child);
    assert_eq!(
        (parent[0], parent[127]),
        (1024, -3),
        "the parent is untouched"
    );
    w.check_invariants().unwrap();
}

#[test]
fn snapshots_carry_tables_in_both_encodings_and_reject_what_does_not_fit() {
    let mut w = World::generate(
        WorldConfig {
            seed: 9,
            width: 24,
            height: 24,
            initial_agents: 60,
            patch_count: 2,
            ..WorldConfig::default()
        },
        |i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Reflex,
            _ => PolicyKind::Learner,
        },
    )
    .unwrap();
    w.run(50);
    if let Some(PolicyTable::Learner { q }) = w.agents[2].params.table.as_mut() {
        q[0] = 123;
    }
    w.agents[2].params.hunger_permille = 501; // make the tampering below unambiguous
    let bin = w.to_bytes();
    let json = w.to_snapshot();
    assert_eq!(World::from_bytes(&bin).unwrap(), w);
    assert_eq!(World::from_snapshot(&json).unwrap(), w);
    assert!(std::str::from_utf8(&json)
        .unwrap()
        .starts_with(&format!("{{\"format\":{SNAPSHOT_FORMAT}")));
    println!(
        "mixed-learning snapshot: {} bytes binary, {} bytes json, {} agents",
        bin.len(),
        json.len(),
        w.agents.len()
    );

    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let mut old = v.clone();
    old["format"] = serde_json::json!(6);
    assert_eq!(
        World::from_snapshot(&serde_json::to_vec(&old).unwrap()).err(),
        Some(SnapshotError::Format(6))
    );
    // A policy without its table, a table without its policy, a Q table of
    // the wrong length, a config with no states: the invariants refuse them
    // before the hash gets a say.
    let agents = v["world"]["agents"].as_array().unwrap().clone();
    let learner_ix = agents
        .iter()
        .position(|a| a["policy"] == "Learner")
        .unwrap();
    let forager_ix = agents
        .iter()
        .position(|a| a["policy"] == "Forager")
        .unwrap();
    let invalid = |mutate: &dyn Fn(&mut serde_json::Value)| {
        let mut bad = v.clone();
        mutate(&mut bad);
        matches!(
            World::from_snapshot(&serde_json::to_vec(&bad).unwrap()),
            Err(SnapshotError::Invalid(_))
        )
    };
    assert!(invalid(
        &|b| b["world"]["agents"][learner_ix]["policy"] = "Forager".into()
    ));
    assert!(invalid(
        &|b| b["world"]["agents"][forager_ix]["policy"] = "Learner".into()
    ));
    assert!(invalid(&|b| {
        let q = b["world"]["agents"][learner_ix]["params"]["table"]["learner"]["q"]
            .as_array_mut()
            .unwrap();
        q.pop();
    }));
    assert!(invalid(
        &|b| b["world"]["config"]["learner_states"] = 0.into()
    ));
    // The JSON shape of a table is readable.
    assert!(v["world"]["agents"][learner_ix]["params"]["table"]["learner"]["q"].is_array());
    v["world"]["agents"][forager_ix]["params"]["table"]
        .as_null()
        .expect("fixed policies carry no table");

    // A world built by hand with a mismatch is caught by check_invariants.
    let mut m = empty_world(cfg());
    add_agent(&mut m, Pos::new(1, 1), 50_000, 0);
    m.agents[0].policy = PolicyKind::Reflex;
    let errs = m.check_invariants().unwrap_err();
    assert!(errs.iter().any(|e| e.contains("does not fit")), "{errs:?}");
}

#[test]
fn learner_states_config_sizes_the_table_and_is_validated() {
    let c = WorldConfig {
        learner_states: 128,
        ..small(1)
    };
    let w = World::generate(c.clone(), |_| PolicyKind::Learner).unwrap();
    assert!(matches!(
        &w.agents[0].params.table,
        Some(PolicyTable::Learner { q }) if q.len() == 128 * Action::KIND_COUNT
    ));
    assert!(WorldConfig {
        learner_states: 0,
        ..small(1)
    }
    .validate()
    .is_err());
    assert!(WorldConfig {
        learner_states: WorldConfig::MAX_LEARNER_STATES + 1,
        ..small(1)
    }
    .validate()
    .is_err());
    // A config written before the fields existed still loads with defaults.
    let mut v: serde_json::Value = serde_json::to_value(WorldConfig::default()).unwrap();
    v.as_object_mut().unwrap().remove("learner_states");
    v.as_object_mut().unwrap().remove("learner_inherit_q");
    let c: WorldConfig = serde_json::from_value(v).unwrap();
    assert_eq!((c.learner_states, c.learner_inherit_q), (32, true));
}
