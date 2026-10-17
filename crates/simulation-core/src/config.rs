//! World parameters. All quantities are integers (ADR-0003).

use crate::nomos::Charter;
use crate::policy::PolicyParams;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub seed: u64,
    pub width: u16,
    pub height: u16,
    pub initial_agents: u32,

    // --- resources ---
    /// Number of resource patches generated at world creation.
    pub patch_count: u32,
    /// Manhattan radius of a patch.
    pub patch_radius: u16,
    /// Capacity at a patch centre (food units per cell).
    pub max_cell_capacity: u32,
    /// Food units added to each non-full cell every `regrow_interval` ticks.
    pub regrow_amount: u32,
    pub regrow_interval: u64,

    // --- agent physiology (energy in milli-units) ---
    pub initial_energy: i64,
    pub max_energy: i64,
    pub max_health: i64,
    pub inventory_capacity: u32,
    pub vision_radius: u16,
    /// Energy gained per food unit consumed.
    pub energy_per_food: i64,
    /// Paid every tick by every living agent.
    pub metabolism_cost: i64,
    pub move_cost: i64,
    pub gather_cost: i64,
    /// Food units requested per Gather action.
    pub gather_amount: u32,
    /// Health lost per tick while energy is zero.
    pub starvation_damage: i64,
    /// Health restored by Rest when energy > 0.
    pub rest_heal: i64,

    // --- interaction (ADR-0013 D3) ---
    /// Paid on every Transfer attempt.
    pub transfer_cost: i64,
    /// Paid on every Harm attempt. High by default so that violence is not
    /// free (RISKS.md R6).
    pub harm_cost: i64,
    /// Health taken from the target by one Harm.
    pub harm_damage: i64,
    /// Record `Event::Acted` for every agent every tick. Off by default: it
    /// multiplies ledger size by the population.
    pub record_actions: bool,

    // --- communication and memory (ADR-0013 D4, D5) ---
    /// Paid on every Signal.
    pub signal_cost: i64,
    /// How many interactions an agent remembers (`Agent.recent`); 0 disables memory.
    pub memory_slots: u8,
    /// Record `Event::Signaled` for every signal. Off by default.
    pub record_signals: bool,

    // --- life cycle (ADR-0008) ---
    /// A surviving agent whose energy is at or above this value after upkeep
    /// produces one child. `None` disables reproduction.
    pub birth_threshold: Option<i64>,
    /// Energy moved from parent to child at birth (conserved, not created).
    pub child_energy: i64,
    /// Agents die of old age once `age >= max_age`. `None` disables ageing
    /// deaths. When set, initial agents get a uniform random age in `0..max_age`
    /// so the founding cohort does not die in one tick.
    pub max_age: Option<u64>,
    /// Heritable policy parameters of the founding population.
    pub initial_params: PolicyParams,
    /// Children receive mutated parameters when true. Off by default.
    pub mutation_enabled: bool,
    /// Largest change per parameter per birth, in permille.
    pub mutation_step_permille: u16,

    // --- learning policies (ADR-0013 D6) ---
    /// Rows of a `Learner`'s Q table (`learner_states × Action::KIND_COUNT`
    /// i16 cells). 32 by default; 128 is the fine grid. 3-7 defines how an
    /// observation maps onto a row. `default` so older configs still load.
    #[serde(default = "default_learner_states")]
    pub learner_states: u16,
    /// A `Learner`'s child inherits the parent's Q table (true) or starts
    /// blank (false).
    #[serde(default = "default_true")]
    pub learner_inherit_q: bool,
    /// ε of a `Learner`'s ε-greedy choice, in permille (3-7): with this
    /// probability per decision the action kind is drawn uniformly instead
    /// of greedily. 50 by default; 0 explores only through the tie lottery
    /// of a fresh (all-zero) table. `default` so older configs still load.
    #[serde(default = "default_learner_epsilon")]
    pub learner_epsilon_permille: u16,
    /// Health term of a `Learner`'s reward (ADR-0014 D3): reward =
    /// Δenergy / 100 + `learner_health_weight` × Δhealth, both measured
    /// from the tick's start to the end of Upkeep (harm taken, starvation
    /// damage and Rest healing included). 0 by default, which is exactly
    /// the Phase 3 reward, so the learner golden hash is unchanged. Not
    /// part of `dynamics_hash`.
    #[serde(default)]
    pub learner_health_weight: u16,

    /// Track social facts (ADR-0014 D2, D3): first occupancy
    /// (`Agent.arrived`), encroachments (gathering where someone arrived
    /// earlier), and the audit of accepted deals (`World.nomos.deals`).
    /// Off by default: then `arrived` stays `None` for everyone, `nomos`
    /// stays empty, neither adds a byte to `dynamics_hash`, and the
    /// transition is the Phase 3 one. `default` so older configs load.
    #[serde(default)]
    pub nomos: bool,
    /// The assembly's charter at generation (ADR-0014 D5; `World.nomos.
    /// charter` thereafter, which interventions and, from 4-5, motions
    /// change). Only read with `nomos`. Not part of `dynamics_hash`.
    #[serde(default)]
    pub charter: Charter,
    /// Record `Event::Violated` for every violation (ADR-0014 D9). Off by
    /// default: 4-1 measured about 21 encroachments per tick in a crowded
    /// world, above the 10/tick budget; the ledger's counters and the open
    /// episodes are kept either way. Not part of `dynamics_hash`.
    #[serde(default)]
    pub record_violations: bool,

    /// What agents can perceive about each other (ADR-0013 D2). `default`
    /// so configs written before the field existed still load; it is always
    /// written (postcard cannot skip fields).
    #[serde(default)]
    pub perception: Perception,
}

