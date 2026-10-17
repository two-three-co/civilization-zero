// The real thing: the kernel and the multiverse runtime compiled to wasm
// (crates/cz-web), one world per worker. Every frame carries the kernel's
// state hash. Build the wasm first: `npm run build:wasm` (wasm-pack writes
// src/wasm/, which is gitignored).

import init, { WasmWorld } from "../wasm/cz_web.js";
import type { Envelope, Frame, Request, Response, WorldInit } from "../protocol";

const ready = init();
let world: WasmWorld | null = null;
// Requests are handled strictly in arrival order.
let queue: Promise<void> = Promise.resolve();

function need(): WasmWorld {
  if (!world) throw new Error("init first");
  return world;
}

function create(c: WorldInit): WasmWorld {
  world?.free();
  const patches = c.patches ?? Math.max(1, Math.floor((c.width * c.height) / 700));
  return new WasmWorld(c.seed >>> 0, c.width, c.height, c.agents, patches, c.snapshotEvery ?? 250);
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
  };
}

const post = (msg: Envelope<Response>, transfer: Transferable[] = []) =>
  (self as unknown as { postMessage(m: Envelope<Response>, t: Transferable[]): void }).postMessage(msg, transfer);

async function handle({ id, body }: Envelope<Request>): Promise<void> {
  try {
    await ready;
    switch (body.type) {
      case "init":
        world = create(body.config);
        break;
      case "step":
        need().step(Math.max(0, Math.floor(body.ticks)));
        break;
      case "seek":
        need().seek(Math.max(0, Math.floor(body.tick)));
        break;
      case "frame":
        break;
    }
    const f = frame(need());
    post({ id, body: { type: "frame", frame: f } }, [f.food.buffer, f.agents.buffer]);
  } catch (err) {
    post({ id, body: { type: "error", message: err instanceof Error ? err.message : String(err) } });
  }
}

self.onmessage = (ev: MessageEvent<Envelope<Request>>) => {
  queue = queue.then(() => handle(ev.data));
};
