//! The reflex policy (ADR-0013 3-6): forager-equivalent initial weights,
//! features the weights can act on, a tie lottery that is uniform and
//! deterministic, and weight mutation at birth.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::policy::{
    self, argmax_kind, feature, forager_weights, nearest_adjacent, reflex_features, reflex_scores,
    scan_best, REFLEX_UNIT,
};
use simulation_core::rng::{RngStream, Stream};
use simulation_core::{
    Action, Agent, AgentId, Dir, Event, Intent, Interaction, InteractionKind, Message, PolicyKind,
    PolicyTable, Pos, World, WorldConfig,
};

/// The decision `w.agents[i]` would make at `tick` if it were of `kind`
/// (with the fresh table of that kind), everything else being equal.
fn decide_as(w: &World, i: usize, kind: PolicyKind, tick: u64) -> Intent {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let mut a = w.agents[i].clone();
    a.policy = kind;
    a.params = std::mem::take(&mut a.params).refit(kind, &w.config);
    let obs = w.observation(&a, &index, &norms);
    let mut rng = RngStream::new(w.config.seed, tick, a.id.0, Stream::Decide);
    policy::decide(&obs, &mut rng)
}

fn decide(w: &World, i: usize, tick: u64) -> Intent {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation(&w.agents[i], &index, &norms);
    let mut rng = RngStream::new(w.config.seed, tick, w.agents[i].id.0, Stream::Decide);
    policy::decide(&obs, &mut rng)
}

fn reflex_agent(w: &mut World, pos: Pos, energy: i64, inventory: u32) -> usize {
    let params = w.config.initial_params.clone();
    add_agent_with(w, pos, energy, inventory, PolicyKind::Reflex, params)
}

fn set_weights(w: &mut World, i: usize, weights: Vec<i32>) {
    w.agents[i].params.table = Some(PolicyTable::Reflex { weights });
}

/// Forager weights with one kind's bias raised far above every ladder step.
fn preferring(kind: usize) -> Vec<i32> {
    let mut weights = forager_weights();
    weights[feature::BIAS * Action::KIND_COUNT + kind] = 8 * REFLEX_UNIT;
    weights
}

fn far_i(w: &World, id: AgentId) -> usize {
    w.agents.iter().position(|a| a.id == id).unwrap()
}

fn body(a: &Agent) -> (AgentId, Pos, i64, i64, u32, u64) {
    (a.id, a.pos, a.energy, a.health, a.inventory, a.age)
}

fn bodies(w: &World) -> Vec<(AgentId, Pos, i64, i64, u32, u64)> {
    w.agents.iter().map(body).collect()
}

fn small(seed: u64) -> WorldConfig {
    WorldConfig {
        seed,
        width: 64,
        height: 64,
        initial_agents: 300,
        patch_count: 8,
        ..WorldConfig::default()
    }
}

#[test]
fn forager_weights_reproduce_every_forager_decision() {
    let mut w = World::new(small(7)).unwrap();
    let mut seen = [0u32; Action::KIND_COUNT];
    for tick in 0..600u64 {
        if tick < 3 || tick % 50 == 0 {
            for i in 0..w.agents.len() {
                let f = decide_as(&w, i, PolicyKind::Forager, w.tick);
                let r = decide_as(&w, i, PolicyKind::Reflex, w.tick);
                assert_eq!(f, r, "tick {} agent {:?}", w.tick, w.agents[i].id);
                seen[f.action.kind_index()] += 1;
            }
        }
        w.step();
    }
    assert!(
        seen[0] > 0 && seen[1] > 0 && seen[2] > 0,
        "move, gather and consume all occurred: {seen:?}"
    );
}

