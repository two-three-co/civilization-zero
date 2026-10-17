//! Behaviour policies (the "decide" half of a tick, ADR-0005).
//!
//! A policy is a pure function `(Observation, RngStream) -> Intent`. It cannot
//! mutate the world; memory changes are returned and applied by the kernel.
//! Phase 1 ships two fixed policies: a heuristic forager and a uniform random
//! baseline (needed as a null model for later experiments).

use crate::action::Action;
use crate::agent::{Agent, AgentId, Memory};
use crate::config::WorldConfig;
use crate::resources::ResourceGrid;
use crate::rng::RngStream;
use crate::space::{Bounds, Dir, Pos};
use crate::spatial::SpatialIndex;
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
    /// All agents of the tick (id order) and the index built over them.
    pub agents: &'a [Agent],
    pub index: &'a SpatialIndex,
}

/// Another agent as seen through `Perception`: position and distance are
/// always known, the rest only when the config allows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Neighbor {
    pub id: AgentId,
    pub pos: Pos,
    /// Manhattan distance from the observer.
    pub distance: u32,
    pub policy: Option<PolicyKind>,
    pub inventory: Option<u32>,
    pub energy: Option<i64>,
}

impl Observation<'_> {
    pub fn food_here(&self) -> u32 {
        self.resources.food_at(self.agent.pos)
    }

    /// Other agents within `perception.social_radius`, nearest first and by
    /// id within a distance (ADR-0013 D2). Never includes the observer.
    pub fn neighbors(&self) -> Vec<Neighbor> {
        let p = &self.config.perception;
        self.index
            .within(self.agent.pos, p.social_radius)
            .into_iter()
            .map(|(distance, i)| (distance, &self.agents[i]))
            .filter(|(_, a)| a.id != self.agent.id)
            .map(|(distance, a)| Neighbor {
                id: a.id,
                pos: a.pos,
                distance,
                policy: p.see_policy.then_some(a.policy),
                inventory: p.see_inventory.then_some(a.inventory),
                energy: p.see_energy.then_some(a.energy),
            })
            .collect()
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
    // Uniform over the whole vocabulary, social actions included (ADR-0013
    // D3): the null model must be able to do everything a learner can.
    // Targets are drawn from adjacent agents; with nobody in reach the agent
    // rests instead.
    let adjacent = |rng: &mut RngStream| {
        let near: Vec<AgentId> = obs
            .neighbors()
            .into_iter()
            .filter(|n| n.distance <= 1)
            .map(|n| n.id)
            .collect();
        (!near.is_empty()).then(|| near[rng.below(near.len() as u64) as usize])
    };
    let action = match rng.below(10) {
        0..=3 => Action::Move(Dir::ALL[rng.below(4) as usize]),
        4 => Action::Gather,
        5 => Action::Consume(1),
        6 => Action::Rest,
        7 => adjacent(rng).map_or(Action::Rest, |to| Action::Transfer { to, n: 1 }),
        8 => Action::Store(1),
        _ => adjacent(rng).map_or(Action::Rest, |target| Action::Harm { target }),
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
    //    Two passes over the diamond (ADR-0011): pass 1 is branch-free and
    //    finds the best (food, nearness) key and how many cells hold it;
    //    pass 2 runs only on a tie and draws one lottery key per tied cell,
    //    in scan order, so the choice is uniform among them. Data-dependent
    //    branches per cell were the dominant cost of the single-pass scan.
    if a.inventory < cfg.inventory_capacity {
        let (w, h) = (obs.bounds.width as i32, obs.bounds.height as i32);
        let r = cfg.vision_radius as i32;
        let (cx, cy) = (a.pos.x as i32, a.pos.y as i32);
        let food = &obs.resources.food;

        let (mut best_key, mut best_x, mut best_y, mut ties) = (0u64, cx, cy, 0u32);
        scan_diamond(food, w, h, r, cx, cy, |x, y, key| {
            let better = key > best_key;
            let equal = key == best_key;
            ties = if better { 1 } else { ties + equal as u32 };
            best_x = if better { x } else { best_x };
            best_y = if better { y } else { best_y };
            best_key = best_key.max(key);
        });

        if best_key != 0 {
            let mut target = Pos::new(best_x as u16, best_y as u16);
            if ties > 1 {
                let mut win_key: Option<u64> = None;
                scan_diamond(food, w, h, r, cx, cy, |x, y, key| {
                    if key == best_key {
                        let k = rng.next_u64();
                        if win_key.is_none_or(|wk| k > wk) {
                            win_key = Some(k);
                            target = Pos::new(x as u16, y as u16);
                        }
                    }
                });
            }
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

/// Visits exactly the cells of `Bounds::diamond((cx, cy), r)` in the same
/// row-major order, walking each row as one contiguous slice of the food
/// grid. `key` ranks a cell by (food, nearness) and is 0 for cells that
/// cannot be a target (no food, or the agent's own cell).
#[inline(always)]
fn scan_diamond(
    food: &[u32],
    w: i32,
    h: i32,
    r: i32,
    cx: i32,
    cy: i32,
    mut visit: impl FnMut(i32, i32, u64),
) {
    for dy in -r..=r {
        let y = cy + dy;
        if y < 0 || y >= h {
            continue;
        }
        let span = r - dy.abs();
        let (x0, x1) = ((cx - span).max(0), (cx + span).min(w - 1));
        if x0 > x1 {
            continue;
        }
        let row = y as usize * w as usize;
        for (i, &f) in food[row + x0 as usize..=row + x1 as usize]
            .iter()
            .enumerate()
        {
            let x = x0 + i as i32;
            let dist = ((x - cx).abs() + dy.abs()) as u64;
            let valid = (f != 0) & !((dy == 0) & (x == cx));
            let key = (((f as u64) << 32) | (u32::MAX as u64 - dist)) * valid as u64;
            visit(x, y, key);
        }
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
