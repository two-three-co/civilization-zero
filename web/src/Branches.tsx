import { useState } from "react";
import { describeCompareRule, describeDecree, normName, sumKinds } from "./nomos";
import { describePolicies, describeSetPolicy, policyCounts } from "./policies";
import { NORM_KINDS, POLICY_KINDS, type Frame, type Intervention, type NormKind, type PolicyKind } from "./protocol";

export interface Verification {
  status: "running" | "done";
  /** Hash of the fork at the compared tick, and of a fresh re-fork driven to the same tick. */
  hashB?: string;
  hashC?: string;
  tick?: number;
  match?: boolean;
  ms?: number;
}

interface Props {
  a: Frame;
  b: Frame | null;
  forkedAt: number | null;
  intervention: Intervention | null;
  forkMs: number | null;
  verification: Verification | null;
  onVerify: () => void;
  onDiscard: () => void;
  /** "Compare: fixed vs learning" (ADR-0013 D8 / 3-9c): fork B at A's current tick with everyone in it switched to `kind`. */
  onCompare: (kind: PolicyKind) => void;
  /** "Compare: with vs without rule" (ADR-0014 D8 / 4-8): fork B at A's current tick and enact the default norm of `kind` there — or repeal A's rule of that kind. */
  onCompareRule: (kind: NormKind) => void;
}

/**
 * The one-press comparison: a fork of A at its current tick plus
 * `set_policy { all, kind, default }` applied in B. The world decides what
 * differs from then on; the page only lines the two up.
 */
function CompareControls({ tick, replacing, onCompare }: { tick: number; replacing: boolean; onCompare: (kind: PolicyKind) => void }) {
  const [kind, setKind] = useState<PolicyKind>("Learner");
  return (
    <>
      <div className="row compare-row">
        <button
          onClick={() => onCompare(kind)}
          title="fork a second world at this tick and switch every agent in it to the chosen policy (set_policy: all, default params)"
        >
          Compare: fixed vs learning
        </button>
        <select value={kind} onChange={(e) => setKind(e.target.value as PolicyKind)} aria-label="policy for B" title="what everyone in B becomes">
          {POLICY_KINDS.map((k) => (
            <option key={k}>{k}</option>
          ))}
        </select>
      </div>
      <p className="muted hint">
        {replacing ? "Replaces B: " : ""}a second world forks from A at tick {tick} and every agent in it is switched to{" "}
        {kind.toLowerCase()} — bodies, positions and memories stay, only the policy and its blank table change. A learner
        starts with an all-zero Q table and learns within its life; a reflex starts forager-equivalent and only evolves
        where mutation is on (reflex / mixed-learning worlds).
      </p>
    </>
  );
}

/**
 * The one-press institution comparison (ADR-0014 D8 / D12 (e)): a fork of A
 * at its current tick plus one decree in B — `enact` of the chosen kind's
 * default norm when A has no such rule, `repeal` of A's rule when it has.
 * The two worlds then differ by that rule alone; the Norms · rates card
 * draws both.
 */
function CompareRuleControls({ a, replacing, onCompareRule }: { a: Frame; replacing: boolean; onCompareRule: (kind: NormKind) => void }) {
  const [kind, setKind] = useState<NormKind>("encroach");
  const inst = a.institution;
  return (
    <>
      <div className="row compare-row">
        <button
          onClick={() => onCompareRule(kind)}
          disabled={!inst.nomos}
          title={
            inst.nomos
              ? "fork a second world at this tick that differs from this one by one rule (enacted there, or repealed there)"
              : "this world has no institutions: tick institutions in the header first"
          }
        >
          Compare: with vs without rule
        </button>
        <select value={kind} onChange={(e) => setKind(e.target.value as NormKind)} aria-label="norm kind for B" title="which kind of rule the two worlds differ by">
          {NORM_KINDS.map((k) => (
            <option key={k} value={k}>
              {normName(k)}
            </option>
          ))}
        </select>
      </div>
      <p className="muted hint">
        {inst.nomos
          ? `${replacing ? "Replaces B: " : ""}a second world forks from A at tick ${a.tick} and ${describeCompareRule(inst, kind)}. Everything else — bodies, positions, memories, the other rules — is shared, so the Norms · rates card (A solid, B dashed) shows what that one rule does.`
          : "Needs a world with institutions (header: institutions, or ?nomos=1); the kernel rejects decrees elsewhere."}
      </p>
    </>
  );
}

