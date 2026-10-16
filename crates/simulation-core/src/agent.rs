use crate::policy::PolicyKind;
use crate::space::Pos;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AgentId(pub u64);

/// Minimal internal state carried across ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Memory {
    /// Cell the agent is currently heading to / exploiting.
    pub target: Option<Pos>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub pos: Pos,
    /// Milli-energy. Invariant: `0 <= energy <= max_energy`.
    pub energy: i64,
    /// Invariant: `0 < health <= max_health` while alive.
    pub health: i64,
    /// Carried food units. Invariant: `inventory <= inventory_capacity`.
    pub inventory: u32,
    pub age: u64,
    pub policy: PolicyKind,
    pub memory: Memory,
}
