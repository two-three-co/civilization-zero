//! Multiverse Runtime (ADR-0012): time travel over one deterministic kernel.
//!
//! - [`replay`] re-executes a world from a snapshot, applying the ledger's
//!   interventions at their ticks and checking that the kernel reproduces
//!   the ledger's lifecycle events exactly.
//! - [`Multiverse`] keeps branches: a head world, its full ledger from tick 0
//!   and periodic snapshots, so any past tick can be restored and forked.
//!
//! The kernel stays physics-only; nothing here changes how a tick runs.

pub mod branch;
pub mod replay;

pub use branch::{Branch, BranchId, Multiverse, MultiverseError};
pub use replay::{replay, ReplayError, ReplayStats};
