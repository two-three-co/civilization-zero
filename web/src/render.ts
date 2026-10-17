import { AGENT_FIELDS, MARKER_FIELDS, MARKER_HARM, MARKER_SIGNAL, MARKER_TRANSFER, type Frame } from "./protocol";

export const SCALE = 4;

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

let scratch: { canvas: OffscreenCanvas; image: ImageData; w: number; h: number } | null = null;

/** Colours of the social markers, resolved once per frame. */
interface Palette {
  xfer: string;
  harm: string;
  sig: string;
}

/** How long a marker stays visible after the frame that produced it. */
const TRAIL_MS = 600;

/**
 * Markers of the last few frames, so an interaction that happened in one
 * 16 ms frame stays visible long enough to be seen. Each entry is drawn
 * where its agents were in the frame that reported it, fading with age; the
 * frame currently shown is drawn at full strength by `drawFrame`. Nothing
 * here is state about the world — only about what was already shown.
 */
export class MarkerTrail {
  private past: { at: number; tick: number; markers: Int32Array }[] = [];
  private lastTick: number | null = null;

  /** Remembers a frame's markers (once per tick). */
  push(frame: Frame): void {
    if (frame.tick === this.lastTick) return;
    this.lastTick = frame.tick;
    const now = performance.now();
    if (frame.social.length > 0) this.past.push({ at: now, tick: frame.tick, markers: frame.social });
    this.prune(now);
  }

  reset(): void {
    this.past = [];
    this.lastTick = null;
  }

  private prune(now: number): void {
    while (this.past.length > 0 && now - this.past[0].at > TRAIL_MS) this.past.shift();
  }

  /** Older frames' markers, faded by age; the frame being shown is skipped. */
  draw(ctx: CanvasRenderingContext2D, shownTick: number, palette: Palette): void {
    const now = performance.now();
    this.prune(now);
    for (const e of this.past) {
      if (e.tick === shownTick) continue;
      const alpha = 0.7 * (1 - (now - e.at) / TRAIL_MS);
      if (alpha > 0.02) drawMarkers(ctx, e.markers, alpha, palette);
    }
  }
}

/**
 * Transfers (line from giver to receiver, dot at the receiver), harms (line
 * from attacker to victim, cross at the victim) and signals (ring around
 * the speaker) of one frame, at the agents' positions in that frame. A
 * marker whose other party is gone (`x2 = -1`) is drawn at the one left.
 */
function drawMarkers(ctx: CanvasRenderingContext2D, m: Int32Array, alpha: number, palette: Palette): void {
  if (m.length === 0) return;
  const c = (v: number) => (v + 0.5) * SCALE;
  ctx.save();
  ctx.globalAlpha = alpha;
  ctx.lineWidth = 1.5;
  for (let k = 0; k < m.length; k += MARKER_FIELDS) {
    const x = c(m[k + 2]);
    const y = c(m[k + 3]);
    const other = m[k + 4] >= 0;
    const x2 = other ? c(m[k + 4]) : x;
    const y2 = other ? c(m[k + 5]) : y;
    switch (m[k]) {
      case MARKER_TRANSFER:
        ctx.strokeStyle = palette.xfer;
        ctx.fillStyle = palette.xfer;
        if (other) {
          ctx.beginPath();
          ctx.moveTo(x, y);
          ctx.lineTo(x2, y2);
          ctx.stroke();
        }
        ctx.beginPath();
        ctx.arc(x2, y2, 2.5, 0, Math.PI * 2);
        ctx.fill();
        break;
      case MARKER_HARM:
        ctx.strokeStyle = palette.harm;
        ctx.beginPath();
        if (other) {
          ctx.moveTo(x, y);
          ctx.lineTo(x2, y2);
        }
        ctx.moveTo(x2 - 3, y2 - 3);
        ctx.lineTo(x2 + 3, y2 + 3);
        ctx.moveTo(x2 + 3, y2 - 3);
        ctx.lineTo(x2 - 3, y2 + 3);
        ctx.stroke();
        break;
      case MARKER_SIGNAL:
        ctx.strokeStyle = palette.sig;
        ctx.beginPath();
        ctx.arc(x, y, SCALE * 1.25, 0, Math.PI * 2);
        ctx.stroke();
        break;
    }
  }
  ctx.restore();
}

