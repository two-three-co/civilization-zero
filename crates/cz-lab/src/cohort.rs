//! Per-agent tracking (ADR-0015 D4, 5-2): the table that separates
//! *removal* from *learning*.
//!
//! EXP-004 lowered the learners' encroachment rate under a rule, and the
//! population figures said why: the encroachers were killed. Whether the
//! survivors encroach less — learning — cannot be read from a rate over a
//! population whose members change. D4 therefore tallies every agent: its
//! policy, birth and death, gathers, encroachments, and the sanctions it
//! gave and received, over the whole run and over the analysis window
//! (split at its midpoint), from the material every `TickReport` carries:
//! `gatherers` and `encroachers` (5-2), `Born`, `Died` and `Sanctioned`
//! (always recorded). Nothing is read from the ledger's events (gathers
//! are not recorded there: 220 MB per 2,000 ticks, ADR-0013 3-2) and the
//! kernel keeps no per-agent counter; the report dies with the tick.
//!
//! Agent ids are monotonic and never reused, so a `Vec` indexed by id is
//! the whole store: no hash map, and id order is the iteration order.
//!
//! The **window** `(start, end)` is the design's analysis window in
//! executed ticks, `start ≤ t < end` — the same ticks the summary's
//! `win_*` columns cover. Its **first half** is `start ≤ t < mid` with
//! `mid = start + (end − start) / 2` (the shorter half when the length is
//! odd). The **cohort** is every agent alive at the start of the window:
//! tracked and not yet dead when the report of tick `start` arrives.
//! [`CohortTally::metrics`] reduces the table to the eleven summary
//! columns of D4, all over the learners:
//!
//! - `win_cohort_learners`: learners in the cohort; `win_cohort_survived`:
//!   of them, in the world at the end of the run; `win_cohort_died_violence`:
//!   of them, dead of violence inside the window.
//! - `win_e_learner_survivors`: Σ encroachments / Σ gathers of the
//!   surviving cohort learners inside the window (the learning reading).
//!   `win_e_learner_removed`: the same ratio over every learner that died
//!   of violence inside the window (cohort or born in it), up to its death.
//!   `win_encroach_share_removed`: the share of all learners' window
//!   encroachments made by those removed learners (the removal reading).
//! - `win_e_survivors_first_half` / `_second_half`: the surviving cohort
//!   learners' rate in each half of the window; `win_e_survivors_delta_mean`:
//!   the mean over those survivors that gathered in both halves of (second
//!   half − first half), each agent its own pair.
//! - `win_sanctioned_learners`: learners sanctioned inside the window;
//!   `win_sanctioned_then_died`: of them, dead of violence inside the
//!   window within `standing_ticks` of their last sanction (the horizon of
//!   the rule that sanction answered).
//!
//! A rate over no gather is undefined and written as the empty CSV cell,
//! like `win_encroach_rate_learner`. Agents put into the world by
//! `SpawnAgents` carry no `Born` event and are taken from the world at the
//! first report after they appear (born = that tick); `RemoveAgent` is not
//! a death, so a removed agent stays "alive" in the table — the survivors
//! are therefore counted from the final world, not from the table.

use simulation_core::{AgentId, DeathCause, Event, PolicyKind, TickReport, World};

/// Header of `<cell>_agents.csv` (`cz lab run --agents`): one row per
/// agent the run saw. `died` / `cause` / `last_sanctioned` are empty when
/// there is nothing to say; `cohort` is 1 for an agent alive at the start
/// of the window.
pub const AGENTS_COLUMNS: &str = "seed,id,policy,born,died,cause,cohort,gathers,encroachments,sanctions_received,sanctions_given,win_gathers,win_encroachments,win_sanctions_received,win_sanctions_given,win_gathers_first_half,win_encroachments_first_half,win_gathers_second_half,win_encroachments_second_half,last_sanctioned";

/// Counts over a stretch of ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub gathers: u64,
    pub encroachments: u64,
    pub sanctions_received: u64,
    pub sanctions_given: u64,
}

impl Counts {
    fn add(&mut self, o: &Counts) {
        self.gathers += o.gathers;
        self.encroachments += o.encroachments;
        self.sanctions_received += o.sanctions_received;
        self.sanctions_given += o.sanctions_given;
    }

    /// `encroachments / gathers`, `None` without a gather.
    pub fn encroach_rate(&self) -> Option<f64> {
        (self.gathers > 0).then(|| self.encroachments as f64 / self.gathers as f64)
    }
}

