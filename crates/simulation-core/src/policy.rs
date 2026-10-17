//! Behaviour policies (the "decide" half of a tick, ADR-0005).
//!
//! A policy is a pure function `(Observation, RngStream) -> Intent`. It cannot
//! mutate the world; memory changes are returned and applied by the kernel.
//! Phase 1 ships two fixed policies: a heuristic forager and a uniform random
//! baseline (needed as a null model for later experiments). Phase 3 adds the
//! frame for two learning policies (ADR-0013 D6): `Reflex` (evolving
//! fixed-point weights, 3-6) and `Learner` (within-life tabular Q, 3-7).
//! Until those tasks land, both *decide like the forager*; what 3-5 provides
//! is the kinds, their heritable tables, their serialisation and the
//! `SetPolicy` intervention that switches a running world over to them.

use crate::action::Action;
use crate::agent::{Agent, AgentId, Memory};
use crate::config::WorldConfig;
use crate::memory::{InteractionKind, Message};
use crate::resources::ResourceGrid;
use crate::rng::RngStream;
use crate::space::{Bounds, Dir, Pos};
use crate::spatial::SpatialIndex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PolicyKind {
    Forager,
    Random,
    /// Evolving reflex policy (ADR-0013 D6): a linear score per action kind
    /// over a fixed feature vector, weights in `PolicyParams.table`,
    /// inherited and mutated. **3-5 frame: decides like the forager until
    /// 3-6 defines the features, the forager-equivalent initial weights and
    /// the weight mutation.**
    Reflex,
    /// Within-life tabular Q learner (ADR-0013 D6): Q table in
    /// `PolicyParams.table`, inherited by children unless
    /// `learner_inherit_q` is off. **3-5 frame: decides like the forager
    /// until 3-7 defines the state discretisation, reward and update.**
    Learner,
}

impl PolicyKind {
    pub const ALL: [PolicyKind; 4] = [
        PolicyKind::Forager,
        PolicyKind::Random,
        PolicyKind::Reflex,
        PolicyKind::Learner,
    ];

    /// Lower-case name as the CLI spells it.
    pub fn name(self) -> &'static str {
        match self {
            PolicyKind::Forager => "forager",
            PolicyKind::Random => "random",
            PolicyKind::Reflex => "reflex",
            PolicyKind::Learner => "learner",
        }
    }

    pub fn parse(name: &str) -> Option<PolicyKind> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    /// Whether agents of this kind carry a `PolicyTable`.
    pub fn has_table(self) -> bool {
        matches!(self, PolicyKind::Reflex | PolicyKind::Learner)
    }
}

/// Number of integer features a `Reflex` policy scores (ADR-0013 D6, about
/// 16: hunger, food here and in sight, neighbours, heard symbols, memory
/// flags …). 3-6 fixes their meaning; the frame fixes the count so the
/// weight vector has a shape to be serialised and inherited with.
pub const REFLEX_FEATURES: usize = 16;

/// The numeric table a learning policy decides with (ADR-0013 D6). Lives in
/// `PolicyParams`, so it is inherited at birth and travels with snapshots.
/// Its shape must fit the agent's `PolicyKind` (`PolicyParams::fits`, checked
/// by `World::check_invariants`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyTable {
    /// `REFLEX_FEATURES × Action::KIND_COUNT` fixed-point weights (1/1024
    /// units), row-major by feature. Heritable; mutated when
    /// `mutation_enabled` (3-6). The frame starts them at zero.
    Reflex { weights: Vec<i32> },
    /// `learner_states × Action::KIND_COUNT` Q values (i16 fixed point),
    /// row-major by state. Updated within life (3-7); children inherit it
    /// unless `learner_inherit_q` is off. The frame starts it at zero.
    Learner { q: Vec<i16> },
}

impl PolicyTable {
    pub const REFLEX_LEN: usize = REFLEX_FEATURES * Action::KIND_COUNT;

    /// Q table length for a config.
    pub fn learner_len(cfg: &WorldConfig) -> usize {
        cfg.learner_states as usize * Action::KIND_COUNT
    }

    /// The blank table a fresh agent of `kind` starts with (`None` for the
    /// fixed policies).
    pub fn blank(kind: PolicyKind, cfg: &WorldConfig) -> Option<PolicyTable> {
        match kind {
            PolicyKind::Forager | PolicyKind::Random => None,
            PolicyKind::Reflex => Some(PolicyTable::Reflex {
                weights: vec![0; Self::REFLEX_LEN],
            }),
            PolicyKind::Learner => Some(PolicyTable::Learner {
                q: vec![0; Self::learner_len(cfg)],
            }),
        }
    }

