import { useCallback, useEffect, useRef, useState } from "react";
import { BranchesPanel, type Verification } from "./Branches";
import { WorldClient } from "./client";
import { InterventionPanel } from "./Intervene";
import { AGENT_FIELDS, type Frame, type Intervention, type InterventionEvent, type WorldInit } from "./protocol";
import { drawFrame, drawSparkline, pickAgent, SCALE, type Sample } from "./render";

const DEFAULT_WORLD: WorldInit = { seed: 42, width: 128, height: 128, agents: 1000, snapshotEvery: 250 };
const SPEEDS = [1, 4, 15, 30];
// `?mock=1` keeps the stand-in world available for UI work without a wasm build.
const USE_MOCK = new URLSearchParams(location.search).get("mock") === "1";

interface Rates {
  /** Simulated ticks per wall-clock second over the last ~1 s of play (both worlds advance together). */
  simTps: number;
  /** Rendered frames per second over the same window. */
  fps: number;
  /** Worker time per tick for the last step request of world A. */
  stepMsPerTick: number | null;
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
  history: Map<number, Sample>;
}

export default function App() {
  const client = useRef<WorldClient | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const canvasB = useRef<HTMLCanvasElement>(null);
  const spark = useRef<HTMLCanvasElement>(null);
  const history = useRef(new Map<number, Sample>());
  const branch = useRef<Branch | null>(null);
  const busy = useRef(false);
  const tickSamples = useRef<{ t: number; tick: number }[]>([]);
  const frameTimes = useRef<number[]>([]);
  const seekWanted = useRef<number | null>(null);
  const seeking = useRef(false);
  const [frame, setFrame] = useState<Frame | null>(null);
  const [frameB, setFrameB] = useState<Frame | null>(null);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(4);
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [rates, setRates] = useState<Rates>({ simTps: 0, fps: 0, stepMsPerTick: null });
  const [lastSeek, setLastSeek] = useState<SeekInfo | null>(null);
  const [events, setEvents] = useState<InterventionEvent[]>([]);
  const [goTo, setGoTo] = useState("");
  const [forkMs, setForkMs] = useState<number | null>(null);
  const [verification, setVerification] = useState<Verification | null>(null);

  const show = useCallback((f: Frame) => {
    history.current.set(f.tick, { alive: f.alive, food: f.foodTotal });
    setFrame(f);
  }, []);

  const showB = useCallback((f: Frame) => {
    branch.current?.history.set(f.tick, { alive: f.alive, food: f.foodTotal });
    setFrameB(f);
  }, []);

  const discardBranch = useCallback(() => {
    branch.current?.client.terminate();
    branch.current = null;
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
        return c.history();
      })
      .then((h) => setEvents(h.history))
      .catch((e: Error) => setError(e.message));
    return () => {
      c.terminate();
      discardBranch();
    };
  }, [show, discardBranch]);

  // Play loop: ask both worlds for `speed` ticks per animation frame, never
  // overlapping, and measure what actually happened over the last second.
  useEffect(() => {
    if (!playing) return;
    let cancelled = false;
    tickSamples.current = [];
    frameTimes.current = [];
    const tickOnce = async () => {
      if (cancelled || !client.current || busy.current) return;
      busy.current = true;
      try {
        const b = branch.current;
        const [f, fb] = await Promise.all([client.current.step(speed), b ? b.client.step(speed) : null]);
        const now = performance.now();
        const ts = tickSamples.current;
        const ft = frameTimes.current;
        ts.push({ t: now, tick: f.tick });
        ft.push(now);
        while (ts.length > 1 && now - ts[0].t > 1000) ts.shift();
        while (ft.length > 1 && now - ft[0] > 1000) ft.shift();
        const dt = (ts[ts.length - 1].t - ts[0].t) / 1000;
        const fdt = (ft[ft.length - 1] - ft[0]) / 1000;
        setRates({
          simTps: dt > 0 ? (ts[ts.length - 1].tick - ts[0].tick) / dt : 0,
          fps: fdt > 0 ? (ft.length - 1) / fdt : 0,
          stepMsPerTick:
            f.timing?.stepMs !== undefined && f.timing.stepTicks ? f.timing.stepMs / f.timing.stepTicks : null,
        });
        show(f);
        if (fb) showB(fb);
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
  }, [playing, speed, show, showB]);

  useEffect(() => {
    if (frame && canvas.current) drawFrame(canvas.current, frame, selected);
    if (frameB && canvasB.current) drawFrame(canvasB.current, frameB, selected);
    if (frame && spark.current) drawSparkline(spark.current, history.current, frame.frontier, branch.current?.history);
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
            show(f);
            if (fb) showB(fb);
          }
        } catch (e) {
          setError((e as Error).message);
        } finally {
          seeking.current = false;
        }
      })();
    },
    [show, showB],
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
      branch.current = { client: c, forkedAt: fa.tick, intervention, origin, history: new Map() };
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
  };

  const agent = frame && selected !== null ? findAgent(frame, selected) : null;
  const atHead = !!frame && frame.tick === frame.frontier;

  return (
    <>
      <header>
        <h1>Civilization Zero · Toy 1</h1>
        {frame?.source === "kernel" ? (
          <span className="badge kernel">kernel · wasm</span>
        ) : (
          <span className="badge mock">{USE_MOCK ? "mock worker — not the kernel" : "loading kernel…"}</span>
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
            <i className="dot forager" /> forager <i className="dot random" /> random walker · green = food · opacity =
            energy · drag the timeline to rewind (restored from the nearest snapshot, then re-executed and checked
            against the ledger){frameB ? " · both worlds follow the timeline; B cannot go before its fork tick" : ""}
          </p>
        </section>
        <aside>
          <div className="card">
            <h2>World A</h2>
            <dl>
              <dt>tick</dt>
              <dd>{frame?.tick ?? "–"}</dd>
              <dt>alive</dt>
              <dd>{frame?.alive ?? "–"}</dd>
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
          </div>
          <div className="card">
            <h2>Population and food (ticks seen)</h2>
            <canvas ref={spark} className="spark" />
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
            />
          )}
          {frame && (
            <InterventionPanel
              atHead={atHead}
              tick={frame.tick}
              width={frame.width}
              height={frame.height}
              selectedAgent={selected}
              events={events}
              onApply={(iv) => void intervene(iv)}
              onFork={(iv) => void forkHere(iv)}
            />
          )}
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
                <dd>{agent.policy ? "random" : "forager"}</dd>
              </dl>
            ) : (
              <p className="muted">
                {selected === null ? "nothing selected" : `agent ${selected} is not alive in A at this tick`}
              </p>
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
        v2 (web/src/protocol.ts) · one worker per world
      </footer>
    </>
  );
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
