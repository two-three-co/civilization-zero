//! External changes to a running world (ADR-0010, task 2a-1).
//!
//! An intervention is applied *between* ticks by `World::intervene`, uses no
//! randomness, and is either applied completely or rejected without touching
//! the state. Each applied intervention is recorded as `Event::Intervened`,
//! so "same snapshot + same intervention sequence" replays to the same hash.
//! Food and energy an intervention adds or removes are booked in the ledger
//! (`food_intervened`, `energy_intervened`) so conservation stays checkable.

use crate::agent::AgentId;
use crate::policy::PolicyKind;
use crate::space::Pos;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Intervention {
    /// Replace the regrowth parameters. A drought lowers them; applying the
    /// original values again ends it.
    Drought {
        regrow_amount: u32,
        regrow_interval: u64,
    },
    /// Multiply every cell's food by `permille / 1000`, rounding down.
    /// `0..=MAX_PERMILLE`; values above 1000 are a resource boom.
    ScaleFood { permille: u32 },
    /// Add `n` agents on one cell with the config's initial energy and
    /// params, age 0 and empty memory.
    SpawnAgents {
        n: u32,
        pos: Pos,
        policy: PolicyKind,
    },
    /// Remove one living agent. Its carried food drops on its cell; its
    /// energy leaves the world through `energy_intervened`, not as a death.
    RemoveAgent { id: AgentId },
}

impl Intervention {
    pub const MAX_PERMILLE: u32 = 10_000;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterventionError {
    Invalid(&'static str),
    NoSuchAgent(AgentId),
}

impl std::fmt::Display for InterventionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterventionError::Invalid(why) => write!(f, "invalid intervention: {why}"),
            InterventionError::NoSuchAgent(id) => write!(f, "no living agent with id {}", id.0),
        }
    }
}

impl std::error::Error for InterventionError {}
