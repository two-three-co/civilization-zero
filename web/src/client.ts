import type { Deal, Envelope, Frame, Intervention, InterventionEvent, Request, Response, Watch, WorldInit } from "./protocol";

type FrameResponse = Extract<Response, { type: "frame" }>;

/** Promise-based handle on one world worker. One instance = one world. */
export class WorldClient {
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: FrameResponse) => void; reject: (e: Error) => void }>();
  /** Attached to every request, so each frame carries the watched agent's detail (`Frame.watched`). */
  private watched: Watch | null = null;

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
      const envelope: Envelope<Request> = { id, body, watch: this.watched ?? undefined };
      this.worker.postMessage(envelope);
    });
  }

  /**
   * Follow one agent: from now on every frame this client returns carries
   * the agent's kernel-written memory, last signal and ledger history
   * (`Frame.watched`), and — when the watch names a `partner` — the
   * negotiation thread between the two (`Watched.thread`, 3-9b). `null`
   * stops. Takes effect from the next request.
   */
  watch(w: Watch | null): void {
    this.watched = w;
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
  /** Every deal (accepted proposal) of ticks `from..to`, judged by the kernel-side reader as of the window's end (3-9b). */
  deals(from: number, to: number): Promise<{ frame: Frame; deals: Deal[] }> {
    return this.send({ type: "deals", from, to }).then((r) => ({ frame: r.frame, deals: r.deals ?? [] }));
  }
  /** Binary snapshot of the current view (the state its tick starts with), to fork from. */
  snapshot(): Promise<{ frame: Frame; snapshot: Uint8Array }> {
    return this.send({ type: "snapshot" }).then((r) => {
      if (!r.snapshot) throw new Error("worker returned no snapshot");
      return { frame: r.frame, snapshot: r.snapshot };
    });
  }
  terminate(): void {
    this.worker.terminate();
  }
}
