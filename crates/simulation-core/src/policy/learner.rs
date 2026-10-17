//! The Q learner (ADR-0013 D6, 3-7): within-life tabular Q learning over a
//! discretised observation, all in integers.
//!
//! The policy half is pure, as every policy is (ADR-0005): it discretises the
//! observation into a row of the agent's Q table (`PolicyParams.table`),
//! picks an action kind ε-greedily with the `Decide` stream and lets the
//! reflex actuators fill in the kind's parameters. It reports the row it
//! decided in through `Intent.state`. The learning half is the kernel's: in
//! Apply it credits the *previous* step — `Q[s, a] += (r + γ·max Q[s'] − Q[s, a]) >> 4`
//! with the row just decided in as `s'` — and records this step
//! (`Agent.last_step`, kernel-owned like `recent`) for the next tick to
//! credit. Rewards are the agent's own energy change (ADR-0013 D6), so
//! nothing here knows about institutions, sharing or norms.

use super::{argmax_kind, reflex_act, scan_best, Intent, Observation, PolicyTable, Scan};
use crate::action::Action;
use crate::memory::InteractionKind;
use crate::rng::RngStream;
use crate::space::Dir;
use serde::{Deserialize, Serialize};

/// Rows of the fine grid: every `state` bit in use (7 bits). A table with
/// fewer rows folds the grid with `state_row`; one with more never visits the
/// rows beyond this.
pub const FINE_STATES: u16 = 128;

/// Bits of the fine state index, most important first, so that a table of
/// `2^k` rows (`state_row`) keeps the `k` most important ones: 32 rows (the
/// default) see the five body-and-sight bits, 128 all seven. Each bit is a
/// 0/1 fact about the observation; none names an institution.
pub mod state {
    /// `energy * 1000 < max_energy * hunger_permille` (the forager's rule 1
    /// threshold; the same `HUNGRY` as the reflex feature).
    pub const HUNGRY: u16 = 1 << 0;
    /// `inventory > 0`: Consume can do something.
    pub const HAS_FOOD: u16 = 1 << 1;
    /// Food on the agent's own cell: Gather can do something.
    pub const FOOD_HERE: u16 = 1 << 2;
    /// Food on some other cell within `vision_radius`, scanned only while the
    /// pack has room (a full pack cannot gather; skipping the scan then is
    /// what keeps the learner as cheap as the forager). 0 with a full pack.
    pub const FOOD_IN_SIGHT: u16 = 1 << 3;
    /// Another agent within distance 1 and within `social_radius`: exactly
    /// when `nearest_adjacent` finds a Transfer / Harm target.
    pub const ADJACENT: u16 = 1 << 4;
    /// The upper half of the agent's hunger band, which with `HUNGRY` makes
    /// four energy levels: hungry and below half the threshold (starving),
    /// hungry above it, fed below the midpoint between the threshold and
    /// `max_energy`, fed above it.
    pub const UPPER: u16 = 1 << 5;
    /// A `Say` was heard last tick.
    pub const HEARD: u16 = 1 << 6;
}

/// Reward unit: milli-energy / 100 (ADR-0013 D6). A consumed food unit of
/// the default world is +40, a tick's metabolism −5, a move −3.
pub const REWARD_DIVISOR: i64 = 100;
/// Fixed point of the Q values: 16 = one reward unit. Without fractional
/// bits the `>> 4` step would floor every TD error below one unit to zero.
pub const Q_UNIT: i64 = 16;
/// Learning rate α = 1 / 2^4.
pub const ALPHA_SHIFT: u32 = 4;
/// Discount γ = 29 / 32 (a horizon of about ten ticks).
pub const GAMMA_NUM: i64 = 29;
pub const GAMMA_SHIFT: u32 = 5;

/// What the kernel remembers of a learner's last decision until the next
/// one tells it `s'` (ADR-0013 3-7). Kernel-owned, like `Agent.recent`:
/// policies never see it, children never inherit it, a `SetPolicy` clears
/// it. `None` adds no bytes to `dynamics_hash`, so worlds without learners
/// keep their golden hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LearnerStep {
    /// Row of the Q table the agent decided in.
    pub state: u16,
    /// `Action::kind_index` of what it did.
    pub action: u8,
    /// Its energy change during that tick in reward units, before the birth
    /// transfer (energy handed to a child counts as kept: the birth bonus).
    pub reward: i16,
}

/// The fine (7-bit) state of `obs`, given pass 1 of the sight scan (`None`
/// with a full pack) and the symbol heard last tick + 1 (0 when none).
pub fn learner_state(obs: &Observation<'_>, scan: Option<&Scan>, heard: i64) -> u16 {
    let a = obs.agent;
    let cfg = obs.config;
    // i128 so that arbitrary (validated) i64 energies cannot overflow.
    let e = a.energy as i128 * 1000;
    let threshold = cfg.max_energy as i128 * a.params.hunger_permille as i128;
    let hungry = e < threshold;
    let upper = if hungry {
        e * 2 >= threshold
    } else {
        e * 2 >= threshold + cfg.max_energy as i128 * 1000
    };
    let mut s = 0u16;
    s |= state::HUNGRY * hungry as u16;
    s |= state::HAS_FOOD * (a.inventory > 0) as u16;
    s |= state::FOOD_HERE * (obs.food_here() > 0) as u16;
    s |= state::FOOD_IN_SIGHT * (scan.is_some_and(|s| s.food() > 0)) as u16;
    s |= state::ADJACENT * someone_adjacent(obs) as u16;
    s |= state::UPPER * upper as u16;
    s |= state::HEARD * (heard > 0) as u16;
    s
}

