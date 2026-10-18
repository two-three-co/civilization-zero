#!/usr/bin/env python3
"""Regenerates the tables of EXP-004-first-occupant-norm.md from the CSVs in
this directory (or the directory given as the first argument), computes the
paired statistics of the pre-registered contrasts and checks the
pre-registered predictions P0-P8 (run with `python3 -I summarize.py`).

Reads <arm>_summary.csv (one row per seed, written by `cz sweep --out`),
<arm>_series.csv (every 100 ticks; 10 for n_rand) and <arm>_qpolicy.csv
(per Q row, the greedy preference of the living learners). Prints Markdown.
Standard library only."""
import csv
import math
import os
import random
import statistics as st
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DATA = sys.argv[1] if len(sys.argv) > 1 else HERE

ARMS = [
    ("l0", "L0", 30), ("l1", "L1", 10), ("le0", "LE0", 30), ("le", "LE", 30),
    ("n_rand", "N-rand", 10), ("n_blind", "N-blind", 10), ("n_nosanction", "N-nosanction", 10),
    ("n_nocomm", "N-nocomm", 10), ("n_nomem", "N-nomem", 10), ("w0", "W0", 10), ("w1", "W1", 10),
]
LABEL = {a: l for a, l, _ in ARMS}
SPANS = {"early": (2000, 4000), "late": (10000, 20000), "tail": (15000, 20000)}
SESOI = 0.01
BOOT = 10_000
RNG_SEED = 20261018
KINDS = ["move", "gather", "consume", "rest", "transfer", "store", "harm", "signal"]
PHYS = ["alive", "food", "gathered", "consumed", "births", "deaths_starvation", "deaths_old_age",
        "deaths_violence", "gathers", "encroachments", "harms", "gathers_learner",
        "encroachments_learner", "alive_learner", "alive_enforcer"]
ROW_VIOLATOR = range(64, 128)  # violator_adjacent = 1 in the 128-row table
HARM_FLOOR = 0.0012  # ε 10‰ / 8 kinds × adjacency 0.93 (EXP-003, ADR-0014 D11)


def read(path):
    if not os.path.exists(path):
        return None
    with open(path, newline="") as fh:
        return list(csv.DictReader(fh))


def by_seed(rows):
    out = {}
    for r in rows or []:
        out.setdefault(int(r["seed"]), []).append(r)
    return out


def load(arm):
    s = read(os.path.join(DATA, f"{arm}_summary.csv"))
    if not s:
        return None
    return {
        "summary": {int(r["seed"]): r for r in s},
        "series": by_seed(read(os.path.join(DATA, f"{arm}_series.csv"))),
        "qpolicy": by_seed(read(os.path.join(DATA, f"{arm}_qpolicy.csv"))),
    }


def f(r, k):
    v = r.get(k, "")
    return float(v) if v not in ("", None) else math.nan


def fmt(x, d=3):
    if x is None or (isinstance(x, float) and (math.isnan(x) or math.isinf(x))):
        return "—"
    return f"{x:.{d}f}"


def pct(x):
    return "—" if x is None or math.isnan(x) else f"{100 * x:.1f}%"


def span_rows(rows, a, b):
    return [r for r in rows if a < int(r["tick"]) <= b]


def ratio(rows, num, den):
    n = sum(int(r[num]) for r in rows)
    d = sum(int(r[den]) for r in rows)
    return n / d if d else math.nan


def per_tick(rows, col, ticks):
    return sum(int(r[col]) for r in rows) / ticks if ticks else math.nan


