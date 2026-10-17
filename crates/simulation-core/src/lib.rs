//! Civilization Zero — Deterministic World Kernel (Phase 1).
//!
//! The kernel owns physics only: space, resources, bodies and the action
//! resolver. It knows nothing about cognition, institutions or the
//! multiverse runtime; those layers depend on it, never the reverse.
//!
//! Determinism contract (ADR-0002/0003/0004): given the same crate version,
//! `WorldConfig` (including `seed`) and the same sequence of external inputs,
//! `World::step` produces bit-identical states on every platform. The kernel
//! uses no floating point, no hash-map iteration and no shared RNG state.

pub mod action;
pub mod agent;
pub mod config;
pub mod event;
pub mod event_log;
pub mod intervention;
pub mod memory;
pub mod policy;
pub mod resources;
pub mod rng;
pub mod space;
pub mod spatial;
pub mod world;

pub use action::{Action, Failure, Outcome};
pub use agent::{Agent, AgentId, Memory};
pub use config::{Perception, WorldConfig};
pub use event::{DeathCause, Event};
pub use event_log::{EventLog, LogError};
pub use intervention::{Intervention, InterventionError, ParamsMode, Who};
pub use memory::{Interaction, InteractionKind, Message, Signal, Terms};
pub use policy::{
    Intent, LearnerStep, Neighbor, Observation, PolicyKind, PolicyParams, PolicyTable,
    REFLEX_FEATURES,
};
pub use space::{Dir, Pos};
pub use spatial::SpatialIndex;
pub use world::{Ledger, SnapshotError, TickReport, World};
