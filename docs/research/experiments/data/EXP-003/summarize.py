#!/usr/bin/env python3
"""Regenerates the tables of EXP-003-baseline-grid.md from the CSVs in this
directory, applies the pre-registered decision rule for `nomos_preset` and
checks the pre-registered predictions P0-P6 (run with `python3 -I summarize.py`).

Reads <arm>_summary.csv (one row per seed, written by `cz sweep --out`),
<arm>_qpolicy.csv (per Q row, the greedy preference of the living learners
and the row's visits) and <arm>_series.csv (every 100 ticks), plus the
EXP-002 files for the identity check P0. Prints Markdown."""
import collections
import csv
import math
import os
import statistics as st

HERE = os.path.dirname(os.path.abspath(__file__))
EXP002 = os.path.join(HERE, "..", "EXP-002")
EPS = (0, 10, 50)
HC = (2000, 8000)
HW = (0, 2)
CELLS = [(e, h, w) for e in EPS for h in HC for w in HW]
DEFAULT = (50, 2000, 0)
F_BAND = 1311.5  # EXP-002: F の窓平均の 10 シード平均
BAND_MIN = 918  # その 70% (決定規則 b, P2a, P5a)
VIOLENCE_MAX = 0.10  # 決定規則 c
ROW9 = 9  # hungry, no food, none here, food in sight, nobody adjacent
ROW9_OCC = 9 + 32  # the same row with OCCUPIED (128-row table only)
KINDS = ["move", "gather", "consume", "rest", "transfer", "store", "harm", "signal"]
SERIES16 = [
    "seed", "tick", "alive", "food", "gathered", "consumed", "births", "transfers", "harms",
    "signals", "proposals", "acceptances", "rejections", "deaths_starvation", "deaths_old_age",
    "deaths_violence",
]


def cell_name(c):
    e, h, w = c
    return f"grid_e{e}_h{h}_w{w}"


def label(c):
    return "({}, {}, {})".format(*c)


def read(path):
    if not os.path.exists(path):
        return None
    with open(path, newline="") as fh:
        return list(csv.DictReader(fh))


def summary(name):
    return read(os.path.join(HERE, f"{name}_summary.csv"))


def series(name, where=HERE):
    return read(os.path.join(where, f"{name}_series.csv"))


def qpolicy(name):
    return read(os.path.join(HERE, f"{name}_qpolicy.csv"))


def f(r, k):
    v = r.get(k, "")
    return float(v) if v not in ("", None) else math.nan


def mean_sd(xs):
    xs = [x for x in xs if not math.isnan(x)]
    if not xs:
        return math.nan, math.nan
    m = sum(xs) / len(xs)
    return m, math.sqrt(sum((x - m) ** 2 for x in xs) / len(xs))


def fmt(x, d=1):
    return "—" if x is None or (isinstance(x, float) and math.isnan(x)) else f"{x:.{d}f}"


def pct(x):
    return "—" if x is None or math.isnan(x) else f"{100 * x:.1f}%"


def violence_share(rs):
    """Window deaths by violence over all window deaths, pooled over seeds (M5)."""
    dv = sum(int(r["win_deaths_violence"]) for r in rs)
    ds = sum(int(r["win_deaths_starvation"]) for r in rs)
    da = sum(int(r["win_deaths_old_age"]) for r in rs)
    tot = dv + ds + da
    return dv / tot if tot else math.nan


def seed_violence(r):
    dv, ds, da = (int(r[k]) for k in ("win_deaths_violence", "win_deaths_starvation", "win_deaths_old_age"))
    return dv / (dv + ds + da) if dv + ds + da else math.nan


def majority(qrows, row):
    """Per seed, the kind most living learners pick greedily in `row` (M13):
    'untouched' when every learner is tied there, None when no learner lives."""
    out = {}
    for r in qrows:
        if int(r["row"]) != row:
            continue
        learners, tie = int(r["learners"]), int(r["tie"])
        if learners == 0:
            out[r["seed"]] = None
        elif tie == learners:
            out[r["seed"]] = ("untouched", 1.0, int(r["visits"]))
        else:
            counts = {k: int(r[k]) for k in KINDS}
            best = max(KINDS, key=lambda k: counts[k])
            out[r["seed"]] = (best, counts[best] / learners, int(r["visits"]))
    return out


