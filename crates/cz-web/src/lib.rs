//! Civilization Zero in the browser (ADR-0010 2b-1, ADR-0013 3-9a).
//!
//! `WasmWorld` is the only type JavaScript sees. It owns a [`Multiverse`]
//! with one branch and a *cursor*: `seek` to a past tick restores that tick's
//! start state as the cursor; `step` from there re-executes the branch's own
//! history (verified against its ledger) until the cursor reaches the head,
//! after which stepping advances the head itself. Interventions are accepted
//! at the head only; intervening behind the head is a fork (2b-5).
//!
//! The social layer (3-9a) is read, never remembered: `social()` lists the
//! transfers, harms and signals of the tick the view just executed, placed
//! at the agents' positions in the view; `agent_json` is one agent with the
//! memory the kernel wrote for it; `agent_events_json` filters the ledger
//! to one agent; `ledger_json` is the running totals. Negotiation threads
//! (3-9b) are the same ledger read for a pair: `thread_json` lists what
//! passed between two agents and judges each accepted deal from the
//! transfers that followed ([`thread`]); `deals_json` lists every deal of
//! a window so one can be found. "Fixed vs learning" (3-9c) needs nothing
//! new here: the page forks at the current tick and applies
//! `Intervention::SetPolicy` through `intervene` (ADR-0013 D7); this crate
//! only widens the founding mixes to the learning policies and reports the
//! policy of every agent (`policy_code`) so the map can colour all four.
//!
//! Numbers cross the boundary as `u32` so JavaScript never needs `BigInt`;
//! the state hash is a 16-hex-digit string. Errors are `String`s (wasm-bindgen
//! throws them; natively they stay plain `Result`s, so the same code is
//! testable without a JS runtime). Built with
//! `wasm-pack build crates/cz-web --target web` (see `web/package.json`).

pub mod thread;

use multiverse::{replay, BranchId, Multiverse};
use simulation_core::{AgentId, Event, Intervention, PolicyKind, Pos, World, WorldConfig};
use wasm_bindgen::prelude::*;

/// Fields per agent in `agents()`, in the kernel's trace order.
pub const AGENT_FIELDS: usize = 8;

/// Fields per marker in `social()`: kind (`MARKER_TRANSFER` /
/// `MARKER_HARM` / `MARKER_SIGNAL`), tick, x, y, x2, y2, value. `x2, y2` is
/// the other party's position, `-1, -1` when there is none (a signal) or it
/// is no longer in the view (died that tick).
pub const MARKER_FIELDS: usize = 7;
pub const MARKER_TRANSFER: i32 = 0;
pub const MARKER_HARM: i32 = 1;
pub const MARKER_SIGNAL: i32 = 2;

fn js<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Names of the founding mixes a world can be built with, as `cz run
/// --policy` spells them.
pub const POLICY_MIXES: [&str; 6] = [
    "forager",
    "random",
    "mixed",
    "reflex",
    "learner",
    "mixed-learning",
];

/// How a fresh world's founding population is split between the policies.
/// The rule is `cz run --policy`'s, so a browser world and a CLI world with
/// the same mix reach the same hashes: `mixed` alternates Forager / Random
/// by founding index, `mixed-learning` cycles Forager / Reflex / Learner
/// (3-5). A forager-only world has no social behaviour at all (foragers
/// never transfer, harm or signal), so the UI offers `mixed` and `random`
/// to see the social layer at work, and the learning mixes (3-9c) to watch
/// a world that learns from the start.
pub fn policy_mix(name: &str) -> Result<fn(u32) -> PolicyKind, String> {
    match name {
        "forager" => Ok(|_| PolicyKind::Forager),
        "random" => Ok(|_| PolicyKind::Random),
        "mixed" => Ok(|i| {
            if i % 2 == 0 {
                PolicyKind::Forager
            } else {
                PolicyKind::Random
            }
        }),
        "reflex" => Ok(|_| PolicyKind::Reflex),
        "learner" => Ok(|_| PolicyKind::Learner),
        "mixed-learning" => Ok(|i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Reflex,
            _ => PolicyKind::Learner,
        }),
        other => Err(format!(
            "unknown policy mix {other:?}: expected one of {}",
            POLICY_MIXES.join(", ")
        )),
    }
}

/// Whether a founding mix turns weight mutation on, as `cz run --policy`
/// does without `--no-mutate` (ADR-0013 D6: Reflex evolves between
/// generations, so the evolving mixes mutate by default; every other mix
/// keeps the config default, off). Same rule, same hashes.
pub fn evolves(name: &str) -> bool {
    matches!(name, "reflex" | "mixed-learning")
}