fn default_learner_states() -> u16 {
    32
}

fn default_learner_epsilon() -> u16 {
    50
}

fn default_true() -> bool {
    true
}

/// Information limits on observing other agents. Position and distance are
/// always visible within `social_radius`; the rest is opt-in so that memory
/// and communication capacity can be experimental parameters (H2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Perception {
    /// Manhattan radius within which other agents are observable.
    pub social_radius: u16,
    /// Manhattan radius within which signals are heard and seen (ADR-0013 D5).
    pub signal_radius: u16,
    pub see_policy: bool,
    pub see_inventory: bool,
    pub see_energy: bool,
    /// Whether the normative facts of ADR-0014 D6 are perceived: the rules
    /// in force, who stands accused (`Observation::my_standing` /
    /// `adjacent_violator` / `rule` / `open_motion` / `i_voted`) and the
    /// memory of having been sanctioned. Off, a learning policy's norm
    /// features read 0 and its Harm actuator aims as it did without rules
    /// (the "blind" null model of EXP-004, ADR-0014 D11). `occupied()` is
    /// unaffected: who arrived first on the agent's own cell is a physical
    /// fact, not an institution. Not part of `dynamics_hash`. `default`
    /// so configs written before the field existed load as seeing.
    #[serde(default = "default_true")]
    pub see_standing: bool,
}

impl Default for Perception {
    fn default() -> Self {
        Self {
            social_radius: 2,
            signal_radius: 2,
            see_policy: false,
            see_inventory: true,
            see_energy: false,
            see_standing: true,
        }
    }
}

impl Perception {
    pub const MAX_RADIUS: u16 = 32;

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            width: 128,
            height: 128,
            initial_agents: 1_000,
            patch_count: 24,
            patch_radius: 8,
            max_cell_capacity: 20,
            regrow_amount: 1,
            regrow_interval: 10,
            initial_energy: 50_000,
            max_energy: 100_000,
            max_health: 100,
            inventory_capacity: 10,
            vision_radius: 4,
            energy_per_food: 4_000,
            metabolism_cost: 500,
            move_cost: 300,
            gather_cost: 200,
            gather_amount: 2,
            starvation_damage: 5,
            rest_heal: 1,
            transfer_cost: 100,
            harm_cost: 2_000,
            harm_damage: 20,
            record_actions: false,
            signal_cost: 0,
            memory_slots: 8,
            record_signals: false,
            birth_threshold: Some(80_000),
            child_energy: 40_000,
            max_age: Some(3_000),
            initial_params: PolicyParams::default(),
            mutation_enabled: false,
            mutation_step_permille: 50,
            learner_states: default_learner_states(),
            learner_inherit_q: true,
            learner_epsilon_permille: default_learner_epsilon(),
            learner_health_weight: 0,
            nomos: false,
            charter: Charter::default(),
            record_violations: false,
            perception: Perception::default(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ConfigError(pub &'static str);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid WorldConfig: {}", self.0)
    }
}

impl std::error::Error for ConfigError {}

impl WorldConfig {
    /// Largest Q table: 1,024 rows × 8 actions × 2 B = 16 KB per agent.
    pub const MAX_LEARNER_STATES: u16 = 1024;

