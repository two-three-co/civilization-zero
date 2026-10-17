//! `WorldConfig::nomos_preset()` (ADR-0014 D10, task 4-4): the institution
//! world's baseline, whose three values EXP-003's pre-registered decision
//! rule fixed. The default config — EXP-001 / EXP-002's world and both
//! golden hashes — is not touched by it.

use simulation_core::{PolicyKind, World, WorldConfig};

/// The grid EXP-003 searched (docs/research/experiments/EXP-003-baseline-grid.md).
const EPSILONS: [u16; 3] = [0, 10, 50];
const HARM_COSTS: [i64; 2] = [2_000, 8_000];
const HEALTH_WEIGHTS: [u16; 2] = [0, 2];

fn small(cfg: WorldConfig, seed: u64) -> WorldConfig {
    WorldConfig {
        seed,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..cfg
    }
}

#[test]
fn the_preset_validates_and_differs_from_the_default_only_in_the_registered_fields() {
    let p = WorldConfig::nomos_preset();
    p.validate().unwrap();
    assert!(p.nomos, "the institution world tracks social facts");
    assert!(EPSILONS.contains(&p.learner_epsilon_permille));
    assert!(HARM_COSTS.contains(&p.harm_cost));
    assert!(HEALTH_WEIGHTS.contains(&p.learner_health_weight));
    // Nothing outside EXP-003's three axes and the `nomos` switch moves:
    // 32 rows stay the default (D6: 128 rows are chosen per world).
    let d = WorldConfig::default();
    let back = WorldConfig {
        nomos: d.nomos,
        learner_epsilon_permille: d.learner_epsilon_permille,
        harm_cost: d.harm_cost,
        learner_health_weight: d.learner_health_weight,
        ..p.clone()
    };
    assert_eq!(back, d);
    assert_eq!(p.learner_states, 32);
}

/// The values EXP-003's decision rule fixed (its results section, 2026-10-17):
/// change them only with a new pre-registered run, and update the ADR.
#[test]
fn the_preset_is_the_cell_exp_003_chose() {
    let p = WorldConfig::nomos_preset();
    assert_eq!(p.learner_epsilon_permille, 10);
    assert_eq!(p.harm_cost, 2_000);
    assert_eq!(p.learner_health_weight, 0);
    assert!(p.nomos);
}

#[test]
fn the_default_config_is_the_exp_002_world() {
    let d = WorldConfig::default();
    assert!(!d.nomos);
    assert_eq!(d.learner_epsilon_permille, 50);
    assert_eq!(d.harm_cost, 2_000);
    assert_eq!(d.learner_health_weight, 0);
    assert_eq!(d.learner_states, 32);
    assert_ne!(WorldConfig::nomos_preset(), d);
}

#[test]
fn learner_worlds_on_the_preset_run_deterministically_with_invariants() {
    for rows in [32u16, 128] {
        let cfg = WorldConfig {
            learner_states: rows,
            ..small(WorldConfig::nomos_preset(), 7)
        };
        let mut a = World::generate(cfg.clone(), |_| PolicyKind::Learner).unwrap();
        let mut b = World::generate(cfg, |_| PolicyKind::Learner).unwrap();
        for _ in 0..300 {
            let ra = a.step();
            #[cfg(feature = "parallel")]
            let rb = b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            let rb = b.step();
            assert_eq!(ra, rb);
            if let Err(e) = a.check_invariants() {
                panic!("{rows} rows, tick {}: {e:?}", a.tick);
            }
        }
        assert_eq!(a.state_hash(), b.state_hash());
        assert!(
            !a.agents.is_empty(),
            "{rows} rows: everyone died in 300 ticks"
        );
        assert!(
            a.agents.iter().any(|x| x.arrived.is_some()),
            "{rows} rows: tracking is on, so whoever moved has an arrival tick"
        );
    }
}
