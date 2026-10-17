import { useEffect, useState } from "react";
import { defaultNorm, describeDecree, normName } from "./nomos";
import {
  NORM_KINDS,
  POLICY_KINDS,
  SANCTION_HARM,
  SANCTION_REJECT,
  type Institution,
  type Intervention,
  type InterventionEvent,
  type NormKind,
  type ParamsMode,
  type PolicyKind,
} from "./protocol";

interface Props {
  /** Interventions are applied at the head only; behind it they would be a fork (2b-6). */
  atHead: boolean;
  tick: number;
  width: number;
  height: number;
  selectedAgent: number | null;
  /** The institution of the view (ADR-0014 D12): whether decrees are accepted, the rules a repeal can name, the charter the controls start from. */
  institution: Institution;
  /** The ledger's record of every intervention so far, oldest first. */
  events: InterventionEvent[];
  onApply: (intervention: Intervention) => void;
  /** Fork the current view (any tick) into a second world with this intervention applied there. */
  onFork: (intervention: Intervention) => void;
}

/** Every op the kernel's `Intervention` has, the decrees (ADR-0014 D8) included. */
type Op = Intervention["op"];

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
  {
    op: "enact",
    label: "Enact",
    hint: "put a norm in force as a rule (ADR-0014 D5 / D8): which fact is a violation, how many ticks a violator stands accused after each one, which answers to an accused agent count as sanctions. A rule of the same kind is replaced and its episodes close. Fork here with it = with vs without rule",
  },
  { op: "repeal", label: "Repeal", hint: "take a rule out of force by id; its open episodes close (counted, nobody sanctioned)" },
  {
    op: "charter",
    label: "Charter",
    hint: "replace the assembly's charter: quorum and threshold in permille, ticks a motion stays open, whether motions may amend. Read by motions from 4-5; decrees are never bound by it",
  },
];

type WhoChoice = "all" | "fraction" | "id";

