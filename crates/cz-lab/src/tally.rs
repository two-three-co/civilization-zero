//! Counts over a stretch of ticks and the table statistics of the sweep's
//! summary (EXP-002, ADR-0013 3-8; the 4-1 / 4-2 / 4-5 / 4-6 counts
//! appended since). Moved from `cz-cli` in ADR-0015 5-1 **unchanged**: the
//! CSV columns these produce are a contract (completion condition 5), so
//! the column strings below never move and `tests/columns.rs` pins them.

use simulation_core::policy::learner::Q_UNIT;
use simulation_core::policy::{forager_weights, REFLEX_UNIT};
use simulation_core::{Action, DeathCause, PolicyKind, PolicyTable, TickReport, World};
use std::fmt::Write as _;

/// Counts over a stretch of ticks: one sampling interval of the series, or
/// the analysis window of the summary.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub ticks: u64,
    /// Agents alive at the start of each tick, summed: the decisions made.
    pub agent_ticks: u64,
    pub gathered: u64,
    pub consumed: u64,
    pub births: u64,
    pub transfers: u64,
    pub harms: u64,
    pub signals: u64,
    pub proposals: u64,
    pub acceptances: u64,
    pub rejections: u64,
    pub deaths: [u64; DeathCause::COUNT],
    /// Decisions by action kind (`Action::kind_index`), successful or not.
    pub actions: [u64; Action::KIND_COUNT],
    /// Social facts (ADR-0014 4-1), by the gatherer's `PolicyKind`.
    pub gathers: [u64; PolicyKind::COUNT],
    pub encroachments: [u64; PolicyKind::COUNT],
    pub promises_due: u64,
    pub promises_broken: u64,
    /// Food regrown (`Ledger.food_regrown` deltas).
    pub regrown: u64,
    /// Standing food after each tick, summed (mean = `/ ticks`).
    pub food_standing: u64,
    /// Learner row visits (`TickReport.learner_rows`, summed).
    pub rows: Vec<u64>,
    /// Institutions (ADR-0014 4-2), summed over the kinds.
    pub violations: u64,
    pub sanctions: u64,
    pub episodes_closed: u64,
    pub episodes_sanctioned: u64,
    /// The assembly (ADR-0014 4-5).
    pub motions: u64,
    pub votes: u64,
    pub motions_carried: u64,
    pub motions_defeated: u64,
    /// 4-6: births by the child's kind and how many were born a kind other
    /// than the parent's (`enforcer_share_permille`), sanctions by the
    /// sanctioner's kind, and the living by kind after each tick, summed
    /// (mean = `/ ticks`).
    pub births_by_kind: [u64; PolicyKind::COUNT],
    pub births_reassigned: u64,
    pub sanctions_by_kind: [u64; PolicyKind::COUNT],
    pub alive_by_kind: [u64; PolicyKind::COUNT],
}

/// The policy kinds of Phase 3, whose per-kind columns the EXP-002 / 4-1
/// CSVs already carry in place; the enforcer's (4-5) are appended at the
/// end so no column moves.
pub const PHASE_3_KINDS: [PolicyKind; 4] = [
    PolicyKind::Forager,
    PolicyKind::Random,
    PolicyKind::Reflex,
    PolicyKind::Learner,
];

impl Tally {
    /// Column names of `counts_csv`: the EXP-002 columns first (unchanged
    /// in name, order and meaning), then the 4-1 counts, then the 4-2
    /// counts, then (4-5) the enforcer's gathers / encroachments and the
    /// assembly's counts, then (4-6) births and sanctions by kind.
    pub const COUNT_COLUMNS: &'static str = "gathered,consumed,births,transfers,harms,signals,proposals,acceptances,rejections,deaths_starvation,deaths_old_age,deaths_violence,gathers,encroachments,promises_due,promises_broken,regrown,gathers_forager,encroachments_forager,gathers_random,encroachments_random,gathers_reflex,encroachments_reflex,gathers_learner,encroachments_learner,violations,sanctions,episodes_closed,episodes_sanctioned,gathers_enforcer,encroachments_enforcer,motions,votes,motions_carried,motions_defeated,births_forager,births_random,births_reflex,births_learner,births_enforcer,births_reassigned,sanctions_forager,sanctions_random,sanctions_reflex,sanctions_learner,sanctions_enforcer";

    /// Column names of `alive_by_kind_csv`: the living by kind, a state
    /// (not a count) read where the row is written.
    pub const ALIVE_COLUMNS: &'static str =
        "alive_forager,alive_random,alive_reflex,alive_learner,alive_enforcer";

    /// The living by `PolicyKind` (index order of `PolicyKind::ALL`).
    pub fn alive_by_kind(world: &World) -> [u64; PolicyKind::COUNT] {
        let mut out = [0u64; PolicyKind::COUNT];
        for a in &world.agents {
            out[a.policy.index()] += 1;
        }
        out
    }

