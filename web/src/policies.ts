// What the page says about policies (ADR-0013 3-9c): the composition of a
// frame by policy kind, read from the agent array's policy column, and the
// wording of a `set_policy` intervention. Nothing here decides anything;
// the kernel assigns and switches policies, the page only counts and names.

import { AGENT_FIELDS, POLICY_KINDS, type Frame, type Intervention, type PolicyKind, type Who } from "./protocol";

/** Living agents of each policy kind in a frame, in `POLICY_KINDS` order. */
export function policyCounts(frame: Frame): number[] {
  const counts = new Array<number>(POLICY_KINDS.length).fill(0);
  const A = frame.agents;
  for (let k = 0; k < A.length; k += AGENT_FIELDS) {
    const code = A[k + 5];
    if (code >= 0 && code < counts.length) counts[code]++;
  }
  return counts;
}

/** `1,000 forager · 300 learner`, kinds with nobody left out. */
export function describePolicies(counts: number[]): string {
  const parts = counts
    .map((n, i) => (n > 0 ? `${n} ${POLICY_KINDS[i].toLowerCase()}` : null))
    .filter((p): p is string => p !== null);
  return parts.length === 0 ? "nobody" : parts.join(" · ");
}

export function describeWho(who: Who): string {
  if (who === "all") return "everyone";
  if ("id" in who) return `agent ${who.id}`;
  return `each agent with probability ${who.fraction.permille}‰`;
}

/** `set policy → learner for everyone (default params)` */
export function describeSetPolicy(iv: Extract<Intervention, { op: "set_policy" }>): string {
  return `set policy → ${iv.kind.toLowerCase()} for ${describeWho(iv.who)} (${iv.params} params)`;
}

/** The fork that "Compare: fixed vs learning" makes: everyone in B switched to `kind` with the default params (ADR-0013 D7 / D8). */
export function compareIntervention(kind: PolicyKind): Intervention {
  return { op: "set_policy", who: "all", kind, params: "default" };
}