def seed_metrics(arm, data, seed):
    """Every pre-registered reading of one seed (M1-M11)."""
    s = data["summary"][seed]
    rows = data["series"].get(seed, [])
    m = {"extinct": s["extinct_at"] != "-", "extinct_at": s["extinct_at"]}
    for name, (a, b) in SPANS.items():
        rs = span_rows(rows, a, b)
        ticks = b - a
        m[f"EL_{name}"] = ratio(rs, "encroachments_learner", "gathers_learner")
        m[f"S_{name}"] = ratio(rs, "episodes_sanctioned", "episodes_closed")
        m[f"Sp_{name}"] = ratio(rs, "sanctions", "violations")
        m[f"SpE_{name}"] = ratio(rs, "sanctions_enforcer", "violations")
        m[f"SpL_{name}"] = ratio(rs, "sanctions_learner", "violations")
        dv = sum(int(r["deaths_violence"]) for r in rs)
        dall = dv + sum(int(r["deaths_starvation"]) + int(r["deaths_old_age"]) for r in rs)
        m[f"viol_{name}"] = dv / dall if dall else math.nan
        m[f"deaths_{name}"] = dall / ticks
        m[f"alive_{name}"] = st.mean(int(r["alive"]) for r in rs) if rs else math.nan
        m[f"aliveL_{name}"] = st.mean(int(r["alive_learner"]) for r in rs) if rs else math.nan
        m[f"aliveE_{name}"] = st.mean(int(r["alive_enforcer"]) for r in rs) if rs else math.nan
        m[f"viol_pt_{name}"] = per_tick(rs, "violations", ticks)
        m[f"sanc_pt_{name}"] = per_tick(rs, "sanctions", ticks)
        m[f"sancL_pt_{name}"] = per_tick(rs, "sanctions_learner", ticks)
        m[f"sancE_pt_{name}"] = per_tick(rs, "sanctions_enforcer", ticks)
        m[f"harms_pt_{name}"] = per_tick(rs, "harms", ticks)
        m[f"reassigned_pt_{name}"] = per_tick(rs, "births_reassigned", ticks)
        m[f"births_pt_{name}"] = per_tick(rs, "births", ticks)
    # M4: temporal decline and persistence (D11's definition)
    m["dEL"] = m["EL_late"] - m["EL_early"]
    m["EL_late_over_early"] = m["EL_late"] / m["EL_early"] if m["EL_early"] else math.nan
    m["persist"] = (m["EL_tail"] <= 0.8 * m["EL_early"]) if not math.isnan(m["EL_tail"]) else False
    # M9: composition from tick 2,000 on
    late_rows = [r for r in rows if int(r["tick"]) >= 2000 and int(r["alive"]) > 0]
    shares = [1000 * int(r["alive_enforcer"]) / int(r["alive"]) for r in late_rows]
    m["share_min"] = min(shares) if shares else math.nan
    m["share_max"] = max(shares) if shares else math.nan
    m["minL"] = min(int(r["alive_learner"]) for r in late_rows) if late_rows else 0
    m["composition_ok"] = bool(shares) and 80 <= m["share_min"] and m["share_max"] <= 120 and m["minL"] >= 300
    # valley
    early_rows = [r for r in rows if int(r["tick"]) <= 3000]
    if early_rows:
        lo = min(early_rows, key=lambda r: int(r["alive"]))
        m["valley"] = (int(lo["tick"]), int(lo["alive"]))
    # summary readings (late window = cz sweep's window)
    m["win_mean"] = f(s, "win_mean")
    m["act_harm"] = f(s, "win_act_harm")
    m["act_rest"] = f(s, "win_act_rest")
    m["consumed"] = f(s, "win_consumed_per_agent_tick")
    m["win_aliveL"] = f(s, "win_alive_learner_mean")
    m["win_viol_pt"] = f(s, "win_violations_per_tick")
    m["win_sancL_pt"] = f(s, "win_sanctions_learner_per_tick")
    m["floor"] = HARM_FLOOR * m["win_aliveL"] / m["win_viol_pt"] if m["win_viol_pt"] else math.nan
    m["motions"] = int(s["motions"]) if s.get("motions") else 0
    m["motions_carried"] = int(s["motions_carried"]) if s.get("motions_carried") else 0
    m["tps"] = f(s, "ticks_per_s")
    # M8: greedy Harm in the violator-adjacent rows (visited rows only)
    qp = data["qpolicy"].get(seed, [])
    harm_rows, visited = 0, 0
    for r in qp:
        row = int(r["row"])
        if row not in ROW_VIOLATOR:
            continue
        learners, tie = int(r["learners"]), int(r["tie"])
        if learners == 0 or tie == learners:
            continue
        visited += 1
        counts = {k: int(r[k]) for k in KINDS}
        if max(KINDS, key=lambda k: counts[k]) == "harm":
            harm_rows += 1
    m["harm_rows"] = harm_rows
    m["visited_rows"] = visited
    return m


def arm_metrics(arm, data):
    return {seed: seed_metrics(arm, data, seed) for seed in sorted(data["summary"])}


