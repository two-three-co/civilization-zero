//! Branches and forks (ADR-0012).
//!
//! A branch owns a head world, the full ledger of everything applied to it
//! since tick 0 (the shared past is copied at fork time, so every branch
//! replays on its own) and periodic snapshots. `state_at(tick)` is the
//! state that tick *starts* with, rebuilt from the nearest earlier snapshot;
//! the head may additionally carry interventions already applied at its
//! tick, which the next step consumes.

use crate::replay::{replay, ReplayError};
use serde::{Deserialize, Serialize};
use simulation_core::{Event, EventLog, Intervention, InterventionError, LogError, World};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BranchId(pub u32);

pub struct Branch {
    id: BranchId,
    parent: Option<BranchId>,
    forked_at: Option<u64>,
    world: World,
    log: EventLog,
    /// Ascending by tick. `snapshots[0]` is the origin: tick 0 for the root,
    /// the fork tick (before the fork's interventions) for a child.
    snapshots: Vec<World>,
}

impl Branch {
    pub fn id(&self) -> BranchId {
        self.id
    }
    pub fn parent(&self) -> Option<BranchId> {
        self.parent
    }
    /// Tick of the parent this branch split from; `None` for the root.
    pub fn forked_at(&self) -> Option<u64> {
        self.forked_at
    }
    pub fn head(&self) -> &World {
        &self.world
    }
    pub fn log(&self) -> &EventLog {
        &self.log
    }
    pub fn origin_tick(&self) -> u64 {
        self.snapshots[0].tick
    }
    pub fn snapshot_ticks(&self) -> impl Iterator<Item = u64> + '_ {
        self.snapshots.iter().map(|w| w.tick)
    }
    fn nearest_snapshot(&self, tick: u64) -> Option<&World> {
        self.snapshots.iter().rev().find(|w| w.tick <= tick)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultiverseError {
    UnknownBranch(BranchId),
    BeyondHead { tick: u64, head: u64 },
    BeforeOrigin { tick: u64, origin: u64 },
    Replay(ReplayError),
    Intervention(InterventionError),
    Log(LogError),
}

impl std::fmt::Display for MultiverseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MultiverseError::UnknownBranch(id) => write!(f, "no branch {}", id.0),
            MultiverseError::BeyondHead { tick, head } => {
                write!(f, "tick {tick} is beyond the branch head {head}")
            }
            MultiverseError::BeforeOrigin { tick, origin } => {
                write!(f, "tick {tick} is before the branch origin {origin}")
            }
            MultiverseError::Replay(e) => write!(f, "{e}"),
            MultiverseError::Intervention(e) => write!(f, "{e}"),
            MultiverseError::Log(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for MultiverseError {}

impl From<ReplayError> for MultiverseError {
    fn from(e: ReplayError) -> Self {
        MultiverseError::Replay(e)
    }
}
impl From<InterventionError> for MultiverseError {
    fn from(e: InterventionError) -> Self {
        MultiverseError::Intervention(e)
    }
}
impl From<LogError> for MultiverseError {
    fn from(e: LogError) -> Self {
        MultiverseError::Log(e)
    }
}

pub struct Multiverse {
    snapshot_every: u64,
    branches: Vec<Branch>,
}

impl Multiverse {
    /// Branch 0 is `root` at whatever tick it is in; a snapshot is taken of
    /// it immediately. `snapshot_every` bounds the replay distance of
    /// `state_at` (and the memory per branch).
    pub fn new(root: World, snapshot_every: u64) -> Self {
        assert!(snapshot_every > 0, "snapshot_every must be > 0");
        let branch = Branch {
            id: BranchId(0),
            parent: None,
            forked_at: None,
            snapshots: vec![root.clone()],
            world: root,
            log: EventLog::new(),
        };
        Self {
            snapshot_every,
            branches: vec![branch],
        }
    }

    pub fn snapshot_every(&self) -> u64 {
        self.snapshot_every
    }

    pub fn branches(&self) -> &[Branch] {
        &self.branches
    }

    pub fn branch(&self, id: BranchId) -> Result<&Branch, MultiverseError> {
        self.branches
            .get(id.0 as usize)
            .ok_or(MultiverseError::UnknownBranch(id))
    }

    fn branch_mut(&mut self, id: BranchId) -> Result<&mut Branch, MultiverseError> {
        self.branches
            .get_mut(id.0 as usize)
            .ok_or(MultiverseError::UnknownBranch(id))
    }

    /// Advances a branch head, recording every tick's events and taking a
    /// snapshot whenever the tick is a multiple of `snapshot_every`.
    pub fn step(&mut self, id: BranchId, ticks: u64) -> Result<(), MultiverseError> {
        let every = self.snapshot_every;
        let b = self.branch_mut(id)?;
        for _ in 0..ticks {
            let report = b.world.step();
            b.log.record(&report)?;
            if b.world.tick % every == 0 {
                b.snapshots.push(b.world.clone());
            }
        }
        Ok(())
    }

    /// Applies an intervention to a branch head and records it.
    pub fn intervene(
        &mut self,
        id: BranchId,
        intervention: Intervention,
    ) -> Result<Event, MultiverseError> {
        let b = self.branch_mut(id)?;
        let e = b.world.intervene(intervention)?;
        b.log.push(e)?;
        Ok(e)
    }

    /// The state `tick` starts with, rebuilt from the nearest earlier
    /// snapshot by replaying the branch's own ledger.
    pub fn state_at(&self, id: BranchId, tick: u64) -> Result<World, MultiverseError> {
        let b = self.branch(id)?;
        if tick > b.world.tick {
            return Err(MultiverseError::BeyondHead {
                tick,
                head: b.world.tick,
            });
        }
        let base = b
            .nearest_snapshot(tick)
            .ok_or(MultiverseError::BeforeOrigin {
                tick,
                origin: b.origin_tick(),
            })?;
        let mut world = base.clone();
        replay(&mut world, &b.log, tick)?;
        Ok(world)
    }

    /// New branch = the state `from` started tick `at` with, plus
    /// `interventions` applied there. The parent's ledger before `at` is
    /// copied (shared past); the parent's own interventions at `at` are not
    /// inherited, so the child's ledger at `at` holds exactly what was
    /// applied to the child.
    pub fn fork(
        &mut self,
        from: BranchId,
        at: u64,
        interventions: &[Intervention],
    ) -> Result<BranchId, MultiverseError> {
        let world = self.state_at(from, at)?;
        let mut log = EventLog::new();
        for e in self.branch(from)?.log.range(0..at) {
            log.push(*e)?;
        }
        let id = BranchId(self.branches.len() as u32);
        let mut branch = Branch {
            id,
            parent: Some(from),
            forked_at: Some(at),
            snapshots: vec![world.clone()],
            world,
            log,
        };
        for iv in interventions {
            let e = branch.world.intervene(*iv)?;
            branch.log.push(e)?;
        }
        self.branches.push(branch);
        Ok(id)
    }
}
