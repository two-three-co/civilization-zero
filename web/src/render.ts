import { AGENT_FIELDS, type Frame } from "./protocol";

export const SCALE = 4;

const css = (name: string) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

let scratch: { canvas: OffscreenCanvas; image: ImageData; w: number; h: number } | null = null;

/** Food as a translucent green layer (1 px per cell, scaled up), agents as squares whose opacity is their energy. */
export function drawFrame(canvas: HTMLCanvasElement, frame: Frame, selected: number | null): void {
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

/** Population over the ticks seen so far. */
export function drawSparkline(canvas: HTMLCanvasElement, series: Map<number, number>, frontier: number): void {
  const r = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  const ctx = canvas.getContext("2d")!;
  ctx.scale(dpr, dpr);
  const pts = [...series.entries()].sort((a, b) => a[0] - b[0]);
  if (pts.length < 2) return;
  const max = Math.max(1, ...pts.map((p) => p[1]));
  const span = Math.max(1, frontier);
  ctx.strokeStyle = css("--forager");
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  pts.forEach(([t, n], i) => {
    const x = (t / span) * r.width;
    const y = r.height - 4 - (n / max) * (r.height - 18);
    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  });
  ctx.stroke();
  ctx.fillStyle = css("--muted");
  ctx.font = "11px system-ui";
  ctx.fillText(`max ${max}`, 2, 11);
}
