//! External changes to a running world (ADR-0010, task 2a-1).
//!
//! An intervention is applied *between* ticks by `World::intervene` and is
//! either applied completely or rejected without touching the state. The
//! only randomness any of them uses is the seed-keyed `Stream::Intervention`
//! draw of `SetPolicy { who: Fraction }`, so "same snapshot + same
//! intervention sequence" replays to the same hash. Each applied intervention
//! is recorded as `Event::Intervened`. Food and energy an intervention adds
//! or removes are booked in the ledger (`food_intervened`,
//! `energy_intervened`) so conservation stays checkable.

use crate::agent::AgentId;
use crate::nomos::{Charter, Norm};
use crate::policy::PolicyKind;
use crate::space::Pos;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Intervention {
    /// Replace the regrowth parameters. A drought lowers them; applying the
    /// original values again ends it.
    Drought {
        regrow_amount: u32,
        regrow_interval: u64,
    },
    /// Multiply every cell's food by `permille / 1000`, rounding down.
    /// `0..=MAX_PERMILLE`; values above 1000 are a resource boom.
    ScaleFood { permille: u32 },
    /// Add `n` agents on one cell with the config's initial energy and
    /// params, age 0 and empty memory.
    SpawnAgents {
        n: u32,
        pos: Pos,
        policy: PolicyKind,
    },
    /// Remove one living agent. Its carried food drops on its cell; its
    /// energy leaves the world through `energy_intervened`, not as a death.
    RemoveAgent { id: AgentId },
    /// Give some living agents another policy (ADR-0013 D7). Bodies,
    /// positions, memories and signals are untouched: only `policy` and, as
    /// `params` says, `PolicyParams` change. "Fixed vs learning" is a fork
    /// plus this; `who: Fraction` is the seed-keyed randomisation of H4.
    SetPolicy {
        who: Who,
        kind: PolicyKind,
        params: ParamsMode,
    },
    /// Put a norm in force by decree (ADR-0014 D8): a new rule with the
    /// next id, from the next tick on. A rule of the same kind already in
    /// force is repealed first (its open episodes close), so this is also
    /// how a rule is amended. "With vs without rule" is a fork plus this.
    /// `Invalid` while `config.nomos` is off. Decrees are not bound by the
    /// charter's `amendable`.
    Enact { norm: Norm },
    /// Take the rule with this id out of force; its open episodes close.
    /// `Invalid` if no such rule is in force or `config.nomos` is off.
    Repeal { rule: u16 },
    /// Replace the assembly's charter. `Invalid` while `config.nomos` is off.
    Charter { charter: Charter },
}

impl Intervention {
    pub const MAX_PERMILLE: u32 = 10_000;
}

/// Which living agents a `SetPolicy` applies to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Who {
    All,
    /// One agent by id (one intervention per agent keeps the record
    /// fixed-size and `Copy`; a list is several interventions).
    Id(AgentId),
    /// Each living agent independently with probability `permille / 1000`,
    /// drawn from `RngStream::new(seed, tick, id, Stream::Intervention)`:
    /// deterministic, keyed by the seed, independent of agent order.
    Fraction {
        permille: u32,
    },
}

impl Who {
    pub const PERMILLE_MAX: u32 = 1000;
}

/// What happens to the heritable parameters of an agent whose policy is set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamsMode {
    /// The config's `initial_params` with the blank table of the new kind.
    Default,
    /// Keep the scalars and a table that already fits the new kind (a
    /// learner keeps what it learned); replace a table that does not fit by
    /// the blank one, drop it when the new kind has none.
    Keep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterventionError {
    Invalid(&'static str),
    NoSuchAgent(AgentId),
}

impl std::fmt::Display for InterventionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InterventionError::Invalid(why) => write!(f, "invalid intervention: {why}"),
            InterventionError::NoSuchAgent(id) => write!(f, "no living agent with id {}", id.0),
        }
    }
}

impl std::error::Error for InterventionError {}
