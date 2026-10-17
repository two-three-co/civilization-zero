// The real thing: the kernel and the multiverse runtime compiled to wasm
// (crates/cz-web), one world per worker. Every frame carries the kernel's
// state hash and the wall-clock cost of what the worker just did. Build the
// wasm first: `npm run build:wasm` (wasm-pack writes src/wasm/, gitignored).

import init, { WasmWorld } from "../wasm/cz_web.js";
import type { Envelope, Frame, InterventionEvent, Request, Response, Timing, WorldInit } from "../protocol";

const ready = init();
let world: WasmWorld | null = null;
let timing: Timing = {};
// Requests are handled strictly in arrival order.
let queue: Promise<void> = Promise.resolve();

function need(): WasmWorld {
  if (!world) throw new Error("init first");
  return world;
}

function create(c: WorldInit): WasmWorld {
  world?.free();
  const every = c.snapshotEvery ?? 250;
  if (c.snapshot) return WasmWorld.fromSnapshot(c.snapshot, every);
  const patches = c.patches ?? Math.max(1, Math.floor((c.width * c.height) / 700));
  return new WasmWorld(c.seed >>> 0, c.width, c.height, c.agents, patches, every);
}

function frame(w: WasmWorld): Frame {
  return {
    source: "kernel",
    tick: w.tick(),
    width: w.width(),
    height: w.height(),
    hash: w.state_hash_hex(),
    food: w.food(),
    agents: w.agents(),
    alive: w.alive(),
    foodTotal: w.food_total(),
    frontier: w.frontier(),
    snapshotEvery: w.snapshot_every(),
    origin: w.origin(),
    timing: { ...timing },
  };
}

const post = (msg: Envelope<Response>, transfer: Transferable[] = []) =>
  (self as unknown as { postMessage(m: Envelope<Response>, t: Transferable[]): void }).postMessage(msg, transfer);

async function handle({ id, body }: Envelope<Request>): Promise<void> {
  try {
    await ready;
    let event: InterventionEvent | undefined;
    let history: InterventionEvent[] | undefined;
    let snapshot: Uint8Array | undefined;
    switch (body.type) {
      case "init":
        world = create(body.config);
        timing = {};
        break;
      case "step": {
        const ticks = Math.max(0, Math.floor(body.ticks));
        const t0 = performance.now();
        need().step(ticks);
        timing = { ...timing, stepMs: performance.now() - t0, stepTicks: ticks };
        break;
      }
      case "seek": {
        const t0 = performance.now();
        need().seek(Math.max(0, Math.floor(body.tick)));
        timing = { ...timing, seekMs: performance.now() - t0 };
        break;
      }
      case "intervene":
        event = JSON.parse(need().intervene(JSON.stringify(body.intervention))) as InterventionEvent;
        break;
      case "history":
        history = JSON.parse(need().interventions_json()) as InterventionEvent[];
        break;
      case "snapshot":
        snapshot = need().snapshot_bytes();
        break;
      case "frame":
        break;
    }
    const f = frame(need());
    const transfer: Transferable[] = [f.food.buffer, f.agents.buffer];
    if (snapshot) transfer.push(snapshot.buffer);
    post({ id, body: { type: "frame", frame: f, event, history, snapshot } }, transfer);
  } catch (err) {
    post({ id, body: { type: "error", message: err instanceof Error ? err.message : String(err) } });
  }
}

self.onmessage = (ev: MessageEvent<Envelope<Request>>) => {
  queue = queue.then(() => handle(ev.data));
};
