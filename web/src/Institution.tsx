// The institution explorer's cards (ADR-0014 D12, 4-8): what the kernel's
// state says at the view's tick (`Frame.institution`) and what the
// kernel-side reader (crates/cz-web/src/institution.rs) read from the ledger
// — decrees with the rule ids they received, violations with their
// grounds or the episodes open now, sanctions tied to the episode each
// answered — plus the rates cut from the ledger's counters. Every judgement
// shown here was made in Rust; the page orders the words (D12).

import type { RefObject } from "react";
import {
  describeCharter,
  describeMotion,
  describeRule,
  describeSanctionEntry,
  describeTimelineEntry,
  describeViolation,
  normName,
} from "./nomos";
import { NORM_KINDS, type Frame, type Sanctions, type Timeline, type Violations } from "./protocol";
import { NORM_CURVES, NORM_WINDOW } from "./render";

/** What the page last read from the kernel-side reader for world A (one-shot requests, refreshed while playing and after each seek or decree). */
export interface Readings {
  /** The view's tick the readings were taken at. */
  at: number;
  timeline: Timeline;
  violations: Violations;
  sanctions: Sanctions;
}

/** Most rows a list shows. */
const ROWS = 12;

/** An agent id that selects the agent on click. */
function Id({ id, onSelect }: { id: number; onSelect: (id: number) => void }) {
  return (
    <button className="link id" onClick={() => onSelect(id)} title={`select agent ${id}`}>
      #{id}
    </button>
  );
}

interface CardProps {
  frame: Frame;
  readings: Readings | null;
  onSelect: (id: number) => void;
}

/** The ledger card for a world without institutions: how to get them. */
function NoInstitutions({ onEnable }: { onEnable: () => void }) {
  return (
    <div className="card placeholder">
      <h2>Institution · ledger</h2>
      <p className="muted">
        This world has no institutions: the kernel rejects every decree and counts no violation. Tick{" "}
        <button className="link" onClick={onEnable}>
          institutions
        </button>{" "}
        in the header (or open with <code>?nomos=1</code>) to rebuild it with <code>WorldConfig::nomos_preset()</code>{" "}
        — EXP-003's decision: ε 10‰, harm cost 2,000 — after which Intervene offers Enact / Repeal / Charter and the
        cards below fill from the ledger.
      </p>
    </div>
  );
}