/// One agent's life as the tracking sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTally {
    pub id: AgentId,
    pub policy: PolicyKind,
    /// The state tick the agent first existed at: the founders' is the
    /// tick the tally started at, a child's is its birth tick + 1.
    pub born: u64,
    /// The executed tick and cause of death, once dead.
    pub died: Option<(u64, DeathCause)>,
    /// Over the whole run.
    pub run: Counts,
    /// Alive at the start of the window.
    pub cohort: bool,
    /// Inside the window, split at its midpoint: `[first half, second half]`.
    pub win: [Counts; 2],
    /// The last sanction received: its tick and the horizon
    /// (`standing_ticks`) of the rule it answered — 0 when that rule was
    /// no longer in force after the tick (replaced in the same tick).
    pub last_sanctioned: Option<(u64, u16)>,
}

fn cause_name(c: DeathCause) -> &'static str {
    match c {
        DeathCause::Starvation => "starvation",
        DeathCause::OldAge => "old_age",
        DeathCause::Violence => "violence",
    }
}

impl AgentTally {
    pub fn alive(&self) -> bool {
        self.died.is_none()
    }

    /// The window's counts (both halves).
    pub fn window(&self) -> Counts {
        let mut c = self.win[0];
        c.add(&self.win[1]);
        c
    }

    /// Dead of `cause` at a tick `start ≤ t < end`.
    fn died_of_in(&self, cause: DeathCause, start: u64, end: u64) -> bool {
        matches!(self.died, Some((t, c)) if c == cause && t >= start && t < end)
    }

    /// The agent's row of [`AGENTS_COLUMNS`].
    pub fn csv(&self, seed: u64) -> String {
        let w = self.window();
        let (died, cause) = match self.died {
            Some((t, c)) => (t.to_string(), cause_name(c).to_string()),
            None => (String::new(), String::new()),
        };
        format!(
            "{seed},{},{},{},{died},{cause},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.id.0,
            self.policy.name(),
            self.born,
            u8::from(self.cohort),
            self.run.gathers,
            self.run.encroachments,
            self.run.sanctions_received,
            self.run.sanctions_given,
            w.gathers,
            w.encroachments,
            w.sanctions_received,
            w.sanctions_given,
            self.win[0].gathers,
            self.win[0].encroachments,
            self.win[1].gathers,
            self.win[1].encroachments,
            self.last_sanctioned
                .map_or(String::new(), |(t, _)| t.to_string())
        )
    }
}

/// The eleven cohort columns of the summary (ADR-0015 D4), in the order of
/// [`CohortMetrics::COLUMNS`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CohortMetrics {
    pub cohort_learners: u64,
    pub cohort_survived: u64,
    pub cohort_died_violence: u64,
    pub e_learner_survivors: Option<f64>,
    pub e_learner_removed: Option<f64>,
    pub encroach_share_removed: Option<f64>,
    pub e_survivors_first_half: Option<f64>,
    pub e_survivors_second_half: Option<f64>,
    pub e_survivors_delta_mean: Option<f64>,
    pub sanctioned_learners: u64,
    pub sanctioned_then_died: u64,
}

impl CohortMetrics {
    /// The summary's cohort columns, appended before `ticks_per_s` in 5-2.
    pub const COLUMNS: &'static str = "win_cohort_learners,win_cohort_survived,win_cohort_died_violence,win_e_learner_survivors,win_e_learner_removed,win_encroach_share_removed,win_e_survivors_first_half,win_e_survivors_second_half,win_e_survivors_delta_mean,win_sanctioned_learners,win_sanctioned_then_died";

    /// The values in the order of [`Self::COLUMNS`]; rates with five
    /// decimals, undefined ones empty.
    pub fn csv(&self) -> String {
        let opt = |v: Option<f64>| v.map_or(String::new(), |x| format!("{x:.5}"));
        format!(
            "{},{},{},{},{},{},{},{},{},{},{}",
            self.cohort_learners,
            self.cohort_survived,
            self.cohort_died_violence,
            opt(self.e_learner_survivors),
            opt(self.e_learner_removed),
            opt(self.encroach_share_removed),
            opt(self.e_survivors_first_half),
            opt(self.e_survivors_second_half),
            opt(self.e_survivors_delta_mean),
            self.sanctioned_learners,
            self.sanctioned_then_died
        )
    }
}

/// Every agent the run has seen, by id, and the window it tallies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CohortTally {
    agents: Vec<Option<AgentTally>>,
    /// Executed ticks `start ≤ t < end`.
    start: u64,
    end: u64,
}

impl CohortTally {
    /// Starts tracking with the world's living agents as founders, over
    /// the window `(start, end)` of executed ticks (`Design::window()`).
    pub fn new(world: &World, window: (u64, u64)) -> Self {
        let mut t = Self {
            agents: Vec::with_capacity(world.next_agent_id as usize),
            start: window.0,
            end: window.1,
        };
        for a in &world.agents {
            t.insert(a.id, a.policy, world.tick);
        }
        t
    }

