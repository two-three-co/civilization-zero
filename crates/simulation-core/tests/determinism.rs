//! Reproducibility, snapshot/restore and long-run invariant tests.

use proptest::prelude::*;
use simulation_core::{PolicyKind, World, WorldConfig};

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
/// bump the kernel version and record why in docs/STATUS.md.
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
const GOLDEN_200: u64 = 0xaac5_7b73_180a_3d65;

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

    /// Across arbitrary seeds and parameter choices: invariants hold every
    /// tick, energy never rises without consumption, and replays agree.
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
    ) {
        let cfg = WorldConfig {
            seed, width: side, height: side, initial_agents: agents,
            patch_count: patches, regrow_interval: regrow, gather_amount: gather,
            metabolism_cost: metabolism, starvation_damage: starve,
            ..WorldConfig::default()
        };
        let mut w = mixed(cfg.clone());
        let mut twin = mixed(cfg);
        for _ in 0..150 {
            let before: Vec<_> = w.agents.iter().map(|a| (a.id, a.energy)).collect();
            let r = w.step();
            twin.step();
            prop_assert!(w.check_invariants().is_ok(), "{:?}", w.check_invariants());
            if r.food_consumed == 0 {
                for a in &w.agents {
                    let prev = before.iter().find(|(id, _)| *id == a.id).unwrap().1;
                    prop_assert!(a.energy <= prev, "energy rose without consumption");
                }
            }
        }
        prop_assert_eq!(w.state_hash(), twin.state_hash());
    }
}
