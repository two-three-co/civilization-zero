import type { Frame, Intervention } from "./protocol";

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
}

/** Side-by-side numbers for the main world (A) and its fork (B). */
export function BranchesPanel({ a, b, forkedAt, intervention, forkMs, verification, onVerify, onDiscard }: Props) {
  if (!b || forkedAt === null) {
    return (
      <div className="card placeholder">
        <h2>Branches</h2>
        <p className="muted">
          No fork yet. Rewind to any tick, set an intervention above and press <b>Fork here</b>: a second world
          starts from that tick with the intervention, and both run in lockstep from then on.
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
        checked here in the browser rather than claimed.
      </p>
    </div>
  );
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
  }
}
