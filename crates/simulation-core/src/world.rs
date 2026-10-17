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
//! 4. **Move**    – all moves applied simultaneously (cells are unbounded,
//!    so moves never conflict).
//! 5. **Upkeep**  – metabolism + action costs, then Rest healing, then
//!    starvation damage, then ageing.
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
use crate::policy::{decide, Intent, Observation, PolicyKind};
use crate::resources::ResourceGrid;
use crate::rng::{RngStream, Stream};
use crate::space::{Bounds, Pos};
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
    /// Lifecycle events in the order they happened: deaths in id order,
    /// then births in parent-id order.
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

    fn decide_one(&self, a: &Agent) -> Intent {
        let obs = Observation {
            agent: a,
            resources: &self.resources,
            bounds: self.bounds(),
            config: &self.config,
        };
        let mut rng = RngStream::new(self.config.seed, self.tick, a.id.0, Stream::Decide);
        decide(&obs, &mut rng)
    }

    /// Phase 1 of a tick: compute every agent's intent without mutating state.
    pub fn decide_all(&self) -> Vec<Intent> {
        self.agents.iter().map(|a| self.decide_one(a)).collect()
    }

    #[cfg(feature = "parallel")]
    pub fn decide_all_parallel(&self) -> Vec<Intent> {
        use rayon::prelude::*;
        // `collect` on an indexed parallel iterator preserves input order.
        self.agents.par_iter().map(|a| self.decide_one(a)).collect()
    }

    /// Advance one tick with the serial decision phase.
    pub fn step(&mut self) -> TickReport {
        let intents = self.decide_all();
        self.apply(intents)
    }

    /// Advance one tick deciding in parallel. Must equal `step` bit-for-bit.
    #[cfg(feature = "parallel")]
    pub fn step_parallel(&mut self) -> TickReport {
        let intents = self.decide_all_parallel();
        self.apply(intents)
    }

    pub fn run(&mut self, ticks: u64) {
        for _ in 0..ticks {
            self.step();
        }
    }

    /// Phases 2-8. `intents[i]` belongs to `self.agents[i]`.
    pub fn apply(&mut self, intents: Vec<Intent>) -> TickReport {
        assert_eq!(intents.len(), self.agents.len(), "one intent per agent");
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

        // --- Phase 6: death -----------------------------------------------
        let before = self.agents.len();
        let max_age = cfg.max_age;
        let (resources, ledger) = (&mut self.resources, &mut self.ledger);
        let events = &mut report.events;
        let mut deaths_by_cause = [0u32; DeathCause::COUNT];
        self.agents.retain(|a| {
            let cause = if a.health <= 0 {
                DeathCause::Starvation
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
            if !self.bounds().contains(a.pos) {
                errs.push(format!("{:?} out of bounds: {:?}", a.id, a.pos));
            }
        }
        let l = &self.ledger;
        let food_now = self.resources.total_food() + self.total_inventory();
        let food_expected = l.food_initial + l.food_regrown - l.food_consumed;
        if food_now != food_expected {
            errs.push(format!(
                "food not conserved: have {food_now}, ledger says {food_expected}"
            ));
        }
        let energy_now = self.total_energy();
        let energy_expected =
            l.energy_initial + l.energy_gained - l.energy_spent - l.energy_written_off;
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
        h.u64(self.tick);
        h.u64(self.next_agent_id);
        let l = &self.ledger;
        for v in [
            l.food_initial,
            l.food_regrown,
            l.food_consumed,
            l.births,
            l.deaths,
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
        ] {
            h.u64(v as u64);
        }
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
        }
        h.finish()
    }

    pub fn to_snapshot(&self) -> Vec<u8> {
        let snap = Snapshot {
            format: SNAPSHOT_FORMAT,
            state_hash: self.state_hash(),
            world: self.clone(),
        };
        serde_json::to_vec(&snap).expect("world serialises")
    }

    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let snap: Snapshot =
            serde_json::from_slice(bytes).map_err(|e| SnapshotError::Decode(e.to_string()))?;
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

/// Bumped whenever `World`'s serialised shape changes. 2: life cycle fields
/// (ADR-0008).
pub const SNAPSHOT_FORMAT: u32 = 2;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    format: u32,
    state_hash: u64,
    world: World,
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
