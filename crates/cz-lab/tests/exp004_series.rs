//! Regression against the registered data (ADR-0015 5-1, completion
//! condition 5): the first 2,000 ticks of EXP-004's `le` and `l0` cells,
//! seed 0, built from `data/EXP-004/design.json`, must write exactly the
//! first 21 series rows of `data/EXP-004/<cell>_series.csv` (2026-10-18,
//! `cz sweep` of commit afb3e26). A row at tick 2,000 covers the interval
//! since tick 1,900 either way, so a 2,000-tick run ends on the same row
//! the 20,000-tick run wrote in passing.
//!
//! The same runs check 5-2's per-agent table (ADR-0015 D4): summed over
//! the agents, the table's gathers, encroachments, sanctions, births and
//! deaths — over the run, over the window (1,000, 2,000] and over its
//! halves — are the series' sums, and the summary's cohort columns are
//! what the table says.

use cz_lab::{run_cell, Design, Rows, RunOptions, AGENTS_COLUMNS, SERIES_COLUMNS, SUMMARY_COLUMNS};

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
    let options = RunOptions {
        agents: true,
        ..RunOptions::default()
    };
    let run = run_cell(&design, cell, 0, &options, &mut rows).unwrap();
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
    agents_table_sums_to_the_series(cell_name, &rows, &run.cohort.csv());
}

fn col(header: &str, name: &str) -> usize {
    header
        .split(',')
        .position(|c| c == name)
        .unwrap_or_else(|| panic!("no column {name}"))
}

/// Σ of a series column over the rows with `from < tick ≤ to` — the
/// interval's rows, each covering the 100 executed ticks before it.
fn series_sum(rows: &[String], name: &str, from: u64, to: u64) -> u64 {
    let (t, c) = (col(SERIES_COLUMNS, "tick"), col(SERIES_COLUMNS, name));
    rows.iter()
        .map(|r| r.split(',').collect::<Vec<_>>())
        .filter(|f| {
            let tick: u64 = f[t].parse().unwrap();
            tick > from && tick <= to
        })
        .map(|f| f[c].parse::<u64>().unwrap())
        .sum()
}