    pub fn alive_by_kind_csv(world: &World) -> String {
        Self::alive_by_kind(world)
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Adds one tick: `alive_before` agents decided, `regrown` food units
    /// grew, `standing` is the food on the ground afterwards, `alive_after`
    /// the living by kind afterwards.
    pub fn add(
        &mut self,
        alive_before: usize,
        r: &TickReport,
        regrown: u64,
        standing: u64,
        alive_after: &[u64; PolicyKind::COUNT],
    ) {
        self.ticks += 1;
        self.agent_ticks += alive_before as u64;
        self.gathered += r.food_gathered;
        self.consumed += r.food_consumed;
        self.births += u64::from(r.births);
        self.transfers += u64::from(r.transfers);
        self.harms += u64::from(r.harms);
        self.signals += u64::from(r.signals);
        self.proposals += u64::from(r.proposals);
        self.acceptances += u64::from(r.acceptances);
        self.rejections += u64::from(r.rejections);
        for (acc, d) in self.deaths.iter_mut().zip(r.deaths_by_cause) {
            *acc += u64::from(d);
        }
        for (acc, a) in self.actions.iter_mut().zip(r.actions) {
            *acc += u64::from(a);
        }
        for k in 0..PolicyKind::COUNT {
            self.gathers[k] += u64::from(r.gathers[k]);
            self.encroachments[k] += u64::from(r.encroachments[k]);
        }
        self.promises_due += r.promises_settled.len() as u64;
        self.promises_broken += r.promises_settled.iter().filter(|d| !d.kept()).count() as u64;
        self.regrown += regrown;
        self.food_standing += standing;
        self.violations += r.violations.iter().map(|&v| u64::from(v)).sum::<u64>();
        self.sanctions += r.sanctions.iter().map(|&v| u64::from(v)).sum::<u64>();
        self.episodes_closed += r.episodes_closed.iter().map(|&v| u64::from(v)).sum::<u64>();
        self.episodes_sanctioned += r
            .episodes_sanctioned
            .iter()
            .map(|&v| u64::from(v))
            .sum::<u64>();
        self.motions += u64::from(r.motions);
        self.votes += u64::from(r.votes);
        self.motions_carried += u64::from(r.motions_carried);
        self.motions_defeated += u64::from(r.motions_defeated);
        for (k, &alive) in alive_after.iter().enumerate() {
            self.births_by_kind[k] += u64::from(r.births_by_kind[k]);
            self.sanctions_by_kind[k] += u64::from(r.sanctions_by_kind[k]);
            self.alive_by_kind[k] += alive;
        }
        self.births_reassigned += u64::from(r.births_reassigned);
        if !r.learner_rows.is_empty() {
            if self.rows.len() < r.learner_rows.len() {
                self.rows.resize(r.learner_rows.len(), 0);
            }
            for (acc, v) in self.rows.iter_mut().zip(&r.learner_rows) {
                *acc += u64::from(*v);
            }
        }
    }

    pub fn per_tick(&self, n: u64) -> f64 {
        n as f64 / self.ticks.max(1) as f64
    }

    pub fn per_agent_tick(&self, n: u64) -> f64 {
        n as f64 / self.agent_ticks.max(1) as f64
    }

    pub fn gathers_total(&self) -> u64 {
        self.gathers.iter().sum()
    }

    pub fn encroachments_total(&self) -> u64 {
        self.encroachments.iter().sum()
    }

    /// `encroachments / gathers` of one kind, `None` when that kind gathered
    /// nothing (written as an empty CSV cell).
    pub fn encroach_rate(&self, kind: PolicyKind) -> Option<f64> {
        let (g, e) = (self.gathers[kind.index()], self.encroachments[kind.index()]);
        (g > 0).then(|| e as f64 / g as f64)
    }

    /// The interval's counts in the order of [`Self::COUNT_COLUMNS`].
    pub fn counts_csv(&self) -> String {
        let mut s = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.gathered,
            self.consumed,
            self.births,
            self.transfers,
            self.harms,
            self.signals,
            self.proposals,
            self.acceptances,
            self.rejections,
            self.deaths[DeathCause::Starvation.index()],
            self.deaths[DeathCause::OldAge.index()],
            self.deaths[DeathCause::Violence.index()],
            self.gathers_total(),
            self.encroachments_total(),
            self.promises_due,
            self.promises_broken,
            self.regrown,
        );
        for k in PHASE_3_KINDS {
            let k = k.index();
            let _ = write!(s, ",{},{}", self.gathers[k], self.encroachments[k]);
        }
        let _ = write!(
            s,
            ",{},{},{},{}",
            self.violations, self.sanctions, self.episodes_closed, self.episodes_sanctioned
        );
        let e = PolicyKind::Enforcer.index();
        let _ = write!(
            s,
            ",{},{},{},{},{},{}",
            self.gathers[e],
            self.encroachments[e],
            self.motions,
            self.votes,
            self.motions_carried,
            self.motions_defeated
        );
        // 4-6: births and sanctions by kind, appended so no column moves.
        for b in self.births_by_kind {
            let _ = write!(s, ",{b}");
        }
        let _ = write!(s, ",{}", self.births_reassigned);
        for n in self.sanctions_by_kind {
            let _ = write!(s, ",{n}");
        }
        s
    }