def rest_seeds(maj):
    return sum(1 for v in maj.values() if v and v[0] == "rest")


def arm_stats(name):
    rs = summary(name)
    if not rs:
        return None
    m, sd = mean_sd([f(r, "win_mean") for r in rs])
    qp = qpolicy(name)
    maj9 = majority(qp, ROW9) if qp else {}
    s = {
        "rows": rs,
        "n": len(rs),
        "ext": sum(1 for r in rs if r["extinct_at"] != "-"),
        "mean": m,
        "sd": sd,
        "cv": sd / m if m else math.nan,
        "ratio_max": max(f(r, "win_max_over_min") for r in rs),
        "violence": violence_share(rs),
        "violence_seed_max": max(seed_violence(r) for r in rs),
        "harm_act": mean_sd([f(r, "win_act_harm") for r in rs])[0],
        "harm_act_max": max(f(r, "win_act_harm") for r in rs),
        "rest_act": mean_sd([f(r, "win_act_rest") for r in rs])[0],
        "move_act": mean_sd([f(r, "win_act_move") for r in rs])[0],
        "harms": mean_sd([f(r, "win_harms_per_tick") for r in rs])[0],
        "e0": mean_sd([f(r, "win_encroach_rate") for r in rs]),
        "food": mean_sd([f(r, "win_food_standing_mean") for r in rs])[0],
        "regrown": mean_sd([f(r, "win_regrown_per_tick") for r in rs])[0],
        "cons": mean_sd([f(r, "win_consumed_per_agent_tick") for r in rs])[0],
        "births": mean_sd([f(r, "win_births_per_tick") for r in rs])[0],
        "q_abs": mean_sd([f(r, "q_abs_mean") for r in rs])[0],
        "q_delta": mean_sd([f(r, "q_delta_window") for r in rs])[0],
        "tps": mean_sd([f(r, "ticks_per_s") for r in rs])[0],
        "maj9": maj9,
        "row9_rest": rest_seeds(maj9),
    }
    s["ok_a"] = s["ext"] == 0
    s["ok_b"] = s["mean"] >= BAND_MIN
    s["ok_c"] = s["violence"] <= VIOLENCE_MAX
    return s