def random_metrics(data):
    """N-rand: its life in halves (10-tick rows)."""
    out = {}
    for seed, rows in data["series"].items():
        s = data["summary"][seed]
        ext = s["extinct_at"]
        if ext == "-":
            out[seed] = {"extinct": False, "extinct_at": "-", "E_first": math.nan, "E_second": math.nan}
            continue
        t_ext = int(ext)
        half = t_ext // 2
        first = [r for r in rows if 0 < int(r["tick"]) <= half]
        second = [r for r in rows if half < int(r["tick"]) <= t_ext + 10]
        out[seed] = {
            "extinct": True, "extinct_at": ext,
            "E_first": ratio(first, "encroachments_random", "gathers_random"),
            "E_second": ratio(second, "encroachments_random", "gathers_random"),
            "E_life": ratio(rows, "encroachments_random", "gathers_random"),
            "Sp_life": ratio(rows, "sanctions", "violations"),
            "sanctions_over_harms": ratio(rows, "sanctions", "harms"),
            "motions": int(s["motions"]), "carried": int(s["motions_carried"]),
            "defeated": int(s["motions_defeated"]),
        }
    return out


def paired(ma, mb, key, seeds, rng_seed=RNG_SEED):
    """Paired differences a - b over the seeds alive in both arms."""
    pairs = [(s, ma[s][key] - mb[s][key]) for s in seeds
             if s in ma and s in mb and not ma[s]["extinct"] and not mb[s]["extinct"]
             and not math.isnan(ma[s][key]) and not math.isnan(mb[s][key])]
    d = [x for _, x in pairs]
    n = len(d)
    if n == 0:
        return {"n": 0, "dropped": len(seeds), "mean": math.nan, "lo": math.nan, "hi": math.nan,
                "dz": math.nan, "neg": 0, "pos": 0, "ratio": math.nan, "mean_a": math.nan, "mean_b": math.nan}
    mean = st.mean(d)
    sd = st.stdev(d) if n > 1 else math.nan
    rng = random.Random(rng_seed)
    boots = sorted(st.mean(rng.choices(d, k=n)) for _ in range(BOOT))
    lo, hi = boots[int(0.025 * BOOT)], boots[int(0.975 * BOOT) - 1]
    mean_a = st.mean(ma[s][key] for s, _ in pairs)
    mean_b = st.mean(mb[s][key] for s, _ in pairs)
    return {"n": n, "dropped": len(seeds) - n, "mean": mean, "lo": lo, "hi": hi,
            "dz": mean / sd if sd else math.nan, "neg": sum(1 for x in d if x < 0),
            "pos": sum(1 for x in d if x > 0), "ratio": mean_a / mean_b if mean_b else math.nan,
            "mean_a": mean_a, "mean_b": mean_b, "d": dict(pairs)}


def ci_text(p, d=4):
    if p["n"] == 0:
        return "—"
    return f"{fmt(p['mean'], d)} [{fmt(p['lo'], d)}, {fmt(p['hi'], d)}], d_z {fmt(p['dz'], 2)}, n {p['n']}" + (f" (dropped {p['dropped']})" if p["dropped"] else "")


def mean_sd(ms, key):
    xs = [m[key] for m in ms.values() if not m["extinct"] and not math.isnan(m[key])]
    if not xs:
        return math.nan, math.nan
    return st.mean(xs), (st.stdev(xs) if len(xs) > 1 else math.nan)


