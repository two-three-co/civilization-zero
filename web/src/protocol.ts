// Messages between the UI thread and a world worker (ADR-0010 2b, ADR-0013 3-9a).
//
// The kernel worker (crates/cz-web via wasm) and the mock worker both speak
// this; the UI must not care which one is behind the client. Every request
// is answered with the current frame (or an error), so the UI never has to
// track state it did not get from the worker.
//
// v3 adds the social layer on top of v2 (init / step / seek / frame /
// intervene / history / snapshot, frames as Transferable typed arrays):
// `Frame.social` (markers for the transfers, harms and signals of the tick
// the view just executed), `Frame.ledger` (the kernel's running totals),
// `WorldInit.policy` (how a fresh world's population is split between the
// fixed policies) and a per-request `watch` that attaches one agent's
// kernel-written memory and ledger history to the frame.
//
// v4 adds negotiation threads (ADR-0013 3-9b): a `watch` may name a
// `partner`, and the frame then also carries `Watched.thread` — the ledger
// between the two agents with every accepted deal judged (kept / open /
// broken) by the kernel-side reader, never by the page — and a `deals`
// request lists every deal of a tick window so a thread can be found.

export const PROTOCOL_VERSION = 4;

/** Fields per agent in `Frame.agents`, matching the kernel's trace layout. */
export const AGENT_FIELDS = 8; // id, x, y, energy, inventory, policy (0 forager / 1 random), health, age

/**
 * Fields per marker in `Frame.social`: kind, tick, x, y, x2, y2, value.
 * `x2, y2` is the other party (receiver / victim) or -1, -1 when there is
 * none (a signal) or it is no longer alive in the view.
 */
export const MARKER_FIELDS = 7;
export const MARKER_TRANSFER = 0; // value = food units moved
export const MARKER_HARM = 1; // value = health taken
export const MARKER_SIGNAL = 2; // value = the symbol said

/** How a fresh world's founding population is assigned fixed policies (same rule as `cz run --policy`). */
export type PolicyMix = "forager" | "mixed" | "random";
export const POLICY_MIXES: PolicyMix[] = ["forager", "mixed", "random"];