def per_seed_table(title, name, rs):
    print(f"\n#### {title} (`{name}`)\n")
    print("| seed | 最終 | 絶滅 | 窓平均 | 窓 min | 窓 max | max/min | 出生 | 餓死 | 老衰 | 加害死 | 窓の加害死 | 摂取/体·tick | 加害/t | 決定 Harm | 決定 Rest | 決定 Move | E₀ | 食料 | 再生/t | \\|Q\\| | ΔQ 窓 | tick/s |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for r in rs:
        print("| {seed} | {fin} | {ext} | {wm} | {wmin} | {wmax} | {ratio} | {b} | {ds} | {da} | {dv} | {wdv} ({vs}) | {cons} | {hm} | {ah} | {ar} | {am} | {e0} | {food} | {rg} | {q} | {dq} | {tps} |".format(
            seed=r["seed"], fin=r["final_alive"], ext=r["extinct_at"], wm=fmt(f(r, "win_mean"), 1),
            wmin=r["win_min"], wmax=r["win_max"], ratio=fmt(f(r, "win_max_over_min"), 3),
            b=r["births"], ds=r["deaths_starvation"], da=r["deaths_old_age"], dv=r["deaths_violence"],
            wdv=r["win_deaths_violence"], vs=pct(seed_violence(r)),
            cons=fmt(f(r, "win_consumed_per_agent_tick"), 4), hm=fmt(f(r, "win_harms_per_tick"), 2),
            ah=fmt(f(r, "win_act_harm"), 5), ar=fmt(f(r, "win_act_rest"), 3), am=fmt(f(r, "win_act_move"), 3),
            e0=fmt(f(r, "win_encroach_rate"), 4), food=fmt(f(r, "win_food_standing_mean"), 0),
            rg=fmt(f(r, "win_regrown_per_tick"), 1), q=fmt(f(r, "q_abs_mean"), 2),
            dq=fmt(f(r, "q_delta_window"), 2), tps=fmt(f(r, "ticks_per_s"), 0)))


def grid_table(stats):
    print("\n### 主格子（12 セル、窓 = tick 10,001〜20,000、10 シード）\n")
    print("| (ε, h, w) | 絶滅 | 窓平均 | sd | CV | F 比 | 加害死 (合算) | 最大 (seed) | 加害/t | 決定 Harm | 決定 Rest | 決定 Move | E₀ | 食料 | 再生/t | 摂取/体·tick | 出生/t | \\|Q\\| | 行 9 Rest | (a)(b)(c) |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for c in CELLS:
        s = stats.get(c)
        if not s:
            print(f"| {label(c)} | — |")
            continue
        marks = "".join("✓" if ok else "✗" for ok in (s["ok_a"], s["ok_b"], s["ok_c"]))
        print(f"| {label(c)} | {s['ext']}/{s['n']} | {fmt(s['mean'])} | {fmt(s['sd'])} | {fmt(s['cv'], 3)} | {100 * s['mean'] / F_BAND:.0f}% | {pct(s['violence'])} | {pct(s['violence_seed_max'])} | {fmt(s['harms'], 2)} | {fmt(s['harm_act'], 5)} | {fmt(s['rest_act'], 3)} | {fmt(s['move_act'], 3)} | {fmt(s['e0'][0], 4)} | {fmt(s['food'], 0)} | {fmt(s['regrown'], 1)} | {fmt(s['cons'], 4)} | {fmt(s['births'], 2)} | {fmt(s['q_abs'], 2)} | {s['row9_rest']}/{s['n']} | {marks} |")


def row9_table(stats):
    print("\n### 行 9（hungry・在庫なし・足元に食料なし・食料が視界にある・隣接なし）の貪欲行動（M13、終了時）\n")
    print("| (ε, h, w) | Rest 多数派 | 多数派の種類（seed 0〜9） | 訪問（窓、seed 平均） |")
    print("|---|---|---|---|")
    for c in CELLS:
        s = stats.get(c)
        if not s:
            continue
        kinds = " ".join((v[0] if v else "dead") for _, v in sorted(s["maj9"].items(), key=lambda kv: int(kv[0])))
        visits = [v[2] for v in s["maj9"].values() if v]
        print(f"| {label(c)} | {s['row9_rest']}/{s['n']} | {kinds} | {fmt(st.mean(visits) if visits else math.nan, 0)} |")


def decide(stats):
    """The pre-registered decision rule of EXP-003 for `nomos_preset`."""
    lines = []
    ok = [c for c in CELLS if c in stats and stats[c]["ok_a"] and stats[c]["ok_b"] and stats[c]["ok_c"]]
    lines.append("(a)〜(c) を満たすセル: " + (", ".join(label(c) for c in ok) if ok else "なし"))
    if ok:
        emax = max(c[0] for c in ok)
        cand = [c for c in ok if c[0] == emax]
        lines.append(f"規則 1（ε 最大）: ε {emax} → " + ", ".join(label(c) for c in cand))
    else:
        cand = [(0, 8000, w) for w in HW if (0, 8000, w) in stats]
        lines.append("どのセルも満たさないので ε 0・harm_cost 8,000 を採る（記録: 条件は満たせなかった）")
    ws = sorted({c[2] for c in cand})
    if len(ws) > 1:
        w_ok = {w: all(stats[c]["row9_rest"] <= 5 for c in cand if c[2] == w) for w in ws}
        chosen_w = [w for w in ws if w_ok[w]]
        if len(chosen_w) == 1:
            w = chosen_w[0]
            lines.append(f"規則 2（行 9 の Rest 多数派 ≤ 5/10 になる体力項）: " + ", ".join(f"w {k}: {'満たす' if v else '満たさない'}" for k, v in w_ok.items()) + f" → w = {w}")
        else:
            w = 0 if 0 in ws else min(ws)
            lines.append(f"規則 2: " + ", ".join(f"w {k}: {'満たす' if v else '満たさない'}" for k, v in w_ok.items()) + f"（同点）→ w = {w}")
        cand = [c for c in cand if c[2] == w]
    hs = sorted({c[1] for c in cand})
    if len(hs) > 1:
        cand = [c for c in cand if c[1] == 2000]
        lines.append("規則 3（harm_cost 2,000）→ " + label(cand[0]))
    chosen = cand[0] if cand else None
    return chosen, lines


def check(name, ok, detail):
    print(f"- {name}: {'**支持**' if ok else '**反証**'} — {detail}")
    return ok


def identical(name_a, name_b, cols):
    a, b = series(name_a), series(name_b, EXP002)
    if a is None or b is None:
        return None
    diff = sum(1 for ra, rb in zip(a, b) if any(ra[c] != rb[c] for c in cols))
    return diff, abs(len(a) - len(b)), len(a)


def summary_identical(name_a, name_b):
    a, b = summary(name_a), read(os.path.join(EXP002, f"{name_b}_summary.csv"))
    if a is None or b is None:
        return None
    cols = [c for c in b[0].keys() if c not in ("ticks_per_s", "reflex_weight_drift")]
    diff = sum(1 for ra, rb in zip(a, b) if any(ra[c] != rb[c] for c in cols))
    return diff, len(cols)


def alive_at(name, tick):
    out = {}
    for r in series(name) or []:
        if int(r["tick"]) == tick:
            out[r["seed"]] = int(r["alive"])
    return out


def predictions(stats, fs, l128):
    print("\n### 予測の当否（`summarize.py` の判定）\n")
    # P0
    s_id = identical(cell_name(DEFAULT), "learner32", SERIES16)
    f_id = identical("forager", "forager", SERIES16)
    s_sum = summary_identical(cell_name(DEFAULT), "learner32")
    f_sum = summary_identical("forager", "forager")
    ok0 = all(x is not None for x in (s_id, f_id, s_sum, f_sum)) and s_id[0] == 0 and s_id[1] == 0 and f_id[0] == 0 and f_id[1] == 0 and s_sum[0] == 0 and f_sum[0] == 0
    check("P0 回帰", ok0, f"L(50, 2000, 0) vs EXP-002 learner32: 系列 16 列の不一致 {s_id and s_id[0]} 行 / {s_id and s_id[2]} 行、summary {s_sum and s_sum[1]} 列の不一致 {s_sum and s_sum[0]} 行; F vs EXP-002 forager: 系列の不一致 {f_id and f_id[0]} 行 / {f_id and f_id[2]} 行、summary の不一致 {f_sum and f_sum[0]} 行")
    # P1a monotone
    mono = []
    for h in HC:
        for w in HW:
            v = [stats[(e, h, w)]["violence"] for e in EPS if (e, h, w) in stats]
            mono.append((h, w, v, all(v[i] <= v[i + 1] for i in range(len(v) - 1))))
    check("P1a ε 単調", all(m[3] for m in mono), "; ".join(f"(h {h}, w {w}): " + " ≤ ".join(pct(x) for x in v) + ("" if ok else " ✗") for h, w, v, ok in mono))
    # P1b levels
    lv = {c: stats[c]["violence"] for c in CELLS if c in stats}
    ok_50 = all(0.35 <= lv[c] <= 0.50 for c in lv if c[0] == 50) and abs(lv.get(DEFAULT, math.nan) - 0.432) < 0.005
    ok_10 = all(lv[c] < 0.10 for c in lv if c[0] == 10)
    ok_0 = all(lv[c] <= 0.01 for c in lv if c[0] == 0)
    check("P1b 水準", ok_50 and ok_10 and ok_0, "ε 50: " + ", ".join(pct(lv[c]) for c in CELLS if c[0] == 50 and c in lv) + f" ({'35〜50% かつ既定 43.2%' if ok_50 else '外れ'}); ε 10: " + ", ".join(pct(lv[c]) for c in CELLS if c[0] == 10 and c in lv) + f" ({'< 10%' if ok_10 else '10% 以上あり'}); ε 0: " + ", ".join(pct(lv[c]) for c in CELLS if c[0] == 0 and c in lv) + f" ({'≤ 1%' if ok_0 else '1% 超あり'})")
    # P1c harm_cost
    pairs = []
    for e in EPS:
        for w in HW:
            a, b = (e, 2000, w), (e, 8000, w)
            if a in lv and b in lv:
                pairs.append((e, w, lv[b] - lv[a]))
    check("P1c harm_cost は効かない", all(abs(d) <= 0.03 for _, _, d in pairs), "; ".join(f"(ε {e}, w {w}): {100 * d:+.1f} pt" for e, w, d in pairs))
    # P1d harm act floor
    ok_d = all(stats[c]["harm_act_max"] <= 0.0025 for c in lv if c[0] == 10) and all(stats[c]["harm_act_max"] <= 0.001 for c in lv if c[0] == 0)
    check("P1d Harm の決定割合は床", ok_d, "ε 10 の最大 " + ", ".join(fmt(stats[c]["harm_act_max"], 5) for c in CELLS if c[0] == 10 and c in lv) + "（≤ 0.0025）; ε 0 の最大 " + ", ".join(fmt(stats[c]["harm_act_max"], 5) for c in CELLS if c[0] == 0 and c in lv) + "（≤ 0.001）")
    # P2
    e0 = [c for c in CELLS if c[0] == 0 and c in stats]
    check("P2a ε 0 の帯域", all(stats[c]["ext"] == 0 and stats[c]["mean"] >= BAND_MIN for c in e0), "; ".join(f"{label(c)}: 絶滅 {stats[c]['ext']}/10, 窓平均 {fmt(stats[c]['mean'])}" for c in e0))
    check("P2b 全セル生存", all(stats[c]["ext"] == 0 for c in lv), "絶滅: " + ", ".join(f"{label(c)} {stats[c]['ext']}/10" for c in CELLS if c in stats and stats[c]["ext"] > 0) if any(stats[c]["ext"] for c in lv) else "12 セルとも 0/10")
    # P3
    w2 = [c for c in CELLS if c[2] == 2 and c in stats]
    w0 = [c for c in CELLS if c[2] == 0 and c in stats]
    ok3 = all(stats[c]["row9_rest"] <= 3 for c in w2) and all(stats[c]["row9_rest"] >= 7 for c in w0)
    check("P3 体力項（D10 の予測）", ok3, "w = 2: " + ", ".join(f"{label(c)} {stats[c]['row9_rest']}/10" for c in w2) + "（≤ 3/10 で支持）; w = 0: " + ", ".join(f"{label(c)} {stats[c]['row9_rest']}/10" for c in w0) + "（≥ 7/10 で支持）")
    # P4
    if fs and DEFAULT in stats:
        fe = {r["seed"]: f(r, "win_encroach_rate") for r in fs["rows"]}
        le = {r["seed"]: f(r, "win_encroach_rate") for r in stats[DEFAULT]["rows"]}
        wins = sum(1 for s in fe if s in le and fe[s] > le[s])
        check("P4 E₀", fs["e0"][0] > stats[DEFAULT]["e0"][0] and wins >= 8, f"F {fmt(fs['e0'][0], 4)}（sd {fmt(fs['e0'][1], 4)}）vs L(50, 2000, 0) {fmt(stats[DEFAULT]['e0'][0], 4)}（sd {fmt(stats[DEFAULT]['e0'][1], 4)}）、同 seed で F > L は {wins}/10")
    # P5
    if l128:
        check("P5a L128 生存", l128["ext"] == 0 and l128["mean"] >= BAND_MIN, f"絶滅 {l128['ext']}/10、窓平均 {fmt(l128['mean'])}（F の {100 * l128['mean'] / F_BAND:.0f}%）")
        a, b = alive_at("learner128", 1000), alive_at(cell_name(DEFAULT), 1000)
        ratios = {s: a[s] / b[s] for s in a if s in b and b[s]}
        if ratios:
            m = st.mean(ratios.values())
            check("P5b L128 の回復", m >= 0.80, f"tick 1,000 の生存数の比の 10 シード平均 {m:.3f}、比 ≥ 0.80 のシード {sum(1 for v in ratios.values() if v >= 0.8)}/10（L128 {min(a.values())}〜{max(a.values())} 体、既定セル {min(b.values())}〜{max(b.values())} 体）")
    # P6
    chosen, _ = decide(stats)
    check("P6 preset", chosen is not None and chosen[0] == 10, f"決定規則の選択 {label(chosen) if chosen else '—'}")


def l128_section(stats):
    s = arm_stats("learner128")
    if not s:
        return None
    print("\n### L128（Learner 128 行・新ビット順、既定セル、副次腕）\n")
    per_seed_table("L128", "learner128", s["rows"])
    print(f"\n- 窓平均の 10 シード平均 **{fmt(s['mean'])}**（F の {100 * s['mean'] / F_BAND:.0f}%）、sd {fmt(s['sd'])}、CV {fmt(s['cv'], 3)}、絶滅 {s['ext']}/10、窓の加害死 {pct(s['violence'])}、Harm の決定割合 {fmt(s['harm_act'], 5)}、E₀ {fmt(s['e0'][0], 4)}、食料 {fmt(s['food'], 0)}、再生 {fmt(s['regrown'], 1)}/t。")
    qp = qpolicy("learner128")
    if qp:
        m9, m41 = majority(qp, ROW9), majority(qp, ROW9_OCC)
        print(f"- 行 9（occupied 0）の Rest 多数派 {rest_seeds(m9)}/10: " + " ".join((v[0] if v else 'dead') for _, v in sorted(m9.items(), key=lambda kv: int(kv[0]))) + f"; 行 41（occupied 1）の Rest 多数派 {rest_seeds(m41)}/10: " + " ".join((v[0] if v else 'dead') for _, v in sorted(m41.items(), key=lambda kv: int(kv[0]))))
        # Rows where OCCUPIED changes the greedy majority
        maj = collections.defaultdict(dict)
        for row in range(128):
            for seed, v in majority(qp, row).items():
                maj[row][seed] = v
        occ_rows = []
        for row in range(32):
            base, occ = maj.get(row, {}), maj.get(row + 32, {})
            agree = sum(1 for sd in base if sd in occ and base[sd] and occ[sd] and base[sd][0] == occ[sd][0])
            visited = sum(1 for sd in base if sd in occ and base[sd] and occ[sd] and base[sd][0] != 'untouched' and occ[sd][0] != 'untouched')
            if visited:
                occ_rows.append(f"行 {row}: 一致 {agree}/{visited}")
        print("- `OCCUPIED` が 0 と 1 で貪欲行動の多数派が一致するシード数（両方が訪問済みの seed のみ）: " + "; ".join(occ_rows))
    a, b = alive_at("learner128", 1000), alive_at(cell_name(DEFAULT), 1000)
    print("- tick 1,000 の生存数（seed 0〜9）: L128 " + " ".join(str(a[s]) for s in sorted(a, key=int)) + "; 既定セル " + " ".join(str(b[s]) for s in sorted(b, key=int)))
    lo, hi = {}, {}
    for r in series("learner128") or []:
        t, al = int(r["tick"]), int(r["alive"])
        if t <= 3000:
            if r["seed"] not in lo or al < lo[r["seed"]][1]:
                lo[r["seed"]] = (t, al)
    print("- tick 3,000 までの谷（seed: tick, 体）: " + ", ".join(f"{s}: {t}, {al}" for s, (t, al) in sorted(lo.items(), key=lambda kv: int(kv[0]))))
    return s


def main():
    stats = {}
    for c in CELLS:
        s = arm_stats(cell_name(c))
        if s:
            stats[c] = s
    grid_table(stats)
    row9_table(stats)
    print("\n### 行動構成（決定の割合、窓、10 シード平均）\n")
    print("| (ε, h, w) | " + " | ".join(KINDS) + " |")
    print("|---|" + "---|" * len(KINDS))
    for c in CELLS:
        if c in stats:
            print(f"| {label(c)} | " + " | ".join(f"{st.mean(f(r, 'win_act_' + k) for r in stats[c]['rows']):.4f}" for k in KINDS) + " |")
    fs = arm_stats("forager")
    if fs:
        print("\n### F（Forager、`nomos` オン）\n")
        per_seed_table("F", "forager", fs["rows"])
        print(f"\n- 窓平均の 10 シード平均 **{fmt(fs['mean'])}**、sd {fmt(fs['sd'])}、CV {fmt(fs['cv'], 3)}、絶滅 {fs['ext']}/10。E₀ {fmt(fs['e0'][0], 4)}（sd {fmt(fs['e0'][1], 4)}）、食料 {fmt(fs['food'], 0)}、再生 {fmt(fs['regrown'], 1)}/t。")
    l128 = l128_section(stats)
    print("\n### 決定規則の適用\n")
    chosen, lines = decide(stats)
    for ln in lines:
        print(f"- {ln}")
    if chosen:
        print(f"- **`nomos_preset` = ε {chosen[0]}‰, `harm_cost` {chosen[1]}, `learner_health_weight` {chosen[2]}**")
    predictions(stats, fs, l128)
    print("\n### 付録: 主格子のシードごとの表\n")
    for c in CELLS:
        if c in stats:
            per_seed_table(label(c), cell_name(c), stats[c]["rows"])


if __name__ == "__main__":
    main()
