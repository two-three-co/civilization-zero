//! Replay = re-execution (ADR-0012). The kernel is deterministic, so a past
//! state is rebuilt by stepping again from a snapshot while applying the
//! ledger's interventions at their ticks. Lifecycle events are outputs, not
//! inputs: they are compared against the ledger, which turns every replay
//! into an integrity check of ledger + kernel version.

use simulation_core::{Event, EventLog, InterventionError, World};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplayStats {
    pub from: u64,
    pub to: u64,
    pub interventions: usize,
    pub events_checked: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplayError {
    /// The world is already past the requested tick; replay never rewinds.
    TargetBehind { head: u64, to: u64 },
    /// A logged intervention could not be applied (the ledger does not match
    /// this world).
    Intervention { tick: u64, error: InterventionError },
    /// The kernel produced different lifecycle events than the ledger holds
    /// for that tick: wrong ledger, wrong snapshot, or a changed kernel.
    Diverged {
        tick: u64,
        expected: Vec<Event>,
        got: Vec<Event>,
    },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::TargetBehind { head, to } => {
                write!(f, "cannot replay backwards: head is {head}, target {to}")
            }
            ReplayError::Intervention { tick, error } => {
                write!(f, "logged intervention at tick {tick} failed: {error}")
            }
            ReplayError::Diverged {
                tick,
                expected,
                got,
            } => write!(
                f,
                "replay diverged from the ledger at tick {tick}: expected {} events, got {}",
                expected.len(),
                got.len()
            ),
        }
    }
}

impl std::error::Error for ReplayError {}

/// Steps `world` until `to`. At each tick the ledger's interventions for
/// that tick are applied first, in ledger order, then the tick runs and its
/// lifecycle events must equal the ledger's. The result is the state tick
/// `to` *starts* with: interventions logged at `to` itself are not applied.
pub fn replay(world: &mut World, log: &EventLog, to: u64) -> Result<ReplayStats, ReplayError> {
    if to < world.tick {
        return Err(ReplayError::TargetBehind {
            head: world.tick,
            to,
        });
    }
    let mut stats = ReplayStats {
        from: world.tick,
        to,
        ..ReplayStats::default()
    };
    while world.tick < to {
        let tick = world.tick;
        let logged = log.at(tick);
        for e in logged {
            if let Event::Intervened { intervention, .. } = e {
                world
                    .intervene(*intervention)
                    .map_err(|error| ReplayError::Intervention { tick, error })?;
                stats.interventions += 1;
            }
        }
        let report = world.step();
        let expected: Vec<Event> = logged
            .iter()
            .copied()
            .filter(|e| !matches!(e, Event::Intervened { .. }))
            .collect();
        if report.events != expected {
            return Err(ReplayError::Diverged {
                tick,
                expected,
                got: report.events,
            });
        }
        stats.events_checked += expected.len();
    }
    Ok(stats)
}