fn agents_table_sums_to_the_series(cell_name: &str, rows: &Rows, cohort_csv: &str) {
    let agents: Vec<Vec<&str>> = rows.agents.iter().map(|r| r.split(',').collect()).collect();
    let width = AGENTS_COLUMNS.split(',').count();
    assert!(agents.len() > 1_000, "{cell_name}: founders and births");
    assert!(agents.iter().all(|f| f.len() == width && f[0] == "0"));
    let a = |name: &str| col(AGENTS_COLUMNS, name);
    let sum = |name: &str, keep: &dyn Fn(&[&str]) -> bool| -> u64 {
        agents
            .iter()
            .filter(|f| keep(f))
            .map(|f| f[a(name)].parse::<u64>().unwrap())
            .sum()
    };
    let all = |_: &[&str]| true;
    let learner = |f: &[&str]| f[a("policy")] == "learner";
    let enforcer = |f: &[&str]| f[a("policy")] == "enforcer";
    let s = |name: &str, from: u64, to: u64| series_sum(&rows.series, name, from, to);
    // The whole run: the rows at ticks 100 … 2,000 cover executed ticks 0 … 1,999.
    assert_eq!(sum("gathers", &all), s("gathers", 0, 2_000), "{cell_name}");
    assert_eq!(sum("encroachments", &all), s("encroachments", 0, 2_000));
    assert_eq!(sum("gathers", &learner), s("gathers_learner", 0, 2_000));
    assert_eq!(
        sum("encroachments", &learner),
        s("encroachments_learner", 0, 2_000)
    );
    assert_eq!(sum("gathers", &enforcer), s("gathers_enforcer", 0, 2_000));
    assert_eq!(
        sum("encroachments", &enforcer),
        s("encroachments_enforcer", 0, 2_000)
    );
    assert_eq!(sum("sanctions_received", &all), s("sanctions", 0, 2_000));
    assert_eq!(sum("sanctions_given", &all), s("sanctions", 0, 2_000));
    assert_eq!(
        sum("sanctions_given", &learner),
        s("sanctions_learner", 0, 2_000)
    );
    assert_eq!(
        sum("sanctions_given", &enforcer),
        s("sanctions_enforcer", 0, 2_000)
    );
    // The window (1,000, 2,000] and its halves.
    assert_eq!(sum("win_gathers", &all), s("gathers", 1_000, 2_000));
    assert_eq!(
        sum("win_encroachments", &all),
        s("encroachments", 1_000, 2_000)
    );
    assert_eq!(
        sum("win_sanctions_received", &all),
        s("sanctions", 1_000, 2_000)
    );
    assert_eq!(
        sum("win_sanctions_given", &all),
        s("sanctions", 1_000, 2_000)
    );
    assert_eq!(
        sum("win_gathers_first_half", &all),
        s("gathers", 1_000, 1_500)
    );
    assert_eq!(
        sum("win_gathers_second_half", &all),
        s("gathers", 1_500, 2_000)
    );
    assert_eq!(
        sum("win_encroachments_first_half", &all),
        s("encroachments", 1_000, 1_500)
    );
    assert_eq!(
        sum("win_encroachments_second_half", &all),
        s("encroachments", 1_500, 2_000)
    );
    // Births and deaths.
    let born = agents.iter().filter(|f| f[a("born")] != "0").count() as u64;
    assert_eq!(born, s("births", 0, 2_000));
    for (cause, column) in [
        ("starvation", "deaths_starvation"),
        ("old_age", "deaths_old_age"),
        ("violence", "deaths_violence"),
    ] {
        let n = agents.iter().filter(|f| f[a("cause")] == cause).count() as u64;
        assert_eq!(n, s(column, 0, 2_000), "{cell_name}: {cause}");
    }
    // The summary's cohort columns are the table's. The cohort is the
    // learners alive at tick 1,000 — the series row's `alive_learner` there.
    let summary: Vec<&str> = rows.summary[0].split(',').collect();
    let c = |name: &str| summary[col(SUMMARY_COLUMNS, name)];
    let cohort_cells: Vec<&str> = cohort_csv.split(',').collect();
    assert_eq!(
        &summary[summary.len() - 1 - cohort_cells.len()..summary.len() - 1],
        &cohort_cells[..]
    );
    let row_1000 = rows
        .series
        .iter()
        .find(|r| r.split(',').nth(col(SERIES_COLUMNS, "tick")) == Some("1000"))
        .unwrap();
    assert_eq!(
        c("win_cohort_learners"),
        row_1000
            .split(',')
            .nth(col(SERIES_COLUMNS, "alive_learner"))
            .unwrap()
    );
    let cohort = |f: &[&str]| learner(f) && f[a("cohort")] == "1";
    let count = |keep: &dyn Fn(&[&str]) -> bool| agents.iter().filter(|f| keep(f)).count();
    assert_eq!(c("win_cohort_learners"), count(&cohort).to_string());
    let survived = |f: &[&str]| cohort(f) && f[a("died")].is_empty();
    assert_eq!(c("win_cohort_survived"), count(&survived).to_string());
    let violent = |f: &[&str]| f[a("cause")] == "violence";
    assert_eq!(
        c("win_cohort_died_violence"),
        count(&|f| cohort(f) && violent(f)).to_string()
    );
    let rate = |e: u64, g: u64| -> String {
        if g > 0 {
            format!("{:.5}", e as f64 / g as f64)
        } else {
            String::new()
        }
    };
    assert_eq!(
        c("win_e_learner_survivors"),
        rate(
            sum("win_encroachments", &survived),
            sum("win_gathers", &survived)
        )
    );
    let removed = |f: &[&str]| learner(f) && violent(f);
    assert_eq!(
        c("win_e_learner_removed"),
        rate(
            sum("win_encroachments", &removed),
            sum("win_gathers", &removed)
        )
    );
    assert_eq!(
        c("win_encroach_share_removed"),
        rate(
            sum("win_encroachments", &removed),
            sum("win_encroachments", &learner)
        )
    );
    assert_eq!(
        c("win_e_survivors_first_half"),
        rate(
            sum("win_encroachments_first_half", &survived),
            sum("win_gathers_first_half", &survived)
        )
    );
    assert_eq!(
        c("win_e_survivors_second_half"),
        rate(
            sum("win_encroachments_second_half", &survived),
            sum("win_gathers_second_half", &survived)
        )
    );
    let sanctioned = |f: &[&str]| learner(f) && f[a("win_sanctions_received")] != "0";
    assert_eq!(c("win_sanctioned_learners"), count(&sanctioned).to_string());
    let then_died: usize = agents
        .iter()
        .filter(|f| sanctioned(f) && violent(f))
        .filter(|f| {
            let died: u64 = f[a("died")].parse().unwrap();
            let last: u64 = f[a("last_sanctioned")].parse().unwrap();
            died >= last && died - last <= 32
        })
        .count();
    assert_eq!(c("win_sanctioned_then_died"), then_died.to_string());
    if cell_name == "le" {
        // The ruled arm removes learners — in this early window of seed 0
        // (ticks 1,000 … 1,999, the recovery after the decree's valley)
        // every one of the 196 learners alive at tick 1,000 is dead by
        // tick 2,000, 178 of them by violence, so the survivors' rate is
        // undefined here and the removal share is what is left to read.
        // The registered window (10,000, 20,000] is EXP-006's to report.
        assert!(count(&removed) > 0, "the ruled arm removed learners");
        assert_ne!(c("win_encroach_share_removed"), "");
        assert_ne!(c("win_sanctioned_then_died"), "0");
    } else {
        assert_eq!(sum("sanctions_received", &all), 0, "no rule, no sanction");
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
