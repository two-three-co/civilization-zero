import { useEffect, useState } from "react";
import type { Intervention, InterventionEvent } from "./protocol";

interface Props {
  /** Interventions are applied at the head only; behind it they would be a fork (2b-6). */
  atHead: boolean;
  tick: number;
  width: number;
  height: number;
  selectedAgent: number | null;
  /** The ledger's record of every intervention so far, oldest first. */
  events: InterventionEvent[];
  onApply: (intervention: Intervention) => void;
}

type Op = Intervention["op"];

const OPS: { op: Op; label: string; hint: string }[] = [
  { op: "drought", label: "Drought", hint: "replace regrowth: amount per interval (0 = nothing grows back)" },
  { op: "scale_food", label: "Scale food", hint: "multiply every cell's food by permille / 1000 (500 = halve, 2000 = double)" },
  { op: "spawn_agents", label: "Spawn agents", hint: "add n foragers or random walkers on one cell" },
  { op: "remove_agent", label: "Remove agent", hint: "remove one living agent by id (click the map to pick one)" },
];

export function InterventionPanel({ atHead, tick, width, height, selectedAgent, events, onApply }: Props) {
  const [op, setOp] = useState<Op>("drought");
  const [amount, setAmount] = useState(0);
  const [interval, setInterval] = useState(10);
  const [permille, setPermille] = useState(500);
  const [n, setN] = useState(50);
  const [x, setX] = useState(Math.floor(width / 2));
  const [y, setY] = useState(Math.floor(height / 2));
  const [policy, setPolicy] = useState<"Forager" | "Random">("Forager");
  const [id, setId] = useState(0);

  useEffect(() => {
    if (selectedAgent !== null) setId(selectedAgent);
  }, [selectedAgent]);

  const build = (): Intervention => {
    switch (op) {
      case "drought":
        return { op, regrow_amount: amount, regrow_interval: Math.max(1, interval) };
      case "scale_food":
        return { op, permille: Math.max(0, Math.min(10_000, permille)) };
      case "spawn_agents":
        return { op, n: Math.max(1, n), pos: { x: clamp(x, width), y: clamp(y, height) }, policy };
      case "remove_agent":
        return { op, id };
    }
  };

  const current = OPS.find((o) => o.op === op)!;

  return (
    <div className="card">
      <h2>Intervene at tick {tick}</h2>
      <div className="row">
        <select value={op} onChange={(e) => setOp(e.target.value as Op)} aria-label="intervention">
          {OPS.map((o) => (
            <option key={o.op} value={o.op}>
              {o.label}
            </option>
          ))}
        </select>
        <button onClick={() => onApply(build())} disabled={!atHead} title={atHead ? "" : "rewind in progress"}>
          Apply
        </button>
      </div>
      <p className="muted hint">{current.hint}</p>
      <div className="fields">
        {op === "drought" && (
          <>
            <Num label="regrow amount" value={amount} min={0} onChange={setAmount} />
            <Num label="regrow interval" value={interval} min={1} onChange={setInterval} />
          </>
        )}
        {op === "scale_food" && <Num label="permille" value={permille} min={0} max={10_000} onChange={setPermille} />}
        {op === "spawn_agents" && (
          <>
            <Num label="n" value={n} min={1} onChange={setN} />
            <Num label="x" value={x} min={0} max={width - 1} onChange={setX} />
            <Num label="y" value={y} min={0} max={height - 1} onChange={setY} />
            <label>
              policy
              <select value={policy} onChange={(e) => setPolicy(e.target.value as "Forager" | "Random")}>
                <option>Forager</option>
                <option>Random</option>
              </select>
            </label>
          </>
        )}
        {op === "remove_agent" && <Num label="agent id" value={id} min={0} onChange={setId} />}
      </div>
      {!atHead && (
        <p className="muted hint">
          The view is behind the frontier. Interventions change the head only; applying one here would be a fork
          (2b-6). Seek to the frontier to intervene.
        </p>
      )}
      <h2 className="sub">Ledger · interventions ({events.length})</h2>
      {events.length === 0 ? (
        <p className="muted hint">none yet — every applied intervention is read back from the kernel's ledger</p>
      ) : (
        <ol className="history">
          {events
            .slice()
            .reverse()
            .slice(0, 12)
            .map((e, i) => (
              <li key={`${e.tick}-${i}`}>
                <span className="when">tick {e.tick}</span> {describe(e.intervention)}
              </li>
            ))}
        </ol>
      )}
    </div>
  );
}

function clamp(v: number, size: number): number {
  return Math.max(0, Math.min(size - 1, Math.floor(v)));
}

function describe(iv: Intervention): string {
  switch (iv.op) {
    case "drought":
      return `drought: regrow ${iv.regrow_amount} every ${iv.regrow_interval}`;
    case "scale_food":
      return `scale food ×${(iv.permille / 1000).toFixed(2)}`;
    case "spawn_agents":
      return `spawn ${iv.n} ${iv.policy.toLowerCase()} at (${iv.pos.x}, ${iv.pos.y})`;
    case "remove_agent":
      return `remove agent ${iv.id}`;
  }
}

function Num({
  label,
  value,
  min,
  max,
  onChange,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  onChange: (v: number) => void;
}) {
  return (
    <label>
      {label}
      <input
        type="number"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.target.value))}
        aria-label={label}
      />
    </label>
  );
}