export function InterventionPanel({ atHead, tick, width, height, selectedAgent, institution, events, onApply, onFork }: Props) {
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
  // Enact: the norm's four parameters (D5), starting from the CLI's `--enact` defaults.
  const [normKind, setNormKind] = useState<NormKind>("encroach");
  const [standing, setStanding] = useState(defaultNorm("encroach").standing_ticks);
  const [sanctionHarm, setSanctionHarm] = useState(true);
  const [sanctionReject, setSanctionReject] = useState(false);
  const [exceptSanction, setExceptSanction] = useState(false);
  // Repeal: one of the rules in force.
  const [repealId, setRepealId] = useState<number | null>(null);
  // Charter: the four fields, starting from the charter in force.
  const [quorum, setQuorum] = useState(institution.charter.quorum_permille);
  const [threshold, setThreshold] = useState(institution.charter.threshold_permille);
  const [voteTicks, setVoteTicks] = useState(institution.charter.vote_ticks);
  const [amendable, setAmendable] = useState(institution.charter.amendable);

  useEffect(() => {
    if (selectedAgent !== null) setId(selectedAgent);
  }, [selectedAgent]);

  // The repeal menu follows the rules in force: keep the pick while it is one of them, else the first.
  const rules = institution.rules;
  useEffect(() => {
    if (repealId !== null && rules.some((r) => r.id === repealId)) return;
    setRepealId(rules.length > 0 ? rules[0].id : null);
  }, [rules, repealId]);

  // The charter controls start from what is in force whenever that op is picked.
  useEffect(() => {
    if (op !== "charter") return;
    setQuorum(institution.charter.quorum_permille);
    setThreshold(institution.charter.threshold_permille);
    setVoteTicks(institution.charter.vote_ticks);
    setAmendable(institution.charter.amendable);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [op]);

  const decree = op === "enact" || op === "repeal" || op === "charter";
  /** A decree the kernel would reject: no institutions, or no rule to repeal. */
  const blocked = decree && (!institution.nomos || (op === "repeal" && repealId === null));

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
      case "enact":
        return {
          op,
          norm: {
            kind: normKind,
            standing_ticks: Math.max(0, Math.min(65_535, Math.floor(standing))),
            sanction: (sanctionHarm ? SANCTION_HARM : 0) | (sanctionReject ? SANCTION_REJECT : 0),
            except_sanction: normKind === "harm" && exceptSanction,
          },
        };
      case "repeal":
        return { op, rule: repealId ?? 0 };
      case "charter":
        return {
          op,
          charter: {
            quorum_permille: Math.max(0, Math.min(1000, Math.floor(quorum))),
            threshold_permille: Math.max(0, Math.min(1000, Math.floor(threshold))),
            vote_ticks: Math.max(0, Math.min(65_535, Math.floor(voteTicks))),
            amendable,
          },
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
          disabled={!atHead || blocked}
          title={
            blocked
              ? "the kernel would reject this decree (see below)"
              : atHead
                ? "apply to this world's head"
                : "the view is behind the frontier: fork instead"
          }
        >
          Apply
        </button>
        <button
          onClick={() => onFork(build())}
          disabled={blocked}
          title="start a second world from this tick with the intervention applied"
        >
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
        {op === "enact" && (
          <>
            <label>
              norm
              <select value={normKind} onChange={(e) => setNormKind(e.target.value as NormKind)} aria-label="norm kind">
                {NORM_KINDS.map((k) => (
                  <option key={k} value={k}>
                    {normName(k)}
                  </option>
                ))}
              </select>
            </label>
            <Num label="standing ticks" value={standing} min={0} max={65_535} onChange={setStanding} />
            <Check label="sanction: harm" checked={sanctionHarm} onChange={setSanctionHarm} />
            <Check label="sanction: rejection" checked={sanctionReject} onChange={setSanctionReject} />
            {normKind === "harm" && (
              <Check label="a sanctioning harm is no violation" checked={exceptSanction} onChange={setExceptSanction} />
            )}
          </>
        )}
        {op === "repeal" && rules.length > 0 && (
          <label>
            rule
            <select value={repealId ?? rules[0].id} onChange={(e) => setRepealId(Number(e.target.value))} aria-label="rule to repeal">
              {rules.map((r) => (
                <option key={r.id} value={r.id}>
                  #{r.id} {normName(r.norm.kind)} (since tick {r.enacted})
                </option>
              ))}
            </select>
          </label>
        )}
        {op === "charter" && (
          <>
            <Num label="quorum ‰ of the living" value={quorum} min={0} max={1000} onChange={setQuorum} />
            <Num label="threshold ‰ of the votes" value={threshold} min={0} max={1000} onChange={setThreshold} />
            <Num label="ticks per motion" value={voteTicks} min={0} max={65_535} onChange={setVoteTicks} />
            <Check label="amendable by motion" checked={amendable} onChange={setAmendable} />
          </>
        )}
      </div>
      {decree && !institution.nomos && (
        <p className="muted hint">
          This world has no institutions, so the kernel rejects every decree. Tick <b>institutions</b> in the header
          to rebuild it with them on (<code>WorldConfig::nomos_preset()</code>), then enact here.
        </p>
      )}
      {op === "repeal" && institution.nomos && rules.length === 0 && <p className="muted hint">no rule is in force to repeal</p>}
      {op === "enact" && institution.nomos && (
        <p className="muted hint">
          {standing === 0
            ? "with 0 standing ticks violations are counted but nobody is ever accused or sanctioned"
            : !sanctionHarm && !sanctionReject
              ? "with no sanction allowed violators stand accused (the map outlines them) but no answer counts as a sanction"
              : `a violator stands accused for ${standing} ticks after each violation; a ${[sanctionHarm && "harm", sanctionReject && "rejection"].filter(Boolean).join(" or ")} reaching it then is a sanction`}
          {rules.some((r) => r.norm.kind === normKind) ? ` · replaces rule #${rules.find((r) => r.norm.kind === normKind)!.id}` : ""}
        </p>
      )}
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
      return `set policy → ${iv.kind.toLowerCase()} for ${describeWho(iv.who)} (${iv.params} params)`;
    case "enact":
    case "repeal":
    case "charter":
      return describeDecree(iv);
  }
}

function describeWho(who: Extract<Intervention, { op: "set_policy" }>["who"]): string {
  if (who === "all") return "everyone";
  if ("id" in who) return `agent ${who.id}`;
  return `each agent with probability ${who.fraction.permille}‰`;
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

function Check({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} aria-label={label} />
      {label}
    </label>
  );
}
