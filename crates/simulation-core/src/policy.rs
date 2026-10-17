//! Behaviour policies (the "decide" half of a tick, ADR-0005).
//!
//! A policy is a pure function `(Observation, RngStream) -> Intent`. It cannot
//! mutate the world; memory changes are returned and applied by the kernel.
//! Phase 1 ships two fixed policies: a heuristic forager and a uniform random
//! baseline (needed as a null model for later experiments). Phase 3 adds two
//! learning policies (ADR-0013 D6): `Reflex` (3-6: a linear score per action
//! kind over a fixed integer feature vector, weights inherited and mutated
//! at birth, starting from weights that reproduce the forager exactly) and
//! `Learner` (3-7, [`learner`]: within-life tabular Q over a discretised
//! observation, ε-greedy, integer-only; the policy decides and reports the
//! row it decided in, the kernel does the Q update in Apply). 3-5 provides
//! the kinds, their heritable tables, their serialisation and the
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

pub mod learner;

pub use learner::LearnerStep;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PolicyKind {
    Forager,
    Random,
    /// Evolving reflex policy (ADR-0013 D6, 3-6): a linear score per action
    /// kind over the `feature` vector, weights in `PolicyParams.table`,
    /// inherited and mutated at birth. Fresh agents start from
    /// `forager_weights()`, with which they decide exactly like the forager.
    Reflex,
    /// Within-life tabular Q learner (ADR-0013 D6, 3-7, [`learner`]): Q
    /// table in `PolicyParams.table`, one row per discretised observation
    /// (`learner::state`), ε-greedy over the action kinds, updated by the
    /// kernel in Apply from the agent's own energy change. Children inherit
    /// the table unless `learner_inherit_q` is off; mutation never touches
    /// it.
    Learner,
}

impl PolicyKind {
    pub const ALL: [PolicyKind; 4] = [
        PolicyKind::Forager,
        PolicyKind::Random,
        PolicyKind::Reflex,
        PolicyKind::Learner,
    ];

    /// Number of kinds; the length of the per-kind arrays of `TickReport`.
    pub const COUNT: usize = Self::ALL.len();

