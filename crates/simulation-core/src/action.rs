//! Actions and their outcomes.
//!
//! | Action   | Precondition                                   | Cost           | Effect                                   | Failure                         |
//! |----------|------------------------------------------------|----------------|------------------------------------------|---------------------------------|
//! | Move(d)  | target cell inside the grid                    | `move_cost`    | position := neighbour                    | `OutOfBounds` (no cost)         |
//! | Gather   | cell food > 0, inventory not full              | `gather_cost`  | food moves cell -> inventory (contended) | `NoResource`, `InventoryFull`, `LostContention` (cost paid) |
//! | Consume(n)| inventory > 0, room for >= 1 food of energy   | 0              | inventory -> energy                      | `EmptyInventory`, `EnergyFull`  |
//! | Rest     | -                                              | 0              | health += `rest_heal` if energy > 0      | -                               |
//!
//! `metabolism_cost` is paid every tick regardless of action. Failures here
//! are *physical* infeasibility; rule violations (Phase 4) are a different
//! concept and will never be represented by `Failure`.

use crate::space::Dir;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    Move(Dir),
    Gather,
    /// Consume up to `n` food units from inventory.
    Consume(u32),
    Rest,
}

impl Action {
    pub const KIND_COUNT: usize = 4;

    pub fn kind_index(self) -> usize {
        match self {
            Action::Move(_) => 0,
            Action::Gather => 1,
            Action::Consume(_) => 2,
            Action::Rest => 3,
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
}

impl Failure {
    pub const COUNT: usize = 6;

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Outcome {
    Ok,
    /// Gather that obtained fewer units than requested but more than zero.
    Partial,
    Failed(Failure),
}
