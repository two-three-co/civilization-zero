//! Lifecycle events (ADR-0008).
//!
//! Every record carries its own tick and is a plain tagged enum, so a record
//! can be appended verbatim to the Phase 2 event ledger (one JSON object per
//! line) without translation. Phase 2 will add `Acted { agent, intent,
//! outcome }` next to these; nothing here should need to change for that.

use crate::agent::AgentId;
use crate::space::Pos;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    /// Health reached zero while energy was exhausted.
    Starvation,
    /// `age >= max_age`.
    OldAge,
}

impl DeathCause {
    pub const COUNT: usize = 2;

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Born {
        tick: u64,
        child: AgentId,
        parent: AgentId,
        pos: Pos,
        /// Energy moved from the parent; the child starts with exactly this.
        energy: i64,
    },
    Died {
        tick: u64,
        agent: AgentId,
        cause: DeathCause,
        pos: Pos,
        age: u64,
        /// Energy the body still held; removed from the world via the ledger.
        energy_written_off: i64,
        /// Carried food returned to the cell the agent died on.
        food_dropped: u32,
    },
}

impl Event {
    pub fn tick(&self) -> u64 {
        match self {
            Event::Born { tick, .. } | Event::Died { tick, .. } => *tick,
        }
    }
}
