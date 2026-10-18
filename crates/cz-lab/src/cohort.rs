//! The frame of per-agent tracking (ADR-0015 D4; the body is task 5-2).
//!
//! D4 separates *removal* from *learning*: whether a rule lowers the
//! learners' encroachment rate because encroachers are killed or because
//! survivors encroach less. That needs, per agent, its policy, birth and
//! death, gathers, encroachments, and the sanctions it gave and received,
//! accumulated at run time from `TickReport` (never from the ledger's
//! events, which do not record gathers: 220 MB per 2,000 ticks, ADR-0013
//! 3-2). Agent ids are monotonic and never reused, so a `Vec` indexed by
//! id is the whole store — no hash map, id order is the iteration order.
//!
//! What this frame already reads is in every `TickReport` today: births
//! (`Event::Born`), deaths (`Event::Died`), sanctions (`Event::Sanctioned`,
//! always recorded). Gathers and encroachments per agent wait for
//! `TickReport.gatherers / encroachers` (5-2), as do the cohort columns of
//! the summary (`win_cohort_learners`, `win_e_learner_survivors`,
//! `win_encroach_share_removed`, …) and `cz lab run --agents`. `run_cell`
//! does not drive this frame yet: the summary's columns are a contract and
//! 5-2 adds the new ones in one step, before `ticks_per_s`.

use simulation_core::{AgentId, DeathCause, Event, PolicyKind, TickReport, World};

/// One agent's life as the tracking sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTally {
    pub id: AgentId,
    pub policy: PolicyKind,
    /// The tick the agent was born (the founders: the tick the tally
    /// started at).
    pub born: u64,
    /// The tick and cause of death, once dead.
    pub died: Option<(u64, DeathCause)>,
    /// 5-2: filled from `TickReport.gatherers`.
    pub gathers: u64,
    /// 5-2: filled from `TickReport.encroachers`.
    pub encroachments: u64,
    pub sanctions_received: u64,
    pub sanctions_given: u64,
}

impl AgentTally {
    pub fn alive(&self) -> bool {
        self.died.is_none()
    }
}

/// Every agent the run has seen, by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CohortTally {
    agents: Vec<Option<AgentTally>>,
}

impl CohortTally {
    /// Starts tracking with the world's living agents as founders.
    pub fn new(world: &World) -> Self {
        let mut t = Self::default();
        for a in &world.agents {
            t.insert(a.id, a.policy, world.tick);
        }
        t
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
            gathers: 0,
            encroachments: 0,
            sanctions_received: 0,
            sanctions_given: 0,
        });
    }

    fn get_mut(&mut self, id: AgentId) -> Option<&mut AgentTally> {
        self.agents.get_mut(id.0 as usize).and_then(Option::as_mut)
    }

    /// Reads one tick's report; `world` is the state after that tick (the
    /// newborn's policy is read from it, since `Event::Born` carries none).
    pub fn observe(&mut self, r: &TickReport, world: &World) {
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
                Event::Sanctioned { by, target, .. } => {
                    if let Some(a) = self.get_mut(*by) {
                        a.sanctions_given += 1;
                    }
                    if let Some(a) = self.get_mut(*target) {
                        a.sanctions_received += 1;
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
}
