// Times the rewind (ADR-0010 2b-4, RISKS R11, ADR-0014 completion condition
// 2) under node (V8, Chrome's engine) for one or more wasm-pack output
// directories, interleaved so the machine's state is shared:
// `node web/scripts/bench-seek.mjs web/src/wasm [other-dir ...]`.
// For each directory and world — the page's default world (seed 42, 128²,
// 1,000 founders, 23 patches, snapshot every 250 ticks) as a forager world
// warmed 1,000 ticks, as a learner world (`?policy=learner`, 32 rows)
// warmed 2,000 and as the institution world (`?policy=institution`, the
// first-occupancy norm decreed at tick 0 as the kernel bench does) warmed
// 3,000 — it then times 1,500 ticks taken 30 at a time at the head (the
// steady-state ms/tick), and, with the head H a multiple of 250, the seek
// to H − 1 (restore the snapshot at H − 250, kept in memory by the
// multiverse, and re-execute 249 ticks: the worst case), the seek to
// H − 250 (the restore alone), and the fork's decode — `snapshot_bytes()`
// at H handed to `WasmWorld.fromSnapshot`, the path a fork takes across
// workers — each the minimum of five rounds. The snapshot's size is
// `snapshot_bytes().length` at H.
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const dirs = process.argv.slice(2);
if (dirs.length === 0) {
  console.error("usage: node web/scripts/bench-seek.mjs <wasm-pack out dir> [more dirs...]");
  process.exit(2);
}
const mods = [];
for (const dir of dirs) {
  const mod = await import(pathToFileURL(`${dir}/cz_web.js`).href);
  await mod.default({ module_or_path: readFileSync(`${dir}/cz_web_bg.wasm`) });
  mods.push({ name: dir, WasmWorld: mod.WasmWorld });
}

const EVERY = 250;
const WORLDS = [
  { policy: "forager", warm: 1000, decree: false },
  { policy: "learner", warm: 2000, decree: false },
  { policy: "institution", warm: 3000, decree: true },
];
const ENACT = '{"op":"enact","norm":{"kind":"encroach","standing_ticks":32,"sanction":1,"except_sanction":false}}';

function measure(WasmWorld, { policy, warm, decree }) {
  const w = new WasmWorld(42, 128, 128, 1000, 23, EVERY, policy, "");
  if (decree) w.intervene(ENACT);
  w.step(warm);
  const t0 = performance.now();
  for (let i = 0; i < 50; i++) w.step(30);
  const msPerTick = (performance.now() - t0) / 1500;
  const head = w.tick();
  if (head % EVERY !== 0) throw new Error(`head ${head} is not on a snapshot`);
  const agents = w.agents().length / 9;
  const snapshot = w.snapshot_bytes();
  const bytes = snapshot.length;
  let worst = Infinity;
  let restore = Infinity;
  let decode = Infinity;
  for (let round = 0; round < 5; round++) {
    let t = performance.now();
    w.seek(head - 1);
    worst = Math.min(worst, performance.now() - t);
    w.seek(head);
    t = performance.now();
    w.seek(head - EVERY);
    restore = Math.min(restore, performance.now() - t);
    w.seek(head);
    t = performance.now();
    const fork = WasmWorld.fromSnapshot(snapshot, EVERY);
    decode = Math.min(decode, performance.now() - t);
    if (fork.tick() !== head) throw new Error("the fork starts at the snapshot's tick");
    fork.free();
  }
  w.free();
  return { head, agents, bytes, msPerTick, worst, restore, decode };
}

for (const m of mods) {
  for (const world of WORLDS) {
    const r = measure(m.WasmWorld, world);
    console.log(
      `${m.name} ${world.policy}: head ${r.head}, ${r.agents} agents, snapshot ${r.bytes} B, ` +
        `${r.msPerTick.toFixed(3)} ms/tick, seek worst (249 ticks) ${r.worst.toFixed(1)} ms, restore ${r.restore.toFixed(2)} ms, fork decode ${r.decode.toFixed(2)} ms`,
    );
  }
}
