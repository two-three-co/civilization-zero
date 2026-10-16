//! World parameters. All quantities are integers (ADR-0003).

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
        let checks: [(bool, &'static str); 11] = [
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
                    && self.rest_heal >= 0,
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
        ];
        for (ok, msg) in checks {
            if !ok {
                return Err(ConfigError(msg));
            }
        }
        Ok(())
    }
}
