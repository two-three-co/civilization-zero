//! World state and the deterministic transition `S(t+1) = F(S(t), A(t), E(t), R(t))`.
//!
//! One tick is executed in fixed phases (ADR-0002, ADR-0008):
//!
//! 1. **Decide**  – every living agent produces an `Intent` from a read-only
//!    view of `S(t)`. Pure; order-independent; parallelisable.
//! 2. **Consume** – private inventory → energy conversion.
//! 3. **Gather**  – resolved against `S(t)` positions. Claimants on the same
//!    cell are ordered by a seeded lottery key and served sequentially until
//!    the cell is empty, so food can never be taken twice. With
//!    `config.nomos`, each cell that saw a gather is then judged for
//!    encroachment (ADR-0014 D3): a gatherer that arrived strictly later
//!    than the cell's first occupant encroached on it — counted in the
//!    ledger and remembered by the first occupant, nothing else.
//!    - *3b. Interact* – Transfer / Store / Harm / Signal, resolved at `S(t)`
//!      positions in actor (id) order (ADR-0013 D3, D5). Negotiation
//!      messages are delivered here: the addressee must be another living
//!      agent within `signal_radius`, an `Accept` must name a proposal the
//!      acceptor remembers, and every delivered message becomes a ledger
//!      event. Nothing enforces what was promised (3-4).
//!    - *3c. Audit* – with `config.nomos`, this tick's transfers are booked
//!      against the open deals and the deals whose deadline is this tick
//!      are settled as kept or broken (ADR-0014 D3). Counted, not enforced.
//! 4. **Move**    – all moves applied simultaneously (cells are unbounded,
//!    so moves never conflict). With `config.nomos` a mover's `arrived`
//!    becomes `tick + 1`.
//! 5. **Upkeep**  – metabolism + action costs, then Rest healing, then
//!    starvation damage, then ageing.
//!    - *5a. Learn* – a `Learner`'s previous step is credited with this
//!      tick's energy change as reward (plus `learner_health_weight` times
//!      its health change, ADR-0014 D3) and the row it just decided in as
//!      `s'` (`Q += (r + γ·max Q[s'] − Q) >> 4`), and this step is recorded
//!      in `Agent.last_step` for the next tick (ADR-0013 D6, 3-7). Before
//!      death and birth, so the energy handed to a child is not a penalty.
//!    - *5b. Memory* – the kernel appends what each agent gave, received,
//!      suffered, witnessed, heard, was proposed, had accepted or rejected,
//!      promised, and (with `nomos`) was encroached on this tick to
//!      `Agent.recent` (ADR-0013 D4).
//! 6. **Death**   – `health <= 0` dies of `Starvation`; otherwise
//!    `age >= max_age` dies of `OldAge`. Carried food is dropped on the cell,
//!    remaining energy is written off in the ledger. The dead do not reach
//!    the birth phase.
//! 7. **Birth**   – survivors with `energy >= birth_threshold` are visited in
//!    id order; each transfers `child_energy` to one child. Child ids are
//!    `next_agent_id, next_agent_id + 1, …` in that same order, so "parent
//!    with the smaller id gets the smaller child id" is the specification.
//!    Children are appended (ids ascend, canonical order is kept) and act
//!    for the first time in the next tick.
//! 8. **Environment** – regrowth every `regrow_interval` ticks.

use crate::action::{Action, Failure, Outcome};
use crate::agent::{Agent, AgentId, Memory};
use crate::config::{ConfigError, WorldConfig};
use crate::event::{DeathCause, Event};
use crate::intervention::{Intervention, InterventionError, ParamsMode, Who};
use crate::memory::{remember, Interaction, InteractionKind, Message, Signal, Terms};
use crate::nomos::{self, Closed, Grounds, Nomos, NormIndex, NormKind, OpenDeal, Via};
use crate::policy::learner::{self, LearnerStep};
use crate::policy::{decide, Intent, Observation, PolicyKind, PolicyParams, PolicyTable};
use crate::resources::ResourceGrid;
use crate::rng::{RngStream, Stream};
use crate::space::{Bounds, Pos};
use crate::spatial::SpatialIndex;
use serde::{Deserialize, Serialize};

/// Running totals that make conservation laws checkable at any tick.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ledger {
    pub food_initial: u64,
    pub food_regrown: u64,
    pub food_consumed: u64,
    pub energy_initial: i64,
    pub energy_gained: i64,
    pub energy_spent: i64,
    pub energy_written_off: i64,
    pub births: u64,
    pub deaths: u64,
    pub deaths_by_cause: [u64; DeathCause::COUNT],
    /// Social actions (ADR-0013 D3): counts and food moved.
    pub transfers: u64,
    pub food_transferred: u64,
    pub food_stored: u64,
    pub harms: u64,
    /// `Say` signals made.
    pub signals: u64,
    /// Negotiation messages *delivered* (ADR-0013 D5, 3-4); undeliverable
    /// ones count as failures, not here.
    pub proposals: u64,
    pub acceptances: u64,
    pub rejections: u64,
    /// Net food added (+) or removed (-) by interventions.
    pub food_intervened: i64,
    /// Net energy added (+) or removed (-) by interventions.
    pub energy_intervened: i64,
    pub interventions: u64,
    /// Social facts (ADR-0014 D3). `gathers` counts every Gather that took
    /// at least one unit, in every world; the other three are only ever
    /// non-zero with `config.nomos`. `default` so older ledgers still load.
    #[serde(default)]
    pub gathers: u64,
    /// Gathers on a cell whose first occupant arrived strictly earlier than
    /// the gatherer. A fact, not a violation.
    #[serde(default)]
    pub encroachments: u64,
    /// Accepted deals settled at their deadline …
    #[serde(default)]
    pub promises_due: u64,
    /// … of which at least one side fell short.
    #[serde(default)]
    pub promises_broken: u64,
    /// Institutions (ADR-0014 D4, 4-2), by `NormKind::index`; only ever
    /// non-zero with `config.nomos` and a rule in force. `violations`
    /// counts every judged violation (whether or not `Event::Violated` is
    /// recorded), `sanctions` every harm or rejection read as a sanction,
    /// `episodes_closed` every episode of accusation that ended (deadline,
    /// death, repeal) and `episodes_sanctioned` those that at least one
    /// sanction answered. H1's conditional probabilities are
    /// `episodes_sanctioned / episodes_closed` and `sanctions / violations`.
    #[serde(default)]
    pub violations: [u64; NormKind::COUNT],
    #[serde(default)]
    pub sanctions: [u64; NormKind::COUNT],
    #[serde(default)]
    pub episodes_closed: [u64; NormKind::COUNT],
    #[serde(default)]
    pub episodes_sanctioned: [u64; NormKind::COUNT],
}