/// The row of a table with `rows` rows that a fine state falls into: the
/// fine index modulo `rows`, so a power of two keeps the lowest (most
/// important) bits and any other row count is a deterministic aliasing.
pub fn state_row(fine: u16, rows: u16) -> u16 {
    fine % rows.max(1)
}

/// Whether another agent stands on the observer's cell or, when
/// `social_radius` reaches that far, on one of the four adjacent cells —
/// the condition under which `nearest_adjacent` finds someone. Read off the
/// index with early exit; no list is built.
fn someone_adjacent(obs: &Observation<'_>) -> bool {
    let me = obs.agent;
    if obs.index.cell(me.pos).len() > 1 {
        return true;
    }
    if obs.config.perception.social_radius == 0 {
        return false;
    }
    Dir::ALL.iter().any(|&d| {
        obs.bounds
            .step(me.pos, d)
            .is_some_and(|p| !obs.index.cell(p).is_empty())
    })
}

/// `1 + symbol` of the newest `Say` heard last tick, 0 when none (the reflex
/// `HEARD` feature). One pass over the memory, oldest first.
pub fn heard_last_tick(obs: &Observation<'_>) -> i64 {
    let mut heard = 0;
    for e in &obs.agent.recent {
        if e.kind == InteractionKind::Heard && e.tick + 1 == obs.tick {
            heard = e.value + 1;
        }
    }
    heard
}

/// The learner's decision (see the module doc). Consumes the `Decide`
/// stream in a fixed order: the ε draw (only when ε > 0), then either the
/// exploring kind or the greedy tie lottery (only on a tie), then whatever
/// the actuator draws (the forager's move lottery).
pub(super) fn decide(obs: &Observation<'_>, rng: &mut RngStream) -> Intent {
    let a = obs.agent;
    let cfg = obs.config;
    let k = Action::KIND_COUNT;
    let q = match &a.params.table {
        Some(PolicyTable::Learner { q }) if q.len() >= k => q,
        // Unreachable for a world that passed `check_invariants`; the
        // forager is the defined fallback rather than a panic in the kernel.
        _ => return super::forager(obs, rng),
    };
    let rows = q.len() / k;
    let scan = (a.inventory < cfg.inventory_capacity).then(|| scan_best(obs));
    let heard = heard_last_tick(obs);
    let s = state_row(learner_state(obs, scan.as_ref(), heard), rows as u16);
    let row = &q[s as usize * k..(s as usize + 1) * k];
    let eps = cfg.learner_epsilon_permille as u64;
    let kind = if eps > 0 && rng.chance(eps, 1000) {
        rng.below(k as u64) as usize
    } else {
        greedy_kind(row, rng)
    };
    let it = reflex_act(obs, rng, kind, scan.as_ref(), heard);
    Intent {
        state: Some(s),
        ..it
    }
}

/// The best kind of a Q row; ties go to the `argmax_kind` lottery.
pub fn greedy_kind(row: &[i16], rng: &mut RngStream) -> usize {
    let mut scores = [0i64; Action::KIND_COUNT];
    for (score, &v) in scores.iter_mut().zip(row) {
        *score = v as i64;
    }
    argmax_kind(&scores, rng)
}

/// The reward of an energy change: milli-energy / `REWARD_DIVISOR`,
/// truncated toward zero, saturated to i16.
pub fn reward(delta_energy: i64) -> i16 {
    (delta_energy / REWARD_DIVISOR).clamp(i16::MIN as i64, i16::MAX as i64) as i16
}

/// One Q-learning step on a single value: `q + ((r·Q_UNIT + γ·max_next − q) >> 4)`,
/// computed in i64 and saturated to i16 (no wrap-around, whatever the
/// config's energies). Shifts floor, so the update is deterministic and
/// converges from both sides into the band where the TD error is below one
/// unit.
pub fn q_step(q: i16, reward: i16, max_next: i16) -> i16 {
    let target = reward as i64 * Q_UNIT + ((max_next as i64 * GAMMA_NUM) >> GAMMA_SHIFT);
    let delta = (target - q as i64) >> ALPHA_SHIFT;
    (q as i64 + delta).clamp(i16::MIN as i64, i16::MAX as i64) as i16
}

/// The largest value of a row (0 for an empty slice).
pub fn row_max(row: &[i16]) -> i16 {
    row.iter().copied().max().unwrap_or(0)
}

