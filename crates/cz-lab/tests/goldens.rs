//! The three golden worlds built from `goldens.json` (ADR-0015 completion
//! condition 4, D10): `run_cell` must reach the kernel's pinned
//! `dynamics_hash` after 200 ticks. The same file is the browser's check
//! in `cz-lab-web` (5-8), so the design — not a test's code — is what
//! both paths share.

use cz_lab::{run_cell, Design, Rows, RunOptions, SERIES_COLUMNS, SUMMARY_COLUMNS};

const GOLDENS: &str = include_str!("../goldens.json");

/// `simulation-core/tests/determinism.rs::GOLDEN_200`.
const FORAGER: u64 = 0x658e_e873_d186_6501;
/// `cz-web/tests/native.rs::LEARNER_GOLDEN_200_HEX`.
const LEARNER: u64 = 0xcb8b_c0a9_772c_09a4;
/// `simulation-core/tests/determinism.rs::INSTITUTION_GOLDEN_200`.
const INSTITUTION: u64 = 0xfedc_2815_0318_0c8e;

#[test]
fn the_three_goldens_are_reached_from_the_design() {
    let design = Design::from_json(GOLDENS).unwrap();
    assert_eq!(design.ticks, 200);
    assert_eq!((design.seeds.start, design.seeds.count), (42, 1));
    for (name, expected) in [
        ("forager", FORAGER),
        ("learner", LEARNER),
        ("institution", INSTITUTION),
    ] {
        let cell = design.cell(name).unwrap();
        let mut rows = Rows::default();
        let run = run_cell(&design, cell, 42, &RunOptions::default(), &mut rows).unwrap();
        assert_eq!(
            run.dynamics_hash, expected,
            "{name}: {:#018x} != {expected:#018x}",
            run.dynamics_hash
        );
        assert_eq!(run.ticks, 200);
        assert_eq!(run.seed, 42);
        assert_eq!(rows.summary.len(), 1);
        // Rows at ticks 0, 100 and the final one at 200.
        assert_eq!(rows.series.len(), 3);
        assert_eq!(
            rows.series[0].split(',').count(),
            SERIES_COLUMNS.split(',').count()
        );
        assert_eq!(
            rows.summary[0].split(',').count(),
            SUMMARY_COLUMNS.split(',').count()
        );
        assert_eq!(rows.qpolicy.is_empty(), name == "forager");
        assert!(rows.summary[0].starts_with(&format!("42,{},", design.policy_of(cell))));
    }
}

#[test]
fn the_institution_golden_replaces_the_assemblys_rule() {
    let design = Design::from_json(GOLDENS).unwrap();
    let cell = design.cell("institution").unwrap();
    let mut w = design.build_world(cell, 42).unwrap();
    assert_eq!(w.config.learner_states, 128);
    assert_eq!(w.config.enforcer_share_permille, Some(100));
    assert!(w.config.nomos && w.config.enforcer_motions);
    for tick in 0..200 {
        for s in &cell.schedule {
            if s.tick == tick {
                w.intervene(s.intervention).unwrap();
            }
        }
        w.step();
    }
    assert_eq!(w.dynamics_hash(), INSTITUTION);
    assert_eq!(w.ledger.motions_carried, 1);
    assert_eq!(w.nomos.rules.len(), 1);
    assert_eq!(w.nomos.rules[0].id, 1);
}
