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
//
// v5 brings the learning policies to the page (ADR-0013 3-9c): the policy
// column of `Frame.agents` is the kind's code 0..=3 (it was 0 / 1), the
// founding mixes include reflex / learner / mixed-learning, interventions
// include `set_policy` (D7) — "fixed vs learning" is a fork with it — and
// `AgentDetail` carries a learner's Q table and last step. Nothing else
// moved: the Transferable arrays and every request keep their v4 shape.
//
// v6 brings institutions to the page (ADR-0014 D12, 4-7), read from the
// kernel and never judged here: `Frame.agents` gains a ninth column
// `flags` (accused / voted / harmable / rejectable bits of the kernel's
// NormIndex), `Frame.institution` carries the charter, the rules in force,
// the open motion (none before 4-5) and how many stand accused, `Ledger`
// shows the 4-1 / 4-2 counters (gathers, encroachments, promises,
// violations, sanctions, episodes — the UI draws rates from their
// deltas), `AgentDetail` its `arrived` tick, open episodes and vote,
// `Interaction` the kinds the 4-1 / 4-2 memory writes, `LedgerEvent` the
// `sanctioned` / `violated` records, `Intervention` the decrees `enact` /
// `repeal` / `charter`, `WorldInit` the `nomos` / `recordViolations`
// switches, and four requests read the ledger: `institution` (the same
// object as the frame's), `timeline` (decrees with the rule ids they
// received), `violations` (recorded ones, or the episodes open now) and
// `sanctions` (each tied to the episode it answered). Motions (4-5) will
// join the timeline and the frame without changing these shapes.

export const PROTOCOL_VERSION = 6;

/** Fields per agent in `Frame.agents`: the kernel's trace layout, then `flags`. */
export const AGENT_FIELDS = 9; // id, x, y, energy, inventory, policy (index into POLICY_KINDS), health, age, flags

/** Bits of the `flags` column (cz-web `FLAG_*`, from the kernel's NormIndex at the view's tick). */
export const FLAG_ACCUSED = 1; // some episode of accusation is open
export const FLAG_VOTED = 2; // has voted on the open motion (none before 4-5)
export const FLAG_HARMABLE = 4; // a harm reaching the agent now would count as a sanction
export const FLAG_REJECTABLE = 8; // a rejection reaching the agent now would count as a sanction

/** `PolicyKind` as serde renders it (interventions, `AgentDetail.policy`). */
export type PolicyKind = "Forager" | "Random" | "Reflex" | "Learner";
/** The policy column of `Frame.agents` indexes this (cz-web `policy_code`, the kernel's order). */
export const POLICY_KINDS: PolicyKind[] = ["Forager", "Random", "Reflex", "Learner"];
/** `Action::kind_index` order: what a learner's `last_step.action` and the columns of its Q row mean. */
export const ACTION_KINDS = ["move", "gather", "consume", "rest", "transfer", "store", "harm", "signal"] as const;

/**
 * Fields per marker in `Frame.social`: kind, tick, x, y, x2, y2, value.
 * `x2, y2` is the other party (receiver / victim) or -1, -1 when there is
 * none (a signal) or it is no longer alive in the view.
 */
export const MARKER_FIELDS = 7;
export const MARKER_TRANSFER = 0; // value = food units moved
export const MARKER_HARM = 1; // value = health taken
export const MARKER_SIGNAL = 2; // value = the symbol said

/**
 * How a fresh world's founding population is assigned policies (same rule
 * as `cz run --policy`): `mixed` alternates forager / random by founding
 * index, `mixed-learning` cycles forager / reflex / learner. The evolving
 * mixes (`reflex`, `mixed-learning`) turn weight mutation on, as the CLI
 * does without `--no-mutate` (ADR-0013 D6).
 */
export type PolicyMix = "forager" | "mixed" | "random" | "reflex" | "learner" | "mixed-learning";
export const POLICY_MIXES: PolicyMix[] = ["forager", "mixed", "random", "reflex", "learner", "mixed-learning"];

// --- Institutions (ADR-0014 D5, as serde renders the kernel's types) ---

/** The three facts a rule can turn into a violation (`NormKind`); the index order of the per-kind counters. */
export type NormKind = "encroach" | "broken_promise" | "harm";
export const NORM_KINDS: NormKind[] = ["encroach", "broken_promise", "harm"];
/** `Sanction`: a bit set over the answers that count as sanctions to an accused agent. */
export const SANCTION_NONE = 0;
export const SANCTION_HARM = 1;
export const SANCTION_REJECT = 2;
/** How a sanction reached the accused (`Via`): the physical action it is read from. */
export type Via = "harm" | "reject";