    /// H1's S: episodes sanctioned / episodes closed (`None` without a
    /// closed episode).
    pub fn s(&self) -> Option<f64> {
        (self.episodes_closed > 0)
            .then(|| self.episodes_sanctioned as f64 / self.episodes_closed as f64)
    }

    /// H1's S': sanctions / violations (`None` without a violation).
    pub fn s_prime(&self) -> Option<f64> {
        (self.violations > 0).then(|| self.sanctions as f64 / self.violations as f64)
    }
}

/// Population mean of the living learners' Q tables, cell by cell (fixed
/// point). `None` without learners. All tables have `learner_states × 8`
/// cells (`check_invariants`).
pub fn mean_q_table(world: &World) -> Option<Vec<f64>> {
    let mut sum: Vec<f64> = Vec::new();
    let mut n = 0u32;
    for a in &world.agents {
        if let Some(PolicyTable::Learner { q }) = &a.params.table {
            if sum.is_empty() {
                sum = vec![0.0; q.len()];
            }
            for (s, &v) in sum.iter_mut().zip(q) {
                *s += f64::from(v);
            }
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    for s in &mut sum {
        *s /= f64::from(n);
    }
    Some(sum)
}

/// Mean |Q| per cell over the living learners, in reward units (one unit =
/// 100 milli-energy): how far from the blank table the population has moved.
pub fn q_abs_mean(world: &World) -> Option<f64> {
    let (mut sum, mut cells) = (0.0f64, 0u64);
    for a in &world.agents {
        if let Some(PolicyTable::Learner { q }) = &a.params.table {
            sum += q.iter().map(|&v| f64::from(v).abs()).sum::<f64>();
            cells += q.len() as u64;
        }
    }
    (cells > 0).then(|| sum / cells as f64 / Q_UNIT as f64)
}

/// Mean |w − w₀| per weight over the living reflex agents, in units of 1.0
/// (`REFLEX_UNIT`), with w₀ the forager-equivalent weights every reflex
/// lineage started from: how far mutation has carried the population.
pub fn reflex_weight_drift(world: &World) -> Option<f64> {
    let w0 = forager_weights();
    let (mut sum, mut cells) = (0.0f64, 0u64);
    for a in &world.agents {
        if let Some(PolicyTable::Reflex { weights }) = &a.params.table {
            sum += weights
                .iter()
                .zip(&w0)
                .map(|(&w, &b)| f64::from(w - b).abs())
                .sum::<f64>();
            cells += weights.len() as u64;
        }
    }
    (cells > 0).then(|| sum / cells as f64 / f64::from(REFLEX_UNIT))
}

/// Mean `hunger_permille` of the living (the one scalar mutation moves).
pub fn hunger_mean(world: &World) -> Option<f64> {
    let n = world.agents.len();
    (n > 0).then(|| {
        world
            .agents
            .iter()
            .map(|a| f64::from(a.params.hunger_permille))
            .sum::<f64>()
            / n as f64
    })
}

/// Per Q row, how many living learners would pick each action kind greedily
/// (the row's unique maximum); the last column counts rows tied at the top,
/// which the kernel breaks by lot.
pub fn greedy_counts(world: &World, rows: usize) -> Vec<[u32; Action::KIND_COUNT + 1]> {
    let mut out = vec![[0u32; Action::KIND_COUNT + 1]; rows];
    for a in &world.agents {
        if let Some(PolicyTable::Learner { q }) = &a.params.table {
            for (row, cells) in q.chunks_exact(Action::KIND_COUNT).enumerate().take(rows) {
                let best = cells.iter().copied().max().unwrap_or(0);
                let mut winners = cells.iter().enumerate().filter(|(_, &v)| v == best);
                let first = winners.next().map_or(0, |(k, _)| k);
                let col = if winners.next().is_some() {
                    Action::KIND_COUNT
                } else {
                    first
                };
                out[row][col] += 1;
            }
        }
    }
    out
}

/// An optional statistic as a CSV cell: the number with `digits` decimals,
/// or the empty cell.
pub fn opt_csv(v: Option<f64>, digits: usize) -> String {
    v.map_or(String::new(), |x| format!("{x:.digits$}"))
}