    /// The kind's position in `ALL` (its discriminant).
    pub fn index(self) -> usize {
        self as usize
    }

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

/// Number of integer features a `Reflex` policy scores (ADR-0013 D6). Their
/// meaning is fixed by `feature`; the count fixes the weight vector's shape.
pub const REFLEX_FEATURES: usize = 16;

/// Indices into a reflex policy's feature vector (ADR-0013 D6, 3-6). Every
/// feature is a small non-negative integer computed from the observation
/// alone: the agent's body, its cell and sight, its neighbours, what it
/// heard last tick and what its memory holds. Nothing here names an
/// institution; the weights decide what any of it means.
pub mod feature {
    /// Always 1: the per-action bias.
    pub const BIAS: usize = 0;
    /// `energy * 1000 < max_energy * hunger_permille`.
    pub const HUNGRY: usize = 1;
    /// `inventory > 0`.
    pub const HAS_FOOD: usize = 2;
    /// `inventory >= inventory_capacity`.
    pub const PACK_FULL: usize = 3;
    /// `energy + energy_per_food <= max_energy`: one more unit fits.
    pub const ROOM: usize = 4;
    /// Food on the agent's own cell, 0/1.
    pub const FOOD_HERE: usize = 5;
    /// Food units on the richest other cell within `vision_radius` while the
    /// pack has room; 0 when nothing is in sight or the pack is full (a full
    /// pack cannot gather, and skipping the scan then is what keeps the
    /// reflex as cheap as the forager).
    pub const FOOD_IN_SIGHT: usize = 6;
    /// `memory.target` is set.
    pub const HAS_TARGET: usize = 7;
    /// Other agents within `social_radius`, capped at `COUNT_CAP`.
    pub const NEIGHBOURS: usize = 8;
    /// Other agents within distance 1, capped at `COUNT_CAP`.
    pub const ADJACENT: usize = 9;
    /// `1 + symbol` of the newest `Say` heard last tick, 0 if none.
    pub const HEARD: usize = 10;
    /// A `HarmedBy` entry within `MEMORY_HORIZON` ticks.
    pub const HARMED_RECENTLY: usize = 11;
    /// A `ReceivedFrom` entry within `MEMORY_HORIZON` ticks.
    pub const RECEIVED_RECENTLY: usize = 12;
    /// A `SawHarm` entry within `MEMORY_HORIZON` ticks.
    pub const SAW_HARM_RECENTLY: usize = 13;
    /// A `Promised` entry within `MEMORY_HORIZON` ticks.
    pub const PROMISED_RECENTLY: usize = 14;
    /// Energy in eighths of `max_energy`: `min(8, energy / max(1, max_energy / 8))`,
    /// 0 (empty) to 8 (full).
    pub const ENERGY_LEVEL: usize = 15;
    /// How far back the memory flags look, in ticks.
    pub const MEMORY_HORIZON: u64 = 16;
    /// Cap of the count features.
    pub const COUNT_CAP: i64 = 15;
}

/// Fixed-point scale of the reflex weights: 1024 = 1.0.
pub const REFLEX_UNIT: i32 = 1024;

/// Largest magnitude a reflex weight may take (±1024.0): keeps every score
/// inside i64 whatever the features are, and mutation from drifting to
/// absurd scales. `PolicyParams::is_valid` enforces it.
pub const REFLEX_WEIGHT_MAX: i32 = 1 << 20;

const KIND_MOVE: usize = 0;
const KIND_GATHER: usize = 1;
const KIND_CONSUME: usize = 2;
const KIND_REST: usize = 3;
const KIND_TRANSFER: usize = 4;
const KIND_STORE: usize = 5;
const KIND_HARM: usize = 6;
const KIND_SIGNAL: usize = 7;

/// The weights with which a reflex agent decides exactly like the forager
/// (ADR-0013 3-6). The forager's rule order becomes a priority ladder —
/// Consume (≥ 1.0 when its condition holds) > Gather (0.5) > Move (0.0) >
/// everything else (−1.0) — and each condition a linear threshold over the
/// 0/1 features:
///
/// - Consume: `2·hungry + 2·has_food + 4·pack_full + 8·room − 11 > 0`, which
///   is `room ∧ ((hungry ∧ has_food) ∨ pack_full)`: the forager's rules 1
///   and 3 (rule 3 can never compete with Gather, since it needs a full pack).
/// - Gather: `food_here − pack_full − 0.5 > 0`, the forager's rule 2.
/// - Move: always 0; the actuator then applies the forager's rules 4 and 5.
///
/// Fresh reflex agents and `SetPolicy`'s `Default` start here, so evolution
/// begins at the forager and every reflex test has a known baseline.
pub fn forager_weights() -> Vec<i32> {
    use feature::*;
    let k = Action::KIND_COUNT;
    let mut w = vec![0i32; REFLEX_FEATURES * k];
    let mut set = |f: usize, kind: usize, value: i32| w[f * k + kind] = value;
    set(BIAS, KIND_CONSUME, -11 * REFLEX_UNIT);
    set(HUNGRY, KIND_CONSUME, 2 * REFLEX_UNIT);
    set(HAS_FOOD, KIND_CONSUME, 2 * REFLEX_UNIT);
    set(PACK_FULL, KIND_CONSUME, 4 * REFLEX_UNIT);
    set(ROOM, KIND_CONSUME, 8 * REFLEX_UNIT);
    set(BIAS, KIND_GATHER, -REFLEX_UNIT / 2);
    set(FOOD_HERE, KIND_GATHER, REFLEX_UNIT);
    set(PACK_FULL, KIND_GATHER, -REFLEX_UNIT);
    for kind in [KIND_REST, KIND_TRANSFER, KIND_STORE, KIND_HARM, KIND_SIGNAL] {
        set(BIAS, kind, -REFLEX_UNIT);
    }
    w
}

/// The numeric table a learning policy decides with (ADR-0013 D6). Lives in
/// `PolicyParams`, so it is inherited at birth and travels with snapshots.
/// Its shape must fit the agent's `PolicyKind` (`PolicyParams::fits`, checked
/// by `World::check_invariants`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyTable {
    /// `REFLEX_FEATURES × Action::KIND_COUNT` fixed-point weights
    /// (`REFLEX_UNIT` = 1.0), row-major by feature: the weight of feature
    /// `f` for action kind `k` is `weights[f * KIND_COUNT + k]`. Heritable;
    /// every weight moves by up to `mutation_step_permille` units at birth
    /// when `mutation_enabled`. Fresh agents start from `forager_weights()`.
    Reflex { weights: Vec<i32> },
    /// `learner_states × Action::KIND_COUNT` Q values (i16 fixed point,
    /// `learner::Q_UNIT` = one reward unit = 100 milli-energy), row-major by
    /// state (`learner::state_row` of the fine `learner::state` index).
    /// Updated within life by the kernel (3-7); children inherit it unless
    /// `learner_inherit_q` is off. Fresh agents start from all zeros, which
    /// ties every kind and so tries each once per state before preferring.
    Learner { q: Vec<i16> },
}

