//! Reproducibility, snapshot/restore and long-run invariant tests.

use proptest::prelude::*;
use simulation_core::{AgentId, DeathCause, PolicyKind, World, WorldConfig};
use std::collections::BTreeMap;

fn small_cfg(seed: u64) -> WorldConfig {
    WorldConfig {
        seed,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..WorldConfig::default()
    }
}

fn mixed(cfg: WorldConfig) -> World {
    World::generate(cfg, |i| {
        if i % 3 == 0 {
            PolicyKind::Random
        } else {
            PolicyKind::Forager
        }
    })
    .unwrap()
}

#[test]
fn same_seed_same_history() {
    let mut a = mixed(small_cfg(9));
    let mut b = mixed(small_cfg(9));
    for _ in 0..300 {
        let ra = a.step();
        let rb = b.step();
        assert_eq!(ra, rb);
        assert_eq!(a.state_hash(), b.state_hash());
    }
    assert_eq!(a, b);
}

#[test]
fn different_seed_different_history() {
    let mut a = mixed(small_cfg(1));
    let mut b = mixed(small_cfg(2));
    a.run(50);
    b.run(50);
    assert_ne!(a.state_hash(), b.state_hash());
}

/// Golden fingerprint. If this changes, the transition function changed:
/// record why in docs/STATUS.md (and an ADR if it was a design change).
#[test]
fn golden_hash_is_stable() {
    let mut w = World::new(WorldConfig {
        seed: 42,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(200);
    let h = w.state_hash();
    println!("golden hash after 200 ticks: {h:#018x}");
    assert_eq!(h, GOLDEN_200);
}
const GOLDEN_200: u64 = 0x6e76_7bd1_6052_5c8e;

#[test]
fn snapshot_roundtrip_then_continue_is_identical() {
    let mut original = mixed(small_cfg(5));
    original.run(120);
    let bytes = original.to_snapshot();
    let mut restored = World::from_snapshot(&bytes).unwrap();
    assert_eq!(original, restored);
    original.run(200);
    restored.run(200);
    assert_eq!(original.state_hash(), restored.state_hash());
}

#[test]
fn corrupted_snapshot_is_rejected() {
    let mut w = mixed(small_cfg(5));
    w.run(10);
    let mut json: serde_json::Value = serde_json::from_slice(&w.to_snapshot()).unwrap();
    // Give an agent free energy: conservation check must catch it.
    json["world"]["agents"][0]["energy"] = serde_json::json!(1);
    let bytes = serde_json::to_vec(&json).unwrap();
    assert!(World::from_snapshot(&bytes).is_err());
}

#[test]
fn long_run_keeps_invariants() {
    let mut w = mixed(small_cfg(77));
    for _ in 0..5_000 {
        w.step();
        if let Err(e) = w.check_invariants() {
            panic!("tick {}: {e:?}", w.tick);
        }
    }
}

/// Conservation across generations: births move energy, deaths write it off,
/// and the ledger must account for every unit every tick.
#[test]
fn generations_conserve_energy_and_food() {
    let mut w = World::new(WorldConfig {
        seed: 11,
        width: 64,
        height: 64,
        initial_agents: 300,
        patch_count: 8,
        ..WorldConfig::default()
    })
    .unwrap();
    let mut seen_ids = std::collections::BTreeSet::new();
    for _ in 0..4_000 {
        let r = w.step();
        if let Err(e) = w.check_invariants() {
            panic!("tick {}: {e:?}", w.tick);
        }
        for e in &r.events {
            if let simulation_core::Event::Born { child, .. } = e {
                assert!(seen_ids.insert(*child), "id {child:?} handed out twice");
            }
        }
    }
    let l = &w.ledger;
    assert!(l.births > 0, "no births in 4000 ticks");
    assert!(l.deaths_by_cause[DeathCause::Starvation.index()] > 0);
    assert!(l.deaths_by_cause[DeathCause::OldAge.index()] > 0);
    assert!(!w.agents.is_empty(), "population went extinct");
    assert_eq!(l.births, seen_ids.len() as u64);
    assert_eq!(w.next_agent_id, 300 + l.births);
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_decide_matches_serial() {
    let mut s = World::new(WorldConfig {
        seed: 3,
        ..WorldConfig::default()
    })
    .unwrap();
    let mut p = s.clone();
    for _ in 0..200 {
        assert_eq!(s.step(), p.step_parallel());
    }
    assert_eq!(s.state_hash(), p.state_hash());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, .. ProptestConfig::default() })]

    /// Across arbitrary seeds and parameter choices (life cycle on or off,
    /// mutation on or off): invariants hold every tick, a surviving agent's
    /// energy never rises without consumption, and replays agree.
    #[test]
    fn invariants_hold_for_random_configs(
        seed in any::<u64>(),
        side in 4u16..40,
        agents in 0u32..120,
        patches in 0u32..10,
        regrow in 1u64..20,
        gather in 1u32..6,
        metabolism in 0i64..3_000,
        starve in 0i64..50,
        birth_div in 0i64..4,
        birth_permille in 200i64..=1000,
        max_age in prop_oneof![Just(None), (1u64..400).prop_map(Some)],
        mutation_enabled in any::<bool>(),
    ) {
        let max_energy = WorldConfig::default().max_energy;
        let threshold = max_energy * birth_permille / 1000;
        let cfg = WorldConfig {
            seed, width: side, height: side, initial_agents: agents,
            patch_count: patches, regrow_interval: regrow, gather_amount: gather,
            metabolism_cost: metabolism, starvation_damage: starve,
            birth_threshold: (birth_div > 0).then_some(threshold),
            child_energy: threshold / (birth_div + 1),
            max_age, mutation_enabled,
            ..WorldConfig::default()
        };
        let mut w = mixed(cfg.clone());
        let mut twin = mixed(cfg);
        for _ in 0..150 {
            let before: BTreeMap<AgentId, i64> =
                w.agents.iter().map(|a| (a.id, a.energy)).collect();
            let r = w.step();
            twin.step();
            prop_assert!(w.check_invariants().is_ok(), "{:?}", w.check_invariants());
            prop_assert_eq!(r.births as usize, r.events.iter().filter(|e| matches!(e, simulation_core::Event::Born { .. })).count());
            if r.food_consumed == 0 {
                for a in &w.agents {
                    if let Some(&prev) = before.get(&a.id) {
                        prop_assert!(a.energy <= prev, "energy rose without consumption");
                    }
                }
            }
        }
        prop_assert_eq!(w.state_hash(), twin.state_hash());
    }
}
