//! `cz lab summarize` (ADR-0015 D2): per cell, the distribution of the
//! standard readings over the seeds; per contrast, the paired statistics.
//! Rendered as JSON (`SUMMARY.json`) and Markdown (`SUMMARY.md`) from the
//! same struct, so a dashboard and the CLI show the same numbers. Each
//! EXP's prediction table (P0 … Pn) stays in its `summarize.py`, whose
//! contrasts this cross-checks.

use crate::design::{Columns, Design, Metric};
use crate::stats::{distribution, fmt, paired_stats, CellTables, Distribution, PairedStats};
use crate::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `"lab_summary"` of `SUMMARY.json`.
pub const SUMMARY_FORMAT: u32 = 1;

/// The readings summarised for every cell (ADR-0015 completion condition
/// 1 (b): population, E / E_L, S / S′, the share of violent deaths,
/// consumption, and since 5-2 the cohort readings of D4), all from the
/// `summary` CSV's analysis window. A reading whose columns a CSV lacks
/// (older experiments) is skipped.
pub fn cell_metrics() -> Vec<(&'static str, Metric)> {
    let s = |c: &str| Metric::Summary(c.to_string());
    vec![
        ("final_alive", s("final_alive")),
        ("win_mean", s("win_mean")),
        (
            "win_consumed_per_agent_tick",
            s("win_consumed_per_agent_tick"),
        ),
        ("win_encroach_rate", s("win_encroach_rate")),
        ("win_encroach_rate_learner", s("win_encroach_rate_learner")),
        ("win_s", s("win_s")),
        ("win_s_prime", s("win_s_prime")),
        (
            "win_violence_share",
            Metric::SummaryRatio {
                num: Columns(vec!["win_deaths_violence".to_string()]),
                den: Columns(vec![
                    "win_deaths_violence".to_string(),
                    "win_deaths_starvation".to_string(),
                    "win_deaths_old_age".to_string(),
                ]),
            },
        ),
        ("win_harms_per_tick", s("win_harms_per_tick")),
        ("win_alive_learner_mean", s("win_alive_learner_mean")),
        ("win_alive_enforcer_mean", s("win_alive_enforcer_mean")),
        // 5-2 (D4): learning vs removal.
        ("win_e_learner_survivors", s("win_e_learner_survivors")),
        ("win_e_learner_removed", s("win_e_learner_removed")),
        (
            "win_encroach_share_removed",
            s("win_encroach_share_removed"),
        ),
        (
            "win_e_survivors_delta_mean",
            s("win_e_survivors_delta_mean"),
        ),
        ("win_cohort_died_violence", s("win_cohort_died_violence")),
    ]
}

/// One reading's distribution over a cell's living seeds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MetricSummary {
    pub name: String,
    pub metric: Metric,
    /// `(from, to]` for series metrics; the summary's window otherwise.
    pub window: (u64, u64),
    pub distribution: Distribution,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CellSummary {
    pub name: String,
    /// Seeds in the summary CSV, and how many of them went extinct.
    pub seeds: usize,
    pub extinct: usize,
    pub metrics: Vec<MetricSummary>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    pub lab_summary: u32,
    pub design: String,
    pub ticks: u64,
    /// The design's analysis window `(from, to]`.
    pub window: (u64, u64),
    pub cells: Vec<CellSummary>,
    pub contrasts: Vec<PairedStats>,
}

fn metric_available(tables: &CellTables, metric: &Metric) -> bool {
    let has = |t: &crate::stats::Table, cols: &[String]| cols.iter().all(|c| t.col(c).is_some());
    match metric {
        Metric::Ratio { num, den } => has(&tables.series, &num.0) && has(&tables.series, &den.0),
        Metric::Mean(c) | Metric::PerTick(c) => has(&tables.series, std::slice::from_ref(c)),
        Metric::Summary(c) => has(&tables.summary, std::slice::from_ref(c)),
        Metric::SummaryRatio { num, den } => {
            has(&tables.summary, &num.0) && has(&tables.summary, &den.0)
        }
    }
}

fn cell_summary(
    name: &str,
    tables: &CellTables,
    metrics: &[(String, Metric, (u64, u64))],
) -> Result<CellSummary, Error> {
    let seeds = tables.seeds()?;
    let mut extinct = 0;
    let mut alive = Vec::new();
    for &s in &seeds {
        if tables.extinct(s)?.unwrap_or(false) {
            extinct += 1;
        } else {
            alive.push(s);
        }
    }
    let mut out = Vec::new();
    for (label, metric, window) in metrics {
        if !metric_available(tables, metric) {
            continue;
        }
        let mut values = Vec::with_capacity(alive.len());
        for &s in &alive {
            if let Some(v) = tables.metric(s, metric, *window)? {
                values.push(v);
            }
        }
        if let Some(d) = distribution(&values) {
            out.push(MetricSummary {
                name: label.clone(),
                metric: metric.clone(),
                window: *window,
                distribution: d,
            });
        }
    }
    Ok(CellSummary {
        name: name.to_string(),
        seeds: seeds.len(),
        extinct,
        metrics: out,
    })
}

