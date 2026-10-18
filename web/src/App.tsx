import { useCallback, useEffect, useRef, useState } from "react";
import { BranchesPanel, type Verification } from "./Branches";
import { WorldClient } from "./client";
import { InstitutionCard, NormRatesCard, SanctionsCard, ViolationsCard, type Readings } from "./Institution";
import { InterventionPanel } from "./Intervene";
import { compareRuleIntervention, describeStanding, sumKinds, describeMotionKind } from "./nomos";
import { compareIntervention, describePolicies, policyCounts } from "./policies";
import {
  ACTION_KINDS,
  AGENT_FIELDS,
  POLICY_KINDS,
  POLICY_MIXES,
  PROTOCOL_VERSION,
  type AgentDetail,
  type Deal,
  type Frame,
  type Interaction,
  type Intervention,
  type InterventionEvent,
  type LedgerEvent,
  type Message,
  type PolicyMix,
  type ThreadEntry,
  type Watched,
  type WorldInit,
} from "./protocol";
import {
  drawCurves,
  drawFrame,
  drawSparkline,
  History,
  MarkerTrail,
  NORM_CURVES,
  NORM_WINDOW,
  pickAgent,
  RATE_WINDOW,
  SCALE,
  valuesAt,
  type Sample,
} from "./render";
import { Tutorial } from "./Tutorial";

const PARAMS = new URLSearchParams(location.search);
// `?mock=1` keeps the stand-in world available for UI work without a wasm build.
const USE_MOCK = PARAMS.get("mock") === "1";
// `?policy=forager|mixed|random` picks the founding population (ADR-0013 3-9a).
// Forager is the Toy 1 world and the golden-hash path; foragers never
// interact, so the social layer only shows in `mixed` / `random` worlds (or
// after spawning random walkers into a forager world).
const POLICY: PolicyMix = (POLICY_MIXES as string[]).includes(PARAMS.get("policy") ?? "")
  ? (PARAMS.get("policy") as PolicyMix)
  : "forager";
// `?seed=N` picks the world's seed (3-9b). 42 is the Toy 1 world whose hashes
// STATUS records; `cz run --seed N --policy …` reproduces any other.
const SEED_PARAM = PARAMS.get("seed");
const SEED = SEED_PARAM !== null && /^\d{1,9}$/.test(SEED_PARAM) ? Number(SEED_PARAM) : 42;
// `?nomos=1` builds the world with institutions (ADR-0014 D10: the kernel
// worker uses `WorldConfig::nomos_preset()`, as `cz run --preset nomos`), so
// decrees are accepted and encroachments, violations and sanctions are
// counted; `?violations=1` also records every violation in the ledger (D9:
// ≈ 21 per tick, so off by default — the explorer then lists the episodes
// open now instead). Both reload with a fresh world when toggled.
const NOMOS = PARAMS.get("nomos") === "1";
const RECORD_VIOLATIONS = NOMOS && PARAMS.get("violations") === "1";
const DEFAULT_WORLD: WorldInit = {
  seed: SEED,
  width: 128,
  height: 128,
  agents: 1000,
  snapshotEvery: 250,
  policy: POLICY,
  nomos: NOMOS,
  recordViolations: RECORD_VIOLATIONS,
};
const SPEEDS = [1, 4, 15, 30];
/** Ticks of ledger history shown for the watched agent. */
const WATCH_WINDOW = 100;
/** How often, while playing, the institution cards re-read the ledger (one-shot requests behind the steps). */
const READ_EVERY_MS = 500;
/** Ticks of ledger history a negotiation thread (and the deals finder) covers (3-9b). */
const THREAD_WINDOW = 2000;
/** Most recent thread entries / deals listed. */
const THREAD_ROWS = 40;
const DEAL_ROWS = 15;

interface Rates {
  /** Simulated ticks per wall-clock second over the last ~1 s of play (both worlds advance together). */
  simTps: number;
  /** Rendered frames per second over the same window. */
  fps: number;
  /** Worker time per tick for the last step request of world A. */
  stepMsPerTick: number | null;
  /** Transfers / harms / signals per tick in world A over the same window (ledger deltas). */
  xferPerTick: number | null;
  harmPerTick: number | null;
  sigPerTick: number | null;
}

interface SeekInfo {
  tick: number;
  /** Worker time to restore the view of world A. */
  ms: number;
  /** Ticks re-executed from the nearest snapshot (0 when the seek extended the head). */
  reran: number;
}

/** The comparison world B: a fork of A at `forkedAt` with `intervention` applied there. */
interface Branch {
  client: WorldClient;
  forkedAt: number;
  intervention: Intervention;
  /** The snapshot B was built from, kept so the fork can be repeated for verification. */
  origin: Uint8Array;
  history: History;
}

const NO_RATES: Rates = { simTps: 0, fps: 0, stepMsPerTick: null, xferPerTick: null, harmPerTick: null, sigPerTick: null };