/**
 * Food as a translucent green layer (1 px per cell, scaled up), agents as
 * squares whose opacity is their energy, then the social markers of the
 * tick just executed (and, through `trail`, the fading ones of recent
 * frames), then the selection ring.
 */
export function drawFrame(canvas: HTMLCanvasElement, frame: Frame, selected: number | null, trail?: MarkerTrail): void {
  const { width: w, height: h } = frame;
  if (canvas.width !== w * SCALE || canvas.height !== h * SCALE) {
    canvas.width = w * SCALE;
    canvas.height = h * SCALE;
  }
  if (!scratch || scratch.w !== w || scratch.h !== h) {
    scratch = { canvas: new OffscreenCanvas(w, h), image: new ImageData(w, h), w, h };
  }
  const [fr, fg, fb] = css("--food").split(",").map(Number);
  const d = scratch.image.data;
  for (let k = 0; k < w * h; k++) {
    const a = Math.min(1, frame.food[k] / 20);
    d[k * 4] = fr;
    d[k * 4 + 1] = fg;
    d[k * 4 + 2] = fb;
    d[k * 4 + 3] = Math.round(a * 200);
  }
  scratch.canvas.getContext("2d")!.putImageData(scratch.image, 0, 0);

  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = false;
  ctx.fillStyle = css("--panel");
  ctx.fillRect(0, 0, w * SCALE, h * SCALE);
  ctx.drawImage(scratch.canvas, 0, 0, w * SCALE, h * SCALE);

  // One colour per policy kind, indexed by the agent's policy code (POLICY_KINDS order).
  const colours = [css("--forager"), css("--random"), css("--reflex"), css("--learner")];
  const A = frame.agents;
  for (let k = 0; k < A.length; k += AGENT_FIELDS) {
    ctx.globalAlpha = 0.25 + 0.75 * Math.min(1, A[k + 3] / 100_000);
    ctx.fillStyle = colours[A[k + 5]] ?? colours[0];
    ctx.fillRect(A[k + 1] * SCALE + 0.5, A[k + 2] * SCALE + 0.5, SCALE - 1, SCALE - 1);
  }
  ctx.globalAlpha = 1;

  const palette: Palette = { xfer: css("--xfer"), harm: css("--harm"), sig: css("--sig") };
  if (trail) {
    trail.push(frame);
    trail.draw(ctx, frame.tick, palette);
  }
  drawMarkers(ctx, frame.social, 1, palette);

  if (selected !== null) {
    for (let k = 0; k < A.length; k += AGENT_FIELDS) {
      if (A[k] !== selected) continue;
      ctx.strokeStyle = css("--sel");
      ctx.lineWidth = 1.5;
      ctx.strokeRect(A[k + 1] * SCALE - 3, A[k + 2] * SCALE - 3, SCALE + 6, SCALE + 6);
    }
  }
}

/** Index of the agent nearest to a click, within 3 cells, or null. */
export function pickAgent(frame: Frame, cellX: number, cellY: number): number | null {
  const A = frame.agents;
  let best: number | null = null;
  let bestD = 9;
  for (let k = 0; k < A.length; k += AGENT_FIELDS) {
    const d = (A[k + 1] + 0.5 - cellX) ** 2 + (A[k + 2] + 0.5 - cellY) ** 2;
    if (d < bestD) {
      bestD = d;
      best = A[k];
    }
  }
  return best;
}

/** One shown tick of a world: population, food, and the ledger totals the rate curves are cut from. */
export interface Sample {
  alive: number;
  food: number;
  /** `Ledger.food_consumed` at the tick (running total). */
  consumed: number;
  transfers: number;
  harms: number;
  signals: number;
}

