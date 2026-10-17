// What the page says about institutions (ADR-0014 D12, 4-8): deterministic
// sentences from the kernel's own types — a norm's four parameters (D5), a
// rule, the charter, the kernel-side reader's timeline / violation /
// sanction entries (crates/cz-web/src/institution.rs) and an agent's
// standing — plus the fork "Compare: with vs without rule" makes. Nothing
// here judges anything: every word names a field the kernel or its reader
// wrote, and the page only puts them in order.

import {
  SANCTION_HARM,
  SANCTION_REJECT,
  type Accused,
  type Charter,
  type Episode,
  type Grounds,
  type Institution,
  type Intervention,
  type Norm,
  type NormKind,
  type OpenMotion,
  type Rule,
  type SanctionEntry,
  type TimelineEntry,
  type ViolationEntry,
} from "./protocol";

/** The fact a rule of each kind turns into a violation, as a noun. */
export function normName(kind: NormKind): string {
  switch (kind) {
    case "encroach":
      return "encroachment";
    case "broken_promise":
      return "broken promise";
    case "harm":
      return "harm";
  }
}

/**
 * The CLI's `--enact KIND` defaults (ADR-0014 D8: accused 32 ticks, sanction
 * harm, no exemption) — what "Compare: with vs without rule" enacts and what
 * the Enact controls start from.
 */
export function defaultNorm(kind: NormKind): Norm {
  return { kind, standing_ticks: 32, sanction: SANCTION_HARM, except_sanction: false };
}

/** `harm` · `rejection` · `harm or rejection` · none. */
export function describeSanction(bits: number): string {
  const parts: string[] = [];
  if (bits & SANCTION_HARM) parts.push("harm");
  if (bits & SANCTION_REJECT) parts.push("rejection");
  return parts.length === 0 ? "none (violations are counted, nobody is sanctioned)" : parts.join(" or ");
}

/** `encroachment · accused 32 ticks after each violation · sanction: harm` */
export function describeNorm(n: Norm): string {
  const standing =
    n.standing_ticks === 0 ? "counted, nobody stands accused" : `accused ${n.standing_ticks} ticks after each violation`;
  const except = n.kind === "harm" && n.except_sanction ? " · a sanctioning harm is no violation" : "";
  return `${normName(n.kind)} · ${standing} · sanction: ${describeSanction(n.sanction)}${except}`;
}

export function describeRule(r: Rule): string {
  return `rule #${r.id} · ${describeNorm(r.norm)} · in force since tick ${r.enacted}`;
}

export function describeCharter(c: Charter): string {
  return `quorum ${c.quorum_permille}‰ of the living · threshold ${c.threshold_permille}‰ of the votes · ${c.vote_ticks} ticks per motion · ${c.amendable ? "amendable" : "not amendable"} by motion`;
}

/** The open motion in one line (none before 4-5). */
export function describeMotion(m: OpenMotion): string {
  const what =
    "enact" in m.kind
      ? `enact ${describeNorm(m.kind.enact.norm)}`
      : "repeal" in m.kind
        ? `repeal rule #${m.kind.repeal.rule}`
        : `charter: ${describeCharter(m.kind.charter.charter)}`;
  return `motion #${m.id} by #${m.by}, opened tick ${m.opened}: ${what} · ayes ${m.ayes} · nays ${m.nays}`;
}

/** The decree wording the intervention history and the Branches caption share. */
export function describeDecree(iv: Extract<Intervention, { op: "enact" | "repeal" | "charter" }>): string {
  switch (iv.op) {
    case "enact":
      return `enact ${describeNorm(iv.norm)}`;
    case "repeal":
      return `repeal rule #${iv.rule}`;
    case "charter":
      return `charter: ${describeCharter(iv.charter)}`;
  }
}