impl Ledger {
    /// Books closed episodes (ADR-0014 D4), in the ledger and, during a
    /// tick, in the tick's report.
    fn book_closed(&mut self, closed: &[Closed], report: Option<&mut TickReport>) {
        let mut tick_closed = [0u32; NormKind::COUNT];
        let mut tick_sanctioned = [0u32; NormKind::COUNT];
        for c in closed {
            let k = c.kind.index();
            self.episodes_closed[k] += 1;
            tick_closed[k] += 1;
            if c.sanctioned {
                self.episodes_sanctioned[k] += 1;
                tick_sanctioned[k] += 1;
            }
        }
        if let Some(r) = report {
            for k in 0..NormKind::COUNT {
                r.episodes_closed[k] += tick_closed[k];
                r.episodes_sanctioned[k] += tick_sanctioned[k];
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TickReport {
    /// Tick that was executed (state is now at `tick + 1`).
    pub tick: u64,
    pub alive: u32,
    pub births: u32,
    pub deaths: u32,
    pub deaths_by_cause: [u32; DeathCause::COUNT],
    pub actions: [u32; Action::KIND_COUNT],
    pub failures: [u32; Failure::COUNT],
    pub food_gathered: u64,
    pub food_consumed: u64,
    pub transfers: u32,
    pub food_transferred: u64,
    pub food_stored: u64,
    pub harms: u32,
    pub signals: u32,
    pub proposals: u32,
    pub acceptances: u32,
    pub rejections: u32,
    /// Successful gathers by the gatherer's `PolicyKind` (index order of
    /// `PolicyKind::ALL`), in every world.
    pub gathers: [u32; PolicyKind::COUNT],
    /// Encroachments by the gatherer's kind (ADR-0014 D3; zero without
    /// `config.nomos`). `encroachments / gathers` per kind is the
    /// encroachment rate the experiments read.
    pub encroachments: [u32; PolicyKind::COUNT],
    /// Deals settled this tick (deadline reached), in acceptance order,
    /// with what was delivered; `OpenDeal::kept` says how each ended.
    /// Empty without `config.nomos`.
    pub promises_settled: Vec<OpenDeal>,
    /// How many learners decided in each Q row this tick: `learner_states`
    /// counters, or empty when no learner decided (ADR-0014 D10: the row
    /// visits the sweep accumulates; not state, so not hashed).
    pub learner_rows: Vec<u32>,
    /// Institutions this tick (ADR-0014 D4), by `NormKind::index`: the
    /// ledger's four counters as deltas. Zero without a rule in force.
    pub violations: [u32; NormKind::COUNT],
    pub sanctions: [u32; NormKind::COUNT],
    pub episodes_closed: [u32; NormKind::COUNT],
    pub episodes_sanctioned: [u32; NormKind::COUNT],
    /// Events in the order they happened (ADR-0014 D9): transfers, harms,
    /// delivered negotiation messages and signals (if recorded) in actor
    /// order; `Sanctioned` in actor order; `Violated` (if recorded:
    /// encroachments in cell then gatherer order, harms in actor order,
    /// broken promises in deal order); `Acted` (if recorded); then deaths
    /// in id order, then births in parent-id order.
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct World {
    pub config: WorldConfig,
    pub tick: u64,
    pub resources: ResourceGrid,
    /// Living agents, strictly ascending by id (canonical order).
    pub agents: Vec<Agent>,
    /// Next id to hand out. Monotonic; never reused.
    pub next_agent_id: u64,
    pub ledger: Ledger,
    /// Social facts tracked while `config.nomos` is on (ADR-0014 D2);
    /// empty otherwise, and then absent from `dynamics_hash`. `default` so
    /// older JSON still parses.
    #[serde(default)]
    pub nomos: Nomos,
}

impl World {
    /// Generate a world where every agent uses the forager policy.
    pub fn new(config: WorldConfig) -> Result<Self, ConfigError> {
        Self::generate(config, |_| PolicyKind::Forager)
    }

    pub fn generate(
        config: WorldConfig,
        policy_for: impl Fn(u32) -> PolicyKind,
    ) -> Result<Self, ConfigError> {
        config.validate()?;
        let bounds = Bounds {
            width: config.width,
            height: config.height,
        };
        let mut resources = ResourceGrid::empty(bounds);
        for i in 0..config.patch_count {
            let mut rng = RngStream::new(config.seed, 0, i as u64, Stream::WorldGen);
            let c = Pos::new(
                rng.below(config.width as u64) as u16,
                rng.below(config.height as u64) as u16,
            );
            let r = config.patch_radius as u64;
            for p in bounds.diamond(c, config.patch_radius) {
                let d = c.manhattan(p) as u64;
                let cap = (config.max_cell_capacity as u64 * (r + 1 - d) / (r + 1)) as u32;
                let idx = bounds.index(p);
                resources.capacity[idx] = resources.capacity[idx].max(cap);
            }
        }
        resources.food.clone_from(&resources.capacity);

        let agents: Vec<Agent> = (0..config.initial_agents)
            .map(|i| {
                let mut rng = RngStream::new(config.seed, 0, i as u64, Stream::AgentSpawn);
                let pos = Pos::new(
                    rng.below(config.width as u64) as u16,
                    rng.below(config.height as u64) as u16,
                );
                // Spread the founding cohort over the whole life span so it
                // does not die of old age in a single tick (ADR-0008).
                let age = config.max_age.map_or(0, |m| rng.below(m));
                let policy = policy_for(i);
                Agent {
                    id: AgentId(i as u64),
                    pos,
                    energy: config.initial_energy,
                    health: config.max_health,
                    inventory: 0,
                    age,
                    policy,
                    params: PolicyParams::for_kind(policy, &config),
                    memory: Memory::default(),
                    last_signal: None,
                    recent: Vec::new(),
                    last_step: None,
                    // The founding cohort is the oldest everywhere it
                    // stands (ADR-0014 D3).
                    arrived: None,
                }
            })
            .collect();

        let ledger = Ledger {
            food_initial: resources.total_food(),
            energy_initial: config.initial_energy * agents.len() as i64,
            ..Ledger::default()
        };
        // The assembly starts from the config's charter where institutions
        // exist at all (ADR-0014 D5); off, the state stays at its default
        // and adds no byte to the hash.
        let nomos = if config.nomos {
            Nomos {
                charter: config.charter,
                ..Nomos::default()
            }
        } else {
            Nomos::default()
        };
        Ok(Self {
            next_agent_id: agents.len() as u64,
            config,
            tick: 0,
            resources,
            agents,
            ledger,
            nomos,
        })
    }

    pub fn bounds(&self) -> Bounds {
        self.resources.bounds
    }

    /// The spatial index of the current tick (ADR-0013 D2): derived from
    /// `agents`, rebuilt on demand, never stored.
    pub fn spatial_index(&self) -> SpatialIndex {
        SpatialIndex::build(self.bounds(), &self.agents)
    }

    /// Who stands accused at this tick's start (ADR-0014 D2): derived from
    /// `nomos`, rebuilt per tick right after the spatial index, never
    /// stored. Empty — and free to query — with `nomos` off or nothing in
    /// force.
    pub fn norm_index(&self) -> NormIndex {
        if self.config.nomos {
            NormIndex::build(&self.agents, &self.nomos)
        } else {
            NormIndex::empty()
        }
    }

    /// The read-only view `agent` decides from, given this tick's index.
    pub fn observation<'a>(&'a self, agent: &'a Agent, index: &'a SpatialIndex) -> Observation<'a> {
        Observation {
            agent,
            resources: &self.resources,
            bounds: self.bounds(),
            config: &self.config,
            tick: self.tick,
            agents: &self.agents,
            index,
        }
    }

    fn decide_one(&self, a: &Agent, index: &SpatialIndex) -> Intent {
        let obs = self.observation(a, index);
        let mut rng = RngStream::new(self.config.seed, self.tick, a.id.0, Stream::Decide);
        decide(&obs, &mut rng)
    }

    /// Phase 1 of a tick: compute every agent's intent without mutating state.
    pub fn decide_all(&self) -> Vec<Intent> {
        let index = self.spatial_index();
        self.decide_all_with(&index)
    }

    /// `decide_all` over an index already built for this tick.
    pub fn decide_all_with(&self, index: &SpatialIndex) -> Vec<Intent> {
        self.agents
            .iter()
            .map(|a| self.decide_one(a, index))
            .collect()
    }

    #[cfg(feature = "parallel")]
    pub fn decide_all_parallel(&self) -> Vec<Intent> {
        let index = self.spatial_index();
        self.decide_all_parallel_with(&index)
    }

    #[cfg(feature = "parallel")]
    pub fn decide_all_parallel_with(&self, index: &SpatialIndex) -> Vec<Intent> {
        use rayon::prelude::*;
        // `collect` on an indexed parallel iterator preserves input order;
        // the index is built once and only read.
        self.agents
            .par_iter()
            .map(|a| self.decide_one(a, index))
            .collect()
    }

    /// Advance one tick with the serial decision phase. The tick's spatial
    /// index is built once and shared by Decide and Apply.
    pub fn step(&mut self) -> TickReport {
        let index = self.spatial_index();
        let intents = self.decide_all_with(&index);
        self.apply_with(intents, &index)
    }

    /// Advance one tick deciding in parallel. Must equal `step` bit-for-bit.
    #[cfg(feature = "parallel")]
    pub fn step_parallel(&mut self) -> TickReport {
        let index = self.spatial_index();
        let intents = self.decide_all_parallel_with(&index);
        self.apply_with(intents, &index)
    }

    pub fn run(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.step();
        }
    }

    /// Phases 2-8. `intents[i]` belongs to `self.agents[i]`. Builds the
    /// tick's spatial index itself; `step` shares one with Decide.
    pub fn apply(&mut self, intents: Vec<Intent>) -> TickReport {
        let index = self.spatial_index();
        self.apply_with(intents, &index)
    }

    /// `apply` with the index of the tick's start positions, which locates
    /// the witnesses of a harm and the listeners of a signal.
    pub fn apply_with(&mut self, intents: Vec<Intent>, index: &SpatialIndex) -> TickReport {
        assert_eq!(intents.len(), self.agents.len(), "one intent per agent");
        assert_eq!(index.len(), self.agents.len(), "index built for this tick");
        let cfg = &self.config;
        let bounds = self.resources.bounds;
        let tick = self.tick;
        let n = self.agents.len();
        let mut outcomes = vec![Outcome::Ok; n];
        let mut report = TickReport {
            tick,
            ..TickReport::default()
        };

        let rows = cfg.learner_states as usize;
        for (a, it) in self.agents.iter_mut().zip(&intents) {
            a.memory = it.memory;
            report.actions[it.action.kind_index()] += 1;
            // Row visits (ADR-0014 D10): a histogram only in ticks in
            // which some learner decided; a row outside the table (a
            // hand-built intent) is not counted.
            if let Some(s) = it.state {
                if report.learner_rows.is_empty() {
                    report.learner_rows.resize(rows, 0);
                }
                if let Some(c) = report.learner_rows.get_mut(s as usize) {
                    *c += 1;
                }
            }
        }
        // Energies at S(t), for the learners' rewards (phase 5a). Only
        // worlds with learners pay for the copy; health too only when the
        // reward has a health term (ADR-0014 D3).
        let start_energy: Vec<i64> = if intents.iter().any(|it| it.state.is_some()) {
            self.agents.iter().map(|a| a.energy).collect()
        } else {
            Vec::new()
        };
        let start_health: Vec<i64> = if cfg.learner_health_weight > 0 && !start_energy.is_empty() {
            self.agents.iter().map(|a| a.health).collect()
        } else {
            Vec::new()
        };
        // What agents will remember of this tick, in order of occurrence
        // (ADR-0013 D4). Applied in phase 5b.
        let mut notes: Vec<(usize, Interaction)> = Vec::new();
        // Institutions (ADR-0014 D2, D4): who stood accused at S(t) — the
        // view a sanction is judged against — and the rules in force,
        // which no phase of a tick changes. Empty and free without rules.
        let norms = self.norm_index();
        let rule_encroach = norms.rule(NormKind::Encroach).copied();
        let rule_promise = norms.rule(NormKind::BrokenPromise).copied();
        let rule_harm = norms.rule(NormKind::Harm).copied();
        // This tick's judgements, in the order of ADR-0014 D9: sanctions in
        // actor order; violations as encroachments (cell, then gatherer
        // order), harms (actor order), broken promises (deal order). Both
        // are appended to the events after phase 3c.
        let mut sanctions: Vec<Event> = Vec::new();
        let mut violations: Vec<Event> = Vec::new();

        // --- Phase 2: consume -------------------------------------------
        for (i, it) in intents.iter().enumerate() {
            if let Action::Consume(want) = it.action {
                let a = &mut self.agents[i];
                let room = ((cfg.max_energy - a.energy) / cfg.energy_per_food) as u64;
                let q = (want as u64).min(a.inventory as u64).min(room) as u32;
                outcomes[i] = if a.inventory == 0 {
                    Outcome::Failed(Failure::EmptyInventory)
                } else if q == 0 {
                    Outcome::Failed(Failure::EnergyFull)
                } else {
                    a.inventory -= q;
                    let gain = q as i64 * cfg.energy_per_food;
                    a.energy += gain;
                    self.ledger.food_consumed += q as u64;
                    self.ledger.energy_gained += gain;
                    report.food_consumed += q as u64;
                    Outcome::Ok
                };
            }
        }

        // --- Phase 3: gather (contended) --------------------------------
        // (cell index, lottery key, agent index) — sorted, so each cell's
        // claimants are contiguous and served in lottery order.
        let mut claims: Vec<(usize, u64, usize)> = intents
            .iter()
            .enumerate()
            .filter(|(_, it)| it.action == Action::Gather)
            .map(|(i, _)| {
                let a = &self.agents[i];
                let key = RngStream::new(cfg.seed, tick, a.id.0, Stream::GatherPriority).next_u64();
                (bounds.index(a.pos), key, i)
            })
            .collect();
        claims.sort_unstable();
        let mut current_cell = usize::MAX;
        let mut cell_had_food = false;
        // (cell, gatherer index, units taken) of every successful gather,
        // grouped by cell as `claims` is; only kept with `nomos`.
        let mut taken: Vec<(usize, usize, u32)> = Vec::new();
        for &(cell, _, i) in &claims {
            let available = self.resources.food[cell];
            if cell != current_cell {
                current_cell = cell;
                cell_had_food = available > 0;
            }
            let a = &mut self.agents[i];
            let space = cfg.inventory_capacity - a.inventory;
            outcomes[i] = if space == 0 {
                Outcome::Failed(Failure::InventoryFull)
            } else if available == 0 {
                // Distinguish "someone beat me to it" from "nothing here".
                Outcome::Failed(if cell_had_food {
                    Failure::LostContention
                } else {
                    Failure::NoResource
                })
            } else {
                let q = cfg.gather_amount.min(space).min(available);
                self.resources.food[cell] -= q;
                a.inventory += q;
                report.food_gathered += q as u64;
                report.gathers[a.policy.index()] += 1;
                self.ledger.gathers += 1;
                if cfg.nomos {
                    taken.push((cell, i, q));
                }
                if q < cfg.gather_amount.min(space) {
                    Outcome::Partial
                } else {
                    Outcome::Ok
                }
            };
        }

        // --- Phase 3 (cont.): encroachment (ADR-0014 D3) ------------------
        // On each cell that saw a gather, the first occupant at S(t) is the
        // agent with the smallest `arrived` (founders first, then the
        // smaller id); every other gatherer that arrived strictly later
        // encroached on it. Cell order, then gatherer id order, so the
        // first occupant's memory is deterministic. Nothing physical moves.
        if cfg.nomos {
            let mut start = 0;
            while start < taken.len() {
                let cell = taken[start].0;
                let mut end = start;
                while end < taken.len() && taken[end].0 == cell {
                    end += 1;
                }
                let group = &mut taken[start..end];
                group.sort_unstable_by_key(|&(_, i, _)| i);
                let here = index.cell(self.agents[group[0].1].pos);
                let first =
                    nomos::first_occupant(&self.agents, here).expect("a gatherer stands here");
                for &(_, i, q) in group.iter() {
                    if i != first
                        && nomos::encroaches(self.agents[first].arrived, self.agents[i].arrived)
                    {
                        self.ledger.encroachments += 1;
                        report.encroachments[self.agents[i].policy.index()] += 1;
                        let (gatherer, occupant) = (self.agents[i].id, self.agents[first].id);
                        notes.push((
                            first,
                            Interaction {
                                tick,
                                other: gatherer,
                                kind: InteractionKind::EncroachedBy,
                                value: q as i64,
                            },
                        ));
                        // A violation only under an `Encroach` rule
                        // (ADR-0014 D4): the gatherer stands accused from
                        // the next tick; the occupant is the other party.
                        if let Some(rule) = rule_encroach {
                            self.nomos.accuse(gatherer, rule.id, tick, occupant);
                            violations.push(Event::Violated {
                                tick,
                                agent: gatherer,
                                rule: rule.id,
                                norm: NormKind::Encroach,
                                against: occupant,
                                grounds: Grounds::Encroachment {
                                    cell: self.agents[i].pos,
                                    amount: q,
                                },
                            });
                        }
                    }
                }
                start = end;
            }
        }

        // --- Phase 3b: transfer / store / harm (ADR-0013 D3) -------------
        // Resolved at S(t) positions in actor (id) order. Targets are named
        // by id; `agents` is sorted, so the lookup is a binary search.
        let mut harmed = vec![false; n];
        // This tick's transfers `(from, to, amount)`, booked against the
        // open deals in 3c (after every acceptance of the tick is in).
        let mut transfers: Vec<(AgentId, AgentId, u32)> = Vec::new();
        for (i, it) in intents.iter().enumerate() {
            match it.action {
                Action::Transfer { to, n: want } => {
                    outcomes[i] = match self.target_index(i, to) {
                        Err(f) => Outcome::Failed(f),
                        Ok(j) => {
                            let have = self.agents[i].inventory;
                            let space = cfg.inventory_capacity - self.agents[j].inventory;
                            if have == 0 {
                                Outcome::Failed(Failure::EmptyInventory)
                            } else if space == 0 {
                                Outcome::Failed(Failure::TargetFull)
                            } else {
                                let q = want.min(have).min(space);
                                self.agents[i].inventory -= q;
                                self.agents[j].inventory += q;
                                if q > 0 {
                                    self.ledger.transfers += 1;
                                    self.ledger.food_transferred += q as u64;
                                    report.transfers += 1;
                                    report.food_transferred += q as u64;
                                    let from = self.agents[i].id;
                                    notes.push((
                                        i,
                                        Interaction {
                                            tick,
                                            other: to,
                                            kind: InteractionKind::GaveTo,
                                            value: q as i64,
                                        },
                                    ));
                                    notes.push((
                                        j,
                                        Interaction {
                                            tick,
                                            other: from,
                                            kind: InteractionKind::ReceivedFrom,
                                            value: q as i64,
                                        },
                                    ));
                                    report.events.push(Event::Transferred {
                                        tick,
                                        from: self.agents[i].id,
                                        to,
                                        amount: q,
                                    });
                                    if cfg.nomos {
                                        transfers.push((from, to, q));
                                    }
                                }
                                if q < want.min(have) {
                                    Outcome::Partial
                                } else {
                                    Outcome::Ok
                                }
                            }
                        }
                    };
                }
                Action::Store(want) => {
                    let a = &mut self.agents[i];
                    outcomes[i] = if a.inventory == 0 {
                        Outcome::Failed(Failure::EmptyInventory)
                    } else {
                        let q = want.min(a.inventory);
                        a.inventory -= q;
                        self.resources.food[bounds.index(a.pos)] += q;
                        self.ledger.food_stored += q as u64;
                        report.food_stored += q as u64;
                        Outcome::Ok
                    };
                }
                Action::Harm { target } => {
                    outcomes[i] = match self.target_index(i, target) {
                        Err(f) => Outcome::Failed(f),
                        Ok(j) => {
                            self.agents[j].health -= cfg.harm_damage;
                            harmed[j] = true;
                            self.ledger.harms += 1;
                            report.harms += 1;
                            let (by, at) = (self.agents[i].id, self.agents[j].pos);
                            // A sanction (ADR-0014 D4) if the target stood
                            // accused at S(t) under a rule that lets a harm
                            // answer it: attributed to the oldest such
                            // episode. The physics above is the same.
                            let sanction = if norms.harmable(j) {
                                self.nomos.sanction(target, tick, Via::Harm)
                            } else {
                                None
                            };
                            let (mine, theirs, seen) = match sanction {
                                Some((rule, kind)) => {
                                    self.ledger.sanctions[kind.index()] += 1;
                                    report.sanctions[kind.index()] += 1;
                                    sanctions.push(Event::Sanctioned {
                                        tick,
                                        by,
                                        target,
                                        rule,
                                        via: Via::Harm,
                                    });
                                    (
                                        InteractionKind::Sanctioned,
                                        InteractionKind::SanctionedBy,
                                        InteractionKind::SawSanction,
                                    )
                                }
                                None => (
                                    InteractionKind::Harmed,
                                    InteractionKind::HarmedBy,
                                    InteractionKind::SawHarm,
                                ),
                            };
                            notes.push((
                                i,
                                Interaction {
                                    tick,
                                    other: target,
                                    kind: mine,
                                    value: cfg.harm_damage,
                                },
                            ));
                            notes.push((
                                j,
                                Interaction {
                                    tick,
                                    other: by,
                                    kind: theirs,
                                    value: cfg.harm_damage,
                                },
                            ));
                            // Witnesses: everyone else within social_radius
                            // of the victim at S(t).
                            for (_, k) in index.within(at, cfg.perception.social_radius) {
                                if k != i && k != j {
                                    notes.push((
                                        k,
                                        Interaction {
                                            tick,
                                            other: by,
                                            kind: seen,
                                            value: target.0 as i64,
                                        },
                                    ));
                                }
                            }
                            report.events.push(Event::Harmed {
                                tick,
                                by,
                                target,
                                damage: cfg.harm_damage,
                            });
                            // A violation under a `Harm` rule, unless the
                            // harm is a sanction and the rule exempts
                            // sanctions (ADR-0014 D4).
                            if let Some(rule) = rule_harm {
                                if !(sanction.is_some() && rule.norm.except_sanction) {
                                    self.nomos.accuse(by, rule.id, tick, target);
                                    violations.push(Event::Violated {
                                        tick,
                                        agent: by,
                                        rule: rule.id,
                                        norm: NormKind::Harm,
                                        against: target,
                                        grounds: Grounds::Harm {
                                            damage: cfg.harm_damage,
                                        },
                                    });
                                }
                            }
                            Outcome::Ok
                        }
                    };
                }
                Action::Signal(message @ Message::Say(_)) => {
                    let (who, at) = (self.agents[i].id, self.agents[i].pos);
                    self.agents[i].last_signal = Some(Signal { tick, message });
                    self.ledger.signals += 1;
                    report.signals += 1;
                    if cfg.record_signals {
                        report.events.push(Event::Signaled {
                            tick,
                            agent: who,
                            message,
                        });
                    }
                    // Listeners: everyone else within signal_radius at S(t).
                    for (_, k) in index.within(at, cfg.perception.signal_radius) {
                        if k != i {
                            notes.push((
                                k,
                                Interaction {
                                    tick,
                                    other: who,
                                    kind: InteractionKind::Heard,
                                    value: message.value(),
                                },
                            ));
                        }
                    }
                }
                // --- negotiation (ADR-0013 D5, 3-4): addressed, delivered,
                // recorded, never enforced. The addressee must be another
                // living agent within signal_radius at S(t). Memory is still
                // S(t)'s here (notes land in 5b), so an Accept is checked
                // against what the acceptor remembered at the tick's start.
                Action::Signal(Message::Propose { to, give, want, by }) => {
                    outcomes[i] = match self.addressee_index(i, to) {
                        Err(f) => Outcome::Failed(f),
                        Ok(j) => {
                            let from = self.agents[i].id;
                            notes.push((
                                j,
                                Interaction {
                                    tick,
                                    other: from,
                                    kind: InteractionKind::ProposalFrom,
                                    value: Terms { give, want, by }.pack(),
                                },
                            ));
                            self.ledger.proposals += 1;
                            report.proposals += 1;
                            report.events.push(Event::Proposed {
                                tick,
                                from,
                                to,
                                give,
                                want,
                                by,
                            });
                            Outcome::Ok
                        }
                    };
                }
                Action::Signal(Message::Accept { to, offer_tick }) => {
                    outcomes[i] = match self.addressee_index(i, to) {
                        Err(f) => Outcome::Failed(f),
                        Ok(j) => {
                            let held = self.agents[i].recent.iter().find(|e| {
                                e.kind == InteractionKind::ProposalFrom
                                    && e.other == to
                                    && e.tick == offer_tick
                            });
                            match held {
                                None => Outcome::Failed(Failure::NoSuchOffer),
                                Some(offer) => {
                                    let terms = Terms::unpack(offer.value);
                                    let acceptor = self.agents[i].id;
                                    // Actor first, then the counterparty: the
                                    // acceptor owes `want`, the proposer learns
                                    // of the acceptance and owes `give`.
                                    notes.push((
                                        i,
                                        Interaction {
                                            tick,
                                            other: to,
                                            kind: InteractionKind::Promised,
                                            value: terms.want as i64,
                                        },
                                    ));
                                    notes.push((
                                        j,
                                        Interaction {
                                            tick,
                                            other: acceptor,
                                            kind: InteractionKind::Accepted,
                                            value: terms.pack(),
                                        },
                                    ));
                                    notes.push((
                                        j,
                                        Interaction {
                                            tick,
                                            other: acceptor,
                                            kind: InteractionKind::Promised,
                                            value: terms.give as i64,
                                        },
                                    ));
                                    self.ledger.acceptances += 1;
                                    report.acceptances += 1;
                                    report.events.push(Event::Accepted {
                                        tick,
                                        from: acceptor,
                                        to,
                                        offer_tick,
                                        give: terms.give,
                                        want: terms.want,
                                        by: terms.by,
                                    });
                                    // The deal the kernel will audit
                                    // (ADR-0014 D3), settled at its deadline.
                                    if cfg.nomos {
                                        self.nomos.deals.push(OpenDeal {
                                            proposer: to,
                                            acceptor,
                                            offer_tick,
                                            accepted_tick: tick,
                                            deadline: offer_tick + terms.by as u64,
                                            give: terms.give,
                                            want: terms.want,
                                            given: 0,
                                            repaid: 0,
                                        });
                                    }
                                    Outcome::Ok
                                }
                            }
                        }
                    };
                }
                Action::Signal(Message::Reject { to }) => {
                    outcomes[i] = match self.addressee_index(i, to) {
                        Err(f) => Outcome::Failed(f),
                        Ok(j) => {
                            let from = self.agents[i].id;
                            // A sanction (ADR-0014 D4) if the addressee
                            // stood accused at S(t) under a rule that lets
                            // a rejection answer it; then both parties
                            // remember the sanction instead of the refusal.
                            let sanction = if norms.rejectable(j) {
                                self.nomos.sanction(to, tick, Via::Reject)
                            } else {
                                None
                            };
                            match sanction {
                                Some((rule, kind)) => {
                                    self.ledger.sanctions[kind.index()] += 1;
                                    report.sanctions[kind.index()] += 1;
                                    sanctions.push(Event::Sanctioned {
                                        tick,
                                        by: from,
                                        target: to,
                                        rule,
                                        via: Via::Reject,
                                    });
                                    notes.push((
                                        i,
                                        Interaction {
                                            tick,
                                            other: to,
                                            kind: InteractionKind::Sanctioned,
                                            value: 0,
                                        },
                                    ));
                                    notes.push((
                                        j,
                                        Interaction {
                                            tick,
                                            other: from,
                                            kind: InteractionKind::SanctionedBy,
                                            value: 0,
                                        },
                                    ));
                                }
                                None => notes.push((
                                    j,
                                    Interaction {
                                        tick,
                                        other: from,
                                        kind: InteractionKind::Rejected,
                                        value: 0,
                                    },
                                )),
                            }
                            self.ledger.rejections += 1;
                            report.rejections += 1;
                            report.events.push(Event::Rejected { tick, from, to });
                            Outcome::Ok
                        }
                    };
                }
                _ => {}
            }
        }

        // --- Phase 3c: promise audit (ADR-0014 D3) -------------------------
        // Every transfer of the tick counts for every open deal whose
        // window holds it, including deals accepted this tick by a later
        // actor; then the deals due by now are settled. Counted and
        // reported, never enforced.
        if cfg.nomos {
            for &(from, to, amount) in &transfers {
                self.nomos.credit_transfer(tick, from, to, amount);
            }
            for d in self.nomos.settle(tick) {
                self.ledger.promises_due += 1;
                if !d.kept() {
                    self.ledger.promises_broken += 1;
                    // A violation under a `BrokenPromise` rule (ADR-0014
                    // D4), by each side that fell short — the proposer
                    // first — against the other. Only the living can be
                    // accused: a party that died before the deadline left
                    // a broken promise in the ledger but no violator.
                    if let Some(rule) = rule_promise {
                        let short = [
                            (
                                d.proposer_short(),
                                d.proposer,
                                d.acceptor,
                                d.give as u32 - d.given.min(d.give as u32),
                            ),
                            (
                                d.acceptor_short(),
                                d.acceptor,
                                d.proposer,
                                d.want as u32 - d.repaid.min(d.want as u32),
                            ),
                        ];
                        for (is_short, who, other, owed) in short {
                            if !is_short
                                || self.agents.binary_search_by_key(&who, |a| a.id).is_err()
                            {
                                continue;
                            }
                            self.nomos.accuse(who, rule.id, tick, other);
                            violations.push(Event::Violated {
                                tick,
                                agent: who,
                                rule: rule.id,
                                norm: NormKind::BrokenPromise,
                                against: other,
                                grounds: Grounds::BrokenPromise {
                                    offer_tick: d.offer_tick,
                                    short: owed,
                                },
                            });
                        }
                    }
                }
                report.promises_settled.push(d);
            }
            // Episodes whose accusation does not reach the next tick close
            // here (ADR-0014 D4): `last + standing_ticks <= tick`.
            let closed = self.nomos.expire(tick);
            self.ledger.book_closed(&closed, Some(&mut report));
        }
        // The tick's judgements, after every physical event of 3b and in
        // the order of ADR-0014 D9. Violations are counted either way and
        // recorded only on request.
        report.events.append(&mut sanctions);
        for v in &violations {
            if let Event::Violated { norm, .. } = v {
                self.ledger.violations[norm.index()] += 1;
                report.violations[norm.index()] += 1;
            }
        }
        if cfg.record_violations {
            report.events.append(&mut violations);
        }

        // --- Phase 4: move ------------------------------------------------
        let mut moved = vec![false; n];
        for (i, it) in intents.iter().enumerate() {
            if let Action::Move(d) = it.action {
                match bounds.step(self.agents[i].pos, d) {
                    Some(p) => {
                        self.agents[i].pos = p;
                        moved[i] = true;
                        // A new cell, stood on from the next tick (ADR-0014 D3).
                        if cfg.nomos {
                            self.agents[i].arrived = Some(tick + 1);
                        }
                    }
                    None => outcomes[i] = Outcome::Failed(Failure::OutOfBounds),
                }
            }
        }

        // --- Phase 5: upkeep ----------------------------------------------
        for (i, a) in self.agents.iter_mut().enumerate() {
            let action_cost = match intents[i].action {
                Action::Move(_) if moved[i] => cfg.move_cost,
                Action::Gather => cfg.gather_cost,
                Action::Transfer { .. } => cfg.transfer_cost,
                Action::Harm { .. } => cfg.harm_cost,
                Action::Signal(_) => cfg.signal_cost,
                _ => 0,
            };
            let cost = (cfg.metabolism_cost + action_cost).min(a.energy);
            a.energy -= cost;
            self.ledger.energy_spent += cost;
            if intents[i].action == Action::Rest && a.energy > 0 {
                a.health = (a.health + cfg.rest_heal).min(cfg.max_health);
            }
            if a.energy == 0 {
                a.health -= cfg.starvation_damage;
            }
            a.age += 1;

            // --- Phase 5a: learning (ADR-0013 D6, 3-7) -----------------------
            // The policy reported the row it decided in; the kernel credits
            // the previous step with this tick's reward and that row, then
            // records this step. The reward is the energy change so far in
            // the tick — before birth, so energy moved to a child counts as
            // kept (the birth bonus). An intent without a row (any other
            // kind, or a hand-built intent) ends the chain.
            a.last_step = match (intents[i].state, a.params.table.as_mut()) {
                (Some(now), Some(PolicyTable::Learner { q }))
                    if (now as usize + 1) * Action::KIND_COUNT <= q.len() =>
                {
                    if let Some(prev) = a.last_step {
                        learner::credit(q, prev, now);
                    }
                    Some(LearnerStep {
                        state: now,
                        action: intents[i].action.kind_index() as u8,
                        reward: learner::reward_with_health(
                            a.energy - start_energy[i],
                            start_health.get(i).map_or(0, |h| a.health - h),
                            cfg.learner_health_weight,
                        ),
                    })
                }
                _ => None,
            };
        }

        for o in &outcomes {
            if let Outcome::Failed(f) = o {
                report.failures[f.index()] += 1;
            }
        }
        if cfg.record_actions {
            for (i, it) in intents.iter().enumerate() {
                report.events.push(Event::Acted {
                    tick,
                    agent: self.agents[i].id,
                    action: it.action,
                    outcome: outcomes[i],
                });
            }
        }

        // --- Phase 5b: memory (ADR-0013 D4) --------------------------------
        // Kernel-written, in order of occurrence, while every agent of S(t)
        // is still present so the indices are valid. `memory_slots == 0`
        // means nobody remembers anything.
        let slots = cfg.memory_slots as usize;
        if slots == 0 {
            for a in &mut self.agents {
                a.recent.clear();
            }
        } else {
            for (k, note) in notes {
                remember(&mut self.agents[k].recent, slots, note);
            }
        }

        // --- Phase 6: death -----------------------------------------------
        let before = self.agents.len();
        let max_age = cfg.max_age;
        let (resources, ledger) = (&mut self.resources, &mut self.ledger);
        let events = &mut report.events;
        let mut deaths_by_cause = [0u32; DeathCause::COUNT];
        let mut visiting = 0usize;
        self.agents.retain(|a| {
            let i = visiting;
            visiting += 1;
            let cause = if a.health <= 0 {
                if harmed[i] {
                    DeathCause::Violence
                } else {
                    DeathCause::Starvation
                }
            } else if max_age.is_some_and(|m| a.age >= m) {
                DeathCause::OldAge
            } else {
                return true;
            };
            resources.food[bounds.index(a.pos)] += a.inventory;
            ledger.energy_written_off += a.energy;
            ledger.deaths += 1;
            ledger.deaths_by_cause[cause.index()] += 1;
            deaths_by_cause[cause.index()] += 1;
            events.push(Event::Died {
                tick,
                agent: a.id,
                cause,
                pos: a.pos,
                age: a.age,
                energy_written_off: a.energy,
                food_dropped: a.inventory,
            });
            false
        });
        report.deaths = (before - self.agents.len()) as u32;
        report.deaths_by_cause = deaths_by_cause;
        // The dead stand accused of nothing: their episodes close
        // (ADR-0014 D4), sanctioned or not.
        if report.deaths > 0 && !self.nomos.standing.is_empty() {
            let closed = self.nomos.drop_dead(&self.agents);
            self.ledger.book_closed(&closed, Some(&mut report));
        }

        // --- Phase 7: birth -----------------------------------------------
        if let Some(threshold) = cfg.birth_threshold {
            let survivors = self.agents.len();
            for i in 0..survivors {
                let parent = &self.agents[i];
                if parent.energy < threshold {
                    continue;
                }
                let child_id = AgentId(self.next_agent_id);
                self.next_agent_id += 1;
                let mut params = parent.params.clone();
                if cfg.mutation_enabled {
                    let mut rng = RngStream::new(cfg.seed, tick, child_id.0, Stream::Mutation);
                    params = params.mutated(cfg.mutation_step_permille, &mut rng);
                }
                // A learner's child starts from the parent's Q table unless
                // the config says every life starts blank (ADR-0013 D6).
                if parent.policy == PolicyKind::Learner && !cfg.learner_inherit_q {
                    params.table = PolicyTable::blank(PolicyKind::Learner, cfg);
                }
                let child = Agent {
                    id: child_id,
                    pos: parent.pos,
                    energy: cfg.child_energy,
                    health: cfg.max_health,
                    inventory: 0,
                    age: 0,
                    policy: parent.policy,
                    params,
                    memory: Memory::default(),
                    last_signal: None,
                    recent: Vec::new(),
                    last_step: None,
                    // A family shares the parent's claim on the cell
                    // (ADR-0014 D3); `None` while `nomos` is off.
                    arrived: parent.arrived,
                };
                report.events.push(Event::Born {
                    tick,
                    child: child_id,
                    parent: parent.id,
                    pos: child.pos,
                    energy: child.energy,
                });
                self.agents[i].energy -= cfg.child_energy;
                self.agents.push(child);
                self.ledger.births += 1;
                report.births += 1;
            }
        }

        // --- Phase 8: environment -----------------------------------------
        if (tick + 1) % self.config.regrow_interval == 0 {
            self.ledger.food_regrown += self.resources.regrow(self.config.regrow_amount);
        }

        self.tick += 1;
        report.alive = self.agents.len() as u32;
        report
    }

    /// Resolves the target of a social action: another living agent, adjacent
    /// to the actor at the tick's start positions.
    fn target_index(&self, actor: usize, id: AgentId) -> Result<usize, Failure> {
        self.resolve_other(actor, id, 1, Failure::NotAdjacent)
    }

    /// Resolves the addressee of a negotiation message: another living agent
    /// within `signal_radius` of the actor at the tick's start positions.
    fn addressee_index(&self, actor: usize, id: AgentId) -> Result<usize, Failure> {
        let reach = self.config.perception.signal_radius as u32;
        self.resolve_other(actor, id, reach, Failure::OutOfEarshot)
    }

    /// Another living agent within Manhattan `reach` of the actor, or the
    /// failure that names the problem. Targets are named by id; `agents` is
    /// sorted, so the lookup is a binary search.
    fn resolve_other(
        &self,
        actor: usize,
        id: AgentId,
        reach: u32,
        too_far: Failure,
    ) -> Result<usize, Failure> {
        let j = self
            .agents
            .binary_search_by_key(&id, |a| a.id)
            .map_err(|_| Failure::NoSuchTarget)?;
        if j == actor {
            return Err(Failure::NoSuchTarget);
        }
        if self.agents[actor].pos.manhattan(self.agents[j].pos) > reach {
            return Err(too_far);
        }
        Ok(j)
    }

    /// Applies an external change between ticks (ADR-0010 2a-1) and returns
    /// its ledger record. Never runs inside `apply`; the only randomness is
    /// the seed-keyed draw of `SetPolicy { who: Fraction }`; on error the
    /// state is left untouched.
    pub fn intervene(&mut self, iv: Intervention) -> Result<Event, InterventionError> {
        use InterventionError::Invalid;
        match iv {
            Intervention::Drought {
                regrow_amount,
                regrow_interval,
            } => {
                if regrow_interval == 0 {
                    return Err(Invalid("regrow_interval must be > 0"));
                }
                self.config.regrow_amount = regrow_amount;
                self.config.regrow_interval = regrow_interval;
            }
            Intervention::ScaleFood { permille } => {
                if permille > Intervention::MAX_PERMILLE {
                    return Err(Invalid("permille must be <= 10000"));
                }
                let mut delta: i128 = 0;
                for f in &mut self.resources.food {
                    let scaled = (*f as u64 * permille as u64 / 1000).min(u32::MAX as u64) as u32;
                    delta += scaled as i128 - *f as i128;
                    *f = scaled;
                }
                self.ledger.food_intervened += delta as i64;
            }
            Intervention::SpawnAgents { n, pos, policy } => {
                if n == 0 {
                    return Err(Invalid("n must be > 0"));
                }
                if !self.bounds().contains(pos) {
                    return Err(Invalid("pos out of bounds"));
                }
                let (energy, health) = (self.config.initial_energy, self.config.max_health);
                let params = PolicyParams::for_kind(policy, &self.config);
                // Spawned agents arrive at the intervention's tick (ADR-0014
                // D3): later than everyone already standing there.
                let arrived = self.config.nomos.then_some(self.tick);
                for _ in 0..n {
                    self.agents.push(Agent {
                        id: AgentId(self.next_agent_id),
                        pos,
                        energy,
                        health,
                        inventory: 0,
                        age: 0,
                        policy,
                        params: params.clone(),
                        memory: Memory::default(),
                        last_signal: None,
                        recent: Vec::new(),
                        last_step: None,
                        arrived,
                    });
                    self.next_agent_id += 1;
                }
                self.ledger.energy_intervened += n as i64 * energy;
            }
            Intervention::RemoveAgent { id } => {
                let i = self
                    .agents
                    .binary_search_by_key(&id, |a| a.id)
                    .map_err(|_| InterventionError::NoSuchAgent(id))?;
                let a = self.agents.remove(i);
                let cell = self.bounds().index(a.pos);
                self.resources.food[cell] += a.inventory;
                self.ledger.energy_intervened -= a.energy;
                // Whoever leaves the world stands accused of nothing
                // (ADR-0014 D4): its episodes close like a dead agent's.
                let closed = self.nomos.drop_agent(id);
                self.ledger.book_closed(&closed, None);
            }
            // Decrees (ADR-0014 D8): institutions exist only where the
            // switch is on; nothing here is bound by the charter.
            Intervention::Enact { norm } => {
                if !self.config.nomos {
                    return Err(Invalid("nomos is off: no rule can be in force"));
                }
                if !norm.is_valid() {
                    return Err(Invalid("sanction must be a set of harm (1) and reject (2)"));
                }
                if self.nomos.next_rule_id == u16::MAX {
                    return Err(Invalid("rule ids exhausted"));
                }
                let (_rule, replaced) = self.nomos.enact(norm, self.tick);
                if let Some((_old, closed)) = replaced {
                    self.ledger.book_closed(&closed, None);
                }
            }
            Intervention::Repeal { rule } => {
                if !self.config.nomos {
                    return Err(Invalid("nomos is off: no rule can be in force"));
                }
                let (_old, closed) = self
                    .nomos
                    .repeal(rule)
                    .ok_or(Invalid("no rule with that id is in force"))?;
                self.ledger.book_closed(&closed, None);
            }
            Intervention::Charter { charter } => {
                if !self.config.nomos {
                    return Err(Invalid("nomos is off: there is no assembly"));
                }
                if !charter.is_valid() {
                    return Err(Invalid("charter permilles must be <= 1000"));
                }
                self.nomos.charter = charter;
            }
            Intervention::SetPolicy { who, kind, params } => {
                let chosen: Vec<usize> = match who {
                    Who::All => (0..self.agents.len()).collect(),
                    Who::Id(id) => vec![self
                        .agents
                        .binary_search_by_key(&id, |a| a.id)
                        .map_err(|_| InterventionError::NoSuchAgent(id))?],
                    Who::Fraction { permille } => {
                        if permille > Who::PERMILLE_MAX {
                            return Err(Invalid("permille must be <= 1000"));
                        }
                        let (seed, tick) = (self.config.seed, self.tick);
                        self.agents
                            .iter()
                            .enumerate()
                            .filter(|(_, a)| {
                                RngStream::new(seed, tick, a.id.0, Stream::Intervention)
                                    .chance(permille as u64, Who::PERMILLE_MAX as u64)
                            })
                            .map(|(i, _)| i)
                            .collect()
                    }
                };
                for i in chosen {
                    let a = &mut self.agents[i];
                    a.policy = kind;
                    a.params = match params {
                        ParamsMode::Default => PolicyParams::for_kind(kind, &self.config),
                        ParamsMode::Keep => std::mem::take(&mut a.params).refit(kind, &self.config),
                    };
                    // The switch breaks the chain of decisions: the next
                    // decision has no predecessor to credit (3-7).
                    a.last_step = None;
                }
            }
        }
        self.ledger.interventions += 1;
        Ok(Event::Intervened {
            tick: self.tick,
            intervention: iv,
        })
    }

    pub fn total_energy(&self) -> i64 {
        self.agents.iter().map(|a| a.energy).sum()
    }

    pub fn total_inventory(&self) -> u64 {
        self.agents.iter().map(|a| a.inventory as u64).sum()
    }

    /// Checks every structural invariant and conservation law.
    pub fn check_invariants(&self) -> Result<(), Vec<String>> {
        let cfg = &self.config;
        let mut errs = Vec::new();
        for w in self.agents.windows(2) {
            if w[0].id >= w[1].id {
                errs.push(format!(
                    "agent ids not strictly ascending: {:?} {:?}",
                    w[0].id, w[1].id
                ));
            }
        }
        if let Some(last) = self.agents.last() {
            if last.id.0 >= self.next_agent_id {
                errs.push("next_agent_id not greater than every live id".into());
            }
        }
        for a in &self.agents {
            if !(0..=cfg.max_energy).contains(&a.energy) {
                errs.push(format!("{:?} energy out of range: {}", a.id, a.energy));
            }
            if !(1..=cfg.max_health).contains(&a.health) {
                errs.push(format!("{:?} health out of range: {}", a.id, a.health));
            }
            if a.inventory > cfg.inventory_capacity {
                errs.push(format!(
                    "{:?} inventory over capacity: {}",
                    a.id, a.inventory
                ));
            }
            if cfg.max_age.is_some_and(|m| a.age >= m) {
                errs.push(format!("{:?} older than max_age: {}", a.id, a.age));
            }
            if !a.params.is_valid() {
                errs.push(format!(
                    "{:?} params out of range: hunger_permille {}",
                    a.id, a.params.hunger_permille
                ));
            }
            if !a.params.fits(a.policy, cfg) {
                errs.push(format!(
                    "{:?} policy {:?} does not fit its table {:?}",
                    a.id,
                    a.policy,
                    a.params.table.as_ref().map(|t| match t {
                        PolicyTable::Reflex { weights } => ("reflex", weights.len()),
                        PolicyTable::Learner { q } => ("learner", q.len()),
                    })
                ));
            }
            if a.recent.len() > cfg.memory_slots as usize {
                errs.push(format!(
                    "{:?} remembers more than memory_slots: {}",
                    a.id,
                    a.recent.len()
                ));
            }
            if let Some(s) = a.last_step {
                if a.policy != PolicyKind::Learner
                    || s.state >= cfg.learner_states
                    || s.action as usize >= Action::KIND_COUNT
                {
                    errs.push(format!(
                        "{:?} policy {:?} carries a learner step that does not fit: {s:?}",
                        a.id, a.policy
                    ));
                }
            }
            if !self.bounds().contains(a.pos) {
                errs.push(format!("{:?} out of bounds: {:?}", a.id, a.pos));
            }
            if let Some(t) = a.arrived {
                if !cfg.nomos {
                    errs.push(format!("{:?} has arrived set while nomos is off", a.id));
                } else if t > self.tick {
                    errs.push(format!(
                        "{:?} arrived in the future: {t} > {}",
                        a.id, self.tick
                    ));
                }
            }
        }
        // Social facts (ADR-0014 D2): nothing tracked without `nomos`; with
        // it, open deals were accepted in the past, in order, and are not
        // yet due (settlement happens in the deadline's tick).
        if !cfg.nomos && !self.nomos.is_empty() {
            errs.push("nomos state present while nomos is off".into());
        }
        // Institutions (ADR-0014 D2): rule ids unique, ascending, below the
        // next one, one rule per kind, enacted in the past; episodes
        // ascending by (agent, rule), agent alive, rule in force, opened
        // in the past and still reaching this tick; the open motion's
        // voters alive, ascending, and on one side only.
        let nm = &self.nomos;
        if !nm.charter.is_valid() {
            errs.push("charter permilles out of range".into());
        }
        for (k, r) in nm.rules.iter().enumerate() {
            if r.id >= nm.next_rule_id {
                errs.push(format!(
                    "rule {} not below next_rule_id {}",
                    r.id, nm.next_rule_id
                ));
            }
            if k > 0 && nm.rules[k - 1].id >= r.id {
                errs.push(format!("rule ids not strictly ascending at {}", r.id));
            }
            if nm.rules[..k].iter().any(|o| o.norm.kind == r.norm.kind) {
                errs.push(format!("two rules in force for {:?}", r.norm.kind));
            }
            if !r.norm.is_valid() {
                errs.push(format!("rule {} has unknown sanction bits", r.id));
            }
            if r.enacted > self.tick {
                errs.push(format!("rule {} enacted in the future", r.id));
            }
        }
        for (k, s) in nm.standing.iter().enumerate() {
            if k > 0 {
                let p = &nm.standing[k - 1];
                if (p.agent, p.rule) >= (s.agent, s.rule) {
                    errs.push(format!("standing not ascending by (agent, rule) at {k}"));
                }
            }
            if self
                .agents
                .binary_search_by_key(&s.agent, |a| a.id)
                .is_err()
            {
                errs.push(format!("{:?} stands accused but is not alive", s.agent));
            }
            match nm.rule(s.rule) {
                None => errs.push(format!(
                    "{:?} stands accused under no rule {}",
                    s.agent, s.rule
                )),
                Some(r) => {
                    if s.until(r.norm.standing_ticks) < self.tick {
                        errs.push(format!(
                            "{:?}'s episode under rule {} has expired",
                            s.agent, s.rule
                        ));
                    }
                }
            }
            if s.since > s.last || s.last >= self.tick || s.violations == 0 {
                errs.push(format!(
                    "{:?}'s episode under rule {} is malformed: {s:?}",
                    s.agent, s.rule
                ));
            }
        }
        if let Some(m) = &nm.motion {
            if m.id >= nm.next_motion_id {
                errs.push("motion id not below next_motion_id".into());
            }
            if m.opened > self.tick {
                errs.push("motion opened in the future".into());
            }
            for (side, voters) in [("ayes", &m.ayes), ("nays", &m.nays)] {
                for (k, v) in voters.iter().enumerate() {
                    if k > 0 && voters[k - 1] >= *v {
                        errs.push(format!("motion {side} not strictly ascending"));
                    }
                    if self.agents.binary_search_by_key(v, |a| a.id).is_err() {
                        errs.push(format!("motion voter {:?} is not alive", v));
                    }
                }
            }
            if m.ayes.iter().any(|v| m.nays.binary_search(v).is_ok()) {
                errs.push("an agent voted on both sides".into());
            }
        }
        for (k, d) in self.nomos.deals.iter().enumerate() {
            if d.offer_tick > d.accepted_tick || d.accepted_tick > self.tick {
                errs.push(format!("deal {k} accepted outside the past: {d:?}"));
            }
            if d.deadline < self.tick {
                errs.push(format!("deal {k} is overdue and unsettled: {d:?}"));
            }
            if k > 0 && self.nomos.deals[k - 1].accepted_tick > d.accepted_tick {
                errs.push(format!("deal {k} out of acceptance order"));
            }
        }
        let l = &self.ledger;
        let food_now = (self.resources.total_food() + self.total_inventory()) as i128;
        let food_expected = l.food_initial as i128 + l.food_regrown as i128
            - l.food_consumed as i128
            + l.food_intervened as i128;
        if food_now != food_expected {
            errs.push(format!(
                "food not conserved: have {food_now}, ledger says {food_expected}"
            ));
        }
        let energy_now = self.total_energy();
        let energy_expected =
            l.energy_initial + l.energy_gained - l.energy_spent - l.energy_written_off
                + l.energy_intervened;
        if energy_now != energy_expected {
            errs.push(format!(
                "energy not conserved: have {energy_now}, ledger says {energy_expected}"
            ));
        }
        if l.energy_gained != l.food_consumed as i64 * cfg.energy_per_food {
            errs.push("energy gained without matching food consumption".into());
        }
        if l.deaths != l.deaths_by_cause.iter().sum::<u64>() {
            errs.push("deaths total does not match deaths by cause".into());
        }
        if self.next_agent_id < cfg.initial_agents as u64 + l.births {
            errs.push("fewer ids handed out than agents created".into());
        }
        if errs.is_empty() {
            Ok(())
        } else {
            Err(errs)
        }
    }

    /// Stable 64-bit fingerprint of the full state (FNV-1a over a canonical
    /// encoding). Independent of platform pointer width and of `std::hash`.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv64::new();
        h.bytes(&serde_json::to_vec(&self.config).expect("config serialises"));
        self.hash_ledger(&mut h);
        self.hash_dynamics(&mut h);
        h.finish()
    }

    /// Fingerprint of the physical state only: tick, ids, resources and
    /// agents — not the config and not the ledger's bookkeeping. Golden
    /// tests pin this one, so adding a config or ledger field (an encoding
    /// change) cannot be mistaken for a change of the transition function,
    /// while any change in what agents did or where food is shows up.
    pub fn dynamics_hash(&self) -> u64 {
        let mut h = Fnv64::new();
        self.hash_dynamics(&mut h);
        h.finish()
    }

    fn hash_ledger(&self, h: &mut Fnv64) {
        let l = &self.ledger;
        for v in [
            l.food_initial,
            l.food_regrown,
            l.food_consumed,
            l.births,
            l.deaths,
            l.transfers,
            l.food_transferred,
            l.food_stored,
            l.harms,
            l.signals,
            l.proposals,
            l.acceptances,
            l.rejections,
        ] {
            h.u64(v);
        }
        for v in l.deaths_by_cause {
            h.u64(v);
        }
        for v in [
            l.energy_initial,
            l.energy_gained,
            l.energy_spent,
            l.energy_written_off,
            l.food_intervened,
            l.energy_intervened,
            l.interventions as i64,
        ] {
            h.u64(v as u64);
        }
        // Social-fact counters (ADR-0014 4-1): in `state_hash` only, like
        // every ledger field.
        for v in [
            l.gathers,
            l.encroachments,
            l.promises_due,
            l.promises_broken,
        ] {
            h.u64(v);
        }
        // Institution counters (ADR-0014 4-2): `state_hash` only.
        for arr in [
            &l.violations,
            &l.sanctions,
            &l.episodes_closed,
            &l.episodes_sanctioned,
        ] {
            for &v in arr {
                h.u64(v);
            }
        }
    }

    fn hash_dynamics(&self, h: &mut Fnv64) {
        h.u64(self.tick);
        h.u64(self.next_agent_id);
        h.u64(self.resources.food.len() as u64);
        for (&f, &c) in self.resources.food.iter().zip(&self.resources.capacity) {
            h.u64(((f as u64) << 32) | c as u64);
        }
        h.u64(self.agents.len() as u64);
        for a in &self.agents {
            h.u64(a.id.0);
            h.u64(((a.pos.x as u64) << 16) | a.pos.y as u64);
            h.u64(a.energy as u64);
            h.u64(a.health as u64);
            h.u64(a.inventory as u64);
            h.u64(a.age);
            h.u64(a.policy as u64);
            h.u64(a.params.hunger_permille as u64);
            // A learning policy's table is state; the fixed policies have
            // none and add no bytes, so their golden hash is unchanged
            // (ADR-0013 3-5).
            match &a.params.table {
                None => {}
                Some(PolicyTable::Reflex { weights }) => {
                    h.u64(1);
                    h.u64(weights.len() as u64);
                    for w in weights {
                        h.u64(*w as i64 as u64);
                    }
                }
                Some(PolicyTable::Learner { q }) => {
                    h.u64(2);
                    h.u64(q.len() as u64);
                    for v in q {
                        h.u64(*v as i64 as u64);
                    }
                }
            }
            // A learner's pending step is state too; `None` adds no bytes
            // (ADR-0013 3-7).
            if let Some(s) = a.last_step {
                h.u64(3);
                h.u64(s.state as u64);
                h.u64(s.action as u64);
                h.u64(s.reward as i64 as u64);
            }
            // First occupancy is state only where it is tracked: `None`
            // (every agent of a world with `nomos` off) adds no bytes
            // (ADR-0014 D9).
            if let Some(t) = a.arrived {
                h.u64(4);
                h.u64(t);
            }
            match a.memory.target {
                Some(p) => h.u64((1 << 32) | ((p.x as u64) << 16) | p.y as u64),
                None => h.u64(0),
            }
            // Signals and memory contribute bytes only when present, so a
            // world that never uses them keeps its golden hash (ADR-0013 3-3).
            if let Some(s) = a.last_signal {
                h.u64(1);
                h.u64(s.tick);
                h.u64(s.message.hash_code());
            }
            if !a.recent.is_empty() {
                h.u64(a.recent.len() as u64);
                for e in &a.recent {
                    h.u64(e.tick);
                    h.u64(e.other.0);
                    h.u64(e.kind as u64);
                    h.u64(e.value as u64);
                }
            }
        }
        // The social-fact and institutional state is part of the dynamics
        // only in worlds that track it (ADR-0014 D9): with `nomos` off
        // nothing is added.
        if self.config.nomos {
            let nm = &self.nomos;
            h.u64(nm.charter.hash_code());
            h.u64(nm.rules.len() as u64);
            for r in &nm.rules {
                h.u64(r.id as u64);
                h.u64(r.norm.hash_code());
                h.u64(r.enacted);
            }
            h.u64(nm.next_rule_id as u64);
            match &nm.motion {
                None => h.u64(0),
                Some(m) => {
                    h.u64(1);
                    h.u64(m.id as u64);
                    h.u64(m.by.0);
                    h.u64(m.opened);
                    match m.kind {
                        nomos::MotionKind::Enact { norm } => {
                            h.u64(1);
                            h.u64(norm.hash_code());
                        }
                        nomos::MotionKind::Repeal { rule } => {
                            h.u64(2);
                            h.u64(rule as u64);
                        }
                        nomos::MotionKind::Charter { charter } => {
                            h.u64(3);
                            h.u64(charter.hash_code());
                        }
                    }
                    for side in [&m.ayes, &m.nays] {
                        h.u64(side.len() as u64);
                        for v in side {
                            h.u64(v.0);
                        }
                    }
                }
            }
            h.u64(nm.next_motion_id as u64);
            h.u64(nm.standing.len() as u64);
            for s in &nm.standing {
                h.u64(s.agent.0);
                h.u64(s.rule as u64);
                h.u64(s.since);
                h.u64(s.last);
                h.u64(s.against.0);
                h.u64(((s.violations as u64) << 16) | s.sanctions as u64);
            }
            h.u64(nm.deals.len() as u64);
            for d in &nm.deals {
                h.u64(d.proposer.0);
                h.u64(d.acceptor.0);
                h.u64(d.offer_tick);
                h.u64(d.accepted_tick);
                h.u64(d.deadline);
                h.u64(((d.give as u64) << 8) | d.want as u64);
                h.u64(((d.given as u64) << 32) | d.repaid as u64);
            }
        }
    }

    /// JSON snapshot: readable, for debugging and tests. `to_bytes` is the
    /// compact form used at runtime (ADR-0006).
    pub fn to_snapshot(&self) -> Vec<u8> {
        serde_json::to_vec(&self.snapshot_ref()).expect("world serialises")
    }

    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let header: SnapshotHeader =
            serde_json::from_slice(bytes).map_err(|e| SnapshotError::Decode(e.to_string()))?;
        if header.format != SNAPSHOT_FORMAT {
            return Err(SnapshotError::Format(header.format));
        }
        let snap: Snapshot =
            serde_json::from_slice(bytes).map_err(|e| SnapshotError::Decode(e.to_string()))?;
        Self::validate_snapshot(snap)
    }