/** Ticks a per-tick rate is averaged over (a trailing window ending at the sample). */
export const RATE_WINDOW = 50;
/** Most points a curve is drawn with; longer series are strided so a long run does not slow the frame. */
const MAX_DRAWN = 1200;

type Points = [number, Sample][];

/**
 * The samples of one world, one per tick shown, kept in tick order as they
 * arrive so no frame has to sort them. Play appends; a rewind followed by
 * play overwrites the ticks it revisits (same ledger, same values).
 */
export class History {
  readonly pts: Points = [];

  set(tick: number, s: Sample): void {
    const p = this.pts;
    if (p.length === 0 || tick > p[p.length - 1][0]) {
      p.push([tick, s]);
      return;
    }
    let lo = 0;
    let hi = p.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (p[mid][0] < tick) lo = mid + 1;
      else hi = mid;
    }
    if (lo < p.length && p[lo][0] === tick) p[lo] = [tick, s];
    else p.splice(lo, 0, [tick, s]);
  }
}

/** Indices to draw: every point while the series is short, every k-th (and the last) when it is long. */
function strided(n: number): number[] {
  const stride = Math.max(1, Math.ceil(n / MAX_DRAWN));
  const out: number[] = [];
  for (let i = 0; i < n; i += stride) out.push(i);
  if (n > 0 && out[out.length - 1] !== n - 1) out.push(n - 1);
  return out;
}

/**
 * Per-tick rate of a running total at sample `i`: the total's change over
 * the trailing `RATE_WINDOW` ticks (or since the first sample while the
 * series is shorter than that), divided by the ticks elapsed. `j` is the
 * index of the window's start, maintained by the caller as `i` advances.
 */
function rateAt(pts: Points, i: number, j: number, pick: (s: Sample) => number): number | null {
  const [ti, si] = pts[i];
  const [tj, sj] = pts[j];
  return ti > tj ? (pick(si) - pick(sj)) / (ti - tj) : null;
}

/** The values of one curve at every sample of a series, in tick order (`null` where undefined). */
function valuesOf(pts: Points, curve: Curve): (number | null)[] {
  const out: (number | null)[] = [];
  let j = 0;
  for (let i = 0; i < pts.length; i++) {
    while (j + 1 < i && pts[j + 1][0] <= pts[i][0] - RATE_WINDOW) j++;
    out.push(curve.at(pts, i, j));
  }
  return out;
}

interface Curve {
  label: string;
  digits: number;
  at: (pts: Points, i: number, j: number) => number | null;
}

/** The comparison's curves (ADR-0013 D8 / 3-9c), top to bottom. */
export const CURVES: Curve[] = [
  { label: "alive", digits: 0, at: (p, i) => p[i][1].alive },
  {
    label: `intake / agent / tick (${RATE_WINDOW}-tick mean)`,
    digits: 3,
    at: (p, i, j) => {
      const r = rateAt(p, i, j, (s) => s.consumed);
      return r === null || p[i][1].alive === 0 ? null : r / p[i][1].alive;
    },
  },
  { label: "transfers / tick", digits: 2, at: (p, i, j) => rateAt(p, i, j, (s) => s.transfers) },
  { label: "harms / tick", digits: 2, at: (p, i, j) => rateAt(p, i, j, (s) => s.harms) },
  { label: "signals / tick", digits: 2, at: (p, i, j) => rateAt(p, i, j, (s) => s.signals) },
];

/**
 * Small multiples for "fixed vs learning" (and any other fork): population,
 * intake per agent per tick, and transfers / harms / signals per tick over
 * the ticks seen so far, world A solid and world B dashed on one scale per
 * row. Rates are ledger deltas over the trailing `RATE_WINDOW` ticks, so
 * they read the same at any ticks-per-frame setting. Everything drawn is a
 * frame the worker sent; nothing is simulated here.
 */
