//! Behaviour policies (the "decide" half of a tick, ADR-0005).
//!
//! A policy is a pure function `(Observation, RngStream) -> Intent`. It cannot
//! mutate the world; memory changes are returned and applied by the kernel.
//! Phase 1 ships two fixed policies: a heuristic forager and a uniform random
//! baseline (needed as a null model for later experiments).

use crate::action::Action;
use crate::agent::{Agent, Memory};
use crate::config::WorldConfig;
use crate::resources::ResourceGrid;
use crate::rng::RngStream;
use crate::space::{Bounds, Dir, Pos};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PolicyKind {
    Forager,
    Random,
}

/// Heritable parameters of a policy (ADR-0008). A child copies its parent's
/// values; with `mutation_enabled` each value is perturbed deterministically.
/// Only the forager reads them today; `Random` ignores them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolicyParams {
    /// Eat from the inventory when `energy * 1000 < max_energy * hunger_permille`.
    pub hunger_permille: u16,
}

impl Default for PolicyParams {
    fn default() -> Self {
        Self {
            hunger_permille: 500,
        }
    }
}

impl PolicyParams {
    pub const PERMILLE_MAX: u16 = 1000;

    pub fn is_valid(&self) -> bool {
        self.hunger_permille <= Self::PERMILLE_MAX
    }

    /// Each parameter moves by a uniform integer in `[-step, step]`, clamped
    /// to its range. One draw per parameter, in field order.
    pub fn mutated(self, step: u16, rng: &mut RngStream) -> Self {
        let delta = rng.below(2 * step as u64 + 1) as i64 - step as i64;
        Self {
            hunger_permille: (self.hunger_permille as i64 + delta)
                .clamp(0, Self::PERMILLE_MAX as i64) as u16,
        }
    }
}

/// Read-only view an agent is allowed to see. Kept narrow on purpose: it is
/// the seam where Phase 3 cognition (and its information limits) plugs in.
pub struct Observation<'a> {
    pub agent: &'a Agent,
    pub resources: &'a ResourceGrid,
    pub bounds: Bounds,
    pub config: &'a WorldConfig,
}

impl Observation<'_> {
    pub fn food_here(&self) -> u32 {
        self.resources.food_at(self.agent.pos)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    pub action: Action,
    pub memory: Memory,
}

pub fn decide(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    match obs.agent.policy {
        PolicyKind::Forager => forager(obs, rng),
        PolicyKind::Random => random(obs, rng),
    }
}

fn random(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    let action = match rng.below(7) {
        0..=3 => Action::Move(Dir::ALL[rng.below(4) as usize]),
        4 => Action::Gather,
        5 => Action::Consume(1),
        _ => Action::Rest,
    };
    Intent {
        action,
        memory: obs.agent.memory,
    }
}

fn forager(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    let a = obs.agent;
    let cfg = obs.config;
    let mut memory = a.memory;
    let room_for_food = a.energy + cfg.energy_per_food <= cfg.max_energy;
    // i128 so that arbitrary (validated) i64 energies cannot overflow.
    let hungry =
        (a.energy as i128) * 1000 < (cfg.max_energy as i128) * (a.params.hunger_permille as i128);

    // 1. Eat when hungry.
    if a.inventory > 0 && hungry && room_for_food {
        return Intent {
            action: Action::Consume(1),
            memory,
        };
    }
    // 2. Harvest where standing.
    if obs.food_here() > 0 && a.inventory < cfg.inventory_capacity {
        memory.target = Some(a.pos);
        return Intent {
            action: Action::Gather,
            memory,
        };
    }
    // 3. Top up energy opportunistically when the pack is full.
    if a.inventory >= cfg.inventory_capacity && room_for_food {
        return Intent {
            action: Action::Consume(1),
            memory,
        };
    }
    // 4. Head to the richest visible cell (ties: nearer, then random).
    //    Lottery keys are drawn lazily, only when a cell ties the current
    //    best on (food, distance): each member of the winning tie class
    //    gets exactly one key, so the choice is still uniform among them,
    //    but a scan costs a handful of draws instead of one per cell.
    if a.inventory < cfg.inventory_capacity {
        let mut best: Option<(u32, u32, Option<u64>, Pos)> = None;
        for p in obs.bounds.diamond(a.pos, cfg.vision_radius) {
            let f = obs.resources.food_at(p);
            if f == 0 || p == a.pos {
                continue;
            }
            let rank = (f, u32::MAX - a.pos.manhattan(p));
            match best {
                Some((bf, bd, _, _)) if (bf, bd) > rank => {}
                Some((bf, bd, ref mut bkey, ref mut bpos)) if (bf, bd) == rank => {
                    let bk = *bkey.get_or_insert_with(|| rng.next_u64());
                    let k = rng.next_u64();
                    if k > bk {
                        *bkey = Some(k);
                        *bpos = p;
                    }
                }
                _ => best = Some((rank.0, rank.1, None, p)),
            }
        }
        if let Some((_, _, _, target)) = best {
            memory.target = Some(target);
        } else if memory.target == Some(a.pos) {
            memory.target = None; // arrived and it was depleted
        }
        if let Some(t) = memory.target {
            return Intent {
                action: Action::Move(toward(a.pos, t, rng)),
                memory,
            };
        }
    }
    // 5. Explore.
    Intent {
        action: Action::Move(Dir::ALL[rng.below(4) as usize]),
        memory,
    }
}

fn toward(from: Pos, to: Pos, rng: &mut RngStream) -> Dir {
    let dx = to.x as i32 - from.x as i32;
    let dy = to.y as i32 - from.y as i32;
    let horizontal = match dx.abs().cmp(&dy.abs()) {
        std::cmp::Ordering::Greater => true,
        std::cmp::Ordering::Less => false,
        std::cmp::Ordering::Equal => rng.chance(1, 2),
    };
    if horizontal {
        if dx > 0 {
            Dir::East
        } else {
            Dir::West
        }
    } else if dy > 0 {
        Dir::South
    } else {
        Dir::North
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Stream;

    #[test]
    fn mutation_stays_in_range_and_is_deterministic() {
        let p = PolicyParams {
            hunger_permille: 990,
        };
        let mut a = RngStream::new(1, 2, 3, Stream::Mutation);
        let mut b = RngStream::new(1, 2, 3, Stream::Mutation);
        for _ in 0..200 {
            let x = p.mutated(50, &mut a);
            assert_eq!(x, p.mutated(50, &mut b));
            assert!(x.is_valid());
        }
        let mut r = RngStream::new(9, 9, 9, Stream::Mutation);
        assert_eq!(p.mutated(0, &mut r), p, "step 0 is the identity");
    }
}
