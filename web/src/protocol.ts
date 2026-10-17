// Messages between the UI thread and a world worker (ADR-0010 2b-2).
//
// The mock worker and the future wasm worker (2b-1) both speak this; the UI
// must not care which one is behind the client. Every request is answered
// with the current frame (or an error), so the UI never has to track state
// it did not get from the worker.

/** Fields per agent in `Frame.agents`, matching the kernel's trace layout. */
export const AGENT_FIELDS = 8; // id, x, y, energy, inventory, policy (0 forager / 1 random), health, age

export interface WorldInit {
  seed: number;
  width: number;
  height: number;
  agents: number;
}

export interface Frame {
  tick: number;
  width: number;
  height: number;
  /** Row-major food per cell, clamped to 255. Transferred, not copied. */
  food: Uint8Array;
  /** `AGENT_FIELDS` values per living agent. Transferred, not copied. */
  agents: Int32Array;
  alive: number;
  foodTotal: number;
  /** Furthest tick this world has simulated; the timeline's right edge. */
  frontier: number;
}

export type Request =
  | { type: "init"; config: WorldInit }
  | { type: "step"; ticks: number }
  | { type: "seek"; tick: number }
  | { type: "frame" };

export type Response = { type: "frame"; frame: Frame } | { type: "error"; message: string };

export interface Envelope<T> {
  id: number;
  body: T;
}
