#!/usr/bin/env python3
"""Regenerates the tables of EXP-002-learning-vs-fixed.md from the CSVs in
this directory (pre-registered analysis; run with `python3 -I summarize.py`).

Reads <arm>_summary.csv (one row per seed, written by `cz sweep --out`) and,
for the identity checks P2 / P3, the series files. Prints Markdown."""
import csv
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
EXP001 = os.path.join(HERE, "..", "EXP-001", "rerun-two-pass-scan")
ARMS = [
    ("R", "random"),
    ("F", "forager"),
    ("X0", "reflex_nomut"),
    ("X1", "reflex"),
    ("L32", "learner32"),
    ("L128", "learner128"),
    ("X1-50k", "reflex50k"),
]
FLOOR2 = 0.0125  # twice the exploration floor (epsilon 50 permille / 8 kinds)


def rows(name):
    path = os.path.join(HERE, f"{name}_summary.csv")
    if not os.path.exists(path):
        return None
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def f(r, k):
    v = r.get(k, "")
    return float(v) if v not in ("", None) else math.nan


def mean_sd(xs):
    xs = [x for x in xs if not math.isnan(x)]
    if not xs:
        return math.nan, math.nan
    m = sum(xs) / len(xs)
    sd = math.sqrt(sum((x - m) ** 2 for x in xs) / len(xs))
    return m, sd


def fmt(x, d=1):
    return "—" if x is None or (isinstance(x, float) and math.isnan(x)) else f"{x:.{d}f}"


