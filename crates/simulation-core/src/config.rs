//! World parameters. All quantities are integers (ADR-0003).

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

    /// What agents can perceive about each other (ADR-0013 D2). `default`
    /// so configs written before the field existed still load; it is always
    /// written (postcard cannot skip fields).
    #[serde(default)]
    pub perception: Perception,
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
}

impl Default for Perception {
    fn default() -> Self {
        Self {
            social_radius: 2,
            signal_radius: 2,
            see_policy: false,
            see_inventory: true,
            see_energy: false,
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
    pub fn validate(&self) -> Result<(), ConfigError> {
        let checks: [(bool, &'static str); 17] = [
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
        ];
        for (ok, msg) in checks {
            if !ok {
                return Err(ConfigError(msg));
            }
        }
        Ok(())
    }
}
