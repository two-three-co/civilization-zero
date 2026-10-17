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

  const forager = css("--forager");
  const random = css("--random");
  const A = frame.agents;
  for (let k = 0; k < A.length; k += AGENT_FIELDS) {
    ctx.globalAlpha = 0.25 + 0.75 * Math.min(1, A[k + 3] / 100_000);
    ctx.fillStyle = A[k + 5] ? random : forager;
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

export interface Sample {
  alive: number;
  food: number;
}

/**
 * Population (forager colour) and food on the map (food colour) over the
 * ticks seen so far, each on its own scale. A second world (`other`, the
 * fork) is drawn dashed on the same scales so the two can be compared.
 */
export function drawSparkline(
  canvas: HTMLCanvasElement,
  series: Map<number, Sample>,
  frontier: number,
  other?: Map<number, Sample>,
): void {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);
  const pts = [...series.entries()].sort((a, b) => a[0] - b[0]);
  const opts = other ? [...other.entries()].sort((a, b) => a[0] - b[0]) : [];
  if (pts.length < 2) return;
  const span = Math.max(1, frontier, ...opts.map(([t]) => t));
  const line = (data: [number, Sample][], pick: (s: Sample) => number, max: number, color: string, dashed: boolean) => {
    if (data.length < 2) return;
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.5;
    ctx.setLineDash(dashed ? [3, 3] : []);
    ctx.beginPath();
    data.forEach(([t, s], i) => {
      const x = (t / span) * r.width;
      const y = r.height - 4 - (pick(s) / max) * (r.height - 18);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.setLineDash([]);
  };
  const all = [...pts, ...opts];
  const aliveMax = Math.max(1, ...all.map(([, s]) => s.alive));
  const foodMax = Math.max(1, ...all.map(([, s]) => s.food));
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
