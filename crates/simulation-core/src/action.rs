//! Actions and their outcomes.
//!
//! | Action            | Precondition                                              | Cost                               | Effect                                                             | Failure                                                      |
//! |-------------------|-----------------------------------------------------------|------------------------------------|--------------------------------------------------------------------|--------------------------------------------------------------|
//! | Move(d)           | target cell inside the grid                               | `move_cost`                        | position := neighbour                                              | `OutOfBounds` (no cost)                                      |
//! | Gather            | cell food > 0, inventory not full                         | `gather_cost`                      | food moves cell -> inventory (contended)                           | `NoResource`, `InventoryFull`, `LostContention` (cost paid)  |
//! | Consume(n)        | inventory > 0, room for >= 1 food of energy               | 0                                  | inventory -> energy                                                | `EmptyInventory`, `EnergyFull`                               |
//! | Rest              | -                                                         | 0                                  | health += `rest_heal` if energy > 0                                | -                                                            |
//! | Transfer{to, n}   | `to` alive, not self, adjacent at S(t) (Manhattan <= 1), inventory > 0 | `transfer_cost` (paid on attempt) | min(n, inventory, `to`'s free space) food moves to `to`; actors resolved in id order | `NoSuchTarget`, `NotAdjacent`, `EmptyInventory`, `TargetFull` (cost paid) |
//! | Store(n)          | inventory > 0                                             | 0                                  | min(n, inventory) food onto the own cell (no capacity cap)         | `EmptyInventory`                                             |
//! | Harm{target}      | target alive, not self, adjacent at S(t)                  | `harm_cost` (paid on attempt)      | target.health -= `harm_damage`; stacks; a death this tick is `Violence` | `NoSuchTarget`, `NotAdjacent` (cost paid)                   |
//! | Signal(Say)       | -                                                         | `signal_cost`                      | `last_signal := (t, m)`; others within `signal_radius` at S(t) hear it now and see it next tick | -                              |
//! | Signal(Propose)   | `to` alive, not self, within `signal_radius` at S(t)      | `signal_cost` (paid on attempt)    | `to` remembers `ProposalFrom`; `Event::Proposed`                   | `NoSuchTarget`, `OutOfEarshot` (cost paid)                   |
//! | Signal(Accept)    | as Propose, and the actor remembers `ProposalFrom` from `to` at `offer_tick` | `signal_cost` (paid on attempt) | both remember `Promised`, `to` also `Accepted`; `Event::Accepted` | `NoSuchTarget`, `OutOfEarshot`, `NoSuchOffer` (cost paid)   |
//! | Signal(Reject)    | as Propose                                                | `signal_cost` (paid on attempt)    | `to` remembers `Rejected`; `Event::Rejected`                       | `NoSuchTarget`, `OutOfEarshot` (cost paid)                   |
//!
//! `metabolism_cost` is paid every tick regardless of action. Failures here
//! are *physical* infeasibility; rule violations (Phase 4) are a different
//! concept and will never be represented by `Failure`. The social actions
//! (ADR-0013 D3) are deliberately generic primitives: nothing here is named
//! after an institution. Negotiation messages (ADR-0013 D5, 3-4) are
//! delivered and recorded, never enforced: a broken promise is not a
//! `Failure` of anything, it is a fact the ledger shows.

use crate::agent::AgentId;
use crate::memory::Message;
use crate::space::Dir;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Move(Dir),
    Gather,
    /// Consume up to `n` food units from inventory.
    Consume(u32),
    Rest,
    /// Give up to `n` food units to an adjacent agent.
    Transfer {
        to: AgentId,
        n: u32,
    },
    /// Put up to `n` food units from the inventory onto the current cell.
    Store(u32),
    /// Take `harm_damage` health from an adjacent agent.
    Harm {
        target: AgentId,
    },
    /// Say something (ADR-0013 D5): agents within `signal_radius` hear it
    /// this tick and see it in the next tick's observation. Or address a
    /// negotiation message to one agent within `signal_radius` (3-4).
    Signal(Message),
}

impl Action {
    pub const KIND_COUNT: usize = 8;

    pub fn kind_index(self) -> usize {
        match self {
            Action::Move(_) => 0,
            Action::Gather => 1,
            Action::Consume(_) => 2,
            Action::Rest => 3,
            Action::Transfer { .. } => 4,
            Action::Store(_) => 5,
            Action::Harm { .. } => 6,
            Action::Signal(_) => 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Failure {
    OutOfBounds,
    NoResource,
    InventoryFull,
    /// Food was present but other claimants took it first.
    LostContention,
    EmptyInventory,
    EnergyFull,
    /// The target is more than one cell away (at the tick's start positions).
    NotAdjacent,
    /// The target's inventory had no room, so nothing moved.
    TargetFull,
    /// No *other* living agent has that id.
    NoSuchTarget,
    /// The addressee of a message is beyond `signal_radius` (at the tick's
    /// start positions). Distinct from `NotAdjacent`, whose reach is one cell.
    OutOfEarshot,
    /// `Accept` named a proposal the acceptor holds no memory of: no
    /// `ProposalFrom` from `to` at `offer_tick` in `recent`. One cannot
    /// accept what one does not hold, as one cannot give what one does not
    /// carry (`EmptyInventory`).
    NoSuchOffer,
}

impl Failure {
    pub const COUNT: usize = 11;

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Ok,
    /// Gather or Transfer that moved fewer units than requested but more than zero.
    Partial,
    Failed(Failure),
}
