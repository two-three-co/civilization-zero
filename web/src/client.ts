import type { Envelope, Frame, Intervention, InterventionEvent, Request, Response, WorldInit } from "./protocol";

type FrameResponse = Extract<Response, { type: "frame" }>;

/** Promise-based handle on one world worker. One instance = one world. */
export class WorldClient {
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: FrameResponse) => void; reject: (e: Error) => void }>();

  constructor(private readonly worker: Worker) {
    worker.onmessage = (ev: MessageEvent<Envelope<Response>>) => {
      const { id, body } = ev.data;
      const p = this.pending.get(id);
      if (!p) return;
      this.pending.delete(id);
      if (body.type === "frame") p.resolve(body);
      else p.reject(new Error(body.message));
    };
    worker.onerror = (ev) => {
      for (const p of this.pending.values()) p.reject(new Error(ev.message));
      this.pending.clear();
    };
  }

  /** The real kernel + multiverse runtime (crates/cz-web via wasm-bindgen), one world per worker. */
  static kernel(): WorldClient {
    return new WorldClient(new Worker(new URL("./worker/kernel.worker.ts", import.meta.url), { type: "module" }));
  }

  /** The stand-in world (kept for UI work without a wasm build; `?mock=1`). */
  static mock(): WorldClient {
    return new WorldClient(new Worker(new URL("./worker/mock.worker.ts", import.meta.url), { type: "module" }));
  }

  private send(body: Request): Promise<FrameResponse> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const envelope: Envelope<Request> = { id, body };
      this.worker.postMessage(envelope);
    });
  }

  init(config: WorldInit): Promise<Frame> {
    return this.send({ type: "init", config }).then((r) => r.frame);
  }
  step(ticks: number): Promise<Frame> {
    return this.send({ type: "step", ticks }).then((r) => r.frame);
  }
  seek(tick: number): Promise<Frame> {
    return this.send({ type: "seek", tick }).then((r) => r.frame);
  }
  frame(): Promise<Frame> {
    return this.send({ type: "frame" }).then((r) => r.frame);
  }
  /** Applies an intervention at the head; the returned event is the ledger record. */
  intervene(intervention: Intervention): Promise<{ frame: Frame; event: InterventionEvent }> {
    return this.send({ type: "intervene", intervention }).then((r) => {
      if (!r.event) throw new Error("worker returned no intervention event");
      return { frame: r.frame, event: r.event };
    });
  }
  /** Every intervention on this world, read from the ledger. */
  history(): Promise<{ frame: Frame; history: InterventionEvent[] }> {
    return this.send({ type: "history" }).then((r) => ({ frame: r.frame, history: r.history ?? [] }));
  }
  terminate(): void {
    this.worker.terminate();
  }
}
