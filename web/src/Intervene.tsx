import { useEffect, useState } from "react";
import { describeSetPolicy } from "./policies";
import { POLICY_KINDS, type Intervention, type InterventionEvent, type ParamsMode, type PolicyKind } from "./protocol";

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
  /** Fork the current view (any tick) into a second world with this intervention applied there. */
  onFork: (intervention: Intervention) => void;
}

/** The ops this panel offers; the decrees (`enact` / `repeal` / `charter`, ADR-0014 D8) get their own controls with 4-8. */
type Op = Exclude<Intervention["op"], "enact" | "repeal" | "charter">;

const OPS: { op: Op; label: string; hint: string }[] = [
  { op: "drought", label: "Drought", hint: "replace regrowth: amount per interval (0 = nothing grows back)" },
  { op: "scale_food", label: "Scale food", hint: "multiply every cell's food by permille / 1000 (500 = halve, 2000 = double)" },
  { op: "spawn_agents", label: "Spawn agents", hint: "add n agents of one policy on one cell (fresh params, empty memory)" },
  { op: "remove_agent", label: "Remove agent", hint: "remove one living agent by id (click the map to pick one)" },
  {
    op: "set_policy",
    label: "Set policy",
    hint: "give living agents another policy — everyone, each with a probability (a seed-keyed draw), or one id. Bodies, positions and memories stay; default params start a blank table, keep lets a learner keep its Q. Fork here with it = fixed vs learning (ADR-0013 D7)",
  },
];

type WhoChoice = "all" | "fraction" | "id";

export function InterventionPanel({ atHead, tick, width, height, selectedAgent, events, onApply, onFork }: Props) {
  const [op, setOp] = useState<Op>("drought");
  const [amount, setAmount] = useState(0);
  const [interval, setInterval] = useState(10);
  const [permille, setPermille] = useState(500);
  const [n, setN] = useState(50);
  const [x, setX] = useState(Math.floor(width / 2));
  const [y, setY] = useState(Math.floor(height / 2));
  const [policy, setPolicy] = useState<PolicyKind>("Forager");
  const [id, setId] = useState(0);
  const [who, setWho] = useState<WhoChoice>("all");
  const [share, setShare] = useState(500);
  const [kind, setKind] = useState<PolicyKind>("Learner");
  const [params, setParams] = useState<ParamsMode>("default");

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
      case "set_policy":
        return {
          op,
          who: who === "all" ? "all" : who === "id" ? { id } : { fraction: { permille: Math.max(0, Math.min(1000, share)) } },
          kind,
          params,
        };
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
        <button
          onClick={() => onApply(build())}
          disabled={!atHead}
          title={atHead ? "apply to this world's head" : "the view is behind the frontier: fork instead"}
        >
          Apply
        </button>
        <button onClick={() => onFork(build())} title="start a second world from this tick with the intervention applied">
          Fork here
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
              <select value={policy} onChange={(e) => setPolicy(e.target.value as PolicyKind)}>
                {POLICY_KINDS.map((k) => (
                  <option key={k}>{k}</option>
                ))}
              </select>
            </label>
          </>
        )}
        {op === "remove_agent" && <Num label="agent id" value={id} min={0} onChange={setId} />}
        {op === "set_policy" && (
          <>
            <label>
              who
              <select value={who} onChange={(e) => setWho(e.target.value as WhoChoice)} aria-label="who">
                <option value="all">all</option>
                <option value="fraction">fraction</option>
                <option value="id">one agent</option>
              </select>
            </label>
            <label>
              kind
              <select value={kind} onChange={(e) => setKind(e.target.value as PolicyKind)} aria-label="kind">
                {POLICY_KINDS.map((k) => (
                  <option key={k}>{k}</option>
                ))}
              </select>
            </label>
            {who === "fraction" && <Num label="permille" value={share} min={0} max={1000} onChange={setShare} />}
            {who === "id" && <Num label="agent id" value={id} min={0} onChange={setId} />}
            <label>
              params
              <select value={params} onChange={(e) => setParams(e.target.value as ParamsMode)} aria-label="params">
                <option value="default">default</option>
                <option value="keep">keep</option>
              </select>
            </label>
          </>
        )}
      </div>
      {!atHead && (
        <p className="muted hint">
          The view is behind the frontier, so Apply is off (it changes the head only). Fork here instead: the
          second world starts from this tick with the intervention, and the two run side by side.
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
    case "set_policy":
      return describeSetPolicy(iv);
    case "enact":
      return `enact ${iv.norm.kind.replace("_", " ")} (accused ${iv.norm.standing_ticks} ticks, sanction ${iv.norm.sanction})`;
    case "repeal":
      return `repeal rule #${iv.rule}`;
    case "charter":
      return `charter: quorum ${iv.charter.quorum_permille}‰, threshold ${iv.charter.threshold_permille}‰, ${iv.charter.vote_ticks} ticks, ${iv.charter.amendable ? "amendable" : "not amendable"}`;
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
