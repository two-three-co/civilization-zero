//! Regression against the registered data (ADR-0015 5-1, completion
//! condition 5): the first 2,000 ticks of EXP-004's `le` and `l0` cells,
//! seed 0, built from `data/EXP-004/design.json`, must write exactly the
//! first 21 series rows of `data/EXP-004/<cell>_series.csv` (2026-10-18,
//! `cz sweep` of commit afb3e26). A row at tick 2,000 covers the interval
//! since tick 1,900 either way, so a 2,000-tick run ends on the same row
//! the 20,000-tick run wrote in passing.

use cz_lab::{run_cell, Design, Rows, RunOptions, SERIES_COLUMNS};

fn data(path: &str) -> String {
    let full = format!(
        "{}/../../docs/research/experiments/data/EXP-004/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{full}: {e}"))
}

fn first_2000_ticks_match(cell_name: &str) {
    let mut design = Design::from_json(&data("design.json")).unwrap();
    design.ticks = 2_000;
    // The contrasts' windows lie beyond a 2,000-tick run; only the rows matter here.
    design.contrasts.clear();
    design.validate().unwrap();
    let cell = design.cell(cell_name).unwrap();
    let mut rows = Rows::default();
    let run = run_cell(&design, cell, 0, &RunOptions::default(), &mut rows).unwrap();
    assert_eq!(run.ticks, 2_000);
    let csv = data(&format!("{cell_name}_series.csv"));
    let mut lines = csv.lines();
    assert_eq!(lines.next().unwrap(), SERIES_COLUMNS);
    let expected: Vec<&str> = lines.filter(|l| l.starts_with("0,")).take(21).collect();
    assert_eq!(expected.len(), 21);
    assert_eq!(rows.series.len(), 21);
    for (i, (got, want)) in rows.series.iter().zip(&expected).enumerate() {
        assert_eq!(got, want, "{cell_name}: series row {i} differs");
    }
}

#[test]
fn exp004_le_seed_0_first_2000_ticks_match_the_registered_series() {
    first_2000_ticks_match("le");
}

#[test]
fn exp004_l0_seed_0_first_2000_ticks_match_the_registered_series() {
    first_2000_ticks_match("l0");
}

/// The registered design describes the registered arms: eleven cells, the
/// seeds and sampling of `run.sh`, the decree in the ruled arms only.
#[test]
fn the_registered_design_matches_run_sh() {
    let design = Design::from_json(&data("design.json")).unwrap();
    let names: Vec<&str> = design.cells.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "le0",
            "le",
            "l0",
            "l1",
            "n_rand",
            "n_blind",
            "n_nosanction",
            "n_nocomm",
            "n_nomem",
            "w0",
            "w1"
        ]
    );
    for c in &design.cells {
        let seeds = design.seeds_of(c);
        let expected = if matches!(c.name.as_str(), "le0" | "le" | "l0") {
            30
        } else {
            10
        };
        assert_eq!((seeds.start, seeds.count), (0, expected), "{}", c.name);
        assert_eq!(
            design.sample_of(c),
            if c.name == "n_rand" { 10 } else { 100 }
        );
        let ruled = !matches!(c.name.as_str(), "le0" | "l0" | "w0");
        assert_eq!(c.schedule.len(), usize::from(ruled), "{}", c.name);
        let cfg = design.world_config(c, 0).unwrap();
        // The random null model is `--preset nomos` alone: 32 rows (unused).
        assert_eq!(
            cfg.learner_states,
            if c.name == "n_rand" { 32 } else { 128 },
            "{}",
            c.name
        );
        assert!(cfg.nomos);
        assert_eq!(cfg.learner_epsilon_permille, 10);
        let institution = design.policy_of(c) == "institution";
        assert_eq!(cfg.enforcer_share_permille.is_some(), institution);
        assert_eq!(cfg.enforcer_motions, !institution, "{}", c.name);
        assert_eq!(cfg.perception.see_standing, c.name != "n_blind");
        assert_eq!(
            cfg.perception.signal_radius,
            if c.name == "n_nocomm" { 0 } else { 2 }
        );
        assert_eq!(cfg.memory_slots, if c.name == "n_nomem" { 0 } else { 8 });
        assert_eq!(
            cfg.learner_health_weight,
            if c.name.starts_with('w') { 2 } else { 0 }
        );
    }
    assert_eq!(design.window(), (10_000, 20_000));
}
