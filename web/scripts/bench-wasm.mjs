// Times the kernel under node (V8, the engine Chrome runs it in) for one or
// more wasm-pack output directories, interleaved so the machine's state is
// shared: `node web/scripts/bench-wasm.mjs web/src/wasm [other-dir ...]`.
// Each round builds the page's default world (seed 42, 128², 1,000 agents,
// 23 patches) as a forager world and as a mixed-learning world, steps 300
// ticks to warm the JIT and the population, then times 1,500 ticks taken
// 30 at a time; the minimum over four rounds is reported per directory and
// world, in ms per tick. Used to compare build profiles (STATUS, 4-7).
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

const dirs = process.argv.slice(2);
if (dirs.length === 0) {
  console.error("usage: node web/scripts/bench-wasm.mjs <wasm-pack out dir> [more dirs...]");
  process.exit(2);
}
const mods = [];
for (const dir of dirs) {
  const mod = await import(pathToFileURL(`${dir}/cz_web.js`).href);
  await mod.default({ module_or_path: readFileSync(`${dir}/cz_web_bg.wasm`) });
  mods.push({ name: dir, WasmWorld: mod.WasmWorld });
}

function time(WasmWorld, policy) {
  const w = new WasmWorld(42, 128, 128, 1000, 23, 250, policy, "");
  w.step(300);
  const t0 = performance.now();
  for (let i = 0; i < 50; i++) w.step(30);
  const ms = (performance.now() - t0) / 1500;
  w.free();
  return ms;
}

const best = new Map();
for (let round = 0; round < 4; round++) {
  for (const m of mods) {
    for (const policy of ["forager", "mixed-learning"]) {
      const key = `${m.name} ${policy}`;
      best.set(key, Math.min(best.get(key) ?? Infinity, time(m.WasmWorld, policy)));
    }
  }
}
for (const [key, ms] of best) console.log(`${key}: ${ms.toFixed(3)} ms/tick`);
