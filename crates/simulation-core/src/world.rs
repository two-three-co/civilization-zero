//! World state and the deterministic transition `S(t+1) = F(S(t), A(t), E(t), R(t))`.
//!
//! One tick is executed in fixed phases (ADR-0002, ADR-0008):
//!
//! 1. **Decide**  – every living agent produces an `Intent` from a read-only
//!    view of `S(t)`. Pure; order-independent; parallelisable.
//! 2. **Consume** – private inventory → energy conversion.
//! 3. **Gather**  – resolved against `S(t)` positions. Claimants on the same
//!    cell are ordered by a seeded lottery key and served sequentially until
//!    the cell is empty, so food can never be taken twice.
//!    - *3b. Interact* – Transfer / Store / Harm / Signal, resolved at `S(t)`
//!      positions in actor (id) order (ADR-0013 D3, D5).
//! 4. **Move**    – all moves applied simultaneously (cells are unbounded,
//!    so moves never conflict).
//! 5. **Upkeep**  – metabolism + action costs, then Rest healing, then
//!    starvation damage, then ageing.
//!    - *5b. Memory* – the kernel appends what each agent gave, received,
//!      suffered, witnessed or heard this tick to `Agent.recent` (ADR-0013 D4).
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
use crate::intervention::{Intervention, InterventionError};
use crate::memory::{remember, Interaction, InteractionKind, Signal};
use crate::policy::{decide, Intent, Observation, PolicyKind};
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
    pub signals: u64,
    /// Net food added (+) or removed (-) by interventions.
    pub food_intervened: i64,
    /// Net energy added (+) or removed (-) by interventions.
    pub energy_intervened: i64,
    pub interventions: u64,
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
    /// Events in the order they happened: transfers, harms and signals (if
    /// recorded) in actor order, `Acted` (if recorded), then deaths in id
    /// order, then births in parent-id order.
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
                Agent {
                    id: AgentId(i as u64),
                    pos,
                    energy: config.initial_energy,
                    health: config.max_health,
                    inventory: 0,
                    age,
                    policy: policy_for(i),
                    params: config.initial_params,
                    memory: Memory::default(),
                    last_signal: None,
                    recent: Vec::new(),
                }
            })
            .collect();

        let ledger = Ledger {
            food_initial: resources.total_food(),
            energy_initial: config.initial_energy * agents.len() as i64,
            ..Ledger::default()
        };
        Ok(Self {
            next_agent_id: agents.len() as u64,
            config,
            tick: 0,
            resources,
            agents,
            ledger,
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

        for (a, it) in self.agents.iter_mut().zip(&intents) {
            a.memory = it.memory;
            report.actions[it.action.kind_index()] += 1;
        }

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
                if q < cfg.gather_amount.min(space) {
                    Outcome::Partial
                } else {
                    Outcome::Ok
                }
            };
        }

        // --- Phase 3b: transfer / store / harm (ADR-0013 D3) -------------
        // Resolved at S(t) positions in actor (id) order. Targets are named
        // by id; `agents` is sorted, so the lookup is a binary search.
        let mut harmed = vec![false; n];
        // What agents will remember of this tick, in order of occurrence
        // (ADR-0013 D4). Applied in phase 5b.
        let mut notes: Vec<(usize, Interaction)> = Vec::new();
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
                            notes.push((
                                i,
                                Interaction {
                                    tick,
                                    other: target,
                                    kind: InteractionKind::Harmed,
                                    value: cfg.harm_damage,
                                },
                            ));
                            notes.push((
                                j,
                                Interaction {
                                    tick,
                                    other: by,
                                    kind: InteractionKind::HarmedBy,
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
                                            kind: InteractionKind::SawHarm,
                                            value: target.0 as i64,
                                        },
                                    ));
                                }
                            }
                            report.events.push(Event::Harmed {
                                tick,
                                by: self.agents[i].id,
                                target,
                                damage: cfg.harm_damage,
                            });
                            Outcome::Ok
                        }
                    };
                }
                Action::Signal(message) => {
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
                _ => {}
            }
        }

        // --- Phase 4: move ------------------------------------------------
        let mut moved = vec![false; n];
        for (i, it) in intents.iter().enumerate() {
            if let Action::Move(d) = it.action {
                match bounds.step(self.agents[i].pos, d) {
                    Some(p) => {
                        self.agents[i].pos = p;
                        moved[i] = true;
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
                let mut params = parent.params;
                if cfg.mutation_enabled {
                    let mut rng = RngStream::new(cfg.seed, tick, child_id.0, Stream::Mutation);
                    params = params.mutated(cfg.mutation_step_permille, &mut rng);
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
        let j = self
            .agents
            .binary_search_by_key(&id, |a| a.id)
            .map_err(|_| Failure::NoSuchTarget)?;
        if j == actor {
            return Err(Failure::NoSuchTarget);
        }
        if self.agents[actor].pos.manhattan(self.agents[j].pos) > 1 {
            return Err(Failure::NotAdjacent);
        }
        Ok(j)
    }

    /// Applies an external change between ticks (ADR-0010 2a-1) and returns
    /// its ledger record. Never runs inside `apply`, uses no randomness, and
    /// on error leaves the state untouched.
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
                let (energy, health, params) = (
                    self.config.initial_energy,
                    self.config.max_health,
                    self.config.initial_params,
                );
                for _ in 0..n {
                    self.agents.push(Agent {
                        id: AgentId(self.next_agent_id),
                        pos,
                        energy,
                        health,
                        inventory: 0,
                        age: 0,
                        policy,
                        params,
                        memory: Memory::default(),
                        last_signal: None,
                        recent: Vec::new(),
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
                errs.push(format!("{:?} params out of range: {:?}", a.id, a.params));
            }
            if a.recent.len() > cfg.memory_slots as usize {
                errs.push(format!(
                    "{:?} remembers more than memory_slots: {}",
                    a.id,
                    a.recent.len()
                ));
            }
            if !self.bounds().contains(a.pos) {
                errs.push(format!("{:?} out of bounds: {:?}", a.id, a.pos));
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
pub const SNAPSHOT_FORMAT: u32 = 5;

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
