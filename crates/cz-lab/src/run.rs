//! One world of a design, run to the end and reduced to the rows `cz
//! sweep` writes (ADR-0015 D2). The loop is `cz sweep`'s per-seed loop,
//! moved here unchanged in what it writes: a `series` row at every tick
//! divisible by `sample` (the counts of the interval since the previous
//! row, the population and food at that tick, the living by kind) and one
//! more after the last tick; a `summary` row over the analysis window; and,
//! when learners are alive at the end, one `qpolicy` row per Q row.
//!
//! The headers are constants here and pinned by `tests/columns.rs`; the
//! kernel's `dynamics_hash` of the final state is returned with the run so
//! that a manifest can record it and a browser can match it (D3).

use crate::design::{Cell, Design};
use crate::tally::{
    greedy_counts, hunger_mean, mean_q_table, opt_csv, q_abs_mean, reflex_weight_drift, Tally,
    PHASE_3_KINDS,
};
use crate::Error;
use simulation_core::policy::learner::{state, Q_UNIT};
use simulation_core::{DeathCause, PolicyKind, World};
use std::fmt::Write as _;

/// Header of `<cell>_summary.csv`: one row per seed. Existing columns never
/// move; a new column goes right before `ticks_per_s` (ADR-0015
/// completion condition 5).
pub const SUMMARY_COLUMNS: &str = "seed,policy,final_alive,extinct_at,win_mean,win_min,win_max,win_max_over_min,births,deaths_starvation,deaths_old_age,deaths_violence,mutation,learner_states,learner_epsilon,win_ticks,win_agent_ticks,win_consumed_per_agent_tick,win_gathered_per_agent_tick,win_births_per_tick,win_transfers_per_tick,win_harms_per_tick,win_signals_per_tick,win_proposals_per_tick,win_acceptances_per_tick,win_rejections_per_tick,win_deaths_starvation,win_deaths_old_age,win_deaths_violence,win_act_move,win_act_gather,win_act_consume,win_act_rest,win_act_transfer,win_act_store,win_act_harm,win_act_signal,hunger_permille_mean,q_abs_mean,q_delta_window,reflex_weight_drift,nomos,learner_health_weight,win_gathers_per_tick,win_encroachments_per_tick,win_encroach_rate,win_encroach_rate_forager,win_encroach_rate_random,win_encroach_rate_reflex,win_encroach_rate_learner,win_promises_due_per_tick,win_promises_broken_per_tick,win_food_standing_mean,win_regrown_per_tick,rules,win_violations_per_tick,win_sanctions_per_tick,win_episodes_closed,win_episodes_sanctioned,win_s,win_s_prime,harm_cost,win_encroach_rate_enforcer,motions,motions_carried,motions_defeated,win_votes_per_tick,enforcer_share,see_standing,signal_radius,memory_slots,win_alive_learner_mean,win_alive_enforcer_mean,win_sanctions_learner_per_tick,win_sanctions_enforcer_per_tick,win_births_reassigned_per_tick,ticks_per_s";

/// Header of `<cell>_series.csv`: `seed,tick,alive,food`, then
/// [`Tally::COUNT_COLUMNS`], then [`Tally::ALIVE_COLUMNS`] (a unit test
/// checks the three agree).
pub const SERIES_COLUMNS: &str = "seed,tick,alive,food,gathered,consumed,births,transfers,harms,signals,proposals,acceptances,rejections,deaths_starvation,deaths_old_age,deaths_violence,gathers,encroachments,promises_due,promises_broken,regrown,gathers_forager,encroachments_forager,gathers_random,encroachments_random,gathers_reflex,encroachments_reflex,gathers_learner,encroachments_learner,violations,sanctions,episodes_closed,episodes_sanctioned,gathers_enforcer,encroachments_enforcer,motions,votes,motions_carried,motions_defeated,births_forager,births_random,births_reflex,births_learner,births_enforcer,births_reassigned,sanctions_forager,sanctions_random,sanctions_reflex,sanctions_learner,sanctions_enforcer,alive_forager,alive_random,alive_reflex,alive_learner,alive_enforcer";

