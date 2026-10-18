// Samples the page's own rate readouts once a second and keeps only the
// seconds in which the page really ran in the foreground (ADR-0014 4-9;
// STATUS "その他" explains why: a hidden Browser pane drops rAF to 1–2 Hz
// and the wasm workers to background QoS, and the header then shows 0 or
// a throttled value). Paste the whole file into the page's console, then:
//   cz.go(3000)                       // seek / extend to a tick (the UI's Go)
//   cz.speed(30); cz.btn('Play').click(); cz.start()
//   ... wait, with the pane in front ...
//   cz.stop()                         // medians over the valid seconds
// A second is valid when the rAF counter ran at ≥ 57 Hz over it and the
// frontier's advance over it agrees with the header's tick/s within 15%.
// `cz.btn('Compare: with vs without rule').click()` (or 'Compare: fixed
// vs learning') forks B first for the two-world numbers.
window.cz = { raf: { n: 0 }, last: null };
(function loop() { cz.raf.n++; requestAnimationFrame(loop); })();
cz.btn = (t) => [...document.querySelectorAll("button")].find((b) => b.textContent.trim() === t);
cz.speed = (v) => {
  const sel = [...document.querySelectorAll("select")].find((s) => [...s.options].some((o) => o.textContent.includes("tick/frame")));
  Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(sel, String(v));
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  return sel.value;
};
cz.go = (t) => {
  const inp = document.querySelector('input[aria-label="go to tick"]');
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(inp, String(t));
  inp.dispatchEvent(new Event("input", { bubbles: true }));
  setTimeout(() => cz.btn("Go").click(), 50);
  return "go " + t;
};
cz.read = () => {
  const T = document.body.innerText;
  const now = performance.now();
  const r = {
    t: Math.round(now),
    raf: cz.raf.n,
    sim: (T.match(/sim rate\n(.*)\n/) || [])[1],
    render: (T.match(/render\n(.*)\n/) || [])[1],
    worker: (T.match(/worker step\n(.*)\n/) || [])[1],
    frontier: +(T.match(/frontier\n(\d+)/) || [])[1],
    alive: (T.match(/alive\n(\d+)/) || [])[1],
    aliveB: (T.match(/B · fork @\d+ · tick \d+ · alive (\d+)/) || [])[1],
    seek: (T.match(/last seek\n(.*)\n/) || [])[1],
  };
  if (cz.last) {
    const dt = (now - cz.last.t) / 1000;
    r.rafHz = +((cz.raf.n - cz.last.raf) / dt).toFixed(1);
    r.frontierTps = +((r.frontier - cz.last.frontier) / dt).toFixed(0);
  }
  cz.last = { t: now, raf: cz.raf.n, frontier: r.frontier };
  return r;
};
cz.start = () => {
  cz.samples = [];
  cz.last = null;
  cz.read();
  cz.timer = setInterval(() => cz.samples.push(cz.read()), 1000);
  return "sampling";
};
cz.stop = (keep) => {
  if (!keep) clearInterval(cz.timer);
  const S = cz.samples;
  const num = (s) => parseFloat(s || "0");
  const valid = S.filter((s) => s.rafHz >= 57 && num(s.sim) > 0 && Math.abs(s.frontierTps - num(s.sim)) / num(s.sim) < 0.15);
  const med = (xs) => {
    const a = [...xs].sort((p, q) => p - q);
    return a.length ? a[Math.floor(a.length / 2)] : null;
  };
  return {
    samples: S.length,
    valid: valid.length,
    awake: S.filter((s) => s.rafHz >= 57).length,
    tps: med(valid.map((s) => num(s.sim))),
    fps: med(valid.map((s) => num(s.render))),
    worker: med(valid.map((s) => num(s.worker))),
    frontierTps: med(valid.map((s) => s.frontierTps)),
    alive: med(valid.map((s) => +s.alive)),
    aliveB: med(valid.map((s) => +(s.aliveB || 0))),
    minmax: valid.length ? [Math.min(...valid.map((s) => num(s.sim))), Math.max(...valid.map((s) => num(s.sim)))] : null,
    frontier: [S[0] && S[0].frontier, S[S.length - 1] && S[S.length - 1].frontier],
    seek: S.length ? S[S.length - 1].seek : null,
    sleptWorker: med(S.filter((s) => s.rafHz < 10).map((s) => num(s.worker))),
  };
};
