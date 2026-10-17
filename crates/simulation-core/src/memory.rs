//! Signals and kernel-written memory (ADR-0013 D4, D5).
//!
//! A `Signal` is what an agent last said and when; neighbours within the
//! perception's `signal_radius` see it in the next tick's observation and
//! remember hearing it. `Agent.recent` is a short, oldest-first record of
//! the interactions the kernel knows the agent took part in or witnessed.
//! Policies read it; only the kernel writes it (ADR-0005 stays intact).

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};

/// What a signal carries. `Say` is a symbol with no built-in meaning (R1:
/// the vocabulary's meaning is left to learning). Negotiation messages are
/// added in 3-4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Message {
    Say(u8),
}

impl Message {
    /// The payload as a memory `value`.
    pub fn value(self) -> i64 {
        match self {
            Message::Say(symbol) => symbol as i64,
        }
    }

    /// Stable encoding for `dynamics_hash`.
    pub fn hash_code(self) -> u64 {
        match self {
            Message::Say(symbol) => symbol as u64,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Signal {
    pub tick: u64,
    pub message: Message,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionKind {
    /// I gave `value` food to `other`.
    GaveTo,
    /// `other` gave me `value` food.
    ReceivedFrom,
    /// I took `value` health from `other`.
    Harmed,
    /// `other` took `value` health from me.
    HarmedBy,
    /// I saw `other` harm the agent whose id is `value`.
    SawHarm,
    /// I heard `other` say the symbol `value`.
    Heard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Interaction {
    pub tick: u64,
    pub other: AgentId,
    pub kind: InteractionKind,
    /// Amount, damage, target id or symbol, depending on `kind`.
    pub value: i64,
}

/// Appends to an oldest-first memory of at most `slots` entries. `slots ==
/// 0` means the agent remembers nothing.
pub fn remember(recent: &mut Vec<Interaction>, slots: usize, interaction: Interaction) {
    if slots == 0 {
        recent.clear();
        return;
    }
    while recent.len() >= slots {
        recent.remove(0);
    }
    recent.push(interaction);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_keeps_the_newest_entries_in_order() {
        let mut m = Vec::new();
        let e = |t: u64| Interaction {
            tick: t,
            other: AgentId(1),
            kind: InteractionKind::Heard,
            value: t as i64,
        };
        for t in 0..5 {
            remember(&mut m, 3, e(t));
        }
        assert_eq!(m, vec![e(2), e(3), e(4)]);
        remember(&mut m, 0, e(9));
        assert!(m.is_empty());
    }
}