/// Credits `prev` with the row `now` the agent went on to decide in:
/// `Q[prev.state, prev.action] ← q_step(…, prev.reward, max Q[now])`.
/// Returns false, touching nothing, when either row or the action is outside
/// the table (a snapshot that `check_invariants` rejects anyway).
pub fn credit(q: &mut [i16], prev: LearnerStep, now: u16) -> bool {
    let k = Action::KIND_COUNT;
    let (s, a, s2) = (prev.state as usize, prev.action as usize, now as usize);
    if a >= k || (s + 1) * k > q.len() || (s2 + 1) * k > q.len() {
        return false;
    }
    let max_next = row_max(&q[s2 * k..(s2 + 1) * k]);
    let cell = &mut q[s * k + a];
    *cell = q_step(*cell, prev.reward, max_next);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_steps_toward_the_fixed_point_monotonically_from_both_sides() {
        // A constant reward in a self-loop state: Q must rise monotonically
        // towards r·Q_UNIT / (1 − γ) = 40·16·32/3 ≈ 6827 and settle within
        // one unit of TD error below it, never overshooting.
        let ideal = 40 * Q_UNIT * 32 / 3;
        let (mut q, mut prev) = (0i16, i16::MIN);
        for _ in 0..2_000 {
            let next = q_step(q, 40, q);
            assert!(next >= q, "{next} < {q}");
            assert!((next as i64) <= ideal);
            if next == q {
                break;
            }
            prev = q;
            q = next;
        }
        assert!(prev > 0 && q as i64 > ideal - Q_UNIT * 20, "{q} vs {ideal}");
        assert_eq!(q_step(q, 40, q), q, "fixed point");
        // From above it comes down monotonically to the same band.
        let mut high = i16::MAX / 2;
        loop {
            let next = q_step(high, 40, high);
            assert!(next <= high);
            if next == high {
                break;
            }
            high = next;
        }
        // Both are fixed points inside the band where the TD error is in
        // [0, Q_UNIT): the climb stops at its lower edge, the descent at its
        // upper edge, and the band is Q_UNIT / (1 − γ) ≈ 171 wide.
        assert!(high as i64 <= ideal);
        assert!(
            (0..=Q_UNIT * 32 / 3 + 1).contains(&(high as i64 - q as i64)),
            "{high} vs {q}"
        );

        // Negative rewards step down. A TD error below one unit changes
        // nothing in the positive direction (floor) and steps −1 in the
        // negative one: 110 · 29/32 floors to 99.
        assert!(q_step(0, -5, 0) < 0);
        assert_eq!(q_step(90, 0, 110), 90, "error +9 floors to 0");
        assert_eq!(q_step(99, 0, 110), 99, "error 0");
        assert_eq!(q_step(100, 0, 110), 99, "error −1 steps down");
        assert_eq!(q_step(0, 0, 0), 0);
    }

    #[test]
    fn q_saturates_instead_of_wrapping() {
        let mut q = 0i16;
        for _ in 0..100_000 {
            q = q_step(q, i16::MAX, i16::MAX);
        }
        assert_eq!(q, i16::MAX);
        assert_eq!(q_step(i16::MAX, i16::MAX, i16::MAX), i16::MAX);
        let mut q = 0i16;
        for _ in 0..100_000 {
            q = q_step(q, i16::MIN, i16::MIN);
        }
        assert_eq!(q, i16::MIN);
        assert_eq!(reward(i64::MAX), i16::MAX);
        assert_eq!(reward(i64::MIN), i16::MIN);
        assert_eq!(reward(-550), -5, "truncates toward zero");
        assert_eq!(reward(3_500), 35);
    }

    #[test]
    fn credit_updates_one_cell_from_the_next_rows_maximum_and_checks_bounds() {
        let k = Action::KIND_COUNT;
        let mut q = vec![0i16; 4 * k];
        q[2 * k + 5] = 320; // max of row 2
        q[2 * k + 1] = -7;
        let prev = LearnerStep {
            state: 1,
            action: 3,
            reward: 10,
        };
        assert!(credit(&mut q, prev, 2));
        let expected = q_step(0, 10, 320);
        assert_eq!(q[k + 3], expected);
        assert_eq!(q.iter().filter(|&&v| v != 0).count(), 3, "one cell changed");
        let before = q.clone();
        assert!(!credit(&mut q, prev, 4), "row beyond the table");
        assert!(!credit(&mut q, LearnerStep { state: 4, ..prev }, 0));
        assert!(!credit(&mut q, LearnerStep { action: 8, ..prev }, 0));
        assert_eq!(q, before);
        assert_eq!(row_max(&[]), 0);
        assert_eq!(row_max(&[-3, -1, -2]), -1);
    }

    #[test]
    fn rows_fold_the_fine_grid_by_its_low_bits() {
        assert_eq!(state_row(0b101_0110, 128), 0b101_0110);
        assert_eq!(state_row(0b101_0110, 32), 0b1_0110);
        assert_eq!(state_row(0b101_0110, 8), 0b110);
        assert_eq!(state_row(0b101_0110, 1), 0);
        assert_eq!(state_row(0b101_0110, 1024), 0b101_0110);
        assert_eq!(state_row(100, 3), 1);
        assert_eq!(state_row(5, 0), 0, "a rowless table never indexes");
    }
}
