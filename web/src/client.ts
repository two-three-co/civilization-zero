import type { Envelope, Frame, Request, Response, WorldInit } from "./protocol";

/** Promise-based handle on one world worker. One instance = one world. */
export class WorldClient {
  private nextId = 1;
  private pending = new Map<number, { resolve: (f: Frame) => void; reject: (e: Error) => void }>();

  constructor(private readonly worker: Worker) {
    worker.onmessage = (ev: MessageEvent<Envelope<Response>>) => {
      const { id, body } = ev.data;
      const p = this.pending.get(id);
      if (!p) return;
      this.pending.delete(id);
      if (body.type === "frame") p.resolve(body.frame);
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

  private send(body: Request): Promise<Frame> {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      const envelope: Envelope<Request> = { id, body };
      this.worker.postMessage(envelope);
    });
  }

  init(config: WorldInit): Promise<Frame> {
    return this.send({ type: "init", config });
  }
  step(ticks: number): Promise<Frame> {
    return this.send({ type: "step", ticks });
  }
  seek(tick: number): Promise<Frame> {
    return this.send({ type: "seek", tick });
  }
  frame(): Promise<Frame> {
    return this.send({ type: "frame" });
  }
  terminate(): void {
    this.worker.terminate();
  }
}