/// Header of `<cell>_qpolicy.csv`: per Q row of the final population, the
/// row's bits in the order of `policy::learner::state` (ADR-0014 D6), how
/// many living learners prefer each action kind greedily, the rows tied at
/// the top, and the row's visits over the run and over the window.
pub const QPOLICY_COLUMNS: &str = "seed,row,hungry,has_food,food_here,food_in_sight,adjacent,occupied,violator_adjacent,accused,upper,heard,learners,move,gather,consume,rest,transfer,store,harm,signal,tie,visits,visits_window";

/// Where `run_cell` writes its rows (one line each, without the newline).
pub trait Sink {
    fn summary(&mut self, row: &str);
    fn series(&mut self, row: &str);
    fn qpolicy(&mut self, row: &str);
}

/// A sink that keeps the rows: what the CLI collects per world and writes
/// in seed order, and what the browser hands back as `rows()`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rows {
    pub summary: Vec<String>,
    pub series: Vec<String>,
    pub qpolicy: Vec<String>,
}

impl Sink for Rows {
    fn summary(&mut self, row: &str) {
        self.summary.push(row.to_string());
    }

    fn series(&mut self, row: &str) {
        self.series.push(row.to_string());
    }

    fn qpolicy(&mut self, row: &str) {
        self.qpolicy.push(row.to_string());
    }
}

/// Runner options that are not part of the design.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// `check_invariants` after every tick (`cz sweep --check`).
    pub check: bool,
}

/// What one world's run came to, besides its rows: the figures of `cz
/// sweep`'s console line and the manifest's record.
#[derive(Clone, Debug, PartialEq)]
pub struct CellRun {
    pub cell: String,
    pub seed: u64,
    pub ticks: u64,
    pub final_alive: usize,
    pub extinct_at: Option<u64>,
    /// Population over the window: mean, min, max, max / min (∞ when the
    /// min is 0).
    pub win_mean: f64,
    pub win_min: u64,
    pub win_max: u64,
    pub win_max_over_min: f64,
    /// Over the whole run (the ledger's).
    pub births: u64,
    pub deaths: [u64; DeathCause::COUNT],
    /// Over the window.
    pub consumed_per_agent_tick: f64,
    pub transfers_per_tick: f64,
    pub harms_per_tick: f64,
    pub signals_per_tick: f64,
    pub encroach_rate: f64,
    /// Wall clock of the run (0 where there is no clock: wasm).
    pub ticks_per_s: f64,
    pub ms: u64,
    pub dynamics_hash: u64,
    pub state_hash: u64,
    /// Whether `qpolicy` rows were written (a learner was alive at the end).
    pub learners_alive: bool,
}

impl CellRun {
    /// The header `cz sweep` prints above its per-seed lines.
    pub fn console_header() -> String {
        format!(
            "{:>5} {:>7} {:>8} {:>9} {:>7} {:>7} {:>7} {:>8} {:>8} {:>7} {:>7} {:>8} {:>7} {:>7} {:>7} {:>7} {:>8}",
            "seed",
            "final",
            "extinct",
            "win_mean",
            "min",
            "max",
            "max/min",
            "births",
            "d.starv",
            "d.age",
            "d.viol",
            "cons/at",
            "xfer/t",
            "harm/t",
            "sig/t",
            "encr/g",
            "ticks/s"
        )
    }

    /// `cz sweep`'s per-seed line.
    pub fn console_line(&self) -> String {
        format!(
            "{:>5} {:>7} {:>8} {:>9.1} {:>7} {:>7} {:>8.3} {:>8} {:>8} {:>7} {:>7} {:>8.4} {:>7.2} {:>7.2} {:>7.2} {:>7.4} {:>8.0}",
            self.seed,
            self.final_alive,
            self.extinct_text(),
            self.win_mean,
            self.win_min,
            self.win_max,
            self.win_max_over_min,
            self.births,
            self.deaths[DeathCause::Starvation.index()],
            self.deaths[DeathCause::OldAge.index()],
            self.deaths[DeathCause::Violence.index()],
            self.consumed_per_agent_tick,
            self.transfers_per_tick,
            self.harms_per_tick,
            self.signals_per_tick,
            self.encroach_rate,
            self.ticks_per_s
        )
    }