#[test]
fn a_reflex_world_matches_the_forager_world_for_1000_ticks() {
    let cfg = small(42);
    let mut forager = World::new(cfg.clone()).unwrap();
    let mut reflex = World::generate(cfg, |_| PolicyKind::Reflex).unwrap();
    for t in 1..=1_000u64 {
        forager.step();
        reflex.step();
        if t % 100 == 0 {
            assert_eq!(bodies(&forager), bodies(&reflex), "tick {t}");
            assert_eq!(forager.resources.food, reflex.resources.food, "tick {t}");
            assert_eq!(forager.ledger, reflex.ledger, "tick {t}");
        }
    }
    assert!(
        reflex.ledger.births > 0 && reflex.ledger.deaths > 0,
        "generations passed"
    );
    assert!(forager
        .agents
        .iter()
        .zip(&reflex.agents)
        .all(|(f, r)| f.memory == r.memory));
    // The kind and its table are state, so the hashes differ by design.
    assert_ne!(forager.dynamics_hash(), reflex.dynamics_hash());
    reflex.check_invariants().unwrap();
}

#[test]
fn features_reflect_body_sight_crowd_and_memory() {
    use feature::*;
    let mut w = empty_world(cfg());
    w.tick = 10;
    let me = reflex_agent(&mut w, Pos::new(4, 4), 30_000, 2); // hungry: 30% < 50%
    reflex_agent(&mut w, Pos::new(5, 4), 50_000, 0); // adjacent
    reflex_agent(&mut w, Pos::new(4, 6), 50_000, 0); // distance 2: a neighbour
    reflex_agent(&mut w, Pos::new(7, 7), 50_000, 0); // out of the social radius
    set_food(&mut w, Pos::new(4, 2), 7);
    set_food(&mut w, Pos::new(6, 4), 3);
    let note = |tick, kind, value| Interaction {
        tick,
        other: AgentId(1),
        kind,
        value,
    };
    w.agents[me].recent = vec![
        note(7, InteractionKind::HarmedBy, 20),
        note(8, InteractionKind::Heard, 6), // stale: not last tick
        note(9, InteractionKind::Heard, 3),
    ];
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation(&w.agents[me], &index, &norms);
    let x = reflex_features(&obs, Some(&scan_best(&obs)));
    let mut expected = [0i64; policy::REFLEX_FEATURES];
    expected[BIAS] = 1;
    expected[HUNGRY] = 1;
    expected[HAS_FOOD] = 1;
    expected[PACK_FULL] = 0;
    expected[ROOM] = 1;
    expected[FOOD_HERE] = 0;
    expected[FOOD_IN_SIGHT] = 7;
    expected[HAS_TARGET] = 0;
    expected[NEIGHBOURS] = 2;
    expected[ADJACENT] = 1;
    expected[HEARD] = 4;
    expected[HARMED_RECENTLY] = 1;
    expected[RECEIVED_RECENTLY] = 0;
    expected[SAW_HARM_RECENTLY] = 0;
    expected[PROMISED_RECENTLY] = 0;
    expected[ENERGY_LEVEL] = 2;
    assert_eq!(x, expected);

    // Memory flags expire after MEMORY_HORIZON ticks; the energy level and
    // the pack state move with the body.
    w.tick = 7 + MEMORY_HORIZON + 1;
    w.agents[me].energy = 100_000;
    w.agents[me].inventory = w.config.inventory_capacity;
    w.agents[me].memory.target = Some(Pos::new(4, 2));
    let obs = w.observation(&w.agents[me], &index, &norms);
    let x = reflex_features(&obs, None); // a full pack: no scan
    assert_eq!(x[HARMED_RECENTLY], 0);
    assert_eq!(x[FOOD_IN_SIGHT], 0);
    assert_eq!(x[HEARD], 0);
    assert_eq!(
        (
            x[HUNGRY],
            x[PACK_FULL],
            x[ROOM],
            x[HAS_TARGET],
            x[ENERGY_LEVEL]
        ),
        (0, 1, 0, 1, 8)
    );
}