    /// The window as executed ticks `(start, end)`.
    pub fn window(&self) -> (u64, u64) {
        (self.start, self.end)
    }

    /// The tick that opens the second half of the window.
    pub fn midpoint(&self) -> u64 {
        self.start + (self.end.saturating_sub(self.start)) / 2
    }

    fn insert(&mut self, id: AgentId, policy: PolicyKind, born: u64) {
        let i = id.0 as usize;
        if self.agents.len() <= i {
            self.agents.resize(i + 1, None);
        }
        self.agents[i] = Some(AgentTally {
            id,
            policy,
            born,
            died: None,
            run: Counts::default(),
            cohort: false,
            win: [Counts::default(); 2],
            last_sanctioned: None,
        });
    }

    fn get_mut(&mut self, id: AgentId) -> Option<&mut AgentTally> {
        self.agents.get_mut(id.0 as usize).and_then(Option::as_mut)
    }

    /// Reads one tick's report; `world` is the state after that tick (a
    /// newborn's policy is read from it, since `Event::Born` carries none,
    /// and so is a sanction's horizon).
    pub fn observe(&mut self, r: &TickReport, world: &World) {
        let tick = r.tick;
        // The cohort: whoever is alive when the window opens (a window of
        // no ticks never opens).
        if tick == self.start && self.start < self.end {
            for a in self.agents.iter_mut().flatten() {
                if a.died.is_none() {
                    a.cohort = true;
                }
            }
        }
        // Agents the world has and the table does not (`SpawnAgents`).
        if (world.next_agent_id as usize) > self.agents.len() {
            let from = self.agents.len() as u64;
            let spawned: Vec<(AgentId, PolicyKind)> = world
                .agents
                .iter()
                .filter(|a| a.id.0 >= from)
                .map(|a| (a.id, a.policy))
                .collect();
            for (id, policy) in spawned {
                self.insert(id, policy, tick);
            }
            self.agents.resize(world.next_agent_id as usize, None);
        }
        let half =
            (tick >= self.start && tick < self.end).then(|| usize::from(tick >= self.midpoint()));
        for id in &r.gatherers {
            if let Some(a) = self.get_mut(*id) {
                a.run.gathers += 1;
                if let Some(h) = half {
                    a.win[h].gathers += 1;
                }
            }
        }
        for (gatherer, _) in &r.encroachers {
            if let Some(a) = self.get_mut(*gatherer) {
                a.run.encroachments += 1;
                if let Some(h) = half {
                    a.win[h].encroachments += 1;
                }
            }
        }
        for e in &r.events {
            match e {
                Event::Born { tick, child, .. } => {
                    let policy = world
                        .agents
                        .binary_search_by_key(child, |a| a.id)
                        .ok()
                        .map(|i| world.agents[i].policy);
                    if let Some(policy) = policy {
                        self.insert(*child, policy, *tick + 1);
                    }
                }
                Event::Died {
                    tick, agent, cause, ..
                } => {
                    if let Some(a) = self.get_mut(*agent) {
                        a.died = Some((*tick, *cause));
                    }
                }
                Event::Sanctioned {
                    tick,
                    by,
                    target,
                    rule,
                    ..
                } => {
                    let horizon = world.nomos.rule(*rule).map_or(0, |r| r.norm.standing_ticks);
                    if let Some(a) = self.get_mut(*by) {
                        a.run.sanctions_given += 1;
                        if let Some(h) = half {
                            a.win[h].sanctions_given += 1;
                        }
                    }
                    if let Some(a) = self.get_mut(*target) {
                        a.run.sanctions_received += 1;
                        if let Some(h) = half {
                            a.win[h].sanctions_received += 1;
                        }
                        a.last_sanctioned = Some((*tick, horizon));
                    }
                }
                _ => {}
            }
        }
    }

    pub fn get(&self, id: AgentId) -> Option<&AgentTally> {
        self.agents.get(id.0 as usize).and_then(Option::as_ref)
    }