    /// Whether this table is the right shape for `kind` under `cfg`.
    pub fn fits(&self, kind: PolicyKind, cfg: &WorldConfig) -> bool {
        match (kind, self) {
            (PolicyKind::Reflex, PolicyTable::Reflex { weights }) => {
                weights.len() == Self::REFLEX_LEN
            }
            (PolicyKind::Learner, PolicyTable::Learner { q }) => q.len() == Self::learner_len(cfg),
            _ => false,
        }
    }
}

/// Heritable parameters of a policy (ADR-0008, ADR-0013 D6). A child copies
/// its parent's values; with `mutation_enabled` the scalar parameters are
/// perturbed deterministically (the reflex weights follow in 3-6). The
/// forager reads `hunger_permille`; `Random` ignores everything; the
/// learning kinds carry their `table`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PolicyParams {
    /// Eat from the inventory when `energy * 1000 < max_energy * hunger_permille`.
    pub hunger_permille: u16,
    /// Weights or Q table of a learning policy; `None` for the fixed ones.
    /// Contributes to `dynamics_hash` only when present, so worlds without
    /// learners keep their golden hash.
    pub table: Option<PolicyTable>,
}

impl Default for PolicyParams {
    fn default() -> Self {
        Self {
            hunger_permille: 500,
            table: None,
        }
    }
}

impl PolicyParams {
    pub const PERMILLE_MAX: u16 = 1000;

    pub fn is_valid(&self) -> bool {
        self.hunger_permille <= Self::PERMILLE_MAX
    }

    /// Whether `table` is what an agent of `kind` must carry under `cfg`:
    /// none for the fixed policies, a correctly shaped one for the learners.
    pub fn fits(&self, kind: PolicyKind, cfg: &WorldConfig) -> bool {
        match &self.table {
            None => !kind.has_table(),
            Some(t) => t.fits(kind, cfg),
        }
    }

    /// These parameters made to fit `kind`: a table that already fits is
    /// kept (a learner keeps what it learned), anything else is replaced by
    /// the blank table of the kind, or dropped when the kind has none.
    pub fn refit(mut self, kind: PolicyKind, cfg: &WorldConfig) -> Self {
        if !self.fits(kind, cfg) {
            self.table = PolicyTable::blank(kind, cfg);
        }
        self
    }

    /// The parameters a fresh agent of `kind` gets from the config's
    /// `initial_params` (its scalars, plus the blank table of the kind).
    pub fn for_kind(kind: PolicyKind, cfg: &WorldConfig) -> Self {
        cfg.initial_params.clone().refit(kind, cfg)
    }