export default function App() {
  const client = useRef<WorldClient | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const canvasB = useRef<HTMLCanvasElement>(null);
  const spark = useRef<HTMLCanvasElement>(null);
  const curves = useRef<HTMLCanvasElement>(null);
  const normCurves = useRef<HTMLCanvasElement>(null);
  /** When the institution cards last re-read the ledger (play loop throttle). */
  const lastRead = useRef(0);
  const trailA = useRef(new MarkerTrail());
  const trailB = useRef(new MarkerTrail());
  const history = useRef(new History());
  const branch = useRef<Branch | null>(null);
  const busy = useRef(false);
  const tickSamples = useRef<{ t: number; tick: number; xfer: number; harm: number; sig: number }[]>([]);
  const frameTimes = useRef<number[]>([]);
  const seekWanted = useRef<number | null>(null);
  const seeking = useRef(false);
  const [frame, setFrame] = useState<Frame | null>(null);
  const [frameB, setFrameB] = useState<Frame | null>(null);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(4);
  const [selected, setSelected] = useState<number | null>(null);
  /** The other agent of the thread read with the selected one (3-9b). */
  const [partner, setPartner] = useState<number | null>(null);
  const [partnerInput, setPartnerInput] = useState("");
  /** Deals the kernel-side reader found in the window ending at `at` (3-9b). */
  const [deals, setDeals] = useState<{ at: number; from: number; list: Deal[] } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rates, setRates] = useState<Rates>(NO_RATES);
  const [lastSeek, setLastSeek] = useState<SeekInfo | null>(null);
  const [events, setEvents] = useState<InterventionEvent[]>([]);
  const [goTo, setGoTo] = useState("");
  const [forkMs, setForkMs] = useState<number | null>(null);
  const [verification, setVerification] = useState<Verification | null>(null);
  const [rewound, setRewound] = useState(false);
  /** What the kernel-side reader last said about A's institution (ADR-0014 D12): decrees, violations, sanctions. */
  const [readings, setReadings] = useState<Readings | null>(null);

  const show = useCallback((f: Frame) => {
    history.current.set(f.tick, sampleOf(f));
    setFrame(f);
  }, []);

  // The institution cards: three one-shot requests to A's worker, answered
  // behind whatever step is in flight. Read as of the view's tick, so after
  // a seek they say what the ledger said then. Skipped in a world without
  // institutions, where all three would be empty.
  const readInstitution = useCallback(async (f: Frame) => {
    const c = client.current;
    if (!c || !f.institution.nomos) return;
    lastRead.current = performance.now();
    try {
      const from = Math.max(0, f.tick - NORM_WINDOW);
      const [t, v, s] = await Promise.all([c.timeline(0, f.tick), c.violations(from, f.tick), c.sanctions(from, f.tick)]);
      setReadings({ at: t.frame.tick, timeline: t.timeline, violations: v.violations, sanctions: s.sanctions });
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  const showB = useCallback((f: Frame) => {
    branch.current?.history.set(f.tick, sampleOf(f));
    setFrameB(f);
  }, []);

  const discardBranch = useCallback(() => {
    branch.current?.client.terminate();
    branch.current = null;
    trailB.current.reset();
    setFrameB(null);
    setForkMs(null);
    setVerification(null);
  }, []);

  // One world worker (A) for the lifetime of the page; B comes and goes.
  useEffect(() => {
    const c = USE_MOCK ? WorldClient.mock() : WorldClient.kernel();
    client.current = c;
    c.init(DEFAULT_WORLD)
      .then((f) => {
        show(f);
        void readInstitution(f);
        return c.history();
      })
      .then((h) => setEvents(h.history))
      .catch((e: Error) => setError(e.message));
    return () => {
      c.terminate();
      discardBranch();
    };
  }, [show, discardBranch, readInstitution]);

  // The selected agent is "watched": every frame from A then carries its
  // kernel-written memory, last signal and ledger history — and, once a
  // partner is picked, the thread between the two (3-9b). While paused,
  // ask for a frame so the inspector updates at once.
  useEffect(() => {
    const c = client.current;
    if (!c) return;
    c.watch(
      selected === null
        ? null
        : { id: selected, window: WATCH_WINDOW, partner: partner ?? undefined, threadWindow: THREAD_WINDOW },
    );
    if (!playing && frame && selected !== null) c.frame().then(show).catch((e: Error) => setError(e.message));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selected, partner]);

  // Play loop: ask both worlds for `speed` ticks per animation frame, never
  // overlapping, and measure what actually happened over the last second.
  useEffect(() => {
    if (!playing) return;
    let cancelled = false;
    tickSamples.current = [];
    frameTimes.current = [];
    const tickOnce = async () => {
      if (cancelled || !client.current) return;
      if (busy.current) {
        // A step of the previous loop (speed just changed) is still in
        // flight: try again next frame instead of giving up the loop.
        requestAnimationFrame(tickOnce);
        return;
      }
      busy.current = true;
      try {
        const b = branch.current;
        const [f, fb] = await Promise.all([client.current.step(speed), b ? b.client.step(speed) : null]);
        const now = performance.now();
        const ts = tickSamples.current;
        const ft = frameTimes.current;
        ts.push({ t: now, tick: f.tick, xfer: f.ledger.transfers, harm: f.ledger.harms, sig: f.ledger.signals });
        ft.push(now);
        while (ts.length > 1 && now - ts[0].t > 1000) ts.shift();
        while (ft.length > 1 && now - ft[0] > 1000) ft.shift();
        const first = ts[0];
        const last = ts[ts.length - 1];
        const dt = (last.t - first.t) / 1000;
        const dticks = last.tick - first.tick;
        const fdt = (ft[ft.length - 1] - ft[0]) / 1000;
        const perTick = (a: number, b: number) => (dticks > 0 ? (b - a) / dticks : null);
        setRates({
          simTps: dt > 0 ? dticks / dt : 0,
          fps: fdt > 0 ? (ft.length - 1) / fdt : 0,
          stepMsPerTick:
            f.timing?.stepMs !== undefined && f.timing.stepTicks ? f.timing.stepMs / f.timing.stepTicks : null,
          xferPerTick: perTick(first.xfer, last.xfer),
          harmPerTick: perTick(first.harm, last.harm),
          sigPerTick: perTick(first.sig, last.sig),
        });
        show(f);
        if (fb) showB(fb);
        if (f.institution.nomos && now - lastRead.current > READ_EVERY_MS) void readInstitution(f);
      } catch (e) {
        setError((e as Error).message);
        setPlaying(false);
      } finally {
        busy.current = false;
      }
      if (!cancelled) requestAnimationFrame(tickOnce);
    };
    requestAnimationFrame(tickOnce);
    return () => {
      cancelled = true;
    };
  }, [playing, speed, show, showB, readInstitution]);

  // Paused: the cards catch up with the last frame (the loop's throttle may have skipped it).
  useEffect(() => {
    if (!playing && frame) void readInstitution(frame);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [playing]);

  useEffect(() => {
    if (frame && canvas.current) drawFrame(canvas.current, frame, selected, trailA.current);
    if (frameB && canvasB.current) drawFrame(canvasB.current, frameB, selected, trailB.current);
    if (frame && spark.current) drawSparkline(spark.current, history.current, frame.frontier, branch.current?.history);
    if (frame && curves.current) drawCurves(curves.current, history.current, frame.frontier, branch.current?.history);
    if (frame && normCurves.current && frame.institution.nomos) {
      drawCurves(normCurves.current, history.current, frame.frontier, branch.current?.history, NORM_CURVES);
    }
  }, [frame, frameB, selected]);

  // Seeks are coalesced: while one is in flight, only the newest request
  // survives, so dragging the timeline never queues a backlog of restores.
  // B follows A but cannot go before its fork tick.
  const seek = useCallback(
    (tick: number) => {
      seekWanted.current = tick;
      if (seeking.current) return;
      seeking.current = true;
      void (async () => {
        try {
          while (seekWanted.current !== null && client.current) {
            const target = seekWanted.current;
            seekWanted.current = null;
            const b = branch.current;
            const t0 = performance.now();
            const [f, fb] = await Promise.all([
              client.current.seek(target),
              b ? b.client.seek(Math.max(target, b.forkedAt)) : null,
            ]);
            const ms = f.timing?.seekMs ?? performance.now() - t0;
            setLastSeek({ tick: f.tick, ms, reran: f.tick === f.frontier ? 0 : f.tick % f.snapshotEvery });
            if (f.tick < f.frontier) setRewound(true);
            show(f);
            if (fb) showB(fb);
            if (seekWanted.current === null) void readInstitution(f);
          }
        } catch (e) {
          setError((e as Error).message);
        } finally {
          seeking.current = false;
        }
      })();
    },
    [show, showB, readInstitution],
  );

  const onScrub = (tick: number) => {
    setPlaying(false);
    seek(tick);
  };

  const intervene = async (intervention: Intervention) => {
    if (!client.current) return;
    setPlaying(false);
    try {
      const { frame: f } = await client.current.intervene(intervention);
      show(f);
      // The history shown is the ledger's, not what the UI remembers.
      const h = await client.current.history();
      setEvents(h.history);
      void readInstitution(f);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  // Fork: A exports the state its current tick starts with; a second worker
  // is built from it and the intervention is applied there (ADR-0012).
  const forkHere = async (intervention: Intervention) => {
    if (!client.current) return;
    setPlaying(false);
    try {
      const t0 = performance.now();
      const { frame: fa, snapshot } = await client.current.snapshot();
      const origin = snapshot.slice(); // the worker takes ownership of `snapshot`
      const c = USE_MOCK ? WorldClient.mock() : WorldClient.kernel();
      await c.init({ ...DEFAULT_WORLD, snapshot });
      const { frame: fb } = await c.intervene(intervention);
      discardBranch();
      branch.current = { client: c, forkedAt: fa.tick, intervention, origin, history: new History() };
      setForkMs(performance.now() - t0);
      show(fa);
      showB(fb);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  // Reproducibility check: a fresh fork from the same snapshot with the same
  // intervention, driven to the same tick, must reach the same hash.
  const verifyFork = async () => {
    const b = branch.current;
    if (!b || !client.current) return;
    setPlaying(false);
    setVerification({ status: "running" });
    const c = USE_MOCK ? WorldClient.mock() : WorldClient.kernel();
    try {
      const t0 = performance.now();
      // Compare at least one tick after the fork, where both views are
      // unambiguously "after the intervention".
      const at = Math.max(frameB?.tick ?? b.forkedAt, b.forkedAt + 1);
      const [fa, fb] = await Promise.all([client.current.seek(at), b.client.seek(at)]);
      show(fa);
      showB(fb);
      await c.init({ ...DEFAULT_WORLD, snapshot: b.origin.slice() });
      await c.intervene(b.intervention);
      const fc = await c.seek(at);
      setVerification({
        status: "done",
        hashB: fb.hash,
        hashC: fc.hash,
        tick: at,
        match: fc.hash === fb.hash && fc.tick === fb.tick,
        ms: performance.now() - t0,
      });
    } catch (e) {
      setError((e as Error).message);
      setVerification(null);
    } finally {
      c.terminate();
    }
  };

  const onCanvasClick = (which: "a" | "b") => (ev: React.MouseEvent<HTMLCanvasElement>) => {
    const f = which === "a" ? frame : frameB;
    if (!f) return;
    const r = ev.currentTarget.getBoundingClientRect();
    const cx = ((ev.clientX - r.left) / r.width) * f.width;
    const cy = ((ev.clientY - r.top) / r.height) * f.height;
    setSelected(pickAgent(f, cx, cy));
    setPartner(null);
  };

  // Read the thread between two agents: select one, watch it with the other.
  const readPair = (a: number, b: number) => {
    setSelected(a);
    setPartner(b);
  };

  // Every deal of the last THREAD_WINDOW ticks, judged by the kernel-side
  // reader as of the view's tick (a one-shot request, not part of frames).
  const findDeals = async () => {
    const c = client.current;
    if (!c || !frame) return;
    try {
      const to = frame.tick;
      const from = Math.max(0, to - THREAD_WINDOW);
      const r = await c.deals(from, to);
      setDeals({ at: to, from, list: r.deals });
    } catch (e) {
      setError((e as Error).message);
    }
  };

  // A different founding population is a different world: reload with it.
  const switchPolicy = (p: PolicyMix) => {
    const url = new URL(location.href);
    url.searchParams.set("policy", p);
    location.assign(url.toString());
  };
  // So are institutions on or off (`WorldConfig::nomos_preset()` vs the default config), and recording violations.
  const switchNomos = (on: boolean, record = RECORD_VIOLATIONS) => {
    const url = new URL(location.href);
    url.searchParams.set("nomos", on ? "1" : "0");
    if (on && record) url.searchParams.set("violations", "1");
    else url.searchParams.delete("violations");
    location.assign(url.toString());
  };

  const agent = frame && selected !== null ? findAgent(frame, selected) : null;
  const watched = frame?.watched && frame.watched.id === selected ? frame.watched : null;
  const detail = watched?.agent ?? null;
  const thread = watched?.thread && partner !== null && watched.thread.b === partner ? watched.thread : null;
  const partners = watched ? partnersOf(watched) : [];
  const atHead = !!frame && frame.tick === frame.frontier;
  const L = frame?.ledger;
  const per = (v: number | null) => (v === null ? "–" : v.toFixed(2));
  const inst = frame?.institution ?? null;
  const normValuesA = frame && inst?.nomos ? valuesAt(history.current, NORM_CURVES, frame.tick) : null;
  const normValuesB = frameB && inst?.nomos && branch.current ? valuesAt(branch.current.history, NORM_CURVES, frameB.tick) : null;

  return (
    <>
      <header>
        <h1>Civilization Zero · Toy 1</h1>
        {frame?.source === "kernel" ? (
          <span className="badge kernel">kernel · wasm</span>
        ) : (
          <span className="badge mock">{USE_MOCK ? "mock worker — not the kernel" : "loading kernel…"}</span>
        )}
        <label className="muted pick" title="founding population; same rule as `cz run --policy`. Changing it reloads with a fresh world">
          world
          <select value={POLICY} onChange={(e) => switchPolicy(e.target.value as PolicyMix)} aria-label="policy mix">
            {POLICY_MIXES.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </label>
        <label
          className="muted pick"
          title="institutions (ADR-0014): on, the world is built with WorldConfig::nomos_preset() — EXP-003's decision, as `cz run --preset nomos` — so decrees are accepted and encroachments, violations and sanctions are counted. Changing it reloads with a fresh world"
        >
          <input type="checkbox" checked={NOMOS} onChange={(e) => switchNomos(e.target.checked)} aria-label="institutions" />
          institutions
        </label>
        {NOMOS && (
          <label
            className="muted pick"
            title="record every violation as a ledger event (`cz run --record-violations`; ≈ 21 per tick), so the Violations card names each with its grounds instead of listing the episodes open now. Reloads"
          >
            <input
              type="checkbox"
              checked={RECORD_VIOLATIONS}
              onChange={(e) => switchNomos(true, e.target.checked)}
              aria-label="record violations"
            />
            record violations
          </label>
        )}
        {inst?.nomos && (
          <span
            className="badge inst"
            title="the institution at the view's tick (Frame.institution): rules in force and episodes of accusation open"
          >
            institution · {inst.rules.length} {inst.rules.length === 1 ? "rule" : "rules"} · {inst.accused.episodes} accused
          </span>
        )}
        <span className="muted">
          {frame ? `${frame.width}×${frame.height} · seed ${DEFAULT_WORLD.seed} · frontier ${frame.frontier}` : "starting…"}
          {frameB ? ` · B forked @${branch.current?.forkedAt}` : ""}
        </span>
        <span className="muted rates" title="measured over the last second of play">
          {playing ? `${rates.simTps.toFixed(0)} tick/s · ${rates.fps.toFixed(0)} fps` : "paused"}
          {rates.stepMsPerTick !== null ? ` · worker ${rates.stepMsPerTick.toFixed(2)} ms/tick` : ""}
          {frameB && playing ? " · ×2 worlds" : ""}
        </span>
      </header>
      <main>
        <section className={`stage${frameB ? " split" : ""}`}>
          <div className="worlds">
            <figure>
              <canvas ref={canvas} onClick={onCanvasClick("a")} style={{ aspectRatio: "1 / 1" }} />
              <figcaption>
                A · main{frame ? ` · tick ${frame.tick} · alive ${frame.alive}` : ""}
              </figcaption>
            </figure>
            {frameB && (
              <figure>
                <canvas ref={canvasB} onClick={onCanvasClick("b")} style={{ aspectRatio: "1 / 1" }} />
                <figcaption>
                  B · fork @{branch.current?.forkedAt} · tick {frameB.tick} · alive {frameB.alive}
                </figcaption>
              </figure>
            )}
          </div>
          <div className="controls">
            <button onClick={() => setPlaying((p) => !p)} disabled={!frame}>
              {playing ? "Pause" : "Play"}
            </button>
            <select value={speed} onChange={(e) => setSpeed(Number(e.target.value))}>
              {SPEEDS.map((s) => (
                <option key={s} value={s}>
                  {s} tick/frame
                </option>
              ))}
            </select>
            <input
              type="range"
              min={0}
              max={Math.max(frame?.frontier ?? 0, 1)}
              value={frame?.tick ?? 0}
              onChange={(e) => onScrub(Number(e.target.value))}
              disabled={!frame}
              aria-label="timeline"
            />
            <span className="tick">tick {frame?.tick ?? "–"}</span>
            <input
              type="number"
              min={0}
              className="goto"
              value={goTo}
              onChange={(e) => setGoTo(e.target.value)}
              placeholder="tick"
              aria-label="go to tick"
              title="seek to any tick; beyond the frontier the world is simulated up to it"
            />
            <button onClick={() => onScrub(Math.max(0, Math.floor(Number(goTo))))} disabled={!frame || goTo === ""}>
              Go
            </button>
          </div>
          <p className="legend">
            <i className="dot forager" /> forager <i className="dot random" /> random walker <i className="dot reflex" />{" "}
            reflex (evolving) <i className="dot learner" /> learner (Q) · green = food · opacity = energy · drag the
            timeline to rewind (restored from the nearest snapshot, then re-executed and checked against the ledger)
            {frameB ? " · both worlds follow the timeline; B cannot go before its fork tick" : ""}
          </p>
          <p className="legend">
            interactions of the tick just executed, from the ledger: <i className="mark xfer" /> gave food (line to the
            receiver) <i className="mark harm" /> harmed (cross at the victim) <i className="mark sig" /> said a symbol
            (ring) · markers fade over 0.6 s · foragers never interact — pick the <b>mixed</b> world above or spawn
            random walkers to see them
          </p>
          {inst?.nomos && (
            <p className="legend">
              institutions (ADR-0014), from the kernel's flags and ledger: <i className="mark accused" /> stands accused
              (an episode is open) <i className="mark harmable" /> accused, and a harm now would count as a sanction{" "}
              <i className="mark harm sanction" /> a harm the kernel recorded as a sanction (over the red cross; a ring
              with a bar for a rejection that counted)
              {inst.rules.length === 0 ? " · no rule is in force yet — Enact one in Intervene" : ""}
            </p>
          )}
        </section>
        <aside>
          <Tutorial frame={frame} frameB={frameB} events={events} verification={verification} rewound={rewound} />
          <div className="card">
            <h2>World A</h2>
            <dl>
              <dt>tick</dt>
              <dd>{frame?.tick ?? "–"}</dd>
              <dt>alive</dt>
              <dd>{frame?.alive ?? "–"}</dd>
              <dt title="living agents by policy kind, from the frame's policy column">policies</dt>
              <dd>{frame ? describePolicies(policyCounts(frame)) : "–"}</dd>
              <dt>food on map</dt>
              <dd>{frame?.foodTotal ?? "–"}</dd>
              <dt>frontier</dt>
              <dd>{frame?.frontier ?? "–"}</dd>
              <dt>state hash</dt>
              <dd>
                <code className="hash">{frame?.hash ?? "–"}</code>
              </dd>
              <dt>sim rate</dt>
              <dd>{playing ? `${rates.simTps.toFixed(0)} tick/s` : "paused"}</dd>
              <dt>render</dt>
              <dd>{playing ? `${rates.fps.toFixed(0)} fps` : "–"}</dd>
              <dt>worker step</dt>
              <dd>{rates.stepMsPerTick !== null ? `${rates.stepMsPerTick.toFixed(2)} ms/tick` : "–"}</dd>
              <dt>last seek</dt>
              <dd>{lastSeek ? `${lastSeek.ms.toFixed(1)} ms (re-ran ${lastSeek.reran})` : "–"}</dd>
              <dt>snapshots</dt>
              <dd>{frame ? `every ${frame.snapshotEvery}` : "–"}</dd>
            </dl>
            <h2 className="sub" title="running totals from the kernel's ledger (part of the state, so they rewind with it)">
              Ledger · interactions
            </h2>
            <dl>
              <dt>transfers</dt>
              <dd>{L ? `${L.transfers} (${L.food_transferred} food)` : "–"}</dd>
              <dt>harms</dt>
              <dd>{L ? `${L.harms} (${L.deaths_by_cause[2]} died of violence)` : "–"}</dd>
              <dt>signals</dt>
              <dd>{L ? L.signals : "–"}</dd>
              <dt title="xfer · harm · sig per tick over the last second of play">per tick (1 s)</dt>
              <dd>{playing ? `${per(rates.xferPerTick)} · ${per(rates.harmPerTick)} · ${per(rates.sigPerTick)}` : "–"}</dd>
            </dl>
          </div>
          <div className="card">
            <h2>Population and food (ticks seen)</h2>
            <canvas ref={spark} className="spark" />
          </div>
          <div className="card">
            <h2 title="ADR-0013 D8: the curves of the fixed-vs-learning comparison">Compare · A solid, B dashed</h2>
            <canvas ref={curves} className="curves" />
            <p className="muted hint">
              Population, intake per agent per tick, and transfers / harms / signals per tick over the ticks seen, each
              row on its own scale. Rates are ledger deltas over the trailing {RATE_WINDOW} ticks, so they read the same
              at any speed. Fork B (<b>Compare: fixed vs learning</b> in Branches, or <b>Fork here</b>) to overlay it
              from its fork tick.
            </p>
          </div>
          {frame && (
            <BranchesPanel
              a={frame}
              b={frameB}
              forkedAt={branch.current?.forkedAt ?? null}
              intervention={branch.current?.intervention ?? null}
              forkMs={forkMs}
              verification={verification}
              onVerify={() => void verifyFork()}
              onDiscard={discardBranch}
              onCompare={(kind) => void forkHere(compareIntervention(kind))}
              onCompareRule={(kind) => void forkHere(compareRuleIntervention(frame.institution, kind))}
            />
          )}
          {frame && (
            <InterventionPanel
              atHead={atHead}
              tick={frame.tick}
              width={frame.width}
              height={frame.height}
              selectedAgent={selected}
              institution={frame.institution}
              events={events}
              onApply={(iv) => void intervene(iv)}
              onFork={(iv) => void forkHere(iv)}
            />
          )}
          {frame && (
            <InstitutionCard frame={frame} readings={readings} onSelect={setSelected} onEnable={() => switchNomos(true)} />
          )}
          {frame && inst?.nomos && normValuesA && (
            <>
              <NormRatesCard canvasRef={normCurves} valuesA={normValuesA} valuesB={normValuesB} />
              <ViolationsCard frame={frame} readings={readings} onSelect={setSelected} />
              <SanctionsCard frame={frame} readings={readings} onSelect={setSelected} />
            </>
          )}
          <div className="card">
            <h2>Deals · ledger</h2>
            <p className="muted hint">
              Every accepted proposal of the last {THREAD_WINDOW} ticks, judged by the kernel-side reader from the
              transfers that followed it (ADR-0013 D5): <b>kept</b>, <b>open</b> or <b>broken</b>. The page adds nothing.
            </p>
            <div className="row">
              <button onClick={() => void findDeals()} disabled={!frame} title="asks the worker; foragers never negotiate">
                Find deals
              </button>
              {deals && (
                <span className="muted">
                  as of tick {deals.at}: {deals.list.length} deals · {countDeals(deals.list)}
                </span>
              )}
            </div>
            {deals &&
              (deals.list.length === 0 ? (
                <p className="muted hint">none in ticks {deals.from}–{Math.max(deals.from, deals.at - 1)} — pick the mixed or random world</p>
              ) : (
                <>
                  <p className="muted hint">kept deals with food on both sides first, then newest first; pick one to read its thread</p>
                  <ol className="history deals">
                    {rankDeals(deals.list)
                      .slice(0, DEAL_ROWS)
                      .map((d, i) => (
                        <li key={`${d.accepted_tick}-${d.proposer}-${d.acceptor}-${i}`}>
                          <span className="when">tick {d.accepted_tick}</span>
                          <span className="who">
                            #{d.proposer} → #{d.acceptor}
                          </span>
                          <span className="what">
                            {d.give} for {d.want} by tick {d.deadline}
                          </span>
                          <Pill status={d.status} />
                          <button className="link" onClick={() => readPair(d.proposer, d.acceptor)}>
                            read
                          </button>
                        </li>
                      ))}
                  </ol>
                </>
              ))}
          </div>
          <div className="card">
            <h2>Agent (click a map)</h2>
            {agent ? (
              <dl>
                <dt>id</dt>
                <dd>{agent.id}</dd>
                <dt>pos</dt>
                <dd>
                  {agent.x}, {agent.y}
                </dd>
                <dt>energy</dt>
                <dd>{agent.energy}</dd>
                <dt>inventory</dt>
                <dd>{agent.inventory}</dd>
                <dt>health</dt>
                <dd>{agent.health}</dd>
                <dt>age</dt>
                <dd>{agent.age}</dd>
                <dt>policy</dt>
                <dd>{POLICY_KINDS[agent.policy]?.toLowerCase() ?? `kind ${agent.policy}`}</dd>
                {detail && detail.arrived !== null && (
                  <>
                    <dt title="first occupancy (ADR-0014 D3): the tick the agent began standing on its cell; whoever arrived earlier is the first occupant, and gathering there is an encroachment on them">
                      arrived
                    </dt>
                    <dd>tick {detail.arrived}</dd>
                  </>
                )}
                {detail && inst?.nomos && (
                  <>
                    <dt title="episodes of accusation open against the agent at this tick (the kernel's standing, 4-7)">standing</dt>
                    <dd className="qrow">
                      {detail.standing.length === 0 ? "not accused" : detail.standing.map((a) => describeStanding(a)).join(" · ")}
                    </dd>
                    <dt title="whether the agent has voted on the open motion (ADR-0014 D7: one living agent, one vote)">voted</dt>
                    <dd>{inst.motion ? (detail.voted ? "yes" : "not yet") : "no motion open"}</dd>
                  </>
                )}
                <dt>last signal</dt>
                <dd>
                  {detail
                    ? detail.last_signal
                      ? `${describeMessage(detail.last_signal.message)} @ tick ${detail.last_signal.tick}`
                      : "none"
                    : "…"}
                </dd>
                {detail?.last_step && (
                  <>
                    <dt title="the kernel's record of the learner's previous decision: Q row, action kind, and that tick's reward in units of 100 milli-energy (3-7)">
                      last step
                    </dt>
                    <dd>
                      row {detail.last_step.state} · {ACTION_KINDS[detail.last_step.action] ?? detail.last_step.action} ·
                      reward {detail.last_step.reward}
                    </dd>
                  </>
                )}
                {detail && describeTable(detail) && (
                  <>
                    <dt title="the learner's Q row it last decided in (fixed point, 16 = one reward unit; the greedy pick is the max), or a reflex's weight count">
                      table
                    </dt>
                    <dd className="qrow">{describeTable(detail)}</dd>
                  </>
                )}
              </dl>
            ) : (
              <p className="muted">
                {selected === null ? "nothing selected" : `agent ${selected} is not alive in A at tick ${frame?.tick}`}
              </p>
            )}
            {watched && frame && (
              <>
                <h2 className="sub">Memory · recent ({detail?.recent.length ?? 0})</h2>
                <p className="muted hint">
                  What agent {watched.id} itself remembers at tick {frame.tick}: the kernel writes at most 8
                  interactions (oldest dropped first). Newest first.
                </p>
                {detail && detail.recent.length > 0 ? (
                  <ol className="history memory">
                    {detail.recent
                      .slice()
                      .reverse()
                      .map((r, i) => (
                        <li key={`${r.tick}-${i}`}>
                          <span className="when">tick {r.tick}</span>
                          <span className="what">{describeInteraction(r)}</span>
                        </li>
                      ))}
                  </ol>
                ) : (
                  <p className="muted hint">
                    {detail ? "nothing yet — foragers only interact when a random walker gives to, harms or speaks near them" : "no memory: not alive here"}
                  </p>
                )}
                <h2 className="sub">
                  Ledger · ticks {Math.max(0, frame.tick - watched.window)}–{Math.max(0, frame.tick - 1)} ({watched.events.length})
                </h2>
                <p className="muted hint">
                  Every ledger event naming agent {watched.id} in the last {watched.window} ticks: complete, and it
                  outlives the agent. Newest first{watched.events.length > 12 ? ", first 12" : ""}.
                </p>
                {watched.events.length === 0 ? (
                  <p className="muted hint">no events in this window</p>
                ) : (
                  <ol className="history">
                    {watched.events
                      .slice()
                      .reverse()
                      .slice(0, 12)
                      .map((e, i) => (
                        <li key={`${e.tick}-${i}`}>
                          <span className="when">tick {e.tick}</span>
                          <span className="what">{describeEvent(e, watched.id)}</span>
                        </li>
                      ))}
                  </ol>
                )}
                <h2 className="sub">Thread · with {partner !== null ? `#${partner}` : "…"}</h2>
                <p className="muted hint">
                  The ledger between agent {watched.id} and one other agent, oldest first (last {THREAD_WINDOW} ticks). Pick
                  a partner: those in the memory and ledger above, negotiation partners first, or any id.
                </p>
                <div className="partners">
                  {partners.map((id) => (
                    <button key={id} className={id === partner ? "on" : ""} onClick={() => setPartner(id)}>
                      #{id}
                    </button>
                  ))}
                  <input
                    type="number"
                    min={0}
                    className="goto"
                    value={partnerInput}
                    onChange={(e) => setPartnerInput(e.target.value)}
                    placeholder="id"
                    aria-label="partner id"
                  />
                  <button
                    onClick={() => setPartner(Math.max(0, Math.floor(Number(partnerInput))))}
                    disabled={partnerInput === ""}
                  >
                    read
                  </button>
                </div>
                {thread && (
                  <>
                    <p className="muted hint">
                      ticks {thread.from}–{Math.max(thread.from, thread.now - 1)}: {thread.entries.length} events,{" "}
                      {thread.deals.length} deals. Each acceptance is judged by the kernel-side reader from the transfers in
                      its window (offer tick to deadline): kept once both sides delivered, broken once the deadline passed
                      with a side short, open until then.
                    </p>
                    {thread.entries.length === 0 ? (
                      <p className="muted hint">
                        nothing between #{thread.a} and #{thread.b} in this window
                      </p>
                    ) : (
                      <ol className="history thread">
                        {thread.entries.length > THREAD_ROWS && (
                          <li className="muted">… {thread.entries.length - THREAD_ROWS} earlier</li>
                        )}
                        {thread.entries.slice(-THREAD_ROWS).map((e, i) => (
                          <li key={`${e.tick}-${i}`}>
                            <span className="when">tick {e.tick}</span>
                            <span className="who">{whoOf(e)}</span>
                            <span className="what">{describeThreadEntry(e)}</span>
                          </li>
                        ))}
                      </ol>
                    )}
                    {thread.deals.length > 0 && (
                      <ol className="history deals">
                        {thread.deals.map((d, i) => (
                          <li key={`${d.accepted_tick}-${i}`}>
                            <span className="when">deal {i + 1}</span>
                            <span className="what">{describeDeal(d)}</span>
                            <Pill status={d.status} />
                          </li>
                        ))}
                      </ol>
                    )}
                  </>
                )}
              </>
            )}
          </div>
          {error && (
            <div className="card error" onClick={() => setError(null)} title="click to dismiss">
              {error}
            </div>
          )}
        </aside>
      </main>
      <footer className="muted">
        canvas {frame ? `${frame.width * SCALE}×${frame.height * SCALE}` : ""} · {AGENT_FIELDS} fields per agent · worker protocol
        v{PROTOCOL_VERSION} (web/src/protocol.ts) · one worker per world · built from commit {__COMMIT__} · static files
        only, nothing leaves this tab
      </footer>
    </>
  );
}

/** What a frame contributes to the curves: population, food, and the ledger totals the rates are cut from. */
function sampleOf(f: Frame): Sample {
  return {
    alive: f.alive,
    food: f.foodTotal,
    consumed: f.ledger.food_consumed,
    transfers: f.ledger.transfers,
    harms: f.ledger.harms,
    signals: f.ledger.signals,
    gathers: f.ledger.gathers,
    encroachments: f.ledger.encroachments,
    violations: sumKinds(f.ledger.violations),
    sanctions: sumKinds(f.ledger.sanctions),
    episodesClosed: sumKinds(f.ledger.episodes_closed),
    episodesSanctioned: sumKinds(f.ledger.episodes_sanctioned),
    accused: f.institution.accused.episodes,
  };
}

/**
 * A learning agent's table in one line: the Q row a learner last decided in
 * (one value per action kind, the greedy pick is the max), or how many
 * weights a reflex carries. Fixed policies have none.
 */
function describeTable(d: AgentDetail): string | null {
  const t = d.params.table;
  if (!t) return null;
  if ("reflex" in t) return `${t.reflex.weights.length} reflex weights (16 features × 8 kinds, 1/1024 units)`;
  const rows = t.learner.q.length / ACTION_KINDS.length;
  if (!d.last_step) return `Q table: ${rows} rows × ${ACTION_KINDS.length} kinds, no step recorded yet`;
  const row = t.learner.q.slice(d.last_step.state * ACTION_KINDS.length, (d.last_step.state + 1) * ACTION_KINDS.length);
  const best = Math.max(...row);
  return `Q row ${d.last_step.state} of ${rows}: ${row.map((q, i) => `${ACTION_KINDS[i]} ${q}${q === best ? "*" : ""}`).join(" · ")}`;
}

function findAgent(frame: Frame, id: number) {
  const A = frame.agents;
  for (let k = 0; k < A.length; k += AGENT_FIELDS) {
    if (A[k] === id) {
      return { id, x: A[k + 1], y: A[k + 2], energy: A[k + 3], inventory: A[k + 4], policy: A[k + 5], health: A[k + 6], age: A[k + 7] };
    }
  }
  return null;
}

/** `Terms::pack()` of a proposal as it travels through a memory value (3-4). */
function describeTerms(packed: number): string {
  return `give ${packed & 0xff}, want ${(packed >> 8) & 0xff} back within ${(packed >> 16) & 0xffff} ticks`;
}

function describeMessage(m: Message): string {
  if ("say" in m) return `say ${m.say}`;
  if ("propose" in m) return `propose to #${m.propose.to}: give ${m.propose.give}, want ${m.propose.want} by +${m.propose.by}`;
  if ("accept" in m) return `accept #${m.accept.to}'s offer of tick ${m.accept.offer_tick}`;
  if ("reject" in m) return `reject #${m.reject.to}`;
  if ("motion" in m) return `move: ${describeMotionKind(m.motion)}`;
  return `vote ${m.vote.aye ? "aye" : "nay"}`;
}

/** One entry of the agent's own memory, in its own voice. */
function describeInteraction(r: Interaction): string {
  switch (r.kind) {
    case "gave_to":
      return `gave ${r.value} food to #${r.other}`;
    case "received_from":
      return `received ${r.value} food from #${r.other}`;
    case "harmed":
      return `harmed #${r.other} (−${r.value} health)`;
    case "harmed_by":
      return `harmed by #${r.other} (−${r.value} health)`;
    case "saw_harm":
      return `saw #${r.other} harm #${r.value}`;
    case "heard":
      return `heard #${r.other} say ${r.value}`;
    case "proposal_from":
      return `#${r.other} proposed: ${describeTerms(r.value)}`;
    case "accepted":
      return `#${r.other} accepted: ${describeTerms(r.value)}`;
    case "rejected":
      return `#${r.other} rejected my proposal`;
    case "promised":
      return `promised ${r.value} food to #${r.other}`;
    case "encroached_by":
      return `#${r.other} gathered ${r.value} on my cell after I arrived first`;
    case "sanctioned":
      return r.value > 0 ? `sanctioned #${r.other} (−${r.value} health)` : `sanctioned #${r.other} (rejected)`;
    case "sanctioned_by":
      return r.value > 0 ? `sanctioned by #${r.other} (−${r.value} health)` : `sanctioned by #${r.other} (rejected)`;
    case "saw_sanction":
      return `saw #${r.other} sanction #${r.value}`;
    default:
      return `${(r as Interaction).kind} · #${r.other} · ${r.value}`;
  }
}

/** A ledger record as it concerns agent `id`. */
function describeEvent(e: LedgerEvent, id: number): string {
  switch (e.kind) {
    case "transferred":
      return e.from === id ? `gave ${e.amount} food to #${e.to}` : `received ${e.amount} food from #${e.from}`;
    case "harmed":
      return e.by === id ? `harmed #${e.target} (−${e.damage} health)` : `harmed by #${e.by} (−${e.damage} health)`;
    case "signaled":
      return `said: ${describeMessage(e.message)}`;
    case "born":
      return e.child === id ? `born to #${e.parent} at (${e.pos.x}, ${e.pos.y})` : `bore #${e.child}`;
    case "died":
      return `died of ${e.cause.replace("_", " ")} at age ${e.age}`;
    case "proposed":
      return e.from === id
        ? `proposed to #${e.to}: give ${e.give}, want ${e.want} back within ${e.by} ticks`
        : `#${e.from} proposed: give ${e.give}, want ${e.want} back within ${e.by} ticks`;
    case "accepted":
      return e.from === id ? `accepted #${e.to}'s proposal of tick ${e.offer_tick}` : `#${e.from} accepted the proposal of tick ${e.offer_tick}`;
    case "rejected":
      return e.from === id ? `rejected #${e.to}'s proposal` : `#${e.from} rejected the proposal`;
    case "intervened":
      return "removed by an intervention";
    case "acted":
      return `acted: ${JSON.stringify(e.action)} → ${JSON.stringify(e.outcome)}`;
    case "sanctioned":
      return e.by === id
        ? `sanctioned #${e.target} (${e.via}, rule #${e.rule})`
        : `sanctioned by #${e.by} (${e.via}, rule #${e.rule})`;
    case "violated":
      return e.agent === id
        ? `violated rule #${e.rule} (${e.norm.replace("_", " ")}) against #${e.against}`
        : `#${e.agent} violated rule #${e.rule} (${e.norm.replace("_", " ")}) against me`;
    case "motioned":
      return `moved motion #${e.motion}: ${describeMotionKind(e.asks)}`;
    case "voted":
      return `voted ${e.aye ? "aye" : "nay"} on motion #${e.motion}`;
    case "motion_closed":
      return `motion #${e.motion} ${e.carried ? "carried" : "defeated"} (ayes ${e.ayes} · nays ${e.nays} · ${e.alive} alive)`;
    case "enacted":
      return `rule #${e.rule} enacted by motion #${e.motion}`;
    case "repealed":
      return `rule #${e.rule} repealed by motion #${e.motion}`;
    case "chartered":
      return `charter replaced by motion #${e.motion}`;
  }
}

/** `kept` / `open` / `broken` as a small label. */
function Pill({ status }: { status: Deal["status"] }) {
  const cls = status === "fulfilled" ? "ok" : status === "broken" ? "bad" : "open";
  return <span className={`pill ${cls}`}>{status === "fulfilled" ? "kept" : status}</span>;
}

function countDeals(list: Deal[]): string {
  const n = (s: Deal["status"]) => list.filter((d) => d.status === s).length;
  return `kept ${n("fulfilled")} · open ${n("open")} · broken ${n("broken")}`;
}

/** Kept deals with food on both sides first (the threads worth reading), then kept, open, broken; newest first within each. */
function rankDeals(list: Deal[]): Deal[] {
  const rank = (d: Deal) =>
    d.status === "fulfilled" ? (d.give > 0 && d.want > 0 ? 0 : 1) : d.status === "open" ? 2 : 3;
  return list.slice().sort((p, q) => rank(p) - rank(q) || q.accepted_tick - p.accepted_tick);
}

/**
 * Who the watched agent could read a thread with: every other agent its
 * memory or ledger window names, negotiation partners first, then by the
 * most recent contact.
 */
function partnersOf(w: Watched): number[] {
  const seen = new Map<number, { tick: number; nego: boolean }>();
  const note = (id: number, tick: number, nego: boolean) => {
    if (id === w.id) return;
    const cur = seen.get(id);
    seen.set(id, { tick: Math.max(cur?.tick ?? -1, tick), nego: (cur?.nego ?? false) || nego });
  };
  for (const r of w.agent?.recent ?? []) {
    note(r.other, r.tick, r.kind === "proposal_from" || r.kind === "accepted" || r.kind === "rejected" || r.kind === "promised");
  }
  for (const e of w.events) {
    switch (e.kind) {
      case "proposed":
      case "accepted":
      case "rejected":
        note(e.from === w.id ? e.to : e.from, e.tick, true);
        break;
      case "transferred":
        note(e.from === w.id ? e.to : e.from, e.tick, false);
        break;
      case "harmed":
        note(e.by === w.id ? e.target : e.by, e.tick, false);
        break;
      default:
        break;
    }
  }
  return [...seen.entries()]
    .sort((p, q) => Number(q[1].nego) - Number(p[1].nego) || q[1].tick - p[1].tick)
    .map(([id]) => id)
    .slice(0, 8);
}

function whoOf(e: ThreadEntry): string {
  return e.kind === "harmed" ? `#${e.by} → #${e.target}` : `#${e.from} → #${e.to}`;
}

/** One thread entry in words; the judgement words come from the reader's fields, not from the page. */
function describeThreadEntry(e: ThreadEntry): string {
  const dealNo = (i: number | null) => (i === null ? "" : ` · deal ${i + 1}`);
  switch (e.kind) {
    case "proposed": {
      const answer =
        e.answer === "pending" ? "no answer yet" : e.answer === "lapsed" ? "unanswered, deadline passed" : e.answer;
      return `proposed: give ${e.give} now, want ${e.want} back by tick ${e.deadline} · ${answer}${dealNo(e.deal)}`;
    }
    case "accepted":
      return `accepted the offer of tick ${e.offer_tick}: #${e.to} gives ${e.give} now, #${e.from} returns ${e.want} by tick ${e.deadline}${dealNo(e.deal)}`;
    case "rejected":
      return e.offer_tick === null ? "rejected (no earlier offer in this window)" : `rejected the offer of tick ${e.offer_tick}`;
    case "transferred":
      return e.role === null
        ? `gave ${e.amount} food (no promise called for it)`
        : `gave ${e.amount} food · ${e.role === "give" ? "the give" : "repayment"}${dealNo(e.deal)}`;
    case "harmed":
      return `harmed (−${e.damage} health)`;
  }
}

function describeDeal(d: Deal): string {
  const who = d.broken_by.map((id) => `#${id}`).join(" and ");
  const outcome =
    d.status === "fulfilled"
      ? `kept at tick ${d.settled_tick}`
      : d.status === "broken"
        ? `broken by ${who}: deadline tick ${d.deadline} passed`
        : `open until tick ${d.deadline}`;
  return `offer tick ${d.offer_tick}, accepted tick ${d.accepted_tick}: #${d.proposer} gives ${d.give} (given ${d.given}), #${d.acceptor} returns ${d.want} (repaid ${d.repaid}) · ${outcome}`;
}