    /// The institution world's baseline (ADR-0014 D10, `cz --preset nomos`):
    /// the default world with social facts tracked and the three values
    /// EXP-003's pre-registered decision rule fixed on 2026-10-17
    /// (docs/research/experiments/EXP-003-baseline-grid.md). Of the 12
    /// cells ε ∈ {0, 10, 50}‰ × harm cost ∈ {2,000, 8,000} × health weight
    /// ∈ {0, 2}, the eight with ε ≤ 10 met the rule's gates (no extinction
    /// in 10 seeds, a window band ≥ 70% of the forager's, violence ≤ 10% of
    /// the window's deaths; ε 50 is at 34–43% violence); the rule then takes
    /// the largest ε, and since neither health weight removed the "rest
    /// while hungry" row (row 9 stays Rest in 9–10 of 10 seeds either way)
    /// it keeps the default weight and the default harm cost. Rows stay 32:
    /// EXP-004 and the institution golden set `learner_states` per world.
    /// `default()` is untouched, so EXP-001 / EXP-002 and both golden hashes
    /// are unchanged.
    pub fn nomos_preset() -> Self {
        Self {
            nomos: true,
            learner_epsilon_permille: 10,
            harm_cost: 2_000,
            learner_health_weight: 0,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        let checks: [(bool, &'static str); 20] = [
            (
                self.width > 0 && self.height > 0,
                "width/height must be > 0",
            ),
            (self.regrow_interval > 0, "regrow_interval must be > 0"),
            (self.max_energy > 0, "max_energy must be > 0"),
            (
                (0..=self.max_energy).contains(&self.initial_energy),
                "initial_energy must be in 0..=max_energy",
            ),
            (self.max_health > 0, "max_health must be > 0"),
            (self.energy_per_food > 0, "energy_per_food must be > 0"),
            (
                self.energy_per_food <= self.max_energy,
                "energy_per_food must be <= max_energy",
            ),
            (
                self.metabolism_cost >= 0
                    && self.move_cost >= 0
                    && self.gather_cost >= 0
                    && self.starvation_damage >= 0
                    && self.rest_heal >= 0
                    && self.transfer_cost >= 0
                    && self.harm_cost >= 0
                    && self.harm_damage >= 0
                    && self.signal_cost >= 0,
                "costs must be non-negative",
            ),
            (self.gather_amount > 0, "gather_amount must be > 0"),
            (
                self.inventory_capacity > 0,
                "inventory_capacity must be > 0",
            ),
            (
                (self.width as u32) * (self.height as u32) <= 4_194_304,
                "grid larger than 2048x2048 is not supported",
            ),
            (
                self.birth_threshold.is_none_or(|t| {
                    self.child_energy > 0 && self.child_energy <= t && t <= self.max_energy
                }),
                "birth needs 0 < child_energy <= birth_threshold <= max_energy",
            ),
            (self.max_age.is_none_or(|m| m > 0), "max_age must be > 0"),
            (
                self.initial_params.is_valid(),
                "initial_params out of range",
            ),
            (
                self.mutation_step_permille <= PolicyParams::PERMILLE_MAX,
                "mutation_step_permille must be <= 1000",
            ),
            (
                self.perception.social_radius <= Perception::MAX_RADIUS
                    && self.perception.signal_radius <= Perception::MAX_RADIUS,
                "perception radii must be <= 32",
            ),
            (self.memory_slots <= 64, "memory_slots must be <= 64"),
            (
                (1..=Self::MAX_LEARNER_STATES).contains(&self.learner_states),
                "learner_states must be in 1..=1024",
            ),
            (
                self.learner_epsilon_permille <= PolicyParams::PERMILLE_MAX,
                "learner_epsilon_permille must be <= 1000",
            ),
            (self.charter.is_valid(), "charter permilles must be <= 1000"),
        ];
        for (ok, msg) in checks {
            if !ok {
                return Err(ConfigError(msg));
            }
        }
        Ok(())
    }
}