/** A norm (`Norm`): which fact is a violation, how long a violator stands accused, which answers are sanctions. */
export interface Norm {
  kind: NormKind;
  /** A violator stands accused from the tick after each violation through `last + standing_ticks`; 0 = counted, never accused. */
  standing_ticks: number;
  /** `SANCTION_*` bits. */
  sanction: number;
  /** `harm` rules only: a harm that is a sanction is not a violation. */
  except_sanction: boolean;
}

/** A norm in force (`Rule`). Ids are monotonic and never reused. */
export interface Rule {
  id: number;
  norm: Norm;
  /** Tick the rule came into force. */
  enacted: number;
}

/** How the assembly decides (`Charter`); read by motions from 4-5. */
export interface Charter {
  quorum_permille: number;
  threshold_permille: number;
  vote_ticks: number;
  amendable: boolean;
}

/** What a motion asks for (`MotionKind`, externally tagged). */
export type MotionKind = { enact: { norm: Norm } } | { repeal: { rule: number } } | { charter: { charter: Charter } };

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
   * Institutions on (ADR-0014): the kernel worker builds the world with
   * `WorldConfig::nomos_preset()` (EXP-003's decision, `cz run --preset
   * nomos`), so decrees are accepted and the ledger counts encroachments,
   * violations and sanctions. Off (the default) every decree is rejected
   * and `Frame.institution` stays empty.
   */
  nomos?: boolean;
  /** Record every violation as a `violated` ledger event (`cz run --record-violations`); off by default (ADR-0014 D9: ~21 per tick). */
  recordViolations?: boolean;
  /**
   * Fork origin: a binary snapshot exported by another world (`snapshot`
   * request). When set, seed/width/height/agents/patches/policy are ignored
   * and the new world starts at the snapshot's tick, which becomes its `origin`.
   */
  snapshot?: Uint8Array;
}

/** Which living agents a `set_policy` applies to (`Who`): everyone, one id, or each independently with probability permille / 1000 (a seed-keyed draw). */
export type Who = "all" | { id: number } | { fraction: { permille: number } };
/** What a `set_policy` does to the heritable params (`ParamsMode`): the config defaults with a blank table, or keep them (a learner keeps what it learned). */
export type ParamsMode = "default" | "keep";

/** Exactly the kernel's `Intervention` as serde renders it (ADR-0010 2a-1, ADR-0013 D7). */
export type Intervention =
  | { op: "drought"; regrow_amount: number; regrow_interval: number }
  | { op: "scale_food"; permille: number }
  | { op: "spawn_agents"; n: number; pos: { x: number; y: number }; policy: PolicyKind }
  | { op: "remove_agent"; id: number }
  /** Give some living agents another policy; bodies, memories and signals are untouched. "Fixed vs learning" is a fork plus this. */
  | { op: "set_policy"; who: Who; kind: PolicyKind; params: ParamsMode }
  /** Decrees (ADR-0014 D8): put a norm in force (a rule of the same kind is replaced), take a rule out of force, replace the charter. Rejected unless the world has institutions (`WorldInit.nomos`). "With vs without rule" is a fork plus `enact`. */
  | { op: "enact"; norm: Norm }
  | { op: "repeal"; rule: number }
  | { op: "charter"; charter: Charter };

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
  /** Social facts (ADR-0014 D3, 4-1): gathers that took at least one unit (every world); the rest only with `nomos`. */
  gathers: number;
  /** Gathers on a cell whose first occupant arrived strictly earlier: a fact, not a violation. `encroachments / gathers` is the encroachment rate E. */
  encroachments: number;
  /** Accepted deals settled at their deadline, and those with a side short. */
  promises_due: number;
  promises_broken: number;
  /** Institutions (ADR-0014 D4, 4-2), by `NORM_KINDS` index; non-zero only with a rule in force. S' = sanctions / violations, S = episodes_sanctioned / episodes_closed. */
  violations: [number, number, number];
  sanctions: [number, number, number];
  episodes_closed: [number, number, number];
  episodes_sanctioned: [number, number, number];
}

/** `Message` as serde renders it; `say` is the only one an agent's `last_signal` can hold. */
export type Message = { say: number } | { propose: { to: number; give: number; want: number; by: number } } | { accept: { to: number; offer_tick: number } } | { reject: { to: number } };

export interface Signal {
  tick: number;
  message: Message;
}

/** `InteractionKind` as serde renders it (ADR-0013 D4; `promised` and the three before it with 3-4; `encroached_by` with 4-1; the last three with 4-2). */
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
  | "promised"
  /** Someone gathered on my cell after I arrived first (`value` = units taken). */
  | "encroached_by"
  /** My harm / rejection of `other` counted as a sanction (`value` = damage, 0 for a rejection). */
  | "sanctioned"
  /** `other`'s harm / rejection of me counted as a sanction (`value` = damage, 0 for a rejection). */
  | "sanctioned_by"
  /** I saw `other` sanction agent `value`. */
  | "saw_sanction";

/** One entry of an agent's kernel-written memory (`Agent.recent`). */
export interface Interaction {
  tick: number;
  other: number;
  kind: InteractionKind;
  /** Amount, damage, the harmed agent's id, the symbol heard, or packed terms, depending on `kind`. */
  value: number;
}

/**
 * The table a learning policy decides with (`PolicyTable`, ADR-0013 D6):
 * a reflex's 16 features × 8 action kinds of fixed-point weights
 * (row-major by feature, 1/1024 units), or a learner's `learner_states`
 * rows × 8 kinds of fixed-point Q values (16 = one reward unit = 100
 * milli-energy). Fixed policies have none.
 */
export type PolicyTable = { reflex: { weights: number[] } } | { learner: { q: number[] } };

/** A learner's last decision as the kernel recorded it (`LearnerStep`, 3-7): the Q row, the `ACTION_KINDS` index and that tick's reward in units of 100 milli-energy. */
export interface LearnerStep {
  state: number;
  action: number;
  reward: number;
}

/** The kernel's `Agent` as serde renders it (`WasmWorld::agent_json`). */
export interface AgentDetail {
  id: number;
  pos: Pos;
  energy: number;
  health: number;
  inventory: number;
  age: number;
  policy: PolicyKind;
  params: { hunger_permille: number; table: PolicyTable | null };
  memory: { target: Pos | null };
  last_signal: Signal | null;
  /** Oldest first, at most `memory_slots` (8 by default). Written by the kernel only. */
  recent: Interaction[];
  /** Learners only, once they have decided; `null` for every other kind, a newborn, or right after a `set_policy`. */
  last_step: LearnerStep | null;
  /** Tick the agent began standing on its cell (first occupancy, ADR-0014 D3); `null` for a founder or without `nomos`. */
  arrived: number | null;
  /** Episodes the agent stands accused in at the view's tick, ascending by rule (4-7). Empty without institutions. */
  standing: Accused[];
  /** Has voted on the open motion (always false before 4-5). */
  voted: boolean;
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
  | { kind: "rejected"; tick: number; from: number; to: number }
  /** A harm or rejection read as a sanction of `target`, who stood accused under `rule` (ADR-0014 D4); always recorded, right after the physical event. */
  | { kind: "sanctioned"; tick: number; by: number; target: number; rule: number; via: Via }
  /** A judged violation; recorded only with `WorldInit.recordViolations`. */
  | { kind: "violated"; tick: number; agent: number; rule: number; norm: NormKind; against: number; grounds: Grounds };

/** The evidence a `violated` record rests on (`Grounds`, tagged by `fact`). */
export type Grounds =
  | { fact: "encroachment"; cell: Pos; amount: number }
  | { fact: "broken_promise"; offer_tick: number; short: number }
  | { fact: "harm"; damage: number };

// --- Institutions as cz-web reads them (ADR-0014 D12, 4-7): `WasmWorld::institution_json` / `timeline_json` / `violations_json` / `sanctions_json` ---

/** The open motion, summarised (the voter lists stay in the kernel); none before 4-5. */
export interface OpenMotion {
  id: number;
  by: number;
  opened: number;
  kind: MotionKind;
  ayes: number;
  nays: number;
}

/** The institution of the view's tick (`Frame.institution`, `institution` request). */
export interface Institution {
  /** Whether the world has institutions at all (`WorldInit.nomos`); off, everything below is empty and every decree is rejected. */
  nomos: boolean;
  /** Whether `violations` reads the ledger (true) or the open episodes (false). */
  record_violations: boolean;
  charter: Charter;
  /** Rules in force, ascending by id (at most one per kind). */
  rules: Rule[];
  motion: OpenMotion | null;
  /** Episodes of accusation open at this tick: in all, distinct agents, and by the kind of the rule (`NORM_KINDS` order). */
  accused: { episodes: number; agents: number; by_kind: [number, number, number] };
}

/** One open episode of accusation: the kernel's `Standing` with the rule's kind and the last tick it lasts (`last + standing_ticks`). */
export interface Accused {
  agent: number;
  rule: number;
  norm: NormKind;
  /** Tick of the first violation of the episode. */
  since: number;
  /** Tick of the latest violation. */
  last: number;
  /** The last tick the agent stands accused for this episode. */
  until: number;
  /** The other party of the latest violation. */
  against: number;
  violations: number;
  sanctions: number;
}

/** Who put a rule or charter in place: a decree (an intervention). Motions add their own value with 4-5. */
export type By = "decree";

/** One thing that happened to the institution, as the ledger says it (rule ids recovered by replaying the decrees from the branch's origin). */
export type TimelineEntry =
  /** `norm` came into force as rule `rule`; `replaced` is the rule of the same kind it took out of force, if any. */
  | { kind: "enacted"; tick: number; rule: number; norm: Norm; replaced: number | null; by: By }
  /** Rule `rule` went out of force (`norm` is what it said). */
  | { kind: "repealed"; tick: number; rule: number; norm: Norm | null; by: By }
  | { kind: "chartered"; tick: number; charter: Charter; by: By };

/** The institution's history over ticks `from..now` (`now` = the request's `to` capped at the view's tick), ledger order. */
export interface Timeline {
  from: number;
  now: number;
  entries: TimelineEntry[];
}

/** Where a list of violations or an episode was read from: `violated` ledger events, or the episodes open in the state at the view's tick. */
export type Source = "ledger" | "standing";

/** One violation: a recorded one (who, which rule, against whom, when, on what evidence), or an episode open now (the second path of D12). */
export type ViolationEntry =
  | { kind: "violated"; tick: number; agent: number; rule: number; norm: NormKind; against: number; grounds: Grounds }
  | ({ kind: "accused" } & Accused);

/**
 * Violations of ticks `from..now` when the world records them (`source:
 * "ledger"`), else every episode open at `now`, newest `last` first
 * (`source: "standing"`; `from` does not apply — an agent accused since
 * before it still stands accused).
 */
export interface Violations {
  source: Source;
  from: number;
  now: number;
  entries: ViolationEntry[];
}

/** The episode a sanction answered, as of `now`: the one still open at the view, or its reconstruction from the recorded violations once closed. */
export interface Episode {
  since: number;
  last: number;
  against: number;
  violations: number;
  sanctions: number;
  open: boolean;
  source: Source;
}

/** One sanction: the kernel's `sanctioned` record with the rule's kind and the episode it answered (`null` when it cannot be known: the episode closed and violations were not recorded). */
export interface SanctionEntry {
  tick: number;
  by: number;
  target: number;
  rule: number;
  norm: NormKind | null;
  via: Via;
  episode: Episode | null;
}

/** Sanctions of ticks `from..now`, ledger order. */
export interface Sanctions {
  from: number;
  now: number;
  entries: SanctionEntry[];
}

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
  /** The institution at this tick (ADR-0014 D12): empty, with `nomos: false`, in a world without institutions and from the mock. */
  institution: Institution;
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
  | { type: "deals"; from: number; to: number }
  /** The institution of the view (the same object every frame carries; 4-7). */
  | { type: "institution" }
  /** Decrees (and, from 4-5, motions) of ticks `from..to` (capped at the view's tick) with the rule ids they received. */
  | { type: "timeline"; from: number; to: number }
  /** Recorded violations of ticks `from..to`, or the episodes open at the view's tick. */
  | { type: "violations"; from: number; to: number }
  /** Sanctions of ticks `from..to`, each with the episode it answered. */
  | { type: "sanctions"; from: number; to: number };

export type Response =
  | {
      type: "frame";
      frame: Frame;
      event?: InterventionEvent;
      history?: InterventionEvent[];
      snapshot?: Uint8Array;
      deals?: Deal[];
      institution?: Institution;
      timeline?: Timeline;
      violations?: Violations;
      sanctions?: Sanctions;
    }
  | { type: "error"; message: string };

export interface Envelope<T> {
  id: number;
  body: T;
  /** Requests only: attach this agent's detail (`Frame.watched`) to the answering frame. */
  watch?: Watch;
}