def arm_table(label, name, rs):
    print(f"\n### {label} (`{name}`)\n")
    print("| seed | 最終 | 絶滅 tick | 窓平均 | 窓 min | 窓 max | max/min | 出生 | 餓死 | 老衰 | 加害死 | 摂取/体·tick | 移転/t | 加害/t | 発信/t | 決定: Harm | Transfer | Signal | Rest | Move | \\|Q\\| | ΔQ 窓 | 重み漂移 | hunger | tick/s |")
    print("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for r in rs:
        print("| {seed} | {fin} | {ext} | {wm} | {wmin} | {wmax} | {ratio} | {b} | {ds} | {da} | {dv} | {cons} | {xf} | {hm} | {sg} | {ah} | {at} | {asg} | {ar} | {am} | {q} | {dq} | {dr} | {hu} | {tps} |".format(
            seed=r["seed"], fin=r["final_alive"], ext=r["extinct_at"], wm=fmt(f(r, "win_mean")),
            wmin=r["win_min"], wmax=r["win_max"], ratio=fmt(f(r, "win_max_over_min"), 3),
            b=r["births"], ds=r["deaths_starvation"], da=r["deaths_old_age"], dv=r.get("deaths_violence", ""),
            cons=fmt(f(r, "win_consumed_per_agent_tick"), 4), xf=fmt(f(r, "win_transfers_per_tick"), 2),
            hm=fmt(f(r, "win_harms_per_tick"), 2), sg=fmt(f(r, "win_signals_per_tick"), 2),
            ah=fmt(f(r, "win_act_harm"), 4), at=fmt(f(r, "win_act_transfer"), 4), asg=fmt(f(r, "win_act_signal"), 4),
            ar=fmt(f(r, "win_act_rest"), 3), am=fmt(f(r, "win_act_move"), 3),
            q=fmt(f(r, "q_abs_mean"), 2), dq=fmt(f(r, "q_delta_window"), 2), dr=fmt(f(r, "reflex_weight_drift"), 3),
            hu=fmt(f(r, "hunger_permille_mean"), 1), tps=fmt(f(r, "ticks_per_s"), 0)))
    wm = [f(r, "win_mean") for r in rs]
    m, sd = mean_sd(wm)
    ext = sum(1 for r in rs if r["extinct_at"] != "-")
    dv = sum(int(r.get("win_deaths_violence", 0) or 0) for r in rs)
    ds = sum(int(r.get("win_deaths_starvation", 0) or 0) for r in rs)
    da = sum(int(r.get("win_deaths_old_age", 0) or 0) for r in rs)
    tot = max(1, dv + ds + da)
    print(f"\n- 窓平均の 10 シード平均 **{fmt(m)}**、sd {fmt(sd)}、CV **{fmt(sd / m if m else math.nan, 3)}**、絶滅 **{ext}/{len(rs)}**、"
          f"max/min の最大 {fmt(max(f(r, 'win_max_over_min') for r in rs), 3)}。"
          f"窓の死因: 餓死 {ds} ({100 * ds / tot:.1f}%)、老衰 {da} ({100 * da / tot:.1f}%)、加害死 {dv} ({100 * dv / tot:.1f}%)。")
    for k, label_k in [("win_consumed_per_agent_tick", "摂取/体·tick"), ("win_act_move", "Move 割合"), ("win_act_rest", "Rest 割合"),
                       ("win_act_harm", "Harm 割合"), ("win_act_transfer", "Transfer 割合"), ("win_act_signal", "Signal 割合"),
                       ("win_harms_per_tick", "加害/t"), ("win_transfers_per_tick", "移転/t"), ("win_signals_per_tick", "発信/t"),
                       ("q_abs_mean", "|Q|"), ("q_delta_window", "ΔQ 窓"), ("reflex_weight_drift", "重み漂移"), ("hunger_permille_mean", "hunger"),
                       ("ticks_per_s", "tick/s")]:
        mm, ss = mean_sd([f(r, k) for r in rs])
        if not math.isnan(mm):
            print(f"- {label_k}: 平均 {fmt(mm, 4)}（sd {fmt(ss, 4)}）")
    return m, sd, ext


def paired(a, b):
    """Seeds where arm a's win_mean > arm b's, matched by seed."""
    bm = {r["seed"]: f(r, "win_mean") for r in b}
    return sum(1 for r in a if r["seed"] in bm and f(r, "win_mean") > bm[r["seed"]])


def series_identical(name_a, name_b, dir_b=HERE, max_tick=None):
    """True if (seed, tick, alive) rows agree for every tick < max_tick."""
    def load(path):
        out = {}
        with open(path, newline="") as fh:
            for r in csv.DictReader(fh):
                t = int(r["tick"])
                if max_tick is None or t < max_tick:
                    out[(r["seed"], t)] = r["alive"]
        return out
    pa = os.path.join(HERE, f"{name_a}_series.csv")
    pb = os.path.join(dir_b, f"{name_b}_series.csv")
    if not (os.path.exists(pa) and os.path.exists(pb)):
        return None
    a, b = load(pa), load(pb)
    diff = [k for k in a if k in b and a[k] != b[k]]
    missing = [k for k in b if k not in a]
    return (len(diff), len(missing), len(a))


def main():
    data = {label: rows(name) for label, name in ARMS}
    stats = {}
    for label, name in ARMS:
        if data[label]:
            stats[label] = arm_table(label, name, data[label])
    print("\n### 腕の比較\n")
    print("| 腕 | 窓平均の平均 | sd | CV | 絶滅 | 同 seed で F より多いシード数 | 同 seed で L32 より多い |")
    print("|---|---|---|---|---|---|---|")
    for label, _ in ARMS:
        if label in stats:
            m, sd, ext = stats[label]
            vs_f = paired(data[label], data["F"]) if data.get("F") else None
            vs_l = paired(data[label], data["L32"]) if data.get("L32") else None
            print(f"| {label} | {fmt(m)} | {fmt(sd)} | {fmt(sd / m if m else math.nan, 3)} | {ext}/{len(data[label])} | {vs_f if vs_f is not None else '—'}/10 | {vs_l if vs_l is not None else '—'}/10 |")
    # Identity checks (P2, P3).
    r2 = series_identical("forager", "rerun_forager", EXP001, max_tick=20000)
    if r2:
        print(f"\nP2: F の系列 vs EXP-001 再実行: 不一致 {r2[0]} 行、欠落 {r2[1]} 行（比較 {r2[2]} 行）")
    r3 = series_identical("reflex_nomut", "forager", max_tick=None)
    if r3:
        print(f"P3: X0 の系列 vs F: 不一致 {r3[0]} 行、欠落 {r3[1]} 行（比較 {r3[2]} 行）")
    # Floors (P5d, P5d', P5f).
    for label in ("L32", "L128"):
        rs = data.get(label)
        if not rs:
            continue
        over = {k: sum(1 for r in rs if f(r, k) > FLOOR2) for k in ("win_act_transfer", "win_act_harm", "win_act_signal")}
        print(f"{label}: 決定割合が床の 2 倍 ({FLOOR2}) を超えるシード数 — Transfer {over['win_act_transfer']}, Harm {over['win_act_harm']}, Signal {over['win_act_signal']}")
        ratios = [f(r, "q_delta_window") / f(r, "q_abs_mean") for r in rs if f(r, "q_abs_mean") > 0]
        if ratios:
            print(f"{label}: ΔQ/|Q| の最大 {max(ratios):.3f}、平均 {sum(ratios) / len(ratios):.3f}（P5g の基準 0.25）")
        # Greedy-majority rows per kind from the qpolicy file.
        qp = os.path.join(HERE, f"{ARMS[[a for a, _ in ARMS].index(label)][1]}_qpolicy.csv")
        if os.path.exists(qp):
            kinds = ["move", "gather", "consume", "rest", "transfer", "store", "harm", "signal", "tie"]
            maj = {k: 0 for k in kinds}
            visited = 0
            with open(qp, newline="") as fh:
                for r in csv.DictReader(fh):
                    counts = {k: int(r[k]) for k in kinds}
                    learners = int(r["learners"])
                    if learners and counts["tie"] < learners:  # some learner has a preference in this row
                        visited += 1
                        best = max(kinds[:-1], key=lambda k: counts[k])
                        maj[best] += 1
            print(f"{label}: qpolicy — 好みのある行（seed × 行）{visited}、多数派の種類別の行数: " + ", ".join(f"{k} {v}" for k, v in maj.items() if k != "tie"))


if __name__ == "__main__":
    main()