/** Side-by-side numbers for the main world (A) and its fork (B). */
export function BranchesPanel({ a, b, forkedAt, intervention, forkMs, verification, onVerify, onDiscard, onCompare, onCompareRule }: Props) {
  if (!b || forkedAt === null) {
    return (
      <div className="card placeholder">
        <h2>Branches</h2>
        <CompareControls tick={a.tick} replacing={false} onCompare={onCompare} />
        <CompareRuleControls a={a} replacing={false} onCompareRule={onCompareRule} />
        <p className="muted">
          Or rewind to any tick, set an intervention below and press <b>Fork here</b>: a second world starts from that
          tick with the intervention, and both run in lockstep from then on.
        </p>
      </div>
    );
  }
  const delta = (x: number, y: number) => {
    const d = y - x;
    return `${d > 0 ? "+" : ""}${d}`;
  };
  const sameTick = a.tick === b.tick;
  return (
    <div className="card">
      <h2>Branches</h2>
      <p className="muted hint">
        B forked from A at tick {forkedAt} with {intervention ? describe(intervention) : "no intervention"}
        {forkMs !== null ? ` · fork took ${forkMs.toFixed(0)} ms` : ""}
        {!sameTick ? ` · B starts at ${b.origin}, so it shows tick ${b.tick} while A shows ${a.tick}` : ""}
      </p>
      <table className="compare">
        <thead>
          <tr>
            <th></th>
            <th>A · main</th>
            <th>B · fork</th>
            <th>B − A</th>
          </tr>
        </thead>
        <tbody>
          <tr>
            <td>tick</td>
            <td>{a.tick}</td>
            <td>{b.tick}</td>
            <td></td>
          </tr>
          <tr>
            <td>policies</td>
            <td>{describePolicies(policyCounts(a))}</td>
            <td>{describePolicies(policyCounts(b))}</td>
            <td></td>
          </tr>
          <tr>
            <td>alive</td>
            <td>{a.alive}</td>
            <td>{b.alive}</td>
            <td>{delta(a.alive, b.alive)}</td>
          </tr>
          <tr>
            <td>food on map</td>
            <td>{a.foodTotal}</td>
            <td>{b.foodTotal}</td>
            <td>{delta(a.foodTotal, b.foodTotal)}</td>
          </tr>
          <tr>
            <td>food consumed</td>
            <td>{a.ledger.food_consumed}</td>
            <td>{b.ledger.food_consumed}</td>
            <td>{delta(a.ledger.food_consumed, b.ledger.food_consumed)}</td>
          </tr>
          <tr>
            <td>transfers</td>
            <td>{a.ledger.transfers}</td>
            <td>{b.ledger.transfers}</td>
            <td>{delta(a.ledger.transfers, b.ledger.transfers)}</td>
          </tr>
          <tr>
            <td>harms</td>
            <td>{a.ledger.harms}</td>
            <td>{b.ledger.harms}</td>
            <td>{delta(a.ledger.harms, b.ledger.harms)}</td>
          </tr>
          <tr>
            <td>signals</td>
            <td>{a.ledger.signals}</td>
            <td>{b.ledger.signals}</td>
            <td>{delta(a.ledger.signals, b.ledger.signals)}</td>
          </tr>
          {(a.institution.nomos || b.institution.nomos) && (
            <>
              <tr title="rules in force at each world's tick (Frame.institution)">
                <td>rules in force</td>
                <td>{a.institution.rules.length}</td>
                <td>{b.institution.rules.length}</td>
                <td>{delta(a.institution.rules.length, b.institution.rules.length)}</td>
              </tr>
              <tr title="episodes of accusation open now">
                <td>accused</td>
                <td>{a.institution.accused.episodes}</td>
                <td>{b.institution.accused.episodes}</td>
                <td>{delta(a.institution.accused.episodes, b.institution.accused.episodes)}</td>
              </tr>
              <tr title="ledger totals since tick 0 (B's continue from the fork), all kinds">
                <td>encroachments</td>
                <td>{a.ledger.encroachments}</td>
                <td>{b.ledger.encroachments}</td>
                <td>{delta(a.ledger.encroachments, b.ledger.encroachments)}</td>
              </tr>
              <tr>
                <td>violations</td>
                <td>{sumKinds(a.ledger.violations)}</td>
                <td>{sumKinds(b.ledger.violations)}</td>
                <td>{delta(sumKinds(a.ledger.violations), sumKinds(b.ledger.violations))}</td>
              </tr>
              <tr>
                <td>sanctions</td>
                <td>{sumKinds(a.ledger.sanctions)}</td>
                <td>{sumKinds(b.ledger.sanctions)}</td>
                <td>{delta(sumKinds(a.ledger.sanctions), sumKinds(b.ledger.sanctions))}</td>
              </tr>
            </>
          )}
          <tr title="kernel time per tick of each world's last step request, measured in its worker">
            <td>worker ms/tick</td>
            <td>{msPerTick(a)}</td>
            <td>{msPerTick(b)}</td>
            <td></td>
          </tr>
          <tr>
            <td>state hash</td>
            <td>
              <code className="hash">{a.hash}</code>
            </td>
            <td>
              <code className="hash">{b.hash}</code>
            </td>
            <td>{a.hash === b.hash ? "same" : "differ"}</td>
          </tr>
        </tbody>
      </table>
      <div className="row">
        <button onClick={onVerify} disabled={verification?.status === "running"}>
          {verification?.status === "running" ? "Verifying…" : "Verify: fork again"}
        </button>
        <button onClick={onDiscard}>Discard B</button>
      </div>
      {verification?.status === "done" && (
        <p className={`hint ${verification.match ? "ok" : "bad"}`}>
          {verification.match ? "MATCH" : "MISMATCH"}: a fresh fork from the same snapshot with the same
          intervention, driven to tick {verification.tick}, reached <code className="hash">{verification.hashC}</code>
          {verification.match ? " — identical to B" : ` while B has ${verification.hashB}`} ({verification.ms?.toFixed(0)} ms).
        </p>
      )}
      <p className="muted hint">
        Same snapshot + same intervention ⇒ same history. That is the kernel's determinism contract (ADR-0012),
        checked here in the browser rather than claimed — for a learning world too, since the learner's Q table is
        integer arithmetic in the kernel.
      </p>
      <CompareControls tick={a.tick} replacing={true} onCompare={onCompare} />
      <CompareRuleControls a={a} replacing={true} onCompareRule={onCompareRule} />
    </div>
  );
}

/** Worker time per tick of the frame's last `step`, or a dash while nothing has stepped. */
function msPerTick(f: Frame): string {
  const t = f.timing;
  return t?.stepMs !== undefined && t.stepTicks ? (t.stepMs / t.stepTicks).toFixed(2) : "–";
}

function describe(iv: Intervention): string {
  switch (iv.op) {
    case "drought":
      return `drought (regrow ${iv.regrow_amount} every ${iv.regrow_interval})`;
    case "scale_food":
      return `food ×${(iv.permille / 1000).toFixed(2)}`;
    case "spawn_agents":
      return `spawn ${iv.n} ${iv.policy.toLowerCase()} at (${iv.pos.x}, ${iv.pos.y})`;
    case "remove_agent":
      return `remove agent ${iv.id}`;
    case "set_policy":
      return describeSetPolicy(iv);
    case "enact":
    case "repeal":
    case "charter":
      return describeDecree(iv);
  }
}
