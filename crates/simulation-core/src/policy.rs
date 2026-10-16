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

    // 1. Eat when hungry.
    if a.inventory > 0 && a.energy < cfg.max_energy / 2 && room_for_food {
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
    if a.inventory < cfg.inventory_capacity {
        let mut best: Option<(u32, u32, u64, Pos)> = None;
        for p in obs.bounds.diamond(a.pos, cfg.vision_radius) {
            let f = obs.resources.food_at(p);
            if f == 0 || p == a.pos {
                continue;
            }
            let key = (f, u32::MAX - a.pos.manhattan(p), rng.next_u64(), p);
            if best.is_none_or(|b| (key.0, key.1, key.2) > (b.0, b.1, b.2)) {
                best = Some(key);
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
