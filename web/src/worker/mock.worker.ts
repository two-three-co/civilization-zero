// A stand-in world so the UI can be built before the wasm kernel worker
// (ADR-0010 2b-1). It imitates the kernel's shape — grid of food patches,
// foragers and random walkers, metabolism, birth by energy split, death by
// starvation or age, regrowth every 10 ticks, snapshots for seeking — but it
// is NOT the kernel: no conservation ledger, no determinism contract, no
// hash. Nothing shown from it is a fact about Civilization Zero.

import { AGENT_FIELDS, type Envelope, type Frame, type Request, type Response, type WorldInit } from "../protocol";

interface State {
  tick: number;
  rng: number;
  food: Uint8Array;
  agents: Int32Array;
  alive: number;
  nextId: number;
}

const F = AGENT_FIELDS;
const SNAP_EVERY = 50;
const CAPACITY = 20;
const VISION = 4;

let cfg: WorldInit | null = null;
let cap = new Uint8Array(0);
let state: State | null = null;
let frontier = 0;
const snapshots = new Map<number, State>();

function rand(s: State): number {
  s.rng = (Math.imul(s.rng, 1664525) + 1013904223) >>> 0;
  return s.rng / 4294967296;
}

function clone(s: State): State {
  return { tick: s.tick, rng: s.rng, food: s.food.slice(), agents: s.agents.slice(), alive: s.alive, nextId: s.nextId };
}

function ensureCapacity(s: State, agents: number): void {
  if (s.agents.length >= agents * F) return;
  const bigger = new Int32Array(Math.max(agents * F, s.agents.length * 2));
  bigger.set(s.agents);
  s.agents = bigger;
}

function init(c: WorldInit): void {
  cfg = c;
  snapshots.clear();
  frontier = 0;
  const n = c.width * c.height;
  cap = new Uint8Array(n);
  const s: State = {
    tick: 0,
    rng: c.seed >>> 0 || 1,
    food: new Uint8Array(n),
    agents: new Int32Array(c.agents * F * 2),
    alive: c.agents,
    nextId: c.agents,
  };
  const patches = Math.max(1, Math.floor(n / 700));
  const R = 8;
  for (let p = 0; p < patches; p++) {
    const cx = Math.floor(rand(s) * c.width);
    const cy = Math.floor(rand(s) * c.height);
    for (let dy = -R; dy <= R; dy++) {
      for (let dx = -R; dx <= R; dx++) {
        const d = Math.abs(dx) + Math.abs(dy);
        const x = cx + dx;
        const y = cy + dy;
        if (d > R || x < 0 || y < 0 || x >= c.width || y >= c.height) continue;
        const idx = y * c.width + x;
        cap[idx] = Math.max(cap[idx], Math.floor((CAPACITY * (R + 1 - d)) / (R + 1)));
      }
    }
  }
  s.food.set(cap);
  for (let i = 0; i < c.agents; i++) {
    const b = i * F;
    s.agents[b] = i;
    s.agents[b + 1] = Math.floor(rand(s) * c.width);
    s.agents[b + 2] = Math.floor(rand(s) * c.height);
    s.agents[b + 3] = 50_000;
    s.agents[b + 4] = 0;
    s.agents[b + 5] = i % 4 === 0 ? 1 : 0;
    s.agents[b + 6] = 100;
    s.agents[b + 7] = Math.floor(rand(s) * 3000);
  }
  state = s;
  snapshots.set(0, clone(s));
}

