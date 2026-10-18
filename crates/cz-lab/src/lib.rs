//! `cz-lab` — the native half of the Research Platform (ADR-0015 D2).
//!
//! An experiment is a [`Design`]: a JSON file a human writes and commits
//! with the experiment's specification — the base world (policy mix,
//! preset, grid, founders, `WorldConfig` overrides), the run length, the
//! analysis window, the series' sampling interval, the seeds, the **cells**
//! (one condition each: more overrides, a policy, and a **schedule** of
//! interventions at ticks — EXP-004's decree at tick 0, EXP-005's
//! `ScaleFood` at tick 1,000, EXP-007's `Repeal` at tick 10,000 are all
//! "the same seed with a different schedule"), and the **contrasts** (the
//! same-seed pairs whose differences are reported).
//!
//! [`run_cell`] builds one world from a design, a cell and a seed — by the
//! rules `cz run` / `cz sweep` / `cz fork` have always used, which moved
//! here — advances it, applies the schedule, and writes the rows `cz
//! sweep` has always written: a `series` row every `sample` ticks, one
//! `summary` row over the last `window_pct` of the run, and `qpolicy` rows
//! when learners are alive at the end. [`Tally`] and the column strings
//! moved here from `cz-cli` unchanged: the columns are a contract
//! (ADR-0015 completion condition 5) — existing columns never move, new
//! ones go before `ticks_per_s`, and `tests/columns.rs` pins every header.
//!
//! [`paired_stats`] is the statistic every EXP reports (EXP-004's
//! `summarize.py`, now in Rust so native and wasm agree bit for bit): the
//! mean of the same-seed differences, a 95% bootstrap interval (10,000
//! resamples from a SplitMix counter generator seeded 20261018), d_z, the
//! ratio of means and the seed counts by sign. Floating point lives in
//! this crate's statistics only; the kernel stays integer.
//!
//! [`Manifest`] is `RUN.json` (`lab_result: 1`): the design, the kernel's
//! commit, the machine, start / end, and one record per world (cell, seed,
//! ticks, `dynamics_hash`, ms). [`cohort`] is the per-agent tracking of
//! ADR-0015 D4 (5-2): every run tallies each agent's gathers,
//! encroachments, sanctions, birth and death from the kernel's per-tick
//! material (`TickReport.gatherers / encroachers` and the events), writes
//! the eleven cohort columns at the end of the summary row and, on
//! request, one `agents` row per agent.
//!
//! This crate depends on the kernel only. `cz-cli` drives it from the
//! command line (`cz sweep`, `cz lab run`, `cz lab summarize`);
//! `cz-lab-web` (5-8) wraps the same functions for the browser's worker
//! pool. No LLM is involved anywhere (PRODUCT.md principle 2).

pub mod cohort;
pub mod design;
pub mod manifest;
pub mod run;
pub mod stats;
pub mod summary;
pub mod tally;

pub use cohort::{AgentTally, CohortMetrics, CohortTally, Counts, AGENTS_COLUMNS};
pub use design::{
    evolves, policy_mix, Base, Cell, Columns, Contrast, Design, Metric, Scheduled, Seeds,
    DESIGN_FORMAT, POLICY_MIXES, PRESETS,
};
pub use manifest::{
    iso8601_utc, now_iso8601, Environment, Manifest, Selection, WorldRecord, LAB_RESULT_FORMAT,
};
pub use run::{
    run_cell, series_row, CellRun, Rows, RunOptions, Sink, QPOLICY_COLUMNS, SERIES_COLUMNS,
    SUMMARY_COLUMNS,
};
pub use stats::{
    bootstrap_ci, distribution, paired_stats, BootRng, CellTables, Distribution, PairedStats,
    Table, BOOT, BOOT_SEED,
};
pub use summary::{cell_metrics, summarize, CellSummary, MetricSummary, Summary, SUMMARY_FORMAT};
pub use tally::Tally;

/// What can go wrong outside the kernel: a design that does not parse or
/// does not describe a world, a CSV that does not match its header, a
/// contrast that names a column the rows do not have. The kernel's own
/// errors (`ConfigError`, `InterventionError`) are wrapped as text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error(s.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error(format!("json: {e}"))
    }
}

impl From<simulation_core::config::ConfigError> for Error {
    fn from(e: simulation_core::config::ConfigError) -> Self {
        Error(e.to_string())
    }
}

impl From<simulation_core::InterventionError> for Error {
    fn from(e: simulation_core::InterventionError) -> Self {
        Error(e.to_string())
    }
}