    /// `extinct_at` as the CSV writes it: the tick, or `-`.
    pub fn extinct_text(&self) -> String {
        self.extinct_at.map_or("-".to_string(), |t| t.to_string())
    }
}

/// A `series` row: the seed, the world's tick, population and food, the
/// interval's counts, the living by kind.
pub fn series_row(seed: u64, world: &World, interval: &Tally) -> String {
    format!(
        "{seed},{},{},{},{},{}",
        world.tick,
        world.agents.len(),
        world.resources.total_food(),
        interval.counts_csv(),
        Tally::alive_by_kind_csv(world)
    )
}

#[cfg(not(target_arch = "wasm32"))]
struct Clock(std::time::Instant);

#[cfg(not(target_arch = "wasm32"))]
impl Clock {
    fn start() -> Self {
        Clock(std::time::Instant::now())
    }

    fn secs(&self) -> f64 {
        self.0.elapsed().as_secs_f64()
    }
}

/// No wall clock under wasm32-unknown-unknown: the browser times its
/// workers itself.
#[cfg(target_arch = "wasm32")]
struct Clock;

#[cfg(target_arch = "wasm32")]
impl Clock {
    fn start() -> Self {
        Clock
    }

    fn secs(&self) -> f64 {
        0.0
    }
}

/// Builds the world of `(design, cell, seed)`, runs it `design.ticks`
/// ticks applying the cell's schedule, and writes the rows to `sink`.
pub fn run_cell(
    design: &Design,
    cell: &Cell,
    seed: u64,
    options: &RunOptions,
    sink: &mut dyn Sink,
) -> Result<CellRun, Error> {
    let ticks = design.ticks;
    let sample = design.sample_of(cell);
    let win_start = design.window_start();
    let policy = design.policy_of(cell);
    let mut world = design.build_world(cell, seed)?;
    let (mutation, states, epsilon, nomos, health_weight) = (
        world.config.mutation_enabled,
        world.config.learner_states,
        world.config.learner_epsilon_permille,
        world.config.nomos,
        world.config.learner_health_weight,
    );
    let (mut sum, mut min, mut max, mut count) = (0u64, u64::MAX, 0u64, 0u64);
    let mut extinct_at: Option<u64> = None;
    let mut interval = Tally::default();
    let mut window = Tally::default();
    // Row visits over the whole run (the window's are in `window`).
    let mut run = Tally::default();
    let mut q_at_window_start: Option<Vec<f64>> = None;
    let clock = Clock::start();
    for _ in 0..ticks {
        if world.tick % sample == 0 {
            sink.series(&series_row(seed, &world, &interval));
            interval = Tally::default();
        }
        if world.tick == win_start {
            q_at_window_start = mean_q_table(&world);
        }
        for s in &cell.schedule {
            if s.tick == world.tick {
                world.intervene(s.intervention).map_err(|e| {
                    Error(format!(
                        "cell {:?} seed {seed} tick {}: {e}",
                        cell.name, world.tick
                    ))
                })?;
            }
        }
        let alive_before = world.agents.len();
        let regrown_before = world.ledger.food_regrown;
        let r = world.step();
        if options.check {
            world.check_invariants().map_err(|e| {
                Error(format!(
                    "cell {:?} seed {seed}: invariant violated at tick {}: {e:?}",
                    cell.name, world.tick
                ))
            })?;
        }
        let regrown = world.ledger.food_regrown - regrown_before;
        let standing = world.resources.total_food();
        let by_kind = Tally::alive_by_kind(&world);
        interval.add(alive_before, &r, regrown, standing, &by_kind);
        run.add(alive_before, &r, regrown, standing, &by_kind);
        let alive = world.agents.len() as u64;
        if world.tick > win_start {
            window.add(alive_before, &r, regrown, standing, &by_kind);
            sum += alive;
            min = min.min(alive);
            max = max.max(alive);
            count += 1;
        }
        if extinct_at.is_none() && alive == 0 {
            extinct_at = Some(world.tick);
        }
    }
    let secs = clock.secs();
    sink.series(&series_row(seed, &world, &interval));
    let mean = sum as f64 / count.max(1) as f64;
    let ratio = if min > 0 {
        max as f64 / min as f64
    } else {
        f64::INFINITY
    };
    let l = &world.ledger;
    let extinct = extinct_at.map_or("-".to_string(), |t| t.to_string());
    let q_delta = match (q_at_window_start, mean_q_table(&world)) {
        (Some(a), Some(b)) if !a.is_empty() && a.len() == b.len() => Some(
            a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f64>()
                / a.len() as f64
                / Q_UNIT as f64,
        ),
        _ => None,
    };
    let cons = window.per_agent_tick(window.consumed);
    let (xfer, harm, sig) = (
        window.per_tick(window.transfers),
        window.per_tick(window.harms),
        window.per_tick(window.signals),
    );
    let encroach_rate = {
        let g = window.gathers_total();
        if g > 0 {
            window.encroachments_total() as f64 / g as f64
        } else {
            0.0
        }
    };
    let ticks_per_s = if secs > 0.0 { ticks as f64 / secs } else { 0.0 };
    let mut row = format!(
        "{seed},{policy},{},{extinct},{mean:.2},{min},{max},{ratio:.4},{},{},{},{},{},{states},{epsilon},{},{},{cons:.5},{:.5},{:.4},{xfer:.4},{harm:.4},{sig:.4},{:.4},{:.4},{:.4},{},{},{}",
        world.agents.len(),
        l.births,
        l.deaths_by_cause[DeathCause::Starvation.index()],
        l.deaths_by_cause[DeathCause::OldAge.index()],
        l.deaths_by_cause[DeathCause::Violence.index()],
        u8::from(mutation),
        window.ticks,
        window.agent_ticks,
        window.per_agent_tick(window.gathered),
        window.per_tick(window.births),
        window.per_tick(window.proposals),
        window.per_tick(window.acceptances),
        window.per_tick(window.rejections),
        window.deaths[DeathCause::Starvation.index()],
        window.deaths[DeathCause::OldAge.index()],
        window.deaths[DeathCause::Violence.index()],
    );
    for n in window.actions {
        let _ = write!(row, ",{:.5}", window.per_agent_tick(n));
    }
    let _ = write!(
        row,
        ",{},{},{},{}",
        opt_csv(hunger_mean(&world), 1),
        opt_csv(q_abs_mean(&world), 3),
        opt_csv(q_delta, 3),
        opt_csv(reflex_weight_drift(&world), 4),
    );
    // 4-1: the window's social facts, standing food and regrowth.
    let _ = write!(
        row,
        ",{},{health_weight},{:.4},{:.4},{encroach_rate:.5}",
        u8::from(nomos),
        window.per_tick(window.gathers_total()),
        window.per_tick(window.encroachments_total()),
    );
    for k in PHASE_3_KINDS {
        let _ = write!(row, ",{}", opt_csv(window.encroach_rate(k), 5));
    }
    let _ = write!(
        row,
        ",{:.4},{:.4},{:.1},{:.4}",
        window.per_tick(window.promises_due),
        window.per_tick(window.promises_broken),
        window.per_tick(window.food_standing),
        window.per_tick(window.regrown),
    );
    // 4-2: the rules in force at the end, the window's violations and
    // sanctions, and H1's S / S'.
    // 4-4: the Harm cost (an axis of EXP-003's grid), so that a grid's
    // CSVs describe their own cell.
    // 4-5: the enforcer's encroachment rate and the assembly over the
    // whole run (motions moved, carried, defeated; votes per tick in
    // the window), appended so no earlier column moves.
    // 4-6: the arm's share / perception / memory settings (so a CSV
    // describes its own arm) and the window's living by kind,
    // sanctions by the sanctioner's kind and reassigned births.
    let _ = write!(
        row,
        ",{},{:.4},{:.4},{},{},{},{},{},{},{},{},{},{:.4},{},{},{},{},{:.1},{:.1},{:.4},{:.4},{:.4},{:.0}",
        world.nomos.rules.len(),
        window.per_tick(window.violations),
        window.per_tick(window.sanctions),
        window.episodes_closed,
        window.episodes_sanctioned,
        opt_csv(window.s(), 5),
        opt_csv(window.s_prime(), 5),
        world.config.harm_cost,
        opt_csv(window.encroach_rate(PolicyKind::Enforcer), 5),
        l.motions,
        l.motions_carried,
        l.motions_defeated,
        window.per_tick(window.votes),
        world
            .config
            .enforcer_share_permille
            .map_or(String::new(), |p| p.to_string()),
        u8::from(world.config.perception.see_standing),
        world.config.perception.signal_radius,
        world.config.memory_slots,
        window.per_tick(window.alive_by_kind[PolicyKind::Learner.index()]),
        window.per_tick(window.alive_by_kind[PolicyKind::Enforcer.index()]),
        window.per_tick(window.sanctions_by_kind[PolicyKind::Learner.index()]),
        window.per_tick(window.sanctions_by_kind[PolicyKind::Enforcer.index()]),
        window.per_tick(window.births_reassigned),
        ticks_per_s
    );
    sink.summary(&row);
    let learners_alive = world.agents.iter().any(|a| a.policy == PolicyKind::Learner);
    if learners_alive {
        for (r, counts) in greedy_counts(&world, usize::from(states))
            .iter()
            .enumerate()
        {
            let bit = |b: u16| u8::from(r as u16 & b != 0);
            let learners: u32 = counts.iter().sum();
            // The row's bits in the order of `learner::state` (ADR-0014
            // D6: the norm bits sit below `UPPER` / `HEARD`).
            let mut line = format!(
                "{seed},{r},{},{},{},{},{},{},{},{},{},{},{learners}",
                bit(state::HUNGRY),
                bit(state::HAS_FOOD),
                bit(state::FOOD_HERE),
                bit(state::FOOD_IN_SIGHT),
                bit(state::ADJACENT),
                bit(state::OCCUPIED),
                bit(state::VIOLATOR_ADJACENT),
                bit(state::ACCUSED),
                bit(state::UPPER),
                bit(state::HEARD)
            );
            for c in counts {
                let _ = write!(line, ",{c}");
            }
            // Visits (ADR-0014 D10): decisions made in this row over the
            // whole run and over the window.
            let _ = write!(
                line,
                ",{},{}",
                run.rows.get(r).copied().unwrap_or(0),
                window.rows.get(r).copied().unwrap_or(0)
            );
            sink.qpolicy(&line);
        }
    }
    Ok(CellRun {
        cell: cell.name.clone(),
        seed,
        ticks,
        final_alive: world.agents.len(),
        extinct_at,
        win_mean: mean,
        win_min: min,
        win_max: max,
        win_max_over_min: ratio,
        births: l.births,
        deaths: l.deaths_by_cause,
        consumed_per_agent_tick: cons,
        transfers_per_tick: xfer,
        harms_per_tick: harm,
        signals_per_tick: sig,
        encroach_rate,
        ticks_per_s,
        ms: (secs * 1e3).round() as u64,
        dynamics_hash: world.dynamics_hash(),
        state_hash: world.state_hash(),
        learners_alive,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_series_header_is_the_tally_columns() {
        assert_eq!(
            SERIES_COLUMNS,
            format!(
                "seed,tick,alive,food,{},{}",
                Tally::COUNT_COLUMNS,
                Tally::ALIVE_COLUMNS
            )
        );
        assert_eq!(
            SERIES_COLUMNS.split(',').count(),
            4 + Tally::default().counts_csv().split(',').count()
                + Tally::ALIVE_COLUMNS.split(',').count()
        );
    }

    #[test]
    fn ticks_per_s_is_the_last_summary_column() {
        assert!(SUMMARY_COLUMNS.ends_with(",ticks_per_s"));
        let cols: Vec<&str> = SUMMARY_COLUMNS.split(',').collect();
        let mut unique = cols.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(cols.len(), unique.len(), "a column name repeats");
    }
}
