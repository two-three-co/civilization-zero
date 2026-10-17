//! Social facts the kernel tracks when `config.nomos` is on (ADR-0014 D2,
//! D3; task 4-1): first occupancy, encroachment and the audit of accepted
//! deals. Rules, standing and sanctions (4-2) will live here too.
//!
//! Everything in this module is a fact the ledger and the state already
//! determine, read off at a fixed place in Apply with integer arithmetic
//! and no randomness. Nothing here changes physics: an encroachment costs
//! nothing, a broken promise fails nothing (ADR-0014 D1). With `nomos` off
//! none of it runs, `World.nomos` stays empty and adds no byte to
//! `dynamics_hash`.
//!
//! - **First occupancy** is arrival order: of the agents standing on a cell
//!   at the tick's start, the one with the smallest `Agent.arrived` (`None`,
//!   the founding cohort, is the oldest; ties go to the smaller id) is the
//!   cell's first occupant.
//! - **Encroachment**: agent A gathered at least one unit on cell c in tick
//!   t, and the first occupant B of c at S(t) is another agent with
//!   `arrived(B) < arrived(A)` strictly — arrivals in the same tick are
//!   not encroachments, nor is gathering where nobody arrived earlier.
//!   Judged right after the gather phase, on the cells that saw a gather.
//! - **Deals**: an `Accepted` opens an [`OpenDeal`]; every `Transferred`
//!   between the pair from the acceptance tick through the deadline
//!   (`offer_tick + by`) adds to `given` / `repaid`; at the deadline tick,
//!   after that tick's transfers (Apply phase 3c), the deal is settled:
//!   kept when `given >= give && repaid >= want`, else broken by whoever
//!   fell short. This is cz-web's `thread.rs` reading (ADR-0013 3-9b),
//!   except that transfers made between the offer and the acceptance are
//!   not seen here: the kernel keeps no transfer history, so a deal's
//!   window starts when it exists.

use crate::agent::{Agent, AgentId};
use serde::{Deserialize, Serialize};

/// Whether gathering by an agent that arrived at `mine` encroaches on a
/// first occupant that arrived at `first`: strictly earlier, with `None`
/// (founding) earlier than any tick.
pub fn encroaches(first: Option<u64>, mine: Option<u64>) -> bool {
    first < mine
}

/// The first occupant among `indices` (indices into `agents`, ascending):
/// the smallest `arrived`, ties to the smallest id, which is the first in
/// `indices` order. `None` for an empty cell.
pub fn first_occupant(agents: &[Agent], indices: &[u32]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for &i in indices {
        let i = i as usize;
        if best.is_none_or(|b| agents[i].arrived < agents[b].arrived) {
            best = Some(i);
        }
    }
    best
}

/// An accepted proposal the kernel is auditing (ADR-0014 D3): the terms
/// `Event::Accepted` repeated, plus what has been delivered so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpenDeal {
    pub proposer: AgentId,
    pub acceptor: AgentId,
    pub offer_tick: u64,
    /// Tick the acceptance was delivered; transfers count from this tick on.
    pub accepted_tick: u64,
    /// `offer_tick + by`: the last tick a transfer counts, and the tick the
    /// deal is settled in (or the acceptance tick if that is later).
    pub deadline: u64,
    /// Owed by the proposer to the acceptor.
    pub give: u8,
    /// Owed by the acceptor to the proposer.
    pub want: u8,
    /// Food the proposer handed the acceptor within the window.
    pub given: u32,
    /// Food the acceptor handed the proposer within the window.
    pub repaid: u32,
}

impl OpenDeal {
    /// Both sides delivered.
    pub fn kept(&self) -> bool {
        self.given >= self.give as u32 && self.repaid >= self.want as u32
    }

    /// The proposer still owes part of `give`.
    pub fn proposer_short(&self) -> bool {
        self.given < self.give as u32
    }

    /// The acceptor still owes part of `want`.
    pub fn acceptor_short(&self) -> bool {
        self.repaid < self.want as u32
    }

    /// Books a transfer of `amount` from `from` to `to` at `tick` against
    /// this deal if it is between the parties and inside the window.
    fn credit(&mut self, tick: u64, from: AgentId, to: AgentId, amount: u32) {
        if tick < self.accepted_tick || tick > self.deadline {
            return;
        }
        if from == self.proposer && to == self.acceptor {
            self.given += amount;
        } else if from == self.acceptor && to == self.proposer {
            self.repaid += amount;
        }
    }
}

/// The social-fact state of a world (ADR-0014 D2). Part of the snapshot;
/// hashed only when `config.nomos` is on. 4-2 adds rules, the charter,
/// motions and standing.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nomos {
    /// Accepted deals not yet settled, in acceptance order.
    pub deals: Vec<OpenDeal>,
}

impl Nomos {
    /// Nothing tracked (the state a world with `nomos` off must be in).
    pub fn is_empty(&self) -> bool {
        self.deals.is_empty()
    }

    /// Books one transfer against every open deal whose window holds it.
    /// Overlapping deals between the same pair both count it, as
    /// `thread.rs` does (ADR-0013 D5 addendum).
    pub fn credit_transfer(&mut self, tick: u64, from: AgentId, to: AgentId, amount: u32) {
        for d in &mut self.deals {
            d.credit(tick, from, to, amount);
        }
    }

    /// Removes and returns, in acceptance order, every deal whose deadline
    /// is `tick` or earlier (a late acceptance settles in its own tick).
    pub fn settle(&mut self, tick: u64) -> Vec<OpenDeal> {
        let mut due = Vec::new();
        self.deals.retain(|d| {
            if d.deadline <= tick {
                due.push(*d);
                false
            } else {
                true
            }
        });
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn founders_are_the_oldest_and_same_tick_arrivals_do_not_encroach() {
        assert!(encroaches(None, Some(0)));
        assert!(encroaches(Some(3), Some(4)));
        assert!(!encroaches(Some(4), Some(4)));
        assert!(!encroaches(None, None));
        assert!(!encroaches(Some(4), Some(3)));
        assert!(!encroaches(Some(1), None));
    }

    #[test]
    fn deals_credit_only_the_pair_inside_the_window_and_settle_at_the_deadline() {
        let (p, a, x) = (AgentId(1), AgentId(2), AgentId(3));
        let mut n = Nomos::default();
        n.deals.push(OpenDeal {
            proposer: p,
            acceptor: a,
            offer_tick: 5,
            accepted_tick: 6,
            deadline: 9,
            give: 2,
            want: 3,
            given: 0,
            repaid: 0,
        });
        n.credit_transfer(5, p, a, 2); // before the acceptance: unseen
        n.credit_transfer(6, p, a, 2); // the acceptance tick counts
        n.credit_transfer(7, a, p, 1);
        n.credit_transfer(7, x, p, 5); // a third party
        n.credit_transfer(10, a, p, 9); // after the deadline
        assert_eq!((n.deals[0].given, n.deals[0].repaid), (2, 1));
        assert!(n.settle(8).is_empty(), "not due yet");
        let due = n.settle(9);
        assert_eq!(due.len(), 1);
        assert!(n.is_empty());
        let d = due[0];
        assert!(!d.kept() && !d.proposer_short() && d.acceptor_short());
        let kept = OpenDeal { repaid: 3, ..d };
        assert!(kept.kept() && !kept.acceptor_short());
    }
}