#[test]
fn the_decision_rules_of_the_forager_hold_with_the_initial_weights() {
    let mut w = empty_world(cfg());
    let a = reflex_agent(&mut w, Pos::new(4, 4), 10_000, 3);
    assert_eq!(
        decide(&w, a, 0).action,
        Action::Consume(1),
        "hungry with food in the pack"
    );

    w.agents[a].energy = 90_000;
    set_food(&mut w, Pos::new(4, 4), 5);
    let it = decide(&w, a, 0);
    assert_eq!(it.action, Action::Gather, "not hungry, food underfoot");
    assert_eq!(
        it.memory.target,
        Some(Pos::new(4, 4)),
        "gathering marks the cell"
    );

    w.agents[a].inventory = w.config.inventory_capacity;
    assert_eq!(
        decide(&w, a, 0).action,
        Action::Consume(1),
        "full pack with room: top up"
    );

    w.agents[a].energy = w.config.max_energy;
    let it = decide(&w, a, 0);
    assert!(
        matches!(it.action, Action::Move(_)),
        "full pack, no room: wander"
    );
    assert_eq!(it.memory.target, None, "and does not look for food");

    // With room in the pack and nothing underfoot the move is the forager's
    // move: towards the richest visible cell, target remembered.
    w.agents[a].inventory = 0;
    w.agents[a].energy = 90_000;
    set_food(&mut w, Pos::new(4, 4), 0);
    set_food(&mut w, Pos::new(4, 1), 9);
    set_food(&mut w, Pos::new(5, 4), 3);
    let it = decide(&w, a, 0);
    assert_eq!(it.action, Action::Move(Dir::North));
    assert_eq!(it.memory.target, Some(Pos::new(4, 1)));
    assert_eq!(it, decide_as(&w, a, PolicyKind::Forager, 0));
}

#[test]
fn weights_change_the_decision_and_actuators_fill_in_the_targets() {
    let mut w = empty_world(cfg());
    w.tick = 5;
    let a = reflex_agent(&mut w, Pos::new(4, 4), 10_000, 3); // would eat
    set_weights(&mut w, a, preferring(3));
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Rest,
        "a bias on Rest overrides hunger"
    );

    set_weights(&mut w, a, preferring(4));
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Rest,
        "nobody adjacent to give to"
    );
    set_weights(&mut w, a, preferring(6));
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Rest,
        "nobody adjacent to harm"
    );
    set_weights(&mut w, a, preferring(5));
    assert_eq!(decide(&w, a, 5).action, Action::Store(1));

    let other = reflex_agent(&mut w, Pos::new(5, 4), 50_000, 0);
    let far = reflex_agent(&mut w, Pos::new(4, 6), 50_000, 0);
    let (other, far) = (w.agents[other].id, w.agents[far].id);
    set_weights(&mut w, a, preferring(4));
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Transfer { to: other, n: 1 },
        "the nearest adjacent agent, not the one two cells away ({far:?})"
    );
    set_weights(&mut w, a, preferring(6));
    assert_eq!(decide(&w, a, 5).action, Action::Harm { target: other });

    // The target is the first neighbour within distance 1 in (distance, id)
    // order, exactly as `Observation::neighbors()` lists them.
    let index = w.spatial_index();
    let norms = w.norm_index();
    for i in 0..w.agents.len() {
        let obs = w.observation(&w.agents[i], &index, &norms);
        let listed = obs
            .neighbors()
            .into_iter()
            .find(|n| n.distance <= 1)
            .map(|n| n.id);
        assert_eq!(nearest_adjacent(&obs), listed, "agent {i}");
    }
    let same_cell = reflex_agent(&mut w, Pos::new(4, 4), 50_000, 0);
    let index = w.spatial_index();
    let obs = w.observation(&w.agents[a], &index, &norms);
    assert_eq!(
        nearest_adjacent(&obs),
        Some(w.agents[same_cell].id),
        "same cell first"
    );
    let obs = w.observation(&w.agents[same_cell], &index, &norms);
    assert_eq!(
        nearest_adjacent(&obs),
        Some(w.agents[a].id),
        "and never oneself"
    );
    w.config.perception.social_radius = 0;
    let obs = w.observation(&w.agents[far_i(&w, far)], &index, &norms);
    assert_eq!(
        nearest_adjacent(&obs),
        None,
        "radius 0 reaches no adjacent cell"
    );

    set_weights(&mut w, a, preferring(7));
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Signal(Message::Say(0)),
        "nothing heard: symbol 0"
    );
    w.agents[a].recent = vec![Interaction {
        tick: 4,
        other,
        kind: InteractionKind::Heard,
        value: 5,
    }];
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Signal(Message::Say(5)),
        "echoes what it heard last tick"
    );
    w.check_invariants().unwrap();
}

