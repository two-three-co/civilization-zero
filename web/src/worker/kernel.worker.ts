// The real thing: the kernel and the multiverse runtime compiled to wasm
// (crates/cz-web), one world per worker. Every frame carries the kernel's
// state hash and the wall-clock cost of what the worker just did. Build the
// wasm first: `npm run build:wasm` (wasm-pack writes src/wasm/, gitignored).

import init, { WasmWorld } from "../wasm/cz_web.js";
import {
  AGENT_FIELDS,
  MARKER_SANCTION,
  type AgentDetail,
  type Deal,
  type Envelope,
  type Frame,
  type Institution,
  type InterventionEvent,
  type Ledger,
  type LedgerEvent,
  type Request,
  type Response,
  type Sanctions,
  type Thread,
  type Timeline,
  type Timing,
  type Violations,
  type Watch,
  type WorldInit,
} from "../protocol";

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
  // `WorldOptions` (cz-web): institutions on = `WorldConfig::nomos_preset()`, as `cz run --preset nomos`.
  const options = JSON.stringify({ nomos: c.nomos ?? false, record_violations: c.recordViolations ?? false });
  return new WasmWorld(c.seed >>> 0, c.width, c.height, c.agents, patches, every, c.policy ?? "forager", options);
}

/** A tick window `from..to` as the kernel takes it: non-negative integers (the kernel caps `to` at the view's tick). */
function span(from: number, to: number): [number, number] {
  return [Math.max(0, Math.floor(from)), Math.max(0, Math.floor(to))];
}

/**
 * The sanctions of the tick the view just executed (tick − 1) as markers
 * (4-8): the kernel's `sanctioned` records of that tick — a harm or
 * rejection it counted as a sanction (ADR-0014 D4) — placed at the agents'
 * positions in the view exactly as `social()` places the harms (a
 * sanctioner who died that tick puts the marker at the accused alone; both
 * gone, no marker). Read only where a sanction can exist: a world with
 * institutions that has a rule in force or has sanctioned before. Nothing is
 * judged here; the page is told which harms the kernel called sanctions.
 */
function sanctionMarkers(w: WasmWorld, tick: number, agents: Int32Array, inst: Institution, ledger: Ledger): Int32Array | null {
  const ever = ledger.sanctions[0] + ledger.sanctions[1] + ledger.sanctions[2] > 0;
  if (!inst.nomos || tick === 0 || (inst.rules.length === 0 && !ever)) return null;
  const events = JSON.parse(w.events_at(tick - 1)) as LedgerEvent[];
  const out: number[] = [];
  let at: Map<number, number> | null = null;
  for (const e of events) {
    if (e.kind !== "sanctioned") continue;
    if (!at) {
      at = new Map();
      for (let k = 0; k < agents.length; k += AGENT_FIELDS) at.set(agents[k], k);
    }
    const kb = at.get(e.by);
    const kt = at.get(e.target);
    if (kb === undefined && kt === undefined) continue;
    const from = kb ?? kt!;
    out.push(
      MARKER_SANCTION,
      tick - 1,
      agents[from + 1],
      agents[from + 2],
      kt === undefined ? -1 : agents[kt + 1],
      kt === undefined ? -1 : agents[kt + 2],
      e.via === "harm" ? 1 : 2,
    );
  }
  return out.length > 0 ? Int32Array.from(out) : null;
}

function frame(w: WasmWorld, watch?: Watch): Frame {
  const tick = w.tick();
  const agents = w.agents();
  const ledger = JSON.parse(w.ledger_json()) as Ledger;
  // The institution of the view, read from the state (ADR-0014 D12).
  const institution = JSON.parse(w.institution_json()) as Institution;
  let social = w.social();
  const sanctions = sanctionMarkers(w, tick, agents, institution, ledger);
  if (sanctions) {
    const merged = new Int32Array(social.length + sanctions.length);
    merged.set(social);
    merged.set(sanctions, social.length);
    social = merged;
  }
  const f: Frame = {
    source: "kernel",
    tick,
    width: w.width(),
    height: w.height(),
    hash: w.state_hash_hex(),
    food: w.food(),
    agents,
    social,
    ledger,
    alive: w.alive(),
    foodTotal: w.food_total(),
    frontier: w.frontier(),
    snapshotEvery: w.snapshot_every(),
    origin: w.origin(),
    institution,
    timing: { ...timing },
  };
  if (watch) {
    // Everything here is read from the kernel: the agent as it is in the
    // view (with the memory the kernel wrote for it) and the ledger's events
    // naming it in the last `window` ticks. Nothing is remembered page-side.
    const id = Math.max(0, Math.floor(watch.id));
    const window = Math.max(0, Math.floor(watch.window));
    f.watched = {
      id,
      window,
      agent: JSON.parse(w.agent_json(id)) as AgentDetail | null,
      events: JSON.parse(w.agent_events_json(id, Math.max(0, tick - window), tick)) as LedgerEvent[],
    };
    if (watch.partner !== undefined) {
      // The thread between the two, with every deal judged by the
      // kernel-side reader (3-9b); the page only displays it.
      const partner = Math.max(0, Math.floor(watch.partner));
      const span = Math.max(0, Math.floor(watch.threadWindow ?? window));
      f.watched.thread = JSON.parse(w.thread_json(id, partner, Math.max(0, tick - span), tick)) as Thread;
    }
  }
  return f;
}

const post = (msg: Envelope<Response>, transfer: Transferable[] = []) =>
  (self as unknown as { postMessage(m: Envelope<Response>, t: Transferable[]): void }).postMessage(msg, transfer);

async function handle({ id, body, watch }: Envelope<Request>): Promise<void> {
  try {
    await ready;
    let event: InterventionEvent | undefined;
    let history: InterventionEvent[] | undefined;
    let snapshot: Uint8Array | undefined;
    let deals: Deal[] | undefined;
    let institution: Institution | undefined;
    let timeline: Timeline | undefined;
    let violations: Violations | undefined;
    let sanctions: Sanctions | undefined;
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
      case "deals":
        deals = JSON.parse(need().deals_json(...span(body.from, body.to))) as Deal[];
        break;
      // Institutions (4-7): every judgement is the kernel-side reader's; the
      // worker only parses what cz-web renders.
      case "institution":
        institution = JSON.parse(need().institution_json()) as Institution;
        break;
      case "timeline":
        timeline = JSON.parse(need().timeline_json(...span(body.from, body.to))) as Timeline;
        break;
      case "violations":
        violations = JSON.parse(need().violations_json(...span(body.from, body.to))) as Violations;
        break;
      case "sanctions":
        sanctions = JSON.parse(need().sanctions_json(...span(body.from, body.to))) as Sanctions;
        break;
      case "frame":
        break;
    }
    const f = frame(need(), watch);
    const transfer: Transferable[] = [f.food.buffer, f.agents.buffer, f.social.buffer];
    if (snapshot) transfer.push(snapshot.buffer);
    post(
      { id, body: { type: "frame", frame: f, event, history, snapshot, deals, institution, timeline, violations, sanctions } },
      transfer,
    );
  } catch (err) {
    post({ id, body: { type: "error", message: err instanceof Error ? err.message : String(err) } });
  }
}

self.onmessage = (ev: MessageEvent<Envelope<Request>>) => {
  queue = queue.then(() => handle(ev.data));
};
