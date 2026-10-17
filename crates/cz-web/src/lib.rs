//! Civilization Zero in the browser (ADR-0010 2b-1).
//!
//! `WasmWorld` is the only type JavaScript sees. It owns a [`Multiverse`]
//! with one branch and a *cursor*: `seek` to a past tick restores that tick's
//! start state as the cursor; `step` from there re-executes the branch's own
//! history (verified against its ledger) until the cursor reaches the head,
//! after which stepping advances the head itself. Interventions are accepted
//! at the head only; intervening behind the head is a fork (2b-5).
//!
//! Numbers cross the boundary as `u32` so JavaScript never needs `BigInt`;
//! the state hash is a 16-hex-digit string. Errors are `String`s (wasm-bindgen
//! throws them; natively they stay plain `Result`s, so the same code is
//! testable without a JS runtime). Built with
//! `wasm-pack build crates/cz-web --target web` (see `web/package.json`).

use multiverse::{replay, BranchId, Multiverse};
use simulation_core::{Intervention, PolicyKind, World, WorldConfig};
use wasm_bindgen::prelude::*;

/// Fields per agent in `agents()`, in the kernel's trace order.
pub const AGENT_FIELDS: usize = 8;

fn js<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

#[wasm_bindgen]
pub struct WasmWorld {
    mv: Multiverse,
    branch: BranchId,
    /// Restored view of a past tick; `None` means the view is the head.
    cursor: Option<World>,
}

#[wasm_bindgen]
impl WasmWorld {
    /// A fresh forager world. `patches` is the number of food patches
    /// (`cz` uses `width * height / 700`); `snapshot_every` bounds how far a
    /// `seek` has to re-execute.
    #[wasm_bindgen(constructor)]
    pub fn new(
        seed: u32,
        width: u16,
        height: u16,
        agents: u32,
        patches: u32,
        snapshot_every: u32,
    ) -> Result<WasmWorld, String> {
        let config = WorldConfig {
            seed: seed as u64,
            width,
            height,
            initial_agents: agents,
            patch_count: patches,
            ..WorldConfig::default()
        };
        let world = World::generate(config, |_| PolicyKind::Forager).map_err(js)?;
        Ok(Self {
            mv: Multiverse::new(world, snapshot_every.max(1) as u64),
            branch: BranchId(0),
            cursor: None,
        })
    }

    fn head(&self) -> &World {
        self.mv
            .branch(self.branch)
            .expect("the branch exists")
            .head()
    }

    fn view(&self) -> &World {
        self.cursor.as_ref().unwrap_or_else(|| self.head())
    }

    /// Tick of the current view (cursor or head).
    pub fn tick(&self) -> u32 {
        self.view().tick as u32
    }

    /// Furthest tick simulated on this branch.
    pub fn frontier(&self) -> u32 {
        self.head().tick as u32
    }

    pub fn width(&self) -> u16 {
        self.view().config.width
    }

    pub fn height(&self) -> u16 {
        self.view().config.height
    }

    pub fn alive(&self) -> u32 {
        self.view().agents.len() as u32
    }

    pub fn food_total(&self) -> u32 {
        self.view().resources.total_food().min(u32::MAX as u64) as u32
    }

    pub fn snapshot_every(&self) -> u32 {
        self.mv.snapshot_every() as u32
    }

    /// `state_hash` of the view as 16 lowercase hex digits.
    pub fn state_hash_hex(&self) -> String {
        format!("{:016x}", self.view().state_hash())
    }

    /// Advances the view by `ticks`. Behind the head this re-executes the
    /// branch's history (checked against the ledger); at the head it
    /// extends the branch, recording events and snapshots.
    pub fn step(&mut self, ticks: u32) -> Result<(), String> {
        let mut remaining = ticks as u64;
        if let Some(cursor) = self.cursor.as_mut() {
            let branch = self.mv.branch(self.branch).map_err(js)?;
            let head = branch.head().tick;
            let to = (cursor.tick + remaining).min(head);
            remaining -= to - cursor.tick;
            replay(cursor, branch.log(), to).map_err(js)?;
            if cursor.tick == head {
                self.cursor = None;
            }
        }
        if remaining > 0 && self.cursor.is_none() {
            self.mv.step(self.branch, remaining).map_err(js)?;
        }
        Ok(())
    }

    /// Moves the view to `tick`: past ticks are restored from the nearest
    /// snapshot, ticks beyond the head extend the branch.
    pub fn seek(&mut self, tick: u32) -> Result<(), String> {
        let tick = tick as u64;
        let head = self.head().tick;
        if tick >= head {
            self.cursor = None;
            if tick > head {
                self.mv.step(self.branch, tick - head).map_err(js)?;
            }
        } else {
            self.cursor = Some(self.mv.state_at(self.branch, tick).map_err(js)?);
        }
        Ok(())
    }

    /// Row-major food per cell, clamped to 255.
    pub fn food(&self) -> Vec<u8> {
        self.view()
            .resources
            .food
            .iter()
            .map(|&f| f.min(255) as u8)
            .collect()
    }

    /// `AGENT_FIELDS` values per living agent:
    /// id, x, y, energy, inventory, policy (0 forager / 1 random), health, age.
    pub fn agents(&self) -> Vec<i32> {
        let w = self.view();
        let mut out = Vec::with_capacity(w.agents.len() * AGENT_FIELDS);
        for a in &w.agents {
            out.extend_from_slice(&[
                a.id.0 as i32,
                a.pos.x as i32,
                a.pos.y as i32,
                a.energy.clamp(0, i32::MAX as i64) as i32,
                a.inventory as i32,
                matches!(a.policy, PolicyKind::Random) as i32,
                a.health.clamp(0, i32::MAX as i64) as i32,
                a.age.min(i32::MAX as u64) as i32,
            ]);
        }
        out
    }

    /// Binary snapshot of the view (ADR-0006).
    pub fn snapshot_bytes(&self) -> Vec<u8> {
        self.view().to_bytes()
    }

    /// Applies an intervention (JSON as in the ledger, e.g.
    /// `{"op":"drought","regrow_amount":0,"regrow_interval":10}`) to the head
    /// and returns the recorded event as JSON. Rejected while the view is
    /// behind the head: that is a fork (2b-5).
    pub fn intervene(&mut self, json: &str) -> Result<String, String> {
        if self.cursor.is_some() {
            return Err("the view is behind the head: fork instead of intervening".into());
        }
        let intervention: Intervention = serde_json::from_str(json).map_err(js)?;
        let event = self.mv.intervene(self.branch, intervention).map_err(js)?;
        serde_json::to_string(&event).map_err(js)
    }

    /// Events recorded for one tick, as a JSON array (for the UI's history).
    pub fn events_at(&self, tick: u32) -> Result<String, String> {
        let branch = self.mv.branch(self.branch).map_err(js)?;
        serde_json::to_string(branch.log().at(tick as u64)).map_err(js)
    }

    /// Every intervention this branch has recorded, oldest first, as a JSON
    /// array of `Intervened` events: the UI's history is read from the
    /// ledger, never from its own memory.
    pub fn interventions_json(&self) -> Result<String, String> {
        let branch = self.mv.branch(self.branch).map_err(js)?;
        let events: Vec<&simulation_core::Event> = branch
            .log()
            .events()
            .iter()
            .filter(|e| matches!(e, simulation_core::Event::Intervened { .. }))
            .collect();
        serde_json::to_string(&events).map_err(js)
    }
}
