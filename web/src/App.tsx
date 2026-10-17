import { useCallback, useEffect, useRef, useState } from "react";
import { WorldClient } from "./client";
import { AGENT_FIELDS, type Frame, type WorldInit } from "./protocol";
import { drawFrame, drawSparkline, pickAgent, SCALE } from "./render";

const DEFAULT_WORLD: WorldInit = { seed: 42, width: 128, height: 128, agents: 1000 };
const SPEEDS = [1, 4, 15, 30];

export default function App() {
  const client = useRef<WorldClient | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const spark = useRef<HTMLCanvasElement>(null);
  const history = useRef(new Map<number, number>());
  const busy = useRef(false);
  const [frame, setFrame] = useState<Frame | null>(null);
  const [playing, setPlaying] = useState(false);
  const [speed, setSpeed] = useState(4);
  const [selected, setSelected] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  const show = useCallback((f: Frame) => {
    history.current.set(f.tick, f.alive);
    setFrame(f);
  }, []);

  // One world worker for the lifetime of the page.
  useEffect(() => {
    const c = WorldClient.mock();
    client.current = c;
    c.init(DEFAULT_WORLD).then(show, (e: Error) => setError(e.message));
    return () => c.terminate();
  }, [show]);

  // Play loop: ask for `speed` ticks per animation frame, never overlapping.
  useEffect(() => {
    if (!playing) return;
    let cancelled = false;
    const tickOnce = async () => {
      if (cancelled || !client.current || busy.current) return;
      busy.current = true;
      try {
        show(await client.current.step(speed));
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
  }, [playing, speed, show]);

  useEffect(() => {
    if (frame && canvas.current) drawFrame(canvas.current, frame, selected);
    if (frame && spark.current) drawSparkline(spark.current, history.current, frame.frontier);
  }, [frame, selected]);

  const seek = async (tick: number) => {
    if (!client.current) return;
    setPlaying(false);
    try {
      show(await client.current.seek(tick));
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const onCanvasClick = (ev: React.MouseEvent<HTMLCanvasElement>) => {
    if (!frame) return;
    const r = ev.currentTarget.getBoundingClientRect();
    const cx = ((ev.clientX - r.left) / r.width) * frame.width;
    const cy = ((ev.clientY - r.top) / r.height) * frame.height;
    setSelected(pickAgent(frame, cx, cy));
  };

  const agent = frame && selected !== null ? findAgent(frame, selected) : null;

  return (
    <>
      <header>
        <h1>Civilization Zero · Toy 1</h1>
        <span className="badge mock">mock worker — not the kernel</span>
        <span className="muted">
          {frame ? `${frame.width}×${frame.height} · seed ${DEFAULT_WORLD.seed} · frontier ${frame.frontier}` : "starting…"}
        </span>
      </header>
      <main>
        <section className="stage">
          <canvas ref={canvas} onClick={onCanvasClick} style={{ aspectRatio: "1 / 1" }} />
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
              onChange={(e) => void seek(Number(e.target.value))}
              disabled={!frame}
              aria-label="timeline"
            />
            <span className="tick">tick {frame?.tick ?? "–"}</span>
          </div>
          <p className="legend">
            <i className="dot forager" /> forager <i className="dot random" /> random walker · green = food · opacity =
            energy · drag the timeline to rewind (seek re-simulates from the nearest snapshot)
          </p>
        </section>
        <aside>
          <div className="card">
            <h2>World</h2>
            <dl>
              <dt>tick</dt>
              <dd>{frame?.tick ?? "–"}</dd>
              <dt>alive</dt>
              <dd>{frame?.alive ?? "–"}</dd>
              <dt>food on map</dt>
              <dd>{frame?.foodTotal ?? "–"}</dd>
              <dt>frontier</dt>
              <dd>{frame?.frontier ?? "–"}</dd>
            </dl>
          </div>
          <div className="card">
            <h2>Population (ticks seen)</h2>
            <canvas ref={spark} className="spark" />
          </div>
          <div className="card">
            <h2>Agent (click the map)</h2>
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
              <p className="muted">{selected === null ? "nothing selected" : `agent ${selected} is not alive at this tick`}</p>
            )}
          </div>
          <div className="card placeholder">
            <h2>Intervene</h2>
            <p className="muted">Drought / food scale / spawn / remove — arrives with 2b-5 (kernel API exists: ADR-0010 2a-1).</p>
          </div>
          <div className="card placeholder">
            <h2>Branches</h2>
            <p className="muted">Fork at the current tick and compare side by side — arrives with 2b-6.</p>
          </div>
          {error && <div className="card error">{error}</div>}
        </aside>
      </main>
      <footer className="muted">
        canvas {frame ? `${frame.width * SCALE}×${frame.height * SCALE}` : ""} · {AGENT_FIELDS} fields per agent · worker protocol
        v0 (web/src/protocol.ts)
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
