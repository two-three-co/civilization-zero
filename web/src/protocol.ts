// Messages between the UI thread and a world worker (ADR-0010 2b).
//
// The kernel worker (crates/cz-web via wasm) and the mock worker both speak
// this; the UI must not care which one is behind the client. Every request
// is answered with the current frame (or an error), so the UI never has to
// track state it did not get from the worker.

/** Fields per agent in `Frame.agents`, matching the kernel's trace layout. */
export const AGENT_FIELDS = 8; // id, x, y, energy, inventory, policy (0 forager / 1 random), health, age

export interface WorldInit {
  seed: number;
  width: number;
  height: number;
  agents: number;
  /** Food patches; the kernel worker defaults to `width * height / 700` like `cz`. */
  patches?: number;
  /** Snapshot cadence of the kernel worker; bounds how far a seek re-executes. */
  snapshotEvery?: number;
}

/** Exactly the kernel's `Intervention` as serde renders it (ADR-0010 2a-1). */
export type Intervention =
  | { op: "drought"; regrow_amount: number; regrow_interval: number }
  | { op: "scale_food"; permille: number }
  | { op: "spawn_agents"; n: number; pos: { x: number; y: number }; policy: "Forager" | "Random" }
  | { op: "remove_agent"; id: number };

/** A ledger record of an applied intervention (`Event::Intervened`). */
export interface InterventionEvent {
  kind: "intervened";
  tick: number;
  intervention: Intervention;
}

/** Wall-clock measurements the worker took while serving a request. */
export interface Timing {
  /** Milliseconds spent in the kernel for the last `step`, and its tick count. */
  stepMs?: number;
  stepTicks?: number;
  /** Milliseconds spent restoring the last `seek`. */
  seekMs?: number;
}

export interface Frame {
  /** Which world produced this frame. Only "kernel" frames are facts. */
  source: "kernel" | "mock";
  tick: number;
  width: number;
  height: number;
  /** Kernel `state_hash` as 16 hex digits; the mock has none. */
  hash?: string;
  /** Row-major food per cell, clamped to 255. Transferred, not copied. */
  food: Uint8Array;
  /** `AGENT_FIELDS` values per living agent. Transferred, not copied. */
  agents: Int32Array;
  alive: number;
  foodTotal: number;
  /** Furthest tick this world has simulated; the timeline's right edge. */
  frontier: number;
  /** Snapshot cadence: a seek to tick T re-executes at most T mod this. */
  snapshotEvery: number;
  timing?: Timing;
}

export type Request =
  | { type: "init"; config: WorldInit }
  | { type: "step"; ticks: number }
  | { type: "seek"; tick: number }
  | { type: "frame" }
  | { type: "intervene"; intervention: Intervention }
  | { type: "history" };

export type Response =
  | { type: "frame"; frame: Frame; event?: InterventionEvent; history?: InterventionEvent[] }
  | { type: "error"; message: string };

export interface Envelope<T> {
  id: number;
  body: T;
}