#[test]
fn ties_are_settled_by_a_uniform_deterministic_lottery() {
    // Zero weights tie all eight kinds. With an adjacent agent every kind
    // resolves to its own action, so the action kind is the chosen kind.
    let mut w = empty_world(cfg());
    let a = reflex_agent(&mut w, Pos::new(4, 4), 50_000, 3);
    reflex_agent(&mut w, Pos::new(5, 4), 50_000, 0);
    set_weights(&mut w, a, vec![0; policy::PolicyTable::REFLEX_LEN]);
    let mut counts = [0u32; Action::KIND_COUNT];
    for tick in 0..8_000u64 {
        let it = decide(&w, a, tick);
        assert_eq!(it, decide(&w, a, tick), "same tick, same choice");
        counts[it.action.kind_index()] += 1;
    }
    // n = 8000, p = 1/8: mean 1000, sd 29.6, 5 sd = 148.
    for (k, n) in counts.iter().enumerate() {
        assert!(
            (852..=1148).contains(n),
            "kind {k} chosen {n} times: {counts:?}"
        );
    }
    let scores = reflex_scores(
        &vec![0; policy::PolicyTable::REFLEX_LEN],
        &[1; policy::REFLEX_FEATURES],
    );
    assert_eq!(scores, [0; Action::KIND_COUNT]);
    let mut rng = RngStream::new(1, 1, 1, Stream::Decide);
    assert_eq!(argmax_kind(&[0, 0, 0, 0, 0, 0, 0, 9], &mut rng), 7);
}

#[test]
fn children_inherit_mutated_weights_and_the_world_stays_deterministic() {
    let cfg = WorldConfig {
        mutation_enabled: true,
        ..WorldConfig {
            seed: 3,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            ..WorldConfig::default()
        }
    };
    let step = cfg.mutation_step_permille as i32;
    let mut a = World::generate(cfg, |_| PolicyKind::Reflex).unwrap();
    let mut b = a.clone();
    let mut checked_births = 0;
    for _ in 0..400 {
        let r = a.step();
        #[cfg(feature = "parallel")]
        b.step_parallel();
        #[cfg(not(feature = "parallel"))]
        b.step();
        for e in &r.events {
            if let Event::Born { child, parent, .. } = e {
                let at = |id: AgentId| {
                    let i = a.agents.binary_search_by_key(&id, |x| x.id).unwrap();
                    match &a.agents[i].params.table {
                        Some(PolicyTable::Reflex { weights }) => weights.clone(),
                        other => panic!("{other:?}"),
                    }
                };
                let (p, c) = (at(*parent), at(*child));
                assert!(p.iter().zip(&c).all(|(p, c)| (p - c).abs() <= step));
                assert_ne!(p, c, "weights mutate at birth");
                checked_births += 1;
            }
        }
    }
    assert!(checked_births > 0);
    assert_eq!(
        a.state_hash(),
        b.state_hash(),
        "serial = parallel with mutation on"
    );
    a.check_invariants().unwrap();
    assert!(a
        .agents
        .iter()
        .any(|x| matches!(&x.params.table, Some(PolicyTable::Reflex { weights }) if *weights != forager_weights())));
    assert_eq!(World::from_bytes(&a.to_bytes()).unwrap(), a);
    assert_eq!(World::from_snapshot(&a.to_snapshot()).unwrap(), a);
    println!(
        "reflex world with mutation: {} bytes binary for {} agents at tick {}",
        a.to_bytes().len(),
        a.agents.len(),
        a.tick
    );
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// Evolving reflex worlds are as deterministic as fixed ones: serial and
    /// parallel decisions agree, and the invariants hold whatever the
    /// mutation step.
    #[test]
    fn evolving_reflex_worlds_are_deterministic(
        seed in any::<u64>(),
        step in 0u16..=300,
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
            mutation_enabled: true,
            mutation_step_permille: step,
            ..WorldConfig::default()
        };
        let mut a = World::generate(cfg.clone(), |_| PolicyKind::Reflex).unwrap();
        let mut b = World::generate(cfg, |_| PolicyKind::Reflex).unwrap();
        for _ in 0..ticks {
            a.step();
            #[cfg(feature = "parallel")]
            b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            b.step();
        }
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
    }
}