    /// Compact binary snapshot (postcard): `SNAPSHOT_MAGIC` followed by the
    /// same `{format, state_hash, world}` record as the JSON form.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = SNAPSHOT_MAGIC.to_vec();
        out.extend(postcard::to_allocvec(&self.snapshot_ref()).expect("world serialises"));
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let body = bytes
            .strip_prefix(&SNAPSHOT_MAGIC)
            .ok_or_else(|| SnapshotError::Decode("not a binary snapshot (bad magic)".into()))?;
        // The format is the first field: read it alone first, so an old
        // snapshot is reported as such instead of as garbage.
        let (format, _) = postcard::take_from_bytes::<u32>(body)
            .map_err(|e| SnapshotError::Decode(e.to_string()))?;
        if format != SNAPSHOT_FORMAT {
            return Err(SnapshotError::Format(format));
        }
        let snap: Snapshot =
            postcard::from_bytes(body).map_err(|e| SnapshotError::Decode(e.to_string()))?;
        Self::validate_snapshot(snap)
    }

    fn snapshot_ref(&self) -> SnapshotRef<'_> {
        SnapshotRef {
            format: SNAPSHOT_FORMAT,
            state_hash: self.state_hash(),
            world: self,
        }
    }

    /// Format, config, invariants and hash must all agree before bytes
    /// become a `World`, whichever encoding they came from.
    fn validate_snapshot(snap: Snapshot) -> Result<Self, SnapshotError> {
        if snap.format != SNAPSHOT_FORMAT {
            return Err(SnapshotError::Format(snap.format));
        }
        let world = snap.world;
        world
            .config
            .validate()
            .map_err(|e| SnapshotError::Invalid(vec![e.to_string()]))?;
        world.check_invariants().map_err(SnapshotError::Invalid)?;
        if world.state_hash() != snap.state_hash {
            return Err(SnapshotError::HashMismatch);
        }
        Ok(world)
    }
}

