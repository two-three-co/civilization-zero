import { useEffect, useState } from "react";
import type { Verification } from "./Branches";
import type { Frame, InterventionEvent } from "./protocol";

interface Props {
  frame: Frame | null;
  frameB: Frame | null;
  events: InterventionEvent[];
  verification: Verification | null;
  /** The timeline has been dragged behind the frontier at least once. */
  rewound: boolean;
}

const KEY = "cz.tutorial.hidden";

/**
 * The five-minute tour (PRODUCT.md, Toy 1) plus the Phase 4 step (ADR-0014
 * 4-8). Nothing here drives the UI; each step is ticked off when the world's
 * own state shows it happened.
 */
export function Tutorial({ frame, frameB, events, verification, rewound }: Props) {
  const [hidden, setHidden] = useState(false);
  useEffect(() => {
    try {
      setHidden(localStorage.getItem(KEY) === "1");
    } catch {
      /* private mode or blocked storage: show the tour */
    }
  }, []);
  const hide = (v: boolean) => {
    setHidden(v);
    try {
      localStorage.setItem(KEY, v ? "1" : "0");
    } catch {
      /* ignore */
    }
  };

  const steps = [
    {
      title: "Watch a civilization",
      done: (frame?.frontier ?? 0) >= 200,
      text: "Press Play. Foragers (orange) look for food (green), eat, breed by splitting their energy, and die of hunger or age. Let it run to tick 200 or so and watch the population settle.",
    },
    {
      title: "Rewind",
      done: rewound,
      text: "Drag the timeline left (or type a tick and press Go). The world is restored from the nearest snapshot and re-executed, checked against its own ledger. The state hash is the fingerprint of that exact state.",
    },
    {
      title: "Intervene",
      done: events.length > 0 || frameB !== null,
      text: "At the frontier, pick an intervention (a drought is the dramatic one) and press Apply. It is recorded in the ledger — the history below the panel is read back from the kernel, not remembered by the page.",
    },
    {
      title: "Fork and compare",
      done: frameB !== null,
      text: "Rewind to any tick and press Fork here: a second world starts from that tick with the intervention. Play again — both run in lockstep, and the Branches panel shows how population and food diverge.",
    },
    {
      title: "Check it is real",
      done: verification?.status === "done" && verification.match === true,
      text: "Press Verify: fork again. A third world is built from the same snapshot with the same intervention and driven to the same tick. Same inputs, same hash — that is the determinism contract, demonstrated rather than claimed.",
    },
    {
      title: "Enact a rule",
      done:
        (frame?.institution.rules.length ?? 0) > 0 ||
        (frameB?.institution.rules.length ?? 0) > 0 ||
        events.some((e) => e.intervention.op === "enact"),
      text: "Tick institutions in the header (the world is rebuilt with them on; ?nomos=1), pick Enact in Intervene — encroachment, accused 32 ticks, sanction: harm — and Apply. The Institution card reads the decree back from the ledger with the rule id the kernel gave it; Violations lists who stands accused of what, Sanctions ties each harm that counted to the episode it answered, the map outlines the accused and crosses the sanctions, and Norms · rates draws E, violations / tick and S′. Then press Compare: with vs without rule in Branches: B forks here without the rule, and Verify still says MATCH.",
    },
  ];
  const next = steps.findIndex((s) => !s.done);
  const doneCount = steps.filter((s) => s.done).length;

  if (hidden) {
    return (
      <p className="muted tour-toggle">
        <button className="link" onClick={() => hide(false)}>
          Show the 5-minute tour
        </button>
      </p>
    );
  }

  return (
    <div className="card tour">
      <h2>
        Five-minute tour · {doneCount}/{steps.length}
      </h2>
      <ol className="steps">
        {steps.map((s, i) => (
          <li key={s.title} className={s.done ? "done" : i === next ? "current" : ""}>
            <span className="mark">{s.done ? "✓" : i + 1}</span>
            <div>
              <b>{s.title}</b>
              {(i === next || s.done) && <p className="muted">{s.text}</p>}
            </div>
          </li>
        ))}
      </ol>
      {next === -1 && (
        <p className="hint ok">
          Done. Everything you saw was computed by the kernel in this tab; nothing was fetched from a server. Reload
          to start over with a fresh world.
        </p>
      )}
      <p className="muted tour-toggle">
        <button className="link" onClick={() => hide(true)}>
          Hide the tour
        </button>
      </p>
    </div>
  );
}