impl PolicyTable {
    pub const REFLEX_LEN: usize = REFLEX_FEATURES * Action::KIND_COUNT;

    /// Q table length for a config.
    pub fn learner_len(cfg: &WorldConfig) -> usize {
        cfg.learner_states as usize * Action::KIND_COUNT
    }

    /// The table a fresh agent of `kind` starts with: the forager-equivalent
    /// weights for `Reflex`, a zero Q table for `Learner`, `None` for the
    /// fixed policies.
    pub fn blank(kind: PolicyKind, cfg: &WorldConfig) -> Option<PolicyTable> {
        match kind {
            PolicyKind::Forager | PolicyKind::Random => None,
            PolicyKind::Reflex => Some(PolicyTable::Reflex {
                weights: forager_weights(),
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

    /// Every value within its bounds (`REFLEX_WEIGHT_MAX` for weights).
    pub fn is_valid(&self) -> bool {
        match self {
            PolicyTable::Reflex { weights } => weights.iter().all(|w| w.abs() <= REFLEX_WEIGHT_MAX),
            PolicyTable::Learner { .. } => true,
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
            && self.table.as_ref().is_none_or(PolicyTable::is_valid)
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

    /// Each scalar parameter moves by a uniform integer in `[-step, step]`
    /// (permille), clamped to its range, then — for a reflex table — every
    /// weight moves by a uniform integer in `[-step, step]` (1/1024 units),
    /// clamped to `±REFLEX_WEIGHT_MAX`. One draw per value, in field then
    /// table order, so a child's parameters are a function of its id's
    /// `Mutation` stream alone. A Q table is inherited unchanged.
    pub fn mutated(self, step: u16, rng: &mut RngStream) -> Self {
        let draw = |rng: &mut RngStream| rng.below(2 * step as u64 + 1) as i64 - step as i64;
        let hunger_permille =
            (self.hunger_permille as i64 + draw(rng)).clamp(0, Self::PERMILLE_MAX as i64) as u16;
        let table = match self.table {
            Some(PolicyTable::Reflex { mut weights }) => {
                let bound = REFLEX_WEIGHT_MAX as i64;
                for w in &mut weights {
                    *w = (*w as i64 + draw(rng)).clamp(-bound, bound) as i32;
                }
                Some(PolicyTable::Reflex { weights })
            }
            other => other,
        };
        Self {
            hunger_permille,
            table,
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

    /// How many others stand within `social_radius`, and how many of those
    /// within distance 1: read off the index's prefix sums, without building
    /// the sorted neighbour list.
    pub fn crowd(&self) -> (i64, i64) {
        let (near, adjacent) = self
            .index
            .crowd(self.agent.pos, self.config.perception.social_radius);
        // The observer stands in its own cell, which the diamond includes.
        ((near as i64 - 1).max(0), (adjacent as i64 - 1).max(0))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Intent {
    pub action: Action,
    pub memory: Memory,
    /// The Q-table row a `Learner` decided in (ADR-0013 3-7); `None` for
    /// every other kind. The kernel reads it in Apply to credit the agent's
    /// previous step and to record this one — the policy stays a pure
    /// function and never touches its table.
    pub state: Option<u16>,
}

pub fn decide(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    match obs.agent.policy {
        PolicyKind::Forager => forager(obs, rng),
        PolicyKind::Random => random(obs, rng),
        PolicyKind::Reflex => reflex(obs, rng),
        PolicyKind::Learner => learner(obs, rng),
    }
}

/// The reflex policy (ADR-0013 D6, 3-6). Builds the `feature` vector from
/// the observation, scores every action kind linearly with the agent's
/// weights, takes the best kind (ties: a lottery on the `Decide` stream,
/// drawn only when there is a tie) and lets a fixed actuator fill in the
/// kind's parameters: Move follows the forager's target rules, Gather marks
/// the cell as the target like the forager does, Transfer / Harm aim at the
/// nearest adjacent agent (and rest when alone), Signal echoes the symbol
/// heard last tick (0 when none). With `forager_weights()` the choice is the
/// forager's at every tick, and so is the RNG consumption.
fn reflex(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    let a = obs.agent;
    let Some(PolicyTable::Reflex { weights }) = &a.params.table else {
        // Unreachable for a world that passed `check_invariants`; the
        // forager is the defined fallback rather than a panic in the kernel.
        return forager(obs, rng);
    };
    // The sight scan runs only with room in the pack, exactly as the
    // forager's does (`FOOD_IN_SIGHT` is 0 otherwise).
    let scan = (a.inventory < obs.config.inventory_capacity).then(|| scan_best(obs));
    // The crowd features are the one costly input; they are left at 0 when
    // no weight of theirs is non-zero, which changes no score.
    let x = features(obs, scan.as_ref(), reads_crowd(weights));
    let scores = reflex_scores(weights, &x);
    let kind = argmax_kind(&scores, rng);
    reflex_act(obs, rng, kind, scan.as_ref(), x[feature::HEARD])
}

/// The feature vector of `obs` (see `feature`), given pass 1 of the scan
/// (`None` when the pack is full).
pub fn reflex_features(obs: &Observation<'_>, scan: Option<&Scan>) -> [i64; REFLEX_FEATURES] {
    features(obs, scan, true)
}

/// Whether any weight reads the crowd features.
fn reads_crowd(weights: &[i32]) -> bool {
    let k = Action::KIND_COUNT;
    let rows = feature::NEIGHBOURS * k..(feature::ADJACENT + 1) * k;
    weights[rows].iter().any(|&w| w != 0)
}

fn features(
    obs: &Observation<'_>,
    scan: Option<&Scan>,
    with_crowd: bool,
) -> [i64; REFLEX_FEATURES] {
    use feature::*;
    let a = obs.agent;
    let cfg = obs.config;
    let mut x = [0i64; REFLEX_FEATURES];
    x[BIAS] = 1;
    x[HUNGRY] = ((a.energy as i128) * 1000
        < (cfg.max_energy as i128) * (a.params.hunger_permille as i128)) as i64;
    x[HAS_FOOD] = (a.inventory > 0) as i64;
    x[PACK_FULL] = (a.inventory >= cfg.inventory_capacity) as i64;
    x[ROOM] = (a.energy + cfg.energy_per_food <= cfg.max_energy) as i64;
    x[FOOD_HERE] = (obs.food_here() > 0) as i64;
    x[FOOD_IN_SIGHT] = scan.map_or(0, Scan::food);
    x[HAS_TARGET] = a.memory.target.is_some() as i64;
    if with_crowd {
        let (near, adjacent) = obs.crowd();
        x[NEIGHBOURS] = near.min(COUNT_CAP);
        x[ADJACENT] = adjacent.min(COUNT_CAP);
    }
    // One pass over the memory, oldest first: the newest symbol heard last
    // tick wins, and each flag is set by any entry inside the horizon.
    for e in &a.recent {
        let fresh = (e.tick + MEMORY_HORIZON >= obs.tick) as i64;
        match e.kind {
            InteractionKind::Heard if e.tick + 1 == obs.tick => x[HEARD] = e.value + 1,
            InteractionKind::HarmedBy => x[HARMED_RECENTLY] |= fresh,
            InteractionKind::ReceivedFrom => x[RECEIVED_RECENTLY] |= fresh,
            InteractionKind::SawHarm => x[SAW_HARM_RECENTLY] |= fresh,
            InteractionKind::Promised => x[PROMISED_RECENTLY] |= fresh,
            _ => {}
        }
    }
    x[ENERGY_LEVEL] = (a.energy / (cfg.max_energy / 8).max(1)).min(8);
    x
}

/// Linear scores: `score[k] = Σ_f weights[f * KIND_COUNT + k] · x[f]`.
pub fn reflex_scores(weights: &[i32], x: &[i64; REFLEX_FEATURES]) -> [i64; Action::KIND_COUNT] {
    let mut s = [0i64; Action::KIND_COUNT];
    for (row, &xf) in weights.chunks_exact(Action::KIND_COUNT).zip(x.iter()) {
        if xf == 0 {
            continue;
        }
        for (score, &w) in s.iter_mut().zip(row) {
            *score += w as i64 * xf;
        }
    }
    s
}

/// The best-scoring kind; a tie among `n` kinds is settled by one
/// `rng.below(n)` draw (none when there is no tie).
pub fn argmax_kind(scores: &[i64; Action::KIND_COUNT], rng: &mut RngStream) -> usize {
    let (mut best, mut first, mut tied) = (scores[0], 0usize, 1u64);
    for (k, &v) in scores.iter().enumerate().skip(1) {
        if v > best {
            (best, first, tied) = (v, k, 1);
        } else if v == best {
            tied += 1;
        }
    }
    if tied == 1 {
        return first;
    }
    let pick = rng.below(tied) as usize;
    scores
        .iter()
        .enumerate()
        .filter(|(_, &v)| v == best)
        .nth(pick)
        .map(|(k, _)| k)
        .expect("a tied kind exists")
}

/// The agent a social action aims at: the first entry of
/// `Observation::neighbors()` within distance 1 — someone on the same cell
/// (lowest id) before someone on an adjacent cell (lowest id) — read straight
/// off the index without building the sorted list. `None` when nobody is
/// that close or the social radius does not reach them.
pub fn nearest_adjacent(obs: &Observation<'_>) -> Option<AgentId> {
    let me = obs.agent;
    let here = obs.index.cell(me.pos);
    if let Some(&i) = here.iter().find(|&&i| obs.agents[i as usize].id != me.id) {
        return Some(obs.agents[i as usize].id);
    }
    if obs.config.perception.social_radius == 0 {
        return None;
    }
    let mut best: Option<u32> = None;
    for d in Dir::ALL {
        if let Some(p) = obs.bounds.step(me.pos, d) {
            if let Some(&i) = obs.index.cell(p).first() {
                best = Some(best.map_or(i, |b| b.min(i)));
            }
        }
    }
    best.map(|i| obs.agents[i as usize].id)
}

/// Fills in the parameters of the chosen kind (see `reflex`).
fn reflex_act(
    obs: &Observation<'_>,
    rng: &mut RngStream,
    kind: usize,
    scan: Option<&Scan>,
    heard: i64,
) -> Intent {
    let a = obs.agent;
    let mut memory = a.memory;
    let nearest_adjacent = || nearest_adjacent(obs);
    let action = match kind {
        KIND_MOVE => return forage_move(obs, rng, memory, scan),
        KIND_GATHER => {
            memory.target = Some(a.pos);
            Action::Gather
        }
        KIND_CONSUME => Action::Consume(1),
        KIND_REST => Action::Rest,
        KIND_TRANSFER => {
            nearest_adjacent().map_or(Action::Rest, |to| Action::Transfer { to, n: 1 })
        }
        KIND_STORE => Action::Store(1),
        KIND_HARM => nearest_adjacent().map_or(Action::Rest, |target| Action::Harm { target }),
        _ => Action::Signal(Message::Say((heard - 1).clamp(0, 7) as u8)),
    };
    Intent {
        action,
        memory,
        state: None,
    }
}

/// The Q learner (ADR-0013 D6, 3-7): see [`learner`].
fn learner(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    learner::decide(obs, rng)
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
        state: None,
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
            state: None,
        };
    }
    // 2. Harvest where standing.
    if obs.food_here() > 0 && a.inventory < cfg.inventory_capacity {
        memory.target = Some(a.pos);
        return Intent {
            action: Action::Gather,
            memory,
            state: None,
        };
    }
    // 3. Top up energy opportunistically when the pack is full.
    if a.inventory >= cfg.inventory_capacity && room_for_food {
        return Intent {
            action: Action::Consume(1),
            memory,
            state: None,
        };
    }
    // 4. Head to the richest visible cell, else 5. explore (`forage_move`).
    //    The sight scan runs only with room in the pack, as it always did.
    let scan = (a.inventory < cfg.inventory_capacity).then(|| scan_best(obs));
    forage_move(obs, rng, memory, scan.as_ref())
}

/// Pass 1 of the forager's sight scan (ADR-0011): the best (food, nearness)
/// key over the diamond, where it lies, and how many cells share it.
pub struct Scan {
    best_key: u64,
    best_x: i32,
    best_y: i32,
    ties: u32,
}

impl Scan {
    /// Food units on the best cell (0 when nothing is in sight).
    pub fn food(&self) -> i64 {
        (self.best_key >> 32) as i64
    }
}

/// Two passes over the diamond (ADR-0011): this one is branch-free and finds
/// the best (food, nearness) key and how many cells hold it; `forage_move`
/// runs the second only on a tie, drawing one lottery key per tied cell in
/// scan order so the choice is uniform among them. Data-dependent branches
/// per cell were the dominant cost of the single-pass scan.
pub fn scan_best(obs: &Observation<'_>) -> Scan {
    let a = obs.agent;
    let (w, h) = (obs.bounds.width as i32, obs.bounds.height as i32);
    let r = obs.config.vision_radius as i32;
    let (cx, cy) = (a.pos.x as i32, a.pos.y as i32);
    let (mut best_key, mut best_x, mut best_y, mut ties) = (0u64, cx, cy, 0u32);
    scan_diamond(&obs.resources.food, w, h, r, cx, cy, |x, y, key| {
        let better = key > best_key;
        let equal = key == best_key;
        ties = if better { 1 } else { ties + equal as u32 };
        best_x = if better { x } else { best_x };
        best_y = if better { y } else { best_y };
        best_key = best_key.max(key);
    });
    Scan {
        best_key,
        best_x,
        best_y,
        ties,
    }
}

/// Forager rules 4 and 5: head to the richest visible cell (ties: nearer,
/// then the lottery over the tied cells), remembering it as the target;
/// with nothing in sight and the remembered target reached and depleted,
/// forget it; with no target, explore. `scan` is `None` when the pack is
/// full, in which case the agent only explores.
fn forage_move(
    obs: &Observation<'_>,
    rng: &mut RngStream,
    mut memory: Memory,
    scan: Option<&Scan>,
) -> Intent {
    let a = obs.agent;
    if let Some(s) = scan {
        if s.best_key != 0 {
            let mut target = Pos::new(s.best_x as u16, s.best_y as u16);
            if s.ties > 1 {
                let (w, h) = (obs.bounds.width as i32, obs.bounds.height as i32);
                let r = obs.config.vision_radius as i32;
                let (cx, cy) = (a.pos.x as i32, a.pos.y as i32);
                let mut win_key: Option<u64> = None;
                scan_diamond(&obs.resources.food, w, h, r, cx, cy, |x, y, key| {
                    if key == s.best_key {
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
                state: None,
            };
        }
    }
    // 5. Explore.
    Intent {
        action: Action::Move(Dir::ALL[rng.below(4) as usize]),
        memory,
        state: None,
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

    #[test]
    fn forager_weights_rank_consume_gather_move_in_that_order() {
        use feature::*;
        let w = forager_weights();
        assert_eq!(w.len(), PolicyTable::REFLEX_LEN);
        let mut x = [0i64; REFLEX_FEATURES];
        x[BIAS] = 1;
        let top = |x: &[i64; REFLEX_FEATURES]| {
            let s = reflex_scores(&w, x);
            let best = *s.iter().max().unwrap();
            assert_eq!(
                s.iter().filter(|&&v| v == best).count(),
                1,
                "no top ties: {s:?}"
            );
            s.iter().position(|&v| v == best).unwrap()
        };
        assert_eq!(top(&x), KIND_MOVE, "nothing to do: move");
        x[FOOD_HERE] = 1;
        assert_eq!(top(&x), KIND_GATHER);
        x[HUNGRY] = 1;
        assert_eq!(
            top(&x),
            KIND_GATHER,
            "hungry with an empty pack still gathers"
        );
        x[HAS_FOOD] = 1;
        assert_eq!(top(&x), KIND_GATHER, "no room to eat");
        x[ROOM] = 1;
        assert_eq!(top(&x), KIND_CONSUME, "hungry, fed and room: eat first");
        x[HUNGRY] = 0;
        assert_eq!(top(&x), KIND_GATHER, "not hungry: gather");
        x[PACK_FULL] = 1;
        assert_eq!(top(&x), KIND_CONSUME, "full pack with room: top up");
        x[ROOM] = 0;
        assert_eq!(top(&x), KIND_MOVE, "full pack, no room: move on");
    }

    #[test]
    fn argmax_draws_only_on_ties() {
        let mut a = RngStream::new(1, 2, 3, Stream::Decide);
        let mut b = RngStream::new(1, 2, 3, Stream::Decide);
        assert_eq!(argmax_kind(&[0, 5, 1, 5 - 1, 0, 0, 0, 0], &mut a), 1);
        assert_eq!(a.next_u64(), b.next_u64(), "no draw without a tie");
        let mut counts = [0u32; Action::KIND_COUNT];
        for tick in 0..4_000u64 {
            let mut r = RngStream::new(7, tick, 0, Stream::Decide);
            counts[argmax_kind(&[3, 3, 0, 3, 3, -1, 3, 3], &mut r)] += 1;
        }
        assert_eq!(counts[2] + counts[5], 0);
        for k in [0, 1, 3, 4, 6, 7] {
            // n = 4000, p = 1/6: mean 667, sd 23.6, 5 sd = 118.
            assert!((549..=785).contains(&counts[k]), "{counts:?}");
        }
    }

    #[test]
    fn reflex_weights_mutate_within_step_and_stay_bounded() {
        let cfg = WorldConfig::default();
        let p = PolicyParams::for_kind(PolicyKind::Reflex, &cfg);
        let mut a = RngStream::new(4, 5, 6, Stream::Mutation);
        let mut b = RngStream::new(4, 5, 6, Stream::Mutation);
        let x = p.clone().mutated(50, &mut a);
        assert_eq!(x, p.clone().mutated(50, &mut b), "deterministic");
        let (
            Some(PolicyTable::Reflex { weights: before }),
            Some(PolicyTable::Reflex { weights: after }),
        ) = (&p.table, &x.table)
        else {
            panic!("reflex tables");
        };
        assert!(before.iter().zip(after).all(|(b, a)| (a - b).abs() <= 50));
        assert_ne!(before, after);
        assert!(x.is_valid());
        let mut r = RngStream::new(9, 9, 9, Stream::Mutation);
        assert_eq!(p.clone().mutated(0, &mut r), p, "step 0 is the identity");
        let mut edge = p.clone();
        if let Some(PolicyTable::Reflex { weights }) = edge.table.as_mut() {
            weights.fill(REFLEX_WEIGHT_MAX);
        }
        let mut r = RngStream::new(1, 1, 1, Stream::Mutation);
        assert!(
            edge.mutated(1000, &mut r).is_valid(),
            "clamped at the bound"
        );
        let mut bad = p;
        if let Some(PolicyTable::Reflex { weights }) = bad.table.as_mut() {
            weights[0] = REFLEX_WEIGHT_MAX + 1;
        }
        assert!(!bad.is_valid());
    }
}