/** One timeline entry as the reader wrote it (the rule id is the one the kernel assigned, recovered by the reader). */
export function describeTimelineEntry(e: TimelineEntry): string {
  switch (e.kind) {
    case "enacted":
      return `rule #${e.rule} enacted by ${e.by}: ${describeNorm(e.norm)}${e.replaced !== null ? ` · replaced rule #${e.replaced}` : ""}`;
    case "repealed":
      return `rule #${e.rule} repealed by ${e.by}${e.norm ? ` (${describeNorm(e.norm)})` : ""}`;
    case "chartered":
      return `charter replaced by ${e.by}: ${describeCharter(e.charter)}`;
  }
}

/** The evidence a recorded violation rests on (`Grounds`). */
export function describeGrounds(g: Grounds): string {
  switch (g.fact) {
    case "encroachment":
      return `gathered ${g.amount} on cell (${g.cell.x}, ${g.cell.y}) where the other arrived first`;
    case "broken_promise":
      return `the promise of tick ${g.offer_tick} was ${g.short} short at its deadline`;
    case "harm":
      return `harmed for −${g.damage} health`;
  }
}

/**
 * The inspector's line (D12): "Accused under rule #k (encroachment) since
 * tick t (violations n, sanctions m), until tick u".
 */
export function describeStanding(a: Accused): string {
  const latest = a.violations > 1 ? ` · latest tick ${a.last} against #${a.against}` : ` · against #${a.against}`;
  return `Accused under rule #${a.rule} (${normName(a.norm)}) since tick ${a.since} (violations ${a.violations}, sanctions ${a.sanctions}), until tick ${a.until}${latest}`;
}

/** The part of a violation entry after the names ("who / against whom" are rendered separately so the ids can be picked). */
export function describeViolation(v: ViolationEntry): string {
  if (v.kind === "violated") return `violated rule #${v.rule} (${normName(v.norm)}): ${describeGrounds(v.grounds)}`;
  return `accused under rule #${v.rule} (${normName(v.norm)}) since tick ${v.since} (violations ${v.violations}, sanctions ${v.sanctions}) · until tick ${v.until}${v.violations > 1 ? ` · latest tick ${v.last}` : ""}`;
}

/** The episode a sanction answered, as the reader found it, or that it could not be known. */
export function describeEpisode(e: Episode | null): string {
  if (e === null) return "episode unknown (closed, and violations were not recorded)";
  const state = e.open ? "still open" : "closed · rebuilt from the recorded violations";
  return `answering the episode open since tick ${e.since} (violations ${e.violations}, sanctions ${e.sanctions}, ${state})`;
}

/** The part of a sanction entry after the names. */
export function describeSanctionEntry(s: SanctionEntry): string {
  const rule = s.norm ? `rule #${s.rule} (${normName(s.norm)})` : `rule #${s.rule}`;
  return `sanctioned by ${s.via === "harm" ? "a harm" : "a rejection"} under ${rule} · ${describeEpisode(s.episode)}`;
}

/** Sum of a per-kind counter (`NORM_KINDS` order). */
export function sumKinds(v: [number, number, number]): number {
  return v[0] + v[1] + v[2];
}

/**
 * "Compare: with vs without rule" (ADR-0014 D8 / D12): the fork's
 * intervention. B repeals the rule of `kind` when A has one in force, else
 * enacts the default norm of that kind — so the two worlds differ by that
 * one rule and nothing else.
 */
export function compareRuleIntervention(inst: Institution, kind: NormKind): Intervention {
  const r = inst.rules.find((rule) => rule.norm.kind === kind);
  return r ? { op: "repeal", rule: r.id } : { op: "enact", norm: defaultNorm(kind) };
}

/** What the compare button will do, in the words of the caption. */
export function describeCompareRule(inst: Institution, kind: NormKind): string {
  const iv = compareRuleIntervention(inst, kind);
  return iv.op === "repeal"
    ? `B repeals rule #${iv.rule} (${normName(kind)}); A keeps it`
    : `B enacts ${describeNorm((iv as Extract<Intervention, { op: "enact" }>).norm)}; A stays without it`;
}
