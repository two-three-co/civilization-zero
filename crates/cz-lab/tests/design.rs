//! `Design`: the JSON form, the config precedence, the implied defaults
//! of `cz run`'s rules, and what validation rejects.

use cz_lab::{Base, Cell, Design, Scheduled, Seeds, DESIGN_FORMAT};
use serde_json::{json, Value};
use simulation_core::{Intervention, Norm, NormKind, PolicyKind, Sanction, World, WorldConfig};

fn data(path: &str) -> String {
    let full = format!(
        "{}/../../docs/research/experiments/data/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"))
}

fn minimal(policy: &str, preset: Option<&str>, config: Value) -> Design {
    Design {
        format: DESIGN_FORMAT,
        name: "t".to_string(),
        notes: None,
        base: Base {
            policy: policy.to_string(),
            preset: preset.map(String::from),
            width: Some(32),
            height: Some(32),
            agents: Some(50),
            patches: Some(2),
            config: Value::Null,
        },
        ticks: 10,
        window_pct: 50,
        sample: 5,
        seeds: Seeds { start: 0, count: 1 },
        cells: vec![Cell {
            name: "a".to_string(),
            policy: None,
            preset: None,
            config,
            schedule: vec![],
            seeds: None,
            sample: None,
        }],
        contrasts: vec![],
    }
}

#[test]
fn the_registered_designs_round_trip_through_json() {
    for exp in ["EXP-003", "EXP-004"] {
        let text = data(&format!("{exp}/design.json"));
        let design = Design::from_json(&text).unwrap();
        let again = Design::from_json(&design.to_json()).unwrap();
        assert_eq!(design, again, "{exp}");
        assert_eq!(design.name, exp);
        assert!(!design.contrasts.is_empty());
    }
    let goldens = Design::from_json(include_str!("../goldens.json")).unwrap();
    assert_eq!(goldens.cells.len(), 3);
}

#[test]
fn the_institution_mix_implies_the_institution_preset() {
    let d = minimal("institution", None, Value::Null);
    let cfg = d.world_config(&d.cells[0], 7).unwrap();
    let expected = WorldConfig {
        seed: 7,
        width: 32,
        height: 32,
        initial_agents: 50,
        patch_count: 2,
        ..WorldConfig::institution_preset()
    };
    assert_eq!(cfg, expected);
    // `--preset nomos --policy institution` keeps the rows and the share.
    let d = minimal("institution", Some("nomos"), Value::Null);
    let cfg = d.world_config(&d.cells[0], 7).unwrap();
    assert_eq!(cfg, expected);
    // Only the founders' mix is the institution's; `default` is a preset too.
    let d = minimal("institution", Some("default"), Value::Null);
    let cfg = d.world_config(&d.cells[0], 7).unwrap();
    assert!(!cfg.nomos);
    assert_eq!(cfg.learner_states, 128);
    assert_eq!(cfg.enforcer_share_permille, Some(100));
    // A learner on the nomos preset has neither.
    let d = minimal("learner", Some("nomos"), Value::Null);
    let cfg = d.world_config(&d.cells[0], 7).unwrap();
    assert_eq!(
        cfg,
        WorldConfig {
            seed: 7,
            width: 32,
            height: 32,
            initial_agents: 50,
            patch_count: 2,
            ..WorldConfig::nomos_preset()
        }
    );
}

#[test]
fn evolving_mixes_mutate_unless_the_config_says_otherwise() {
    for policy in ["reflex", "mixed-learning"] {
        let d = minimal(policy, None, Value::Null);
        assert!(d.world_config(&d.cells[0], 0).unwrap().mutation_enabled);
        let d = minimal(policy, None, json!({ "mutation_enabled": false }));
        assert!(!d.world_config(&d.cells[0], 0).unwrap().mutation_enabled);
    }
    let d = minimal("forager", None, Value::Null);
    assert!(!d.world_config(&d.cells[0], 0).unwrap().mutation_enabled);
    let d = minimal("forager", None, json!({ "mutation_enabled": true }));
    assert!(d.world_config(&d.cells[0], 0).unwrap().mutation_enabled);
}

#[test]
fn config_overrides_merge_field_by_field_and_the_cell_wins() {
    let mut d = minimal(
        "institution",
        None,
        json!({ "perception": { "see_standing": false }, "harm_cost": 8000, "enforcer_share_permille": null }),
    );
    d.base.config =
        json!({ "harm_cost": 4000, "memory_slots": 0, "charter": { "amendable": false } });
    let cfg = d.world_config(&d.cells[0], 1).unwrap();
    assert_eq!(
        cfg.harm_cost, 8000,
        "the cell's override wins over the base's"
    );
    assert_eq!(cfg.memory_slots, 0);
    assert!(!cfg.charter.amendable);
    assert_eq!(
        cfg.charter.quorum_permille, 100,
        "the rest of the charter stays"
    );
    assert!(!cfg.perception.see_standing);
    assert_eq!(
        cfg.perception.signal_radius, 2,
        "the rest of the perception stays"
    );
    assert_eq!(cfg.enforcer_share_permille, None, "null clears an Option");
    assert_eq!(cfg.learner_states, 128);
    assert_eq!(cfg.seed, 1);
}