def arm_table(M):
    print("\n### 腕ごとの要約（後半の窓 = tick 10,001〜20,000、生存シードの平均 ± sd）\n")
    print("| 腕 | n | 絶滅 | E_L 序盤 | **E_L 後半** | E_L 末尾 | S 後半 | S′ 後半 | S′_E | S′_L | 制裁/t (E / L) | 加害/t | 違反/t | 加害死 | 個体数 | Learner | Enforcer | 摂取/体·tick | 決定 Harm | 決定 Rest | 出生/t | 白紙流入/t |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for arm, label, _ in ARMS:
        if arm not in M or arm == "n_rand":
            continue
        ms = M[arm]
        n = len(ms)
        ext = sum(1 for m in ms.values() if m["extinct"])
        c = lambda k, d=4: "{} ± {}".format(fmt(mean_sd(ms, k)[0], d), fmt(mean_sd(ms, k)[1], d))
        print(f"| {label} | {n} | {ext}/{n} | {c('EL_early')} | **{c('EL_late')}** | {c('EL_tail')} | {c('S_late', 3)} | {c('Sp_late', 3)} | {fmt(mean_sd(ms, 'SpE_late')[0], 3)} | {fmt(mean_sd(ms, 'SpL_late')[0], 3)} | {fmt(mean_sd(ms, 'sancE_pt_late')[0], 2)} / {fmt(mean_sd(ms, 'sancL_pt_late')[0], 2)} | {fmt(mean_sd(ms, 'harms_pt_late')[0], 2)} | {fmt(mean_sd(ms, 'viol_pt_late')[0], 2)} | {pct(mean_sd(ms, 'viol_late')[0])} | {fmt(mean_sd(ms, 'alive_late')[0], 0)} | {fmt(mean_sd(ms, 'aliveL_late')[0], 0)} | {fmt(mean_sd(ms, 'aliveE_late')[0], 0)} | {fmt(mean_sd(ms, 'consumed')[0], 4)} | {fmt(mean_sd(ms, 'act_harm')[0], 5)} | {fmt(mean_sd(ms, 'act_rest')[0], 3)} | {fmt(mean_sd(ms, 'births_pt_late')[0], 2)} | {fmt(mean_sd(ms, 'reassigned_pt_late')[0], 3)} |")


def composition_table(M):
    print("\n### 構成の維持（M9: tick ≥ 2,000 の全サンプル tick）と谷\n")
    print("| 腕 | Enforcer ‰ 最小〜最大（シードの範囲） | Learner の最小（シードの範囲） | M9 を満たすシード | 谷（tick ≤ 3,000 の最小生存、シードの範囲） |")
    print("|---|---|---|---|---|")
    for arm, label, _ in ARMS:
        if arm not in M or arm in ("l0", "l1", "n_rand"):
            continue
        ms = M[arm]
        ok = sum(1 for m in ms.values() if m["composition_ok"])
        smin = [m["share_min"] for m in ms.values() if not math.isnan(m["share_min"])]
        smax = [m["share_max"] for m in ms.values() if not math.isnan(m["share_max"])]
        minl = [m["minL"] for m in ms.values()]
        val = [m["valley"][1] for m in ms.values() if "valley" in m]
        print(f"| {label} | {fmt(min(smin), 0)}〜{fmt(max(smax), 0)} | {min(minl)}〜{max(minl)} | {ok}/{len(ms)} | {min(val)}〜{max(val)} |")
    for arm in ("l0", "l1"):
        if arm in M:
            val = [m["valley"][1] for m in M[arm].values() if "valley" in m]
            print(f"\n- {LABEL[arm]} の谷（tick ≤ 3,000 の最小生存）: {min(val)}〜{max(val)} 体")


def contrasts(M):
    print("\n### 対の比較（同じ seed の差、平均 [95% ブートストラップ CI]、d_z、対の数）\n")
    print("| 対比 | 指標 | 差 A − B | A < B のシード | 比 A / B | 平均 A | 平均 B |")
    print("|---|---|---|---|---|---|---|")
    pairs = [
        ("le", "le0", list(range(30))), ("le0", "l0", list(range(30))), ("le", "l0", list(range(30))),
        ("l1", "l0", list(range(10))), ("n_blind", "le", list(range(10))), ("n_blind", "le0", list(range(10))),
        ("n_nosanction", "le0", list(range(10))), ("n_nosanction", "le", list(range(10))),
        ("n_nocomm", "le", list(range(10))), ("n_nomem", "le", list(range(10))),
        ("w1", "w0", list(range(10))), ("w1", "le", list(range(10))), ("w0", "le0", list(range(10))),
    ]
    out = {}
    for a, b, seeds in pairs:
        if a not in M or b not in M:
            continue
        for key, name, d in [("EL_late", "E_L 後半", 4), ("EL_early", "E_L 序盤", 4), ("aliveL_late", "Learner 後半", 0),
                             ("viol_late", "加害死の割合 後半", 3), ("Sp_late", "S′ 後半", 3), ("S_late", "S 後半", 3),
                             ("consumed", "摂取/体·tick", 4), ("act_rest", "決定 Rest", 3)]:
            p = paired(M[a], M[b], key, seeds)
            out[(a, b, key)] = p
            if key in ("EL_late", "aliveL_late", "viol_late") or (key == "Sp_late" and b in ("le",)) or (key == "S_late" and a == "n_nomem") or (key == "act_rest" and a == "w0") or (key == "EL_early" and (a, b) == ("le", "le0")):
                print(f"| {LABEL[a]} − {LABEL[b]} | {name} | {ci_text(p, d)} | {p['neg']}/{p['n']} | {fmt(p['ratio'], 3)} | {fmt(p['mean_a'], d)} | {fmt(p['mean_b'], d)} |")
    return out


def time_table(M):
    print("\n### 時間構造（M4: E_L の序盤 → 後半 → 末尾）\n")
    print("| 腕 | E_L 序盤 | E_L 後半 | E_L 末尾 | 後半/序盤（平均） | 持続（末尾 ≤ 0.8 × 序盤）のシード |")
    print("|---|---|---|---|---|---|")
    for arm, label, _ in ARMS:
        if arm not in M or arm == "n_rand":
            continue
        ms = M[arm]
        alive = [m for m in ms.values() if not m["extinct"]]
        pers = sum(1 for m in alive if m["persist"])
        rr = [m["EL_late_over_early"] for m in alive if not math.isnan(m["EL_late_over_early"])]
        print(f"| {label} | {fmt(mean_sd(ms, 'EL_early')[0], 4)} | {fmt(mean_sd(ms, 'EL_late')[0], 4)} | {fmt(mean_sd(ms, 'EL_tail')[0], 4)} | {fmt(st.mean(rr) if rr else math.nan, 3)} | {pers}/{len(alive)} |")


def identity(a, b, seeds):
    """Rows of the physical columns that differ between two arms' series."""
    diffs = {}
    for s in seeds:
        ra, rb = a["series"].get(s), b["series"].get(s)
        if ra is None or rb is None:
            diffs[s] = None
            continue
        diffs[s] = sum(1 for x, y in zip(ra, rb) if any(x[c] != y[c] for c in PHYS)) + abs(len(ra) - len(rb))
    return diffs


def check(name, ok, detail):
    print(f"- {name}: {'**支持**' if ok else '**反証**'} — {detail}")
    return ok


def predictions(M, D, C):
    print("\n### 予測の当否（`summarize.py` の判定）\n")
    results = {}
    # P0a composition
    arms_c = [a for a in ("le0", "le", "n_blind", "n_nosanction", "n_nocomm", "n_nomem", "w0", "w1") if a in M]
    bad = {a: [s for s, m in M[a].items() if not m["composition_ok"] and not m["extinct"]] for a in arms_c}
    results["P0a"] = check("P0a 構成", all(not v for v in bad.values()), "; ".join(f"{LABEL[a]} {len(M[a]) - len(bad[a])}/{len(M[a])}" + (f" (外れ seed {bad[a]})" if bad[a] else "") for a in arms_c))
    # P0b survival
    ext = {a: [s for s, m in M[a].items() if m["extinct"]] for a in M if a != "n_rand"}
    ok0b = all(len(ext.get(a, [])) == 0 for a in ("l0", "le0") if a in M) and len(ext.get("le", [])) <= 1 and all(len(ext.get(a, [])) <= 1 for a in ("l1", "n_blind", "n_nosanction", "n_nocomm", "n_nomem", "w0", "w1") if a in M)
    results["P0b"] = check("P0b 生存", ok0b, "; ".join(f"{LABEL[a]} 絶滅 {len(v)}/{len(M[a])}" + (f" (seed {v})" if v else "") for a, v in ext.items()))
    # P1a main contrast
    if ("le", "le0", "EL_late") in C:
        p = C[("le", "le0", "EL_late")]
        neg10 = sum(1 for s in range(10) if s in p.get("d", {}) and p["d"][s] < 0)
        ok = neg10 >= 8 and p["hi"] < 0 and abs(p["mean"]) >= SESOI
        results["P1a"] = check("P1a 主対比（H1）", ok, f"後半の E_L: LE < LE0 が seed 0〜9 で {neg10}/10、30 シードの対の差 {ci_text(p)}、比 {fmt(p['ratio'], 3)}（平均 LE {fmt(p['mean_a'], 4)} vs LE0 {fmt(p['mean_b'], 4)}）")
    # P1b cost
    if ("le", "le0", "aliveL_late") in C:
        pl = C[("le", "le0", "aliveL_late")]
        pv = C[("le", "le0", "viol_late")]
        negl = sum(1 for s in range(10) if s in pl.get("d", {}) and pl["d"][s] < 0)
        posv = sum(1 for s in range(10) if s in pv.get("d", {}) and pv["d"][s] > 0)
        ok = negl >= 8 and pl["hi"] < 0 and posv >= 8
        results["P1b"] = check("P1b コスト", ok, f"Learner の個体数 LE < LE0 が {negl}/10、差 {ci_text(pl, 0)}; 加害死の割合 LE > LE0 が {posv}/10、差 {ci_text(pv, 3)}")
    # P1c time structure
    if ("le", "le0", "EL_early") in C:
        pe = C[("le", "le0", "EL_early")]
        neg_e = sum(1 for s in range(10) if s in pe.get("d", {}) and pe["d"][s] < 0)
        le_alive = [m for m in M["le"].values() if not m["extinct"]]
        rr = [m["EL_late_over_early"] for m in le_alive if not math.isnan(m["EL_late_over_early"])]
        ok_ii = bool(rr) and st.mean(rr) >= 0.5
        pers = {a: (sum(1 for m in M[a].values() if not m["extinct"] and m["persist"]), sum(1 for m in M[a].values() if not m["extinct"])) for a in ("le", "le0", "l0") if a in M}
        pers10 = {a: sum(1 for s, m in M[a].items() if s < 10 and not m["extinct"] and m["persist"]) for a in pers}
        ok_iii = all(v <= 5 for v in pers10.values()) and not (pers10.get("le", 0) >= 7 and pers10.get("le0", 0) < 7)
        ok = neg_e >= 8 and pe["hi"] < 0 and ok_ii and ok_iii
        results["P1c"] = check("P1c 時間構造", ok, f"(i) 序盤の E_L LE < LE0 が {neg_e}/10、差 {ci_text(pe)}; (ii) LE の後半/序盤の平均 {fmt(st.mean(rr) if rr else math.nan, 3)}（≥ 0.5）; (iii) 持続（末尾 ≤ 0.8 × 序盤）のシード: " + ", ".join(f"{LABEL[a]} {pers[a][0]}/{pers[a][1]}（seed 0〜9 では {pers10[a]}/10）" for a in pers))
    # P2a learner sanctions floor
    if "l1" in M:
        ms = M["l1"]
        under = [s for s, m in ms.items() if not m["extinct"] and m["Sp_late"] <= 2 * m["floor"]]
        results["P2a"] = check("P2a L1 の制裁", len(under) >= 9, f"S′ ≤ 2 × 床 が {len(under)}/{len(ms)} シード; S′ " + ", ".join(fmt(m['Sp_late'], 3) for m in ms.values()) + "; 床 " + ", ".join(fmt(m['floor'], 3) for m in ms.values()))
    # P2b learned sanctioning
    if "l1" in M and "l0" in M:
        rows_ok = all(m["harm_rows"] <= 3 for a in ("l1", "le") if a in M for m in M[a].values())
        p = paired(M["l1"], M["l0"], "act_harm", list(range(10)))
        ratio_ok = sum(1 for s in range(10) if s in p.get("d", {}) and M["l1"][s]["act_harm"] <= 1.5 * M["l0"][s]["act_harm"]) >= 9
        results["P2b"] = check("P2b 制裁の学習", rows_ok and ratio_ok, "行 64〜127 で Harm が貪欲多数派の行数（訪問済み）: " + "; ".join(f"{LABEL[a]} " + " ".join(f"{m['harm_rows']}/{m['visited_rows']}" for m in M[a].values()) for a in ("l1", "le") if a in M) + f"; win_act_harm L1 ≤ 1.5 × L0: {sum(1 for s in range(10) if s in p.get('d', {}) and M['l1'][s]['act_harm'] <= 1.5 * M['l0'][s]['act_harm'])}/10（L1 {fmt(mean_sd(M['l1'], 'act_harm')[0], 5)} vs L0 {fmt(mean_sd(M['l0'], 'act_harm')[0], 5)}）")
    # P2c L1 vs L0
    if ("l1", "l0", "EL_late") in C:
        p = C[("l1", "l0", "EL_late")]
        ok = p["n"] > 0 and p["lo"] <= SESOI and p["hi"] >= -SESOI
        results["P2c"] = check("P2c L1 と L0", ok, f"E_L 後半の差 {ci_text(p)}（CI が [−0.01, +0.01] と重なれば支持）")
    # P3 identity
    if "n_nosanction" in D and "le0" in D:
        diffs = identity(D["n_nosanction"], D["le0"], list(range(10)))
        viol = [m["viol_pt_late"] for m in M["n_nosanction"].values()]
        sanc = [m["sanc_pt_late"] for m in M["n_nosanction"].values()]
        ok = all(v == 0 for v in diffs.values()) and all(v > 0 for v in viol) and all(v == 0 for v in sanc)
        results["P3"] = check("P3 N-nosanction ≡ LE0", ok, f"物理列の不一致行数（seed 0〜9）: {list(diffs.values())}; 違反/tick {fmt(st.mean(viol), 2)}, 制裁/tick {fmt(st.mean(sanc), 2)}")
    # P4 blind
    if ("n_blind", "le0", "EL_late") in C and ("n_blind", "le", "EL_late") in C:
        p0 = C[("n_blind", "le0", "EL_late")]
        p1 = C[("n_blind", "le", "EL_late")]
        sl = sum(1 for s in range(10) if s in M["n_blind"] and s in M["le"] and M["n_blind"][s]["win_sancL_pt"] < 0.2 * M["le"][s]["win_sancL_pt"])
        ok = p0["neg"] >= 8 and -SESOI <= p1["mean"] <= SESOI and p1["pos"] >= 7 and sl == 10
        results["P4"] = check("P4 N-blind", ok, f"N-blind < LE0 が {p0['neg']}/{p0['n']}（差 {ci_text(p0)}）; N-blind − LE {ci_text(p1)}、N-blind ≥ LE が {p1['pos']}/{p1['n']}; 学習者の制裁 < 0.2 × LE が {sl}/10（N-blind {fmt(mean_sd(M['n_blind'], 'win_sancL_pt')[0], 3)} vs LE {fmt(mean_sd(M['le'], 'win_sancL_pt')[0], 3)} /tick）")
    # P5 random
    if "n_rand" in D:
        R = random_metrics(D["n_rand"])
        ext_ok = all(r["extinct"] and int(r["extinct_at"]) <= 2000 for r in R.values())
        nodecl = sum(1 for r in R.values() if r["extinct"] and not math.isnan(r["E_second"]) and r["E_second"] >= 0.5 * r["E_first"])
        carried = [r.get("carried", 0) for r in R.values()]
        ok = ext_ok and nodecl >= 8 and all(c <= 1 for c in carried)
        results["P5"] = check("P5 N-rand", ok, f"(a) 絶滅 tick " + " ".join(str(r['extinct_at']) for r in R.values()) + f"; (b) E_R 後半 ≥ 0.5 × 前半 が {nodecl}/{len(R)}（前半 " + " ".join(fmt(r['E_first'], 3) for r in R.values()) + "; 後半 " + " ".join(fmt(r['E_second'], 3) for r in R.values()) + f"）; (c) 可決 {carried}（動議 {[r.get('motions', 0) for r in R.values()]}）; 生涯の S′ " + " ".join(fmt(r.get('Sp_life', math.nan), 3) for r in R.values()) + "、制裁/加害 " + " ".join(fmt(r.get('sanctions_over_harms', math.nan), 3) for r in R.values()))
    # P6 nocomm
    if ("n_nocomm", "le", "EL_late") in C:
        p = C[("n_nocomm", "le", "EL_late")]
        ok = p["n"] > 0 and -SESOI <= p["mean"] <= SESOI and p["lo"] <= 0 <= p["hi"]
        results["P6"] = check("P6 N-nocomm", ok, f"N-nocomm − LE の E_L 後半 {ci_text(p)}")
    # P7 nomem
    if ("n_nomem", "le", "Sp_late") in C:
        ps, pS, pe, pl = (C[("n_nomem", "le", k)] for k in ("Sp_late", "S_late", "EL_late", "aliveL_late"))
        ge_S = sum(1 for s, v in pS.get("d", {}).items() if v >= 0)
        le_E = sum(1 for s, v in pe.get("d", {}).items() if v <= 0)
        ok = ps["pos"] >= 8 and ge_S >= 7 and le_E >= 7 and pl["neg"] >= 7
        results["P7"] = check("P7 N-nomem", ok, f"(a) S′ N-nomem > LE が {ps['pos']}/{ps['n']}（{ci_text(ps, 3)}）; (b) S ≥ が {ge_S}/{pS['n']}（{ci_text(pS, 3)}）; (c) E_L ≤ が {le_E}/{pe['n']}（{ci_text(pe)}）; (d) Learner < が {pl['neg']}/{pl['n']}（{ci_text(pl, 0)}）")
    # P8 W
    if ("w1", "le", "viol_late") in C and ("w1", "w0", "EL_late") in C:
        pv, pe, pw, pr = C[("w1", "le", "viol_late")], C[("w1", "le", "EL_late")], C[("w1", "w0", "EL_late")], C[("w0", "le0", "act_rest")]
        le_E = sum(1 for v in pe.get("d", {}).values() if v <= 0)
        ok = pv["neg"] >= 7 and le_E >= 7 and pw["neg"] >= 8 and pr["mean"] > 0
        results["P8"] = check("P8 W（副次）", ok, f"(a) 加害死 W1 < LE が {pv['neg']}/{pv['n']}（{ci_text(pv, 3)}）; (b) E_L W1 ≤ LE が {le_E}/{pe['n']}（{ci_text(pe)}）; (c) W1 − W0 の E_L が負のシード {pw['neg']}/{pw['n']}（{ci_text(pw)}; LE − LE0 と同符号）; W0 の決定 Rest − LE0 {ci_text(pr, 3)}")
    return results


def random_table(D):
    if "n_rand" not in D:
        return
    R = random_metrics(D["n_rand"])
    print("\n### N-rand（Random + R、生涯 = 絶滅まで、10 tick ごとの系列）\n")
    print("| seed | 絶滅 tick | E_R 前半 | E_R 後半 | E_R 生涯 | S′ 生涯 | 制裁/加害 | 動議 | 可決 | 否決 |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for s, r in sorted(R.items()):
        print(f"| {s} | {r['extinct_at']} | {fmt(r['E_first'], 3)} | {fmt(r['E_second'], 3)} | {fmt(r.get('E_life', math.nan), 3)} | {fmt(r.get('Sp_life', math.nan), 3)} | {fmt(r.get('sanctions_over_harms', math.nan), 3)} | {r.get('motions', '—')} | {r.get('carried', '—')} | {r.get('defeated', '—')} |")


def per_seed(M):
    print("\n### 付録: シードごとの値（後半の窓）\n")
    for arm, label, _ in ARMS:
        if arm not in M or arm == "n_rand":
            continue
        print(f"\n#### {label} (`{arm}`)\n")
        print("| seed | 絶滅 | 谷 (tick, 体) | E_L 序盤 | E_L 後半 | E_L 末尾 | S | S′ | S′_E | S′_L | 加害死 | 個体数 | Learner | Enforcer | Enf ‰ min〜max | Learner min | 決定 Harm | 行 64〜127 Harm | tick/s |")
        print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
        for s, m in sorted(M[arm].items()):
            v = m.get("valley", ("—", "—"))
            print(f"| {s} | {m['extinct_at']} | {v[0]}, {v[1]} | {fmt(m['EL_early'], 4)} | {fmt(m['EL_late'], 4)} | {fmt(m['EL_tail'], 4)} | {fmt(m['S_late'], 3)} | {fmt(m['Sp_late'], 3)} | {fmt(m['SpE_late'], 3)} | {fmt(m['SpL_late'], 3)} | {pct(m['viol_late'])} | {fmt(m['alive_late'], 0)} | {fmt(m['aliveL_late'], 0)} | {fmt(m['aliveE_late'], 0)} | {fmt(m['share_min'], 0)}〜{fmt(m['share_max'], 0)} | {m['minL']} | {fmt(m['act_harm'], 5)} | {m['harm_rows']}/{m['visited_rows']} | {fmt(m['tps'], 0)} |")


def main():
    D = {arm: d for arm, _, _ in ARMS if (d := load(arm))}
    M = {arm: arm_metrics(arm, d) for arm, d in D.items() if arm != "n_rand"}
    if "n_rand" in D:
        M["n_rand"] = {}
    print(f"データ: {DATA}（腕 {len(D)}: " + ", ".join(f"{LABEL[a]} {len(D[a]['summary'])} シード" for a in D) + "）")
    arm_table(M)
    composition_table(M)
    C = contrasts(M)
    time_table(M)
    random_table(D)
    predictions(M, D, C)
    per_seed(M)


if __name__ == "__main__":
    main()