/// First four bytes of every binary snapshot, so JSON or a ledger handed to
/// `from_bytes` fails fast with a clear reason.
pub const SNAPSHOT_MAGIC: [u8; 4] = *b"CZS\0";

/// Bumped whenever `World`'s serialised shape changes. 2: life cycle fields
/// (ADR-0008). 3: intervention ledger fields (ADR-0010 2a-1). 4: perception
/// config, social-action ledger fields, `DeathCause::Violence` (ADR-0013 3-1/3-2).
/// 5: `Agent.last_signal`, `Agent.recent`, `Ledger.signals` (ADR-0013 3-3).
/// 6: `Ledger.proposals` / `acceptances` / `rejections` (ADR-0013 3-4).
/// 7: `PolicyParams.table`, `PolicyKind::{Reflex, Learner}`, config
/// `learner_states` / `learner_inherit_q` (ADR-0013 3-5).
/// 8: `Agent.last_step`, config `learner_epsilon_permille` (ADR-0013 3-7).
/// 9: `Agent.arrived`, `World.nomos`, `InteractionKind::EncroachedBy`,
/// ledger `gathers` / `encroachments` / `promises_due` / `promises_broken`,
/// config `nomos` / `learner_health_weight` (ADR-0014 4-1).
/// 10: `Nomos` gains `charter` / `rules` / `next_rule_id` / `motion` /
/// `next_motion_id` / `standing`; `InteractionKind::{Sanctioned,
/// SanctionedBy, SawSanction}`; ledger `violations` / `sanctions` /
/// `episodes_closed` / `episodes_sanctioned`; config `charter` /
/// `record_violations` (ADR-0014 4-2).
pub const SNAPSHOT_FORMAT: u32 = 10;

/// Written form: borrows the world instead of cloning it.
#[derive(Serialize)]
struct SnapshotRef<'a> {
    format: u32,
    state_hash: u64,
    world: &'a World,
}

#[derive(Deserialize)]
struct Snapshot {
    format: u32,
    state_hash: u64,
    world: World,
}

/// Just the leading field of a snapshot, for the early format check.
#[derive(Deserialize)]
struct SnapshotHeader {
    format: u32,
}

#[derive(Debug, PartialEq, Eq)]
pub enum SnapshotError {
    Decode(String),
    Format(u32),
    Invalid(Vec<String>),
    HashMismatch,
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for SnapshotError {}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= x as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