    /// Every tracked agent in id order.
    pub fn iter(&self) -> impl Iterator<Item = &AgentTally> {
        self.agents.iter().filter_map(Option::as_ref)
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The rows of `<cell>_agents.csv` for this world, in id order.
    pub fn rows(&self, seed: u64) -> Vec<String> {
        self.iter().map(|a| a.csv(seed)).collect()
    }

    /// The cohort columns (module docs); `world` is the state at the end
    /// of the run, whose living are the survivors.
    pub fn metrics(&self, world: &World) -> CohortMetrics {
        let (start, end) = (self.start, self.end);
        let mut m = CohortMetrics::default();
        // Survivors: cohort learners still in the world. Both sequences
        // ascend by id, so one merge walk finds them.
        let mut living = world.agents.iter().map(|a| a.id).peekable();
        let mut survivors = Counts::default();
        let mut halves = [Counts::default(); 2];
        let mut deltas = (0.0f64, 0u64);
        let mut removed = Counts::default();
        let mut all_learners = Counts::default();
        for a in self.iter() {
            if a.policy != PolicyKind::Learner {
                continue;
            }
            while living.peek().is_some_and(|&id| id < a.id) {
                living.next();
            }
            let in_world = living.peek() == Some(&a.id);
            let w = a.window();
            all_learners.add(&w);
            let violent = a.died_of_in(DeathCause::Violence, start, end);
            if violent {
                removed.add(&w);
            }
            if a.cohort {
                m.cohort_learners += 1;
                if in_world {
                    m.cohort_survived += 1;
                    survivors.add(&w);
                    halves[0].add(&a.win[0]);
                    halves[1].add(&a.win[1]);
                    if let (Some(e0), Some(e1)) =
                        (a.win[0].encroach_rate(), a.win[1].encroach_rate())
                    {
                        deltas.0 += e1 - e0;
                        deltas.1 += 1;
                    }
                }
                if violent {
                    m.cohort_died_violence += 1;
                }
            }
            if w.sanctions_received > 0 {
                m.sanctioned_learners += 1;
                if violent {
                    if let (Some((died, _)), Some((last, horizon))) = (a.died, a.last_sanctioned) {
                        if died >= last && died - last <= u64::from(horizon) {
                            m.sanctioned_then_died += 1;
                        }
                    }
                }
            }
        }
        m.e_learner_survivors = survivors.encroach_rate();
        m.e_learner_removed = removed.encroach_rate();
        m.encroach_share_removed = (all_learners.encroachments > 0)
            .then(|| removed.encroachments as f64 / all_learners.encroachments as f64);
        m.e_survivors_first_half = halves[0].encroach_rate();
        m.e_survivors_second_half = halves[1].encroach_rate();
        m.e_survivors_delta_mean = (deltas.1 > 0).then(|| deltas.0 / deltas.1 as f64);
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agents_header_matches_the_row() {
        let a = AgentTally {
            id: AgentId(7),
            policy: PolicyKind::Learner,
            born: 3,
            died: Some((40, DeathCause::Violence)),
            run: Counts {
                gathers: 10,
                encroachments: 2,
                sanctions_received: 5,
                sanctions_given: 1,
            },
            cohort: true,
            win: [
                Counts {
                    gathers: 4,
                    encroachments: 1,
                    sanctions_received: 2,
                    sanctions_given: 0,
                },
                Counts {
                    gathers: 3,
                    encroachments: 1,
                    sanctions_received: 3,
                    sanctions_given: 1,
                },
            ],
            last_sanctioned: Some((40, 32)),
        };
        let row = a.csv(9);
        assert_eq!(row.split(',').count(), AGENTS_COLUMNS.split(',').count());
        assert_eq!(
            row,
            "9,7,learner,3,40,violence,1,10,2,5,1,7,2,5,1,4,1,3,1,40"
        );
        let alive = AgentTally {
            died: None,
            last_sanctioned: None,
            cohort: false,
            ..a
        };
        assert_eq!(alive.csv(9), "9,7,learner,3,,,0,10,2,5,1,7,2,5,1,4,1,3,1,");
    }

    #[test]
    fn the_cohort_header_matches_the_row() {
        let m = CohortMetrics::default();
        assert_eq!(
            m.csv().split(',').count(),
            CohortMetrics::COLUMNS.split(',').count()
        );
        assert_eq!(m.csv(), "0,0,0,,,,,,,0,0");
        let m = CohortMetrics {
            e_learner_survivors: Some(2.0 / 14.0),
            e_survivors_delta_mean: Some(-0.5),
            ..m
        };
        assert_eq!(m.csv(), "0,0,0,0.14286,,,,,-0.50000,0,0");
    }

    #[test]
    fn the_midpoint_gives_the_shorter_first_half() {
        let w = World::new(WorldConfig {
            initial_agents: 0,
            patch_count: 0,
            ..WorldConfig::default()
        })
        .unwrap();
        assert_eq!(CohortTally::new(&w, (4, 12)).midpoint(), 8);
        assert_eq!(CohortTally::new(&w, (4, 13)).midpoint(), 8);
        assert_eq!(CohortTally::new(&w, (10_000, 20_000)).midpoint(), 15_000);
        assert_eq!(CohortTally::new(&w, (5, 5)).midpoint(), 5);
    }

    use simulation_core::WorldConfig;
}
