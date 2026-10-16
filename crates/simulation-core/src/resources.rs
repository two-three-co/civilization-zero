//! Per-cell food stocks stored as two flat arrays (structure of arrays).

use crate::space::{Bounds, Pos};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceGrid {
    pub bounds: Bounds,
    pub food: Vec<u32>,
    pub capacity: Vec<u32>,
}

impl ResourceGrid {
    pub fn empty(bounds: Bounds) -> Self {
        let n = bounds.cell_count();
        Self {
            bounds,
            food: vec![0; n],
            capacity: vec![0; n],
        }
    }

    #[inline]
    pub fn food_at(&self, p: Pos) -> u32 {
        self.food[self.bounds.index(p)]
    }

    pub fn total_food(&self) -> u64 {
        self.food.iter().map(|&f| f as u64).sum()
    }

    /// Adds `amount` to every cell below capacity. Returns units added.
    pub fn regrow(&mut self, amount: u32) -> u64 {
        let mut added = 0u64;
        for (f, &cap) in self.food.iter_mut().zip(&self.capacity) {
            if *f < cap {
                let d = amount.min(cap - *f);
                *f += d;
                added += d as u64;
            }
        }
        added
    }
}
