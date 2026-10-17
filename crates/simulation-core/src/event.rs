//! Lifecycle events (ADR-0008).
//!
//! Every record carries its own tick and is a plain tagged enum, so a record
//! can be appended verbatim to the Phase 2 event ledger (one JSON object per
//! line) without translation. Phase 2 will add `Acted { agent, intent,
//! outcome }` next to these; nothing here should need to change for that.

use crate::action::{Action, Outcome};
use crate::agent::AgentId;
use crate::intervention::Intervention;
use crate::memory::Message;
use crate::space::Pos;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    /// Health reached zero while energy was exhausted.
    Starvation,
    /// `age >= max_age`.
    OldAge,
    /// Health reached zero in a tick in which the agent was harmed
    /// (ADR-0013 D3); takes precedence over starvation.
    Violence,
}

impl DeathCause {
    pub const COUNT: usize = 3;

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
    /// An external change applied by `World::intervene` before `tick` ran.
    Intervened {
        tick: u64,
        intervention: Intervention,
    },
    /// Food handed from one agent to an adjacent one (`Action::Transfer`).
    Transferred {
        tick: u64,
        from: AgentId,
        to: AgentId,
        amount: u32,
    },
    /// Health taken from an adjacent agent (`Action::Harm`).
    Harmed {
        tick: u64,
        by: AgentId,
        target: AgentId,
        damage: i64,
    },
    /// What an agent tried and what came of it. Recorded for every agent of
    /// every tick when `record_actions` is on (ADR-0013 D3): the raw material
    /// for defining rule violations in Phase 4 (R5).
    Acted {
        tick: u64,
        agent: AgentId,
        action: Action,
        outcome: Outcome,
    },
    /// A signal was made (`Action::Signal`). Recorded when `record_signals`.
    Signaled {
        tick: u64,
        agent: AgentId,
        message: Message,
    },
}

impl Event {
    pub fn tick(&self) -> u64 {
        match self {
            Event::Born { tick, .. }
            | Event::Died { tick, .. }
            | Event::Intervened { tick, .. }
            | Event::Transferred { tick, .. }
            | Event::Harmed { tick, .. }
            | Event::Acted { tick, .. }
            | Event::Signaled { tick, .. } => *tick,
        }
    }
}
