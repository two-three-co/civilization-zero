use crate::memory::{Interaction, Signal};
use crate::policy::{LearnerStep, PolicyKind, PolicyParams};
use crate::space::Pos;
use serde::{Deserialize, Serialize};

/// Never reused: ids are handed out in strictly increasing order for the whole
/// life of a world (ADR-0008), so an id in a ledger always names one body.
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
    /// Ticks lived. Invariant: `age < max_age` while alive (when set).
    pub age: u64,
    /// Inherited from the parent unchanged.
    pub policy: PolicyKind,
    /// Inherited from the parent, mutated only if `mutation_enabled`.
    pub params: PolicyParams,
    /// Policy-owned state, returned with each `Intent`.
    pub memory: Memory,
    /// What this agent last said and when (ADR-0013 D5). Kernel-written.
    pub last_signal: Option<Signal>,
    /// Oldest-first record of at most `memory_slots` interactions this agent
    /// took part in or witnessed (ADR-0013 D4). Kernel-written; policies
    /// only read it.
    pub recent: Vec<Interaction>,
    /// A `Learner`'s last decision, awaiting the next one to be credited
    /// (ADR-0013 D6, 3-7): the Q row it decided in, the action kind and the
    /// reward that tick earned. Kernel-written in Apply; `None` for every
    /// other kind, for a newborn and after a `SetPolicy`. Policies never
    /// read it.
    pub last_step: Option<LearnerStep>,
}