function stepOnce(s: State): void {
  const c = cfg!;
  const w = c.width;
  const h = c.height;
  const A = s.agents;
  for (let i = 0; i < s.alive; i++) {
    const b = i * F;
    let x = A[b + 1];
    let y = A[b + 2];
    let e = A[b + 3];
    let inv = A[b + 4];
    const idx = y * w + x;
    let moved = false;
    if (A[b + 5] === 1) {
      const r = rand(s);
      if (r < 0.57) {
        const d = Math.floor(rand(s) * 4);
        const nx = x + (d === 1 ? 1 : d === 3 ? -1 : 0);
        const ny = y + (d === 2 ? 1 : d === 0 ? -1 : 0);
        if (nx >= 0 && ny >= 0 && nx < w && ny < h) {
          x = nx;
          y = ny;
          moved = true;
        }
      } else if (r < 0.71 && s.food[idx] > 0 && inv < 10) {
        s.food[idx]--;
        inv++;
      } else if (r < 0.86 && inv > 0 && e + 4000 <= 100_000) {
        inv--;
        e += 4000;
      }
    } else if (inv > 0 && e < 50_000) {
      inv--;
      e += 4000;
    } else if (s.food[idx] > 0 && inv < 10) {
      const q = Math.min(2, s.food[idx], 10 - inv);
      s.food[idx] -= q;
      inv += q;
    } else {
      let bestF = 0;
      let bestD = 99;
      let tx = -1;
      let ty = -1;
      for (let dy = -VISION; dy <= VISION; dy++) {
        const span = VISION - Math.abs(dy);
        for (let dx = -span; dx <= span; dx++) {
          const nx = x + dx;
          const ny = y + dy;
          if (nx < 0 || ny < 0 || nx >= w || ny >= h || (dx === 0 && dy === 0)) continue;
          const f = s.food[ny * w + nx];
          const d = Math.abs(dx) + Math.abs(dy);
          if (f > bestF || (f === bestF && f > 0 && d < bestD)) {
            bestF = f;
            bestD = d;
            tx = nx;
            ty = ny;
          }
        }
      }
      let nx = x;
      let ny = y;
      if (tx >= 0) {
        if (Math.abs(tx - x) >= Math.abs(ty - y)) nx += Math.sign(tx - x);
        else ny += Math.sign(ty - y);
      } else {
        const d = Math.floor(rand(s) * 4);
        nx += d === 1 ? 1 : d === 3 ? -1 : 0;
        ny += d === 2 ? 1 : d === 0 ? -1 : 0;
      }
      if (nx >= 0 && ny >= 0 && nx < w && ny < h) {
        x = nx;
        y = ny;
        moved = true;
      }
    }
    e -= 500 + (moved ? 300 : 0);
    if (e < 0) e = 0;
    A[b + 1] = x;
    A[b + 2] = y;
    A[b + 3] = e;
    A[b + 4] = inv;
    if (e === 0) A[b + 6] -= 5;
    A[b + 7]++;
  }
  // Deaths: starvation or old age; carried food drops on the cell.
  let j = 0;
  for (let i = 0; i < s.alive; i++) {
    const b = i * F;
    if (A[b + 6] > 0 && A[b + 7] < 3000) {
      if (i !== j) A.copyWithin(j * F, b, b + F);
      j++;
    } else {
      const idx = A[b + 2] * w + A[b + 1];
      s.food[idx] = Math.min(255, s.food[idx] + A[b + 4]);
    }
  }
  s.alive = j;
  // Births: energy split.
  const parents = s.alive;
  for (let i = 0; i < parents; i++) {
    const b = i * F;
    if (A[b + 3] < 80_000) continue;
    A[b + 3] -= 40_000;
    ensureCapacity(s, s.alive + 1);
    const cb = s.alive * F;
    s.agents[cb] = s.nextId++;
    s.agents[cb + 1] = A[b + 1];
    s.agents[cb + 2] = A[b + 2];
    s.agents[cb + 3] = 40_000;
    s.agents[cb + 4] = 0;
    s.agents[cb + 5] = A[b + 5];
    s.agents[cb + 6] = 100;
    s.agents[cb + 7] = 0;
    s.alive++;
  }
  s.tick++;
  if (s.tick % 10 === 0) {
    for (let k = 0; k < s.food.length; k++) if (s.food[k] < cap[k]) s.food[k]++;
  }
}

function advanceTo(target: number): void {
  const s = state!;
  while (s.tick < target) {
    stepOnce(s);
    if (s.tick > frontier) {
      frontier = s.tick;
      if (s.tick % SNAP_EVERY === 0) snapshots.set(s.tick, clone(s));
    }
  }
}

function seek(target: number): void {
  const s = state!;
  if (target >= s.tick) {
    advanceTo(target);
    return;
  }
  let base = Math.floor(target / SNAP_EVERY) * SNAP_EVERY;
  while (base > 0 && !snapshots.has(base)) base -= SNAP_EVERY;
  state = clone(snapshots.get(base)!);
  advanceTo(target);
}

function frame(): Frame {
  const s = state!;
  const c = cfg!;
  let foodTotal = 0;
  for (let k = 0; k < s.food.length; k++) foodTotal += s.food[k];
  return {
    source: "mock",
    tick: s.tick,
    width: c.width,
    height: c.height,
    food: s.food.slice(),
    agents: s.agents.slice(0, s.alive * F),
    alive: s.alive,
    foodTotal,
    frontier,
    snapshotEvery: SNAP_EVERY,
  };
}

const post = (msg: Envelope<Response>, transfer: Transferable[] = []) =>
  (self as unknown as { postMessage(m: Envelope<Response>, t: Transferable[]): void }).postMessage(msg, transfer);

self.onmessage = (ev: MessageEvent<Envelope<Request>>) => {
  const { id, body } = ev.data;
  try {
    switch (body.type) {
      case "init":
        init(body.config);
        break;
      case "step":
        if (!state) throw new Error("init first");
        advanceTo(state.tick + body.ticks);
        break;
      case "seek":
        if (!state) throw new Error("init first");
        seek(Math.max(0, Math.floor(body.tick)));
        break;
      case "frame":
        if (!state) throw new Error("init first");
        break;
      case "intervene":
        throw new Error("the mock world has no interventions: use the kernel worker");
      case "history":
        if (!state) throw new Error("init first");
        break;
    }
    const f = frame();
    post({ id, body: { type: "frame", frame: f, history: body.type === "history" ? [] : undefined } }, [
      f.food.buffer,
      f.agents.buffer,
    ]);
  } catch (err) {
    post({ id, body: { type: "error", message: err instanceof Error ? err.message : String(err) } });
  }
};