/// Policy column of `agents()`: the kind's index in [`PolicyKind::ALL`]
/// (0 forager, 1 random, 2 reflex, 3 learner), fixed by the kernel's order.
pub fn policy_code(kind: PolicyKind) -> i32 {
    PolicyKind::ALL
        .iter()
        .position(|k| *k == kind)
        .expect("every kind is in ALL") as i32
}

/// Whether `e` names `id` in any role (actor, target, child, parent, or the
/// agent an intervention removed).
fn involves(e: &Event, id: AgentId) -> bool {
    match e {
        Event::Born { child, parent, .. } => *child == id || *parent == id,
        Event::Died { agent, .. } | Event::Acted { agent, .. } | Event::Signaled { agent, .. } => {
            *agent == id
        }
        Event::Transferred { from, to, .. }
        | Event::Proposed { from, to, .. }
        | Event::Accepted { from, to, .. }
        | Event::Rejected { from, to, .. } => *from == id || *to == id,
        Event::Harmed { by, target, .. } | Event::Sanctioned { by, target, .. } => {
            *by == id || *target == id
        }
        Event::Violated { agent, against, .. } => *agent == id || *against == id,
        Event::Intervened { intervention, .. } => {
            matches!(intervention, Intervention::RemoveAgent { id: removed } if *removed == id)
        }
    }
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
    /// A fresh world. `patches` is the number of food patches (`cz` uses
    /// `width * height / 700`); `snapshot_every` bounds how far a `seek` has
    /// to re-execute; `policy` is one of [`POLICY_MIXES`] (see
    /// [`policy_mix`]; the evolving mixes mutate, see [`evolves`]). The
    /// rest of the config is the kernel default, as `cz run` without
    /// options: `cz run --seed S --policy P` reaches the same hashes.
    #[wasm_bindgen(constructor)]
    pub fn new(
        seed: u32,
        width: u16,
        height: u16,
        agents: u32,
        patches: u32,
        snapshot_every: u32,
        policy: &str,
    ) -> Result<WasmWorld, String> {
        let config = WorldConfig {
            seed: seed as u64,
            width,
            height,
            initial_agents: agents,
            patch_count: patches,
            mutation_enabled: evolves(policy),
            ..WorldConfig::default()
        };
        let world = World::generate(config, policy_mix(policy)?).map_err(js)?;
        Ok(Self {
            mv: Multiverse::new(world, snapshot_every.max(1) as u64),
            branch: BranchId(0),
            cursor: None,
        })
    }

    /// A world restored from a binary snapshot (ADR-0006) as the origin of
    /// a new branch: this is how a fork crosses worker boundaries (2b-6).
    /// The parent exports `snapshot_bytes()` of the tick to fork at, the
    /// child is built from it and then `intervene`s — exactly
    /// `Multiverse::fork` semantics (the child's ledger starts at the fork
    /// tick with its own interventions). It cannot seek before its origin.
    #[wasm_bindgen(js_name = fromSnapshot)]
    pub fn from_snapshot(bytes: &[u8], snapshot_every: u32) -> Result<WasmWorld, String> {
        let world = World::from_bytes(bytes).map_err(js)?;
        Ok(Self {
            mv: Multiverse::new(world, snapshot_every.max(1) as u64),
            branch: BranchId(0),
            cursor: None,
        })
    }

    /// Tick this branch began at: 0 for a fresh world, the fork tick for a
    /// world built with `fromSnapshot`.
    pub fn origin(&self) -> u32 {
        self.mv
            .branch(self.branch)
            .expect("the branch exists")
            .origin_tick() as u32
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

    /// `dynamics_hash` of the view (state hash without the config), which the
    /// golden tests pin.
    pub fn dynamics_hash_hex(&self) -> String {
        format!("{:016x}", self.view().dynamics_hash())
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

    /// `AGENT_FIELDS` values per living agent: id, x, y, energy, inventory,
    /// policy ([`policy_code`]: 0 forager / 1 random / 2 reflex / 3
    /// learner), health, age.
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
                policy_code(a.policy),
                a.health.clamp(0, i32::MAX as i64) as i32,
                a.age.min(i32::MAX as u64) as i32,
            ]);
        }
        out
    }

    /// `MARKER_FIELDS` values per interaction of the tick the view just
    /// executed (tick − 1; empty at the origin of a fresh world): transfers
    /// and harms from the ledger, signals from each agent's kernel-written
    /// `last_signal` (so nothing needs `record_signals`). Positions are the
    /// view's, i.e. at most one move after the interaction. A transfer or
    /// harm whose actor died that tick is placed at the other party; one
    /// with neither party left is dropped. Ledger order, then signals in id
    /// order.
    pub fn social(&self) -> Vec<i32> {
        let w = self.view();
        let mut out = Vec::new();
        let Some(prev) = w.tick.checked_sub(1) else {
            return out;
        };
        let pos_of = |id: AgentId| -> Option<Pos> {
            w.agents
                .binary_search_by_key(&id, |a| a.id)
                .ok()
                .map(|i| w.agents[i].pos)
        };
        let branch = self.mv.branch(self.branch).expect("the branch exists");
        for e in branch.log().at(prev) {
            let (kind, actor, other, value) = match e {
                Event::Transferred {
                    from, to, amount, ..
                } => (MARKER_TRANSFER, *from, *to, *amount as i32),
                Event::Harmed {
                    by, target, damage, ..
                } => (
                    MARKER_HARM,
                    *by,
                    *target,
                    (*damage).clamp(0, i32::MAX as i64) as i32,
                ),
                _ => continue,
            };
            let (x, y, x2, y2) = match (pos_of(actor), pos_of(other)) {
                (Some(p), Some(q)) => (p.x as i32, p.y as i32, q.x as i32, q.y as i32),
                (Some(p), None) | (None, Some(p)) => (p.x as i32, p.y as i32, -1, -1),
                (None, None) => continue,
            };
            out.extend_from_slice(&[kind, prev as i32, x, y, x2, y2, value]);
        }
        for a in &w.agents {
            if let Some(s) = a.last_signal.filter(|s| s.tick == prev) {
                out.extend_from_slice(&[
                    MARKER_SIGNAL,
                    prev as i32,
                    a.pos.x as i32,
                    a.pos.y as i32,
                    -1,
                    -1,
                    s.message.value().clamp(0, i32::MAX as i64) as i32,
                ]);
            }
        }
        out
    }

    /// The view's `Ledger` (running totals since tick 0: food and energy
    /// flows, births, deaths by cause, transfers, harms, signals,
    /// interventions) as JSON.
    pub fn ledger_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.view().ledger).map_err(js)
    }

    /// The living agent `id` of the view as JSON — the kernel's `Agent` as
    /// serde renders it, including `last_signal` and the kernel-written
    /// memory `recent` (oldest first) — or `null` when no such agent is
    /// alive at this tick.
    pub fn agent_json(&self, id: u32) -> Result<String, String> {
        let w = self.view();
        match w.agents.binary_search_by_key(&AgentId(id as u64), |a| a.id) {
            Ok(i) => serde_json::to_string(&w.agents[i]).map_err(js),
            Err(_) => Ok("null".into()),
        }
    }

    /// Ledger events of ticks `from..to` (half-open) that name agent `id`
    /// in any role, oldest first, as a JSON array. The ledger, not the
    /// agent's memory: complete, and available after the agent has died.
    pub fn agent_events_json(&self, id: u32, from: u32, to: u32) -> Result<String, String> {
        let id = AgentId(id as u64);
        let branch = self.mv.branch(self.branch).map_err(js)?;
        let (from, to) = (from as u64, to as u64);
        let events: Vec<&Event> = branch
            .log()
            .range(from..to.max(from))
            .iter()
            .filter(|e| involves(e, id))
            .collect();
        serde_json::to_string(&events).map_err(js)
    }

    /// The negotiation thread between agents `a` and `b` (ADR-0013 3-9b) as
    /// JSON (`thread::Thread`): every ledger event naming both of them in
    /// ticks `from..now`, ledger order, with each acceptance read as a deal
    /// and judged from the transfers in its window. `now` is `to` capped at
    /// the view's tick — the thread is the ledger as the world stood at
    /// `now`, so a deal still inside its deadline there is `open` even if
    /// the branch has run further. Transfers before `from` are not seen.
    pub fn thread_json(&self, a: u32, b: u32, from: u32, to: u32) -> Result<String, String> {
        let branch = self.mv.branch(self.branch).map_err(js)?;
        let now = (to as u64).min(self.view().tick);
        let from = (from as u64).min(now);
        let t = thread::thread(
            branch.log().range(from..now),
            AgentId(a as u64),
            AgentId(b as u64),
            from,
            now,
        );
        serde_json::to_string(&t).map_err(js)
    }

    /// Every deal (accepted proposal) between any two agents in ticks
    /// `from..now` (`now` = `to` capped at the view's tick), judged as of
    /// `now` exactly as in [`thread_json`], ledger order, as a JSON array
    /// of `thread::Deal`. How the UI finds a thread worth reading.
    pub fn deals_json(&self, from: u32, to: u32) -> Result<String, String> {
        let branch = self.mv.branch(self.branch).map_err(js)?;
        let now = (to as u64).min(self.view().tick);
        let from = (from as u64).min(now);
        serde_json::to_string(&thread::deals(branch.log().range(from..now), now)).map_err(js)
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
        let events: Vec<&Event> = branch
            .log()
            .events()
            .iter()
            .filter(|e| matches!(e, Event::Intervened { .. }))
            .collect();
        serde_json::to_string(&events).map_err(js)
    }
}