    /// Each scalar parameter moves by a uniform integer in `[-step, step]`,
    /// clamped to its range. One draw per parameter, in field order. The
    /// table is inherited unchanged (weight mutation is 3-6).
    pub fn mutated(self, step: u16, rng: &mut RngStream) -> Self {
        let delta = rng.below(2 * step as u64 + 1) as i64 - step as i64;
        Self {
            hunger_permille: (self.hunger_permille as i64 + delta)
                .clamp(0, Self::PERMILLE_MAX as i64) as u16,
            table: self.table,
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
    /// The tick being decided; signals from `tick - 1` are the fresh ones.
    pub tick: u64,
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
    /// What the neighbour said last tick, if within `signal_radius`.
    pub signal: Option<Message>,
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
                signal: a
                    .last_signal
                    .filter(|s| s.tick + 1 == self.tick && distance <= p.signal_radius as u32)
                    .map(|s| s.message),
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
        PolicyKind::Reflex => reflex(obs, rng),
        PolicyKind::Learner => learner(obs, rng),
    }
}

/// 3-5 frame of the reflex policy: the weights in `agent.params.table` are
/// carried, inherited and hashed but not yet read. Decides exactly like the
/// forager, which is also the decision the forager-equivalent initial
/// weights of 3-6 must reproduce (ADR-0013 task 3-6).
fn reflex(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    debug_assert!(matches!(
        obs.agent.params.table,
        Some(PolicyTable::Reflex { .. })
    ));
    forager(obs, rng)
}

/// 3-5 frame of the Q learner: the Q table in `agent.params.table` is
/// carried, inherited and hashed but not yet read or updated. Decides
/// exactly like the forager until 3-7.
fn learner(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    debug_assert!(matches!(
        obs.agent.params.table,
        Some(PolicyTable::Learner { .. })
    ));
    forager(obs, rng)
}

fn random(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    // Uniform over the whole vocabulary, social actions and negotiation
    // included (ADR-0013 D3, D5): the null model must be able to do
    // everything a learner can. Targets are drawn from agents in reach
    // (adjacent for Transfer / Harm, within signal_radius for Propose); an
    // Accept or Reject answers the latest proposal in memory. With nobody in
    // reach or nothing to answer the agent rests instead.
    let within = |rng: &mut RngStream, reach: u32| {
        let near: Vec<AgentId> = obs
            .neighbors()
            .into_iter()
            .filter(|n| n.distance <= reach)
            .map(|n| n.id)
            .collect();
        (!near.is_empty()).then(|| near[rng.below(near.len() as u64) as usize])
    };
    let latest_offer = || {
        obs.agent
            .recent
            .iter()
            .rev()
            .find(|e| e.kind == InteractionKind::ProposalFrom)
    };
    let earshot = obs.config.perception.signal_radius as u32;
    let action = match rng.below(14) {
        0..=3 => Action::Move(Dir::ALL[rng.below(4) as usize]),
        4 => Action::Gather,
        5 => Action::Consume(1),
        6 => Action::Rest,
        7 => within(rng, 1).map_or(Action::Rest, |to| Action::Transfer { to, n: 1 }),
        8 => Action::Store(1),
        9 => within(rng, 1).map_or(Action::Rest, |target| Action::Harm { target }),
        10 => Action::Signal(Message::Say(rng.below(8) as u8)),
        11 => within(rng, earshot).map_or(Action::Rest, |to| {
            Action::Signal(Message::Propose {
                to,
                give: rng.below(3) as u8,
                want: rng.below(3) as u8,
                by: 1 + rng.below(16) as u16,
            })
        }),
        12 => latest_offer().map_or(Action::Rest, |offer| {
            Action::Signal(Message::Accept {
                to: offer.other,
                offer_tick: offer.tick,
            })
        }),
        _ => latest_offer().map_or(Action::Rest, |offer| {
            Action::Signal(Message::Reject { to: offer.other })
        }),
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
            table: None,
        };
        let mut a = RngStream::new(1, 2, 3, Stream::Mutation);
        let mut b = RngStream::new(1, 2, 3, Stream::Mutation);
        for _ in 0..200 {
            let x = p.clone().mutated(50, &mut a);
            assert_eq!(x, p.clone().mutated(50, &mut b));
            assert!(x.is_valid());
        }
        let mut r = RngStream::new(9, 9, 9, Stream::Mutation);
        assert_eq!(p.clone().mutated(0, &mut r), p, "step 0 is the identity");
    }

    #[test]
    fn tables_fit_their_kind_and_refit_keeps_what_fits() {
        let cfg = WorldConfig::default();
        for kind in PolicyKind::ALL {
            let p = PolicyParams::for_kind(kind, &cfg);
            assert!(p.fits(kind, &cfg), "{kind:?}");
            assert_eq!(p.table.is_some(), kind.has_table());
            for other in PolicyKind::ALL.into_iter().filter(|&o| o != kind) {
                assert!(
                    !p.fits(other, &cfg) || !(kind.has_table() || other.has_table()),
                    "{kind:?} params must not fit {other:?}"
                );
            }
        }
        // A learner keeps its Q when refit to Learner, loses it when refit
        // to Forager, and gets a blank reflex table when refit to Reflex.
        let mut learned = PolicyParams::for_kind(PolicyKind::Learner, &cfg);
        if let Some(PolicyTable::Learner { q }) = learned.table.as_mut() {
            q[3] = 77;
        }
        let kept = learned.clone().refit(PolicyKind::Learner, &cfg);
        assert_eq!(kept, learned);
        assert_eq!(learned.clone().refit(PolicyKind::Forager, &cfg).table, None);
        assert_eq!(
            learned.refit(PolicyKind::Reflex, &cfg).table,
            PolicyTable::blank(PolicyKind::Reflex, &cfg)
        );
        assert_eq!(PolicyTable::REFLEX_LEN, 128);
        assert_eq!(PolicyTable::learner_len(&cfg), 32 * Action::KIND_COUNT);
        assert_eq!(PolicyKind::parse("learner"), Some(PolicyKind::Learner));
        assert_eq!(PolicyKind::parse("Learner"), None);
    }
}
