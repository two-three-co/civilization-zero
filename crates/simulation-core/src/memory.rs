//! Signals, negotiation messages and kernel-written memory (ADR-0013 D4, D5).
//!
//! A `Signal` is what an agent last said and when; neighbours within the
//! perception's `signal_radius` see it in the next tick's observation and
//! remember hearing it. Negotiation messages (`Propose` / `Accept` /
//! `Reject`) are addressed to one agent within `signal_radius`: the kernel
//! delivers them into the addressee's memory and records them in the ledger,
//! and that is all it does — nothing holds anyone to a promise (3-4).
//! `Agent.recent` is a short, oldest-first record of the interactions the
//! kernel knows the agent took part in or witnessed. Policies read it; only
//! the kernel writes it (ADR-0005 stays intact).

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};

/// What a signal carries. `Say` is a symbol with no built-in meaning (R1:
/// the vocabulary's meaning is left to learning). The other three are
/// addressed to `to`, which must be another living agent within
/// `signal_radius` at the tick's start positions or the message fails
/// (`Failure::NoSuchTarget` / `Failure::OutOfEarshot`). They never touch
/// `Agent.last_signal`: a proposal is private correspondence, not speech.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Message {
    Say(u8),
    /// "I give you `give` food now; give me `want` food back within `by`
    /// ticks of this one." Delivered as `InteractionKind::ProposalFrom` and
    /// recorded as `Event::Proposed`. Whether anything is ever handed over
    /// is up to the parties: the kernel only carries the message.
    Propose {
        to: AgentId,
        give: u8,
        want: u8,
        /// Deadline relative to the proposal's tick (due tick = `tick + by`).
        by: u16,
    },
    /// Accepts the proposal `to` made at `offer_tick`. The acceptor must still
    /// remember receiving it (`ProposalFrom` from `to` at that tick in
    /// `recent`), otherwise `Failure::NoSuchOffer`. On success both parties
    /// get a `Promised` memory and the ledger gets `Event::Accepted`.
    Accept {
        to: AgentId,
        offer_tick: u64,
    },
    /// Declines whatever `to` proposed. Delivered as `Rejected`, recorded as
    /// `Event::Rejected`; the kernel does not check that an offer existed.
    Reject {
        to: AgentId,
    },
}

impl Message {
    /// The payload as a memory `value`: the symbol for `Say`, the packed
    /// `Terms` for `Propose`, the offer tick for `Accept`, 0 for `Reject`.
    pub fn value(self) -> i64 {
        match self {
            Message::Say(symbol) => symbol as i64,
            Message::Propose { give, want, by, .. } => Terms { give, want, by }.pack(),
            Message::Accept { offer_tick, .. } => offer_tick as i64,
            Message::Reject { .. } => 0,
        }
    }

    /// Stable encoding for `dynamics_hash`. `Say` keeps its 3-3 encoding (the
    /// bare symbol); the addressed messages fold a variant tag, the
    /// addressee and the payload into one word.
    pub fn hash_code(self) -> u64 {
        match self {
            Message::Say(symbol) => symbol as u64,
            Message::Propose { to, give, want, by } => {
                (1 << 60) ^ (to.0 << 32) ^ Terms { give, want, by }.pack() as u64
            }
            Message::Accept { to, offer_tick } => {
                (2 << 60) ^ (to.0 << 32) ^ offer_tick.wrapping_mul(0x9e37_79b9_7f4a_7c15)
            }
            Message::Reject { to } => (3 << 60) ^ to.0,
        }
    }

    /// The agent a message is addressed to; `None` for `Say`, which is heard
    /// by everyone in earshot.
    pub fn addressee(self) -> Option<AgentId> {
        match self {
            Message::Say(_) => None,
            Message::Propose { to, .. } | Message::Accept { to, .. } | Message::Reject { to } => {
                Some(to)
            }
        }
    }
}

/// The terms of a proposal as they travel through a memory `value`:
/// `give` in bits 0..8, `want` in bits 8..16, `by` in bits 16..32. The
/// acceptor's policy reads them from its `ProposalFrom` entry; the kernel
/// reads them back when the proposal is accepted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Terms {
    pub give: u8,
    pub want: u8,
    /// Ticks after the proposal's tick by which `want` is due.
    pub by: u16,
}

impl Terms {
    pub fn pack(self) -> i64 {
        self.give as i64 | (self.want as i64) << 8 | (self.by as i64) << 16
    }

    /// Inverse of `pack`; bits above 32 are ignored.
    pub fn unpack(value: i64) -> Self {
        Self {
            give: (value & 0xff) as u8,
            want: ((value >> 8) & 0xff) as u8,
            by: ((value >> 16) & 0xffff) as u16,
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
    /// `other` proposed a deal to me; `value` is `Terms::pack()` of it. The
    /// entry's `tick` is the `offer_tick` an `Accept` must name.
    ProposalFrom,
    /// `other` accepted my proposal; `value` is `Terms::pack()` of the deal.
    Accepted,
    /// `other` declined my proposal; `value` is 0.
    Rejected,
    /// I promised to give `value` food to `other`: for the proposer that is
    /// the deal's `give` (due now), for the acceptor its `want` (due by the
    /// proposal's deadline). Written to both parties when an acceptance is
    /// delivered. Nothing enforces it; keeping it is a `Transfer` like any
    /// other, and breaking it is a fact the ledger can show (ADR-0013 D5).
    Promised,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Interaction {
    pub tick: u64,
    pub other: AgentId,
    pub kind: InteractionKind,
    /// Amount, damage, target id, symbol or packed terms, depending on `kind`.
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

    #[test]
    fn terms_round_trip_through_a_memory_value() {
        for t in [
            Terms::default(),
            Terms {
                give: 255,
                want: 0,
                by: 65_535,
            },
            Terms {
                give: 2,
                want: 3,
                by: 10,
            },
        ] {
            assert_eq!(Terms::unpack(t.pack()), t);
            assert!(t.pack() >= 0 && t.pack() < 1 << 32);
        }
        let m = Message::Propose {
            to: AgentId(7),
            give: 2,
            want: 3,
            by: 10,
        };
        assert_eq!(
            Terms::unpack(m.value()),
            Terms {
                give: 2,
                want: 3,
                by: 10
            }
        );
        assert_eq!(m.addressee(), Some(AgentId(7)));
        assert_eq!(Message::Say(1).addressee(), None);
    }

    #[test]
    fn hash_codes_tell_messages_apart() {
        let to = AgentId(3);
        let codes = [
            Message::Say(0).hash_code(),
            Message::Say(5).hash_code(),
            Message::Propose {
                to,
                give: 0,
                want: 0,
                by: 0,
            }
            .hash_code(),
            Message::Propose {
                to,
                give: 1,
                want: 0,
                by: 0,
            }
            .hash_code(),
            Message::Accept { to, offer_tick: 0 }.hash_code(),
            Message::Accept { to, offer_tick: 1 }.hash_code(),
            Message::Reject { to }.hash_code(),
            Message::Reject { to: AgentId(4) }.hash_code(),
        ];
        for (i, a) in codes.iter().enumerate() {
            for b in &codes[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(Message::Say(5).hash_code(), 5, "3-3 encoding kept");
    }
}