export interface WorldInit {
  seed: number;
  width: number;
  height: number;
  agents: number;
  /** Food patches; the kernel worker defaults to `width * height / 700` like `cz`. */
  patches?: number;
  /** Snapshot cadence of the kernel worker; bounds how far a seek re-executes. */
  snapshotEvery?: number;
  /** Policy mix of the founding population; `forager` when omitted. Foragers alone never interact. */
  policy?: PolicyMix;
  /**
   * Fork origin: a binary snapshot exported by another world (`snapshot`
   * request). When set, seed/width/height/agents/patches/policy are ignored
   * and the new world starts at the snapshot's tick, which becomes its `origin`.
   */
  snapshot?: Uint8Array;
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

export interface Pos {
  x: number;
  y: number;
}

/** The kernel's `Ledger` as serde renders it: running totals since tick 0 (ADR-0010 2a-1, ADR-0013 D3). */
export interface Ledger {
  food_initial: number;
  food_regrown: number;
  food_consumed: number;
  energy_initial: number;
  energy_gained: number;
  energy_spent: number;
  energy_written_off: number;
  births: number;
  deaths: number;
  /** starvation, old age, violence */
  deaths_by_cause: [number, number, number];
  transfers: number;
  food_transferred: number;
  food_stored: number;
  harms: number;
  /** `Say` signals; negotiation messages are counted separately (3-4). */
  signals: number;
  proposals: number;
  acceptances: number;
  rejections: number;
  food_intervened: number;
  energy_intervened: number;
  interventions: number;
}

/** `Message` as serde renders it; `say` is the only one an agent's `last_signal` can hold. */
export type Message = { say: number } | { propose: { to: number; give: number; want: number; by: number } } | { accept: { to: number; offer_tick: number } } | { reject: { to: number } };

export interface Signal {
  tick: number;
  message: Message;
}

/** `InteractionKind` as serde renders it (ADR-0013 D4); the last four arrive with 3-4. */
export type InteractionKind =
  | "gave_to"
  | "received_from"
  | "harmed"
  | "harmed_by"
  | "saw_harm"
  | "heard"
  | "proposal_from"
  | "accepted"
  | "rejected"
  | "promised";

/** One entry of an agent's kernel-written memory (`Agent.recent`). */
export interface Interaction {
  tick: number;
  other: number;
  kind: InteractionKind;
  /** Amount, damage, the harmed agent's id, the symbol heard, or packed terms, depending on `kind`. */
  value: number;
}

/** The kernel's `Agent` as serde renders it (`WasmWorld::agent_json`). */
export interface AgentDetail {
  id: number;
  pos: Pos;
  energy: number;
  health: number;
  inventory: number;
  age: number;
  policy: "Forager" | "Random";
  params: { hunger_permille: number };
  memory: { target: Pos | null };
  last_signal: Signal | null;
  /** Oldest first, at most `memory_slots` (8 by default). Written by the kernel only. */
  recent: Interaction[];
}

/** Any ledger record (`Event` as serde renders it, tagged by `kind`). */
export type LedgerEvent =
  | InterventionEvent
  | { kind: "born"; tick: number; child: number; parent: number; pos: Pos; energy: number }
  | {
      kind: "died";
      tick: number;
      agent: number;
      cause: "starvation" | "old_age" | "violence";
      pos: Pos;
      age: number;
      energy_written_off: number;
      food_dropped: number;
    }
  | { kind: "transferred"; tick: number; from: number; to: number; amount: number }
  | { kind: "harmed"; tick: number; by: number; target: number; damage: number }
  | { kind: "signaled"; tick: number; agent: number; message: Message }
  | { kind: "acted"; tick: number; agent: number; action: unknown; outcome: unknown }
  | { kind: "proposed"; tick: number; from: number; to: number; give: number; want: number; by: number }
  | { kind: "accepted"; tick: number; from: number; to: number; offer_tick: number; give: number; want: number; by: number }
  | { kind: "rejected"; tick: number; from: number; to: number };

/** Ask the worker to attach one agent's detail to every frame it sends. */
export interface Watch {
  id: number;
  /** How many ticks of ledger history (ending at the view's tick) to include. */
  window: number;
  /** 3-9b: also attach the negotiation thread between `id` and this agent (`Watched.thread`). */
  partner?: number;
  /** Ticks of ledger history the thread covers (ending at the view's tick); `window` when omitted. */
  threadWindow?: number;
}

/** What a `watch` yields: the agent as it is in the view (null if not alive there) and its ledger history. */
export interface Watched {
  id: number;
  window: number;
  agent: AgentDetail | null;
  /** Ledger events of the last `window` ticks that name the agent, oldest first. */
  events: LedgerEvent[];
  /** Present when the watch named a `partner` (3-9b). */
  thread?: Thread;
}

// --- Negotiation threads (ADR-0013 3-9b): `WasmWorld::thread_json` / `deals_json` as serde renders them ---

/** What became of a proposal, as of the thread's `now`. */
export type Answer = "accepted" | "rejected" | "pending" | "lapsed";
/** Which side of a deal a transfer served: the proposer's `give` or the acceptor's `want`. */
export type Role = "give" | "repay";
export type DealStatus = "open" | "fulfilled" | "broken";

/**
 * An accepted proposal, judged from the transfers between the pair in its
 * window (`offer_tick..=deadline`): fulfilled once `given >= give` and
 * `repaid >= want`, broken once the deadline has passed with a side short,
 * open otherwise. The judgement is the kernel-side reader's (ADR-0013 D5 as
 * 3-9b reads it); the page only displays it.
 */
export interface Deal {
  proposer: number;
  acceptor: number;
  offer_tick: number;
  accepted_tick: number;
  /** `offer_tick + by`: the last tick a transfer counts. */
  deadline: number;
  give: number;
  want: number;
  /** Food the proposer handed the acceptor within the window. */
  given: number;
  /** Food the acceptor handed back within the window. */
  repaid: number;
  status: DealStatus;
  /** Tick at which both sides had delivered (never before the acceptance). */
  settled_tick: number | null;
  /** Who fell short when broken: the proposer, the acceptor, or both. */
  broken_by: number[];
}

/** One ledger event between the two agents of a thread, with its place in the deals (`deal` indexes `Thread.deals`). */
export type ThreadEntry =
  | { kind: "proposed"; tick: number; from: number; to: number; give: number; want: number; by: number; deadline: number; answer: Answer; deal: number | null }
  | { kind: "accepted"; tick: number; from: number; to: number; offer_tick: number; give: number; want: number; by: number; deadline: number; deal: number }
  /** `offer_tick` is the latest earlier offer from `to` (the kernel records none for a rejection). */
  | { kind: "rejected"; tick: number; from: number; to: number; offer_tick: number | null }
  /** `deal` / `role` name the promise the transfer went toward, null when no promise still called for it. */
  | { kind: "transferred"; tick: number; from: number; to: number; amount: number; deal: number | null; role: Role | null }
  | { kind: "harmed"; tick: number; by: number; target: number; damage: number };

/** The ledger between two agents over ticks `from..now`, read at tick `now` (the view's). */
export interface Thread {
  a: number;
  b: number;
  from: number;
  now: number;
  /** Every ledger event naming both agents, oldest first. */
  entries: ThreadEntry[];
  /** One per accepted entry, in ledger order. */
  deals: Deal[];
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
  /**
   * `MARKER_FIELDS` values per transfer / harm / signal of the tick the view
   * just executed (tick − 1), at the agents' positions in the view.
   * Transferred, not copied. Empty at tick 0 and from the mock.
   */
  social: Int32Array;
  /** The kernel's running totals at this tick. */
  ledger: Ledger;
  alive: number;
  foodTotal: number;
  /** Furthest tick this world has simulated; the timeline's right edge. */
  frontier: number;
  /** Snapshot cadence: a seek to tick T re-executes at most T mod this. */
  snapshotEvery: number;
  /** Tick this world's history begins at: 0, or the fork tick for a fork. It cannot seek earlier. */
  origin: number;
  /** Present when the request carried a `watch`. */
  watched?: Watched;
  timing?: Timing;
}

export type Request =
  | { type: "init"; config: WorldInit }
  | { type: "step"; ticks: number }
  | { type: "seek"; tick: number }
  | { type: "frame" }
  | { type: "intervene"; intervention: Intervention }
  | { type: "history" }
  /** Binary snapshot of the current view, to fork from in another worker. */
  | { type: "snapshot" }
  /** Every deal of ticks `from..to` (capped at the view's tick), judged as of the window's end (3-9b). */
  | { type: "deals"; from: number; to: number };

export type Response =
  | {
      type: "frame";
      frame: Frame;
      event?: InterventionEvent;
      history?: InterventionEvent[];
      snapshot?: Uint8Array;
      deals?: Deal[];
    }
  | { type: "error"; message: string };

export interface Envelope<T> {
  id: number;
  body: T;
  /** Requests only: attach this agent's detail (`Frame.watched`) to the answering frame. */
  watch?: Watch;
}