#[test]
fn unknown_fields_seeds_and_bad_values_are_rejected() {
    let d = minimal("forager", None, json!({ "harm_cots": 1 }));
    let err = d.validate().unwrap_err();
    assert!(
        err.0.contains("harm_cots") && err.0.contains("not a WorldConfig field"),
        "{err}"
    );
    let d = minimal("forager", None, json!({ "seed": 3 }));
    assert!(d.validate().unwrap_err().0.contains("seed"));
    let d = minimal(
        "forager",
        None,
        json!({ "perception": { "signal_radius": 99 } }),
    );
    assert!(d.validate().unwrap_err().0.contains("radii"));
    let d = minimal("forager", None, json!({ "learner_states": -1 }));
    assert!(d.validate().unwrap_err().0.contains("does not parse"));
    let d = minimal("forager", None, json!(5));
    assert!(d.validate().unwrap_err().0.contains("JSON object"));
    let d = minimal("forager", Some("nomo"), Value::Null);
    assert!(d.validate().unwrap_err().0.contains("unknown preset"));
    let d = minimal("foragers", None, Value::Null);
    assert!(d.validate().unwrap_err().0.contains("unknown policy mix"));
}

#[test]
fn the_structure_is_validated() {
    let ok = minimal("forager", None, Value::Null);
    ok.validate().unwrap();
    let mut d = ok.clone();
    d.format = 2;
    assert!(d.validate().unwrap_err().0.contains("format"));
    let mut d = ok.clone();
    d.cells.push(d.cells[0].clone());
    assert!(d.validate().unwrap_err().0.contains("duplicate"));
    let mut d = ok.clone();
    d.cells[0].name = "a/b".to_string();
    assert!(d.validate().unwrap_err().0.contains("[A-Za-z0-9_-]"));
    let mut d = ok.clone();
    d.cells[0].schedule.push(Scheduled {
        tick: 10,
        intervention: Intervention::ScaleFood { permille: 500 },
    });
    assert!(d
        .validate()
        .unwrap_err()
        .0
        .contains("not before the run's end"));
    let mut d = ok.clone();
    d.cells[0].sample = Some(0);
    assert!(d.validate().unwrap_err().0.contains("sample"));
    let mut d = ok.clone();
    d.window_pct = 101;
    assert!(d.validate().unwrap_err().0.contains("window_pct"));
    let mut d = ok.clone();
    d.contrasts.push(cz_lab::Contrast {
        name: "x".to_string(),
        treat: "a".to_string(),
        control: "b".to_string(),
        metric: cz_lab::Metric::Summary("win_mean".to_string()),
        window: None,
    });
    assert!(d.validate().unwrap_err().0.contains("is not a cell"));
    d.contrasts[0].control = "a".to_string();
    assert!(d.validate().unwrap_err().0.contains("same cell"));
    // Unknown top-level keys are rejected too.
    let mut v: Value = serde_json::from_str(&ok.to_json()).unwrap();
    v["tick"] = json!(5);
    assert!(Design::from_json(&v.to_string()).is_err());
}

#[test]
fn jobs_follow_the_selection() {
    let mut d = minimal("forager", None, Value::Null);
    d.seeds = Seeds { start: 3, count: 4 };
    d.cells.push(Cell {
        name: "b".to_string(),
        seeds: Some(Seeds { start: 0, count: 2 }),
        ..d.cells[0].clone()
    });
    assert_eq!(
        d.jobs(None, None).unwrap(),
        vec![(0, 3), (0, 4), (0, 5), (0, 6), (1, 0), (1, 1)]
    );
    assert_eq!(
        d.jobs(Some(&["b".to_string()]), None).unwrap(),
        vec![(1, 0), (1, 1)]
    );
    assert_eq!(d.jobs(None, Some((4, 6))).unwrap(), vec![(0, 4), (0, 5)]);
    assert!(d.jobs(Some(&["c".to_string()]), None).is_err());
    assert_eq!(d.window(), (5, 10));
    assert_eq!(d.seeds_of(&d.cells[1]).end(), 2);
}

#[test]
fn schedules_serialise_as_the_kernels_interventions() {
    let mut d = minimal("institution", None, Value::Null);
    d.cells[0].schedule.push(Scheduled {
        tick: 2,
        intervention: Intervention::Enact {
            norm: Norm {
                kind: NormKind::Encroach,
                standing_ticks: 32,
                sanction: Sanction::HARM,
                except_sanction: false,
            },
        },
    });
    let text = d.to_json();
    assert!(text.contains(r#""op": "enact""#), "{text}");
    assert!(text.contains(r#""sanction": 1"#), "{text}");
    assert_eq!(Design::from_json(&text).unwrap(), d);
}

#[test]
fn build_world_is_the_kernels_generate() {
    let d = minimal("learner", None, Value::Null);
    let a = d.build_world(&d.cells[0], 5).unwrap();
    let b = World::generate(
        WorldConfig {
            seed: 5,
            width: 32,
            height: 32,
            initial_agents: 50,
            patch_count: 2,
            ..WorldConfig::default()
        },
        |_| PolicyKind::Learner,
    )
    .unwrap();
    assert_eq!(a, b);
}