/** The charter, the rules in force and who stands accused (state), then the decrees read back from the ledger (ADR-0014 D12 (a)). */
export function InstitutionCard({ frame, readings, onEnable }: CardProps & { onEnable: () => void }) {
  const inst = frame.institution;
  if (!inst.nomos) return <NoInstitutions onEnable={onEnable} />;
  const entries = readings?.timeline.entries ?? [];
  const k = inst.accused.by_kind;
  return (
    <div className="card">
      <h2 title="ADR-0014 D12: the charter and the rules in force are the state at this tick; the decrees are read back from the ledger with the rule ids the kernel assigned (recovered by replaying them from the branch's origin)">
        Institution · ledger
      </h2>
      <dl>
        <dt>rules in force</dt>
        <dd>{inst.rules.length}</dd>
        <dt title="episodes of accusation open at this tick (the kernel's standing), and how many distinct agents stand accused">accused now</dt>
        <dd>
          {inst.accused.episodes} episodes · {inst.accused.agents} agents
        </dd>
        <dt>by kind</dt>
        <dd>{NORM_KINDS.map((kind, i) => `${normName(kind)} ${k[i]}`).join(" · ")}</dd>
        <dt title="how the assembly decides (ADR-0014 D7: a motion closes vote_ticks after it was moved and carries when the votes reach the quorum of the living and the ayes the threshold of the votes; decrees are never bound by it)">charter</dt>
        <dd className="qrow">{describeCharter(inst.charter)}</dd>
        <dt title="the assembly's one floor (World.nomos.motion): the motion open at this tick, if any">motion</dt>
        <dd className="qrow">{inst.motion ? describeMotion(inst.motion) : "none open — the floor is free (enforcers move the first-occupancy norm after two encroachments on them; random walkers move anything)"}</dd>
        <dt>violations</dt>
        <dd className="qrow">
          {inst.record_violations
            ? "recorded one by one in the ledger (?violations=1), so the list below names each with its grounds"
            : "not recorded (ADR-0014 D9: ≈ 21 per tick), so the list below shows the episodes open now; open with ?violations=1 to record each"}
        </dd>
      </dl>
      {inst.rules.length === 0 ? (
        <p className="muted hint">
          No rule is in force: nothing is a violation yet and encroachments are only counted. Pick <b>Enact</b> in
          Intervene (encroachment · accused 32 ticks · sanction: harm is EXP-004's norm) and press Apply.
        </p>
      ) : (
        <ol className="history rules">
          {inst.rules.map((r) => (
            <li key={r.id}>
              <span className="what">{describeRule(r)}</span>
            </li>
          ))}
        </ol>
      )}
      <h2 className="sub">Timeline · decrees and motions ({entries.length})</h2>
      {readings === null ? (
        <p className="muted hint">reading the ledger…</p>
      ) : entries.length === 0 ? (
        <p className="muted hint">
          nothing in ticks {readings.timeline.from}–{Math.max(readings.timeline.from, readings.timeline.now - 1)} — every
          decree and motion is read back from the kernel's ledger, so one you apply appears here with the id the kernel
          gave its rule
        </p>
      ) : (
        <ol className="history">
          {entries
            .slice()
            .reverse()
            .slice(0, ROWS)
            .map((e, i) => (
              <li key={`${e.tick}-${i}`}>
                <span className="when">tick {e.tick}</span>
                <span className="what">{describeTimelineEntry(e)}</span>
              </li>
            ))}
        </ol>
      )}
    </div>
  );
}

/** Who violated which rule against whom, when and on what evidence — or who stands accused now (ADR-0014 D12 (b)). */
export function ViolationsCard({ frame, readings, onSelect }: CardProps) {
  const v = readings?.violations ?? null;
  const n = v?.entries.length ?? 0;
  const rows = v === null ? [] : v.source === "ledger" ? v.entries.slice().reverse().slice(0, ROWS) : v.entries.slice(0, ROWS);
  return (
    <div className="card">
      <h2 title="ADR-0014 D12 (b): each violation with its grounds when the world records them, else the episodes of accusation open at this tick (the second path)">
        Violations · recent
      </h2>
      <p className="muted hint">
        {v === null
          ? "reading…"
          : v.source === "ledger"
            ? `Every violation the kernel recorded in ticks ${v.from}–${Math.max(v.from, v.now - 1)}: who, which rule, against whom, and the fact it rests on. ${n} in all${n > ROWS ? `, newest ${ROWS} shown` : ""}.`
            : `Violations are not recorded here, so this is every episode of accusation open at tick ${v.now}, latest violation first: who stands accused under which rule since when, with the episode's counts. ${n} in all${n > ROWS ? `, first ${ROWS} shown` : ""}.`}
      </p>
      {v && n === 0 && (
        <p className="muted hint">
          {frame.institution.rules.length === 0
            ? "no rule is in force, so nothing is a violation"
            : v.source === "ledger"
              ? "none in this window"
              : "nobody stands accused at this tick"}
        </p>
      )}
      {n > 0 && (
        <ol className="history thread">
          {rows.map((e, i) => (
            <li key={`${e.kind === "violated" ? e.tick : e.since}-${e.agent}-${e.rule}-${i}`}>
              <span className="when">{e.kind === "violated" ? `tick ${e.tick}` : `since ${e.since}`}</span>
              <span className="who">
                <Id id={e.agent} onSelect={onSelect} /> → <Id id={e.against} onSelect={onSelect} />
              </span>
              <span className="what">{describeViolation(e)}</span>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

/** Each sanction tied to the episode it answered (ADR-0014 D12 (c)). */
export function SanctionsCard({ frame, readings, onSelect }: CardProps) {
  const s = readings?.sanctions ?? null;
  const n = s?.entries.length ?? 0;
  return (
    <div className="card">
      <h2 title="ADR-0014 D12 (c): the kernel's sanctioned records — a harm or rejection that reached an accused agent and counted — each tied by the kernel-side reader to the episode it answered">
        Sanctions
      </h2>
      <p className="muted hint">
        {s === null
          ? "reading…"
          : `Sanctions of ticks ${s.from}–${Math.max(s.from, s.now - 1)}: who sanctioned whom, how, under which rule, answering which episode (the one open at the view, or rebuilt from recorded violations once closed). ${n} in all${n > ROWS ? `, newest ${ROWS} shown` : ""}. On the map the same harms carry the sanction cross.`}
      </p>
      {s && n === 0 && (
        <p className="muted hint">
          {frame.institution.rules.length === 0
            ? "no rule is in force, so no harm or rejection can be a sanction"
            : "none in this window — a sanction is a harm (or rejection) that reaches an agent while an episode of accusation is open against it under a rule that allows that answer"}
        </p>
      )}
      {s && n > 0 && (
        <ol className="history thread">
          {s.entries
            .slice()
            .reverse()
            .slice(0, ROWS)
            .map((e, i) => (
              <li key={`${e.tick}-${e.by}-${e.target}-${i}`}>
                <span className="when">tick {e.tick}</span>
                <span className="who">
                  <Id id={e.by} onSelect={onSelect} /> → <Id id={e.target} onSelect={onSelect} />
                </span>
                <span className="what">{describeSanctionEntry(e)}</span>
              </li>
            ))}
        </ol>
      )}
    </div>
  );
}

/** The institution's rates over the trailing window, as curves (A solid, B dashed) and as the last values (ADR-0014 D12 (d)). */
export function NormRatesCard({
  canvasRef,
  valuesA,
  valuesB,
}: {
  canvasRef: RefObject<HTMLCanvasElement>;
  valuesA: (number | null)[];
  /** World B's values when forked. */
  valuesB: (number | null)[] | null;
}) {
  const fmt = (v: number | null, digits: number) => (v === null ? "–" : v.toFixed(digits));
  return (
    <div className="card">
      <h2 title="ADR-0014 D12 (d): the encroachment rate, violations per tick and the conditional rate of sanctions, each a ledger delta over the trailing 100 ticks">
        Norms · rates
      </h2>
      <canvas ref={canvasRef} className="curves norms" />
      <dl>
        {NORM_CURVES.curves.map((c, i) => (
          <div key={c.name} className="pair">
            <dt>{c.name}</dt>
            <dd>
              {fmt(valuesA[i], c.digits)}
              {valuesB ? ` · B ${fmt(valuesB[i], c.digits)}` : ""}
            </dd>
          </div>
        ))}
      </dl>
      <p className="muted hint">
        Ledger deltas over the trailing {NORM_WINDOW} ticks (the window <code>cz sweep</code> reports), so they read the
        same at any speed. E = encroachments / gathers is a fact of every world with institutions, rule or not;
        violations, sanctions and episodes exist only under a rule. S′ = sanctions / violations, S = episodes sanctioned
        / episodes closed. A solid, B dashed once forked — <b>Compare: with vs without rule</b> in Branches puts a
        twin that differs by one rule beside this world.
      </p>
    </div>
  );
}