/// Summarises the tables of a design's cells (keyed by cell name; a cell
/// without tables is an error, since a summary of a partial run would read
/// as the whole).
pub fn summarize(design: &Design, tables: &BTreeMap<String, CellTables>) -> Result<Summary, Error> {
    let window = design.window();
    let mut cells = Vec::new();
    for cell in &design.cells {
        let t = tables
            .get(&cell.name)
            .ok_or_else(|| Error(format!("no tables for cell {:?}", cell.name)))?;
        // The standard readings, then the contrasts' metrics that involve
        // this cell (once each).
        let mut metrics: Vec<(String, Metric, (u64, u64))> = cell_metrics()
            .into_iter()
            .map(|(n, m)| (n.to_string(), m, window))
            .collect();
        for c in &design.contrasts {
            if c.treat != cell.name && c.control != cell.name {
                continue;
            }
            let w = if c.metric.reads_series() {
                c.window.unwrap_or(window)
            } else {
                window
            };
            let label = if c.metric.reads_series() {
                format!("{} @({}, {}]", c.metric, w.0, w.1)
            } else {
                c.metric.to_string()
            };
            if !metrics.iter().any(|(l, ..)| *l == label) {
                metrics.push((label, c.metric.clone(), w));
            }
        }
        cells.push(cell_summary(&cell.name, t, &metrics)?);
    }
    let mut contrasts = Vec::new();
    for c in &design.contrasts {
        let treat = tables
            .get(&c.treat)
            .ok_or_else(|| Error(format!("no tables for cell {:?}", c.treat)))?;
        let control = tables
            .get(&c.control)
            .ok_or_else(|| Error(format!("no tables for cell {:?}", c.control)))?;
        contrasts.push(paired_stats(treat, control, c, window)?);
    }
    Ok(Summary {
        lab_summary: SUMMARY_FORMAT,
        design: design.name.clone(),
        ticks: design.ticks,
        window,
        cells,
        contrasts,
    })
}

impl Summary {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("a summary serialises")
    }

    /// The summary as Markdown tables: one long table of cell × reading
    /// (n, mean, sd, min, quartiles, max), one of contrasts (mean [95%
    /// CI], d_z, n, seeds by sign, ratio, the two means).
    pub fn to_markdown(&self) -> String {
        let mut s = format!(
            "# {} — cz lab summarize\n\nwindow ({}, {}] = the last {} ticks of {} · distributions over the seeds alive at the end · quartiles by linear interpolation · contrasts = same-seed differences treat − control over the seeds alive in both, mean [95% bootstrap CI: {} resamples, SplitMix seed {}], d_z = mean / sd (n − 1)\n\n## Cells\n\n| cell | seeds | extinct | reading | n | mean | sd | min | q1 | median | q3 | max |\n|---|---|---|---|---|---|---|---|---|---|---|---|\n",
            self.design,
            self.window.0,
            self.window.1,
            self.window.1 - self.window.0,
            self.ticks,
            crate::stats::BOOT,
            crate::stats::BOOT_SEED
        );
        for c in &self.cells {
            if c.metrics.is_empty() {
                s.push_str(&format!(
                    "| {} | {} | {} | — | 0 | — | — | — | — | — | — | — |\n",
                    c.name, c.seeds, c.extinct
                ));
            }
            for m in &c.metrics {
                let d = &m.distribution;
                s.push_str(&format!(
                    "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                    c.name,
                    c.seeds,
                    c.extinct,
                    m.name,
                    d.n,
                    fmt(d.mean, 4),
                    fmt(d.sd, 4),
                    fmt(d.min, 4),
                    fmt(d.q1, 4),
                    fmt(d.median, 4),
                    fmt(d.q3, 4),
                    fmt(d.max, 4)
                ));
            }
        }
        s.push_str("\n## Contrasts\n\n| contrast | metric | window | treat − control [95% CI], d_z, n | treat < control | treat > control | ratio | mean treat | mean control |\n|---|---|---|---|---|---|---|---|---|\n");
        for p in &self.contrasts {
            s.push_str(&format!(
                "| {} ({} − {}) | {} | ({}, {}] | {} | {}/{} | {}/{} | {} | {} | {} |\n",
                p.name,
                p.treat,
                p.control,
                p.metric,
                p.window.0,
                p.window.1,
                p.ci_text(4),
                p.negative,
                p.n,
                p.positive,
                p.n,
                fmt(p.ratio, 3),
                fmt(p.mean_treat, 4),
                fmt(p.mean_control, 4)
            ));
        }
        s
    }
}