export function drawCurves(canvas: HTMLCanvasElement, series: History, frontier: number, other?: History): void {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);
  const pts = series.pts;
  const opts = other ? other.pts : [];
  ctx.fillStyle = css("--muted");
  ctx.font = "11px system-ui";
  if (pts.length < 2) {
    ctx.fillText("play to collect ticks", 2, 11);
    return;
  }
  const span = Math.max(1, frontier, opts.length > 0 ? opts[opts.length - 1][0] : 0);
  const rowH = r.height / CURVES.length;
  const colourA = css("--forager");
  const colourB = css("--learner");
  CURVES.forEach((curve, row) => {
    const va = valuesOf(pts, curve);
    const vb = valuesOf(opts, curve);
    let max = 1e-9;
    for (const v of va) if (v !== null && v > max) max = v;
    for (const v of vb) if (v !== null && v > max) max = v;
    const top = row * rowH + 13;
    const bottom = (row + 1) * rowH - 3;
    const line = (data: Points, vals: (number | null)[], colour: string, dashed: boolean) => {
      ctx.strokeStyle = colour;
      ctx.lineWidth = 1.5;
      ctx.setLineDash(dashed ? [3, 3] : []);
      ctx.beginPath();
      let pen = false;
      for (const i of strided(data.length)) {
        const v = vals[i];
        if (v === null) {
          pen = false;
          continue;
        }
        const x = (data[i][0] / span) * r.width;
        const y = bottom - (v / max) * (bottom - top);
        if (pen) ctx.lineTo(x, y);
        else ctx.moveTo(x, y);
        pen = true;
      }
      ctx.stroke();
      ctx.setLineDash([]);
    };
    if (row > 0) {
      ctx.strokeStyle = css("--line");
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(0, row * rowH + 0.5);
      ctx.lineTo(r.width, row * rowH + 0.5);
      ctx.stroke();
    }
    line(pts, va, colourA, false);
    line(opts, vb, colourB, true);
    const last = (vals: (number | null)[]) => {
      for (let i = vals.length - 1; i >= 0; i--) if (vals[i] !== null) return vals[i]!.toFixed(curve.digits);
      return "–";
    };
    ctx.fillStyle = css("--muted");
    ctx.fillText(`${curve.label} · A ${last(va)}${other ? ` · B ${last(vb)}` : ""}`, 2, row * rowH + 10);
  });
}

/**
 * Population (forager colour) and food on the map (food colour) over the
 * ticks seen so far, each on its own scale. A second world (`other`, the
 * fork) is drawn dashed on the same scales so the two can be compared.
 */
export function drawSparkline(canvas: HTMLCanvasElement, series: History, frontier: number, other?: History): void {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);
  const pts = series.pts;
  const opts = other ? other.pts : [];
  if (pts.length < 2) return;
  const span = Math.max(1, frontier, opts.length > 0 ? opts[opts.length - 1][0] : 0);
  const line = (data: Points, pick: (s: Sample) => number, max: number, color: string, dashed: boolean) => {
    if (data.length < 2) return;
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.setLineDash(dashed ? [3, 3] : []);
    ctx.beginPath();
    strided(data.length).forEach((i, k) => {
      const [t, s] = data[i];
      const x = (t / span) * r.width;
      const y = r.height - 4 - (pick(s) / max) * (r.height - 18);
      if (k === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.setLineDash([]);
  };
  let aliveMax = 1;
  let foodMax = 1;
  for (const [, s] of pts) {
    aliveMax = Math.max(aliveMax, s.alive);
    foodMax = Math.max(foodMax, s.food);
  }
  for (const [, s] of opts) {
    aliveMax = Math.max(aliveMax, s.alive);
    foodMax = Math.max(foodMax, s.food);
  }
  const foodColor = `rgb(${css("--food")})`;
  const aliveColor = css("--forager");
  line(pts, (s) => s.food, foodMax, foodColor, false);
  line(pts, (s) => s.alive, aliveMax, aliveColor, false);
  line(opts, (s) => s.food, foodMax, foodColor, true);
  line(opts, (s) => s.alive, aliveMax, aliveColor, true);
  ctx.fillStyle = css("--muted");
  ctx.font = "11px system-ui";
  ctx.fillText(`alive max ${aliveMax} · food max ${foodMax}${other ? " · dashed = fork" : ""}`, 2, 11);
}
