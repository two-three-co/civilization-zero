//! `cz sweep` through `cz-lab` writes what it always wrote, `cz lab run`
//! writes the same rows under a design (whatever `--jobs`), `RUN.json` and
//! `cz lab summarize` come out, and `cz run` still builds the institution
//! world of 4-9 (ADR-0015 5-1).

use std::path::{Path, PathBuf};
use std::process::Command;

fn cz() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cz"))
}

fn run(args: &[&str]) -> String {
    let out = cz().args(args).output().expect("cz runs");
    assert!(
        out.status.success(),
        "cz {} failed:\n{}\n{}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cz-lab-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A CSV without its `ticks_per_s` column (the one wall-clock column).
fn without_ticks_per_s(csv: &str) -> String {
    let mut lines = csv.lines();
    let header: Vec<&str> = lines.next().unwrap().split(',').collect();
    let keep: Vec<usize> = header
        .iter()
        .enumerate()
        .filter(|(_, c)| **c != "ticks_per_s")
        .map(|(i, _)| i)
        .collect();
    let mut out = String::new();
    for line in std::iter::once(header.join(",").as_str()).chain(lines) {
        let f: Vec<&str> = line.split(',').collect();
        out.push_str(&keep.iter().map(|&i| f[i]).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

const DESIGN: &str = r#"{
  "format": 1,
  "name": "cli-test",
  "base": { "policy": "institution", "width": 32, "height": 32, "agents": 100, "patches": 2 },
  "ticks": 300, "window_pct": 50, "sample": 50,
  "seeds": { "start": 0, "count": 2 },
  "cells": [
    { "name": "le", "schedule": [ { "tick": 100, "intervention": { "op": "enact",
        "norm": { "kind": "encroach", "standing_ticks": 32, "sanction": 1, "except_sanction": false } } } ] },
    { "name": "le0" }
  ],
  "contrasts": [
    { "name": "le − le0 · E_L", "treat": "le", "control": "le0",
      "metric": { "ratio": { "num": "encroachments_learner", "den": "gathers_learner" } } },
    { "name": "le − le0 · win_mean", "treat": "le", "control": "le0", "metric": { "summary": "win_mean" } }
  ]
}"#;

#[test]
fn sweep_lab_run_and_summarize_agree() {
    let dir = scratch("agree");
    let design = dir.join("design.json");
    std::fs::write(&design, DESIGN).unwrap();
    let design = design.to_str().unwrap();
    let sweep = dir.join("sweep");
    let out = run(&[
        "sweep",
        "--seeds",
        "2",
        "--ticks",
        "300",
        "--side",
        "32",
        "--agents",
        "100",
        "--patches",
        "2",
        "--sample",
        "50",
        "--window",
        "50",
        "--policy",
        "institution",
        "--enact",
        "encroach",
        "--enact-at",
        "100",
        "--out",
        sweep.to_str().unwrap(),
    ]);
    assert!(
        out.contains("window = last 50% of 300 ticks; across 2 seeds"),
        "{out}"
    );
    assert!(
        out.contains("wrote") && out.contains("_qpolicy.csv"),
        "{out}"
    );

    let lab2 = dir.join("lab2");
    let out = run(&[
        "lab",
        "run",
        design,
        "--out",
        lab2.to_str().unwrap(),
        "--jobs",
        "2",
    ]);
    assert!(
        out.contains("cz lab run cli-test: 4 worlds × 300 ticks (2 cells), jobs 2"),
        "{out}"
    );
    assert!(out.contains("wrote") && out.contains("RUN.json"), "{out}");
    let lab1 = dir.join("lab1");
    run(&[
        "lab",
        "run",
        design,
        "--out",
        lab1.to_str().unwrap(),
        "--jobs",
        "1",
    ]);

    // The sweep's cell is the design's `le`: same rows, bar the clock.
    for kind in ["summary", "series", "qpolicy"] {
        let a = without_ticks_per_s(&read(&dir.join(format!("sweep_{kind}.csv"))));
        let b = without_ticks_per_s(&read(&lab2.join(format!("le_{kind}.csv"))));
        let c = without_ticks_per_s(&read(&lab1.join(format!("le_{kind}.csv"))));
        assert_eq!(a, b, "{kind}: sweep vs lab run --jobs 2");
        assert_eq!(b, c, "{kind}: --jobs 2 vs --jobs 1");
        let d = without_ticks_per_s(&read(&lab2.join(format!("le0_{kind}.csv"))));
        let e = without_ticks_per_s(&read(&lab1.join(format!("le0_{kind}.csv"))));
        assert_eq!(d, e, "{kind}: le0 --jobs 2 vs --jobs 1");
        if kind == "series" {
            assert_ne!(b, d, "the decree changes the ruled cell's series");
        }
    }
    // Seeds in order whatever the thread count.
    let summary = read(&lab2.join("le_summary.csv"));
    let seeds: Vec<&str> = summary
        .lines()
        .skip(1)
        .map(|l| l.split(',').next().unwrap())
        .collect();
    assert_eq!(seeds, ["0", "1"]);

    // The manifest.
    let m2: serde_json::Value = serde_json::from_str(&read(&lab2.join("RUN.json"))).unwrap();
    let m1: serde_json::Value = serde_json::from_str(&read(&lab1.join("RUN.json"))).unwrap();
    assert_eq!(m2["lab_result"], 1);
    assert_eq!(m2["design"]["name"], "cli-test");
    assert_eq!(m2["jobs"], 2);
    assert_eq!(m1["jobs"], 1);
    let worlds2 = m2["worlds"].as_array().unwrap();
    let worlds1 = m1["worlds"].as_array().unwrap();
    assert_eq!(worlds2.len(), 4);
    for (a, b) in worlds2.iter().zip(worlds1) {
        assert_eq!(a["cell"], b["cell"]);
        assert_eq!(a["seed"], b["seed"]);
        assert_eq!(a["ticks"], 300);
        assert_eq!(a["dynamics_hash"], b["dynamics_hash"], "{a}");
        assert!(a["dynamics_hash"].as_str().unwrap().starts_with("0x"));
    }
    assert!(m2["started"].as_str().unwrap().ends_with('Z'));
    assert!(m2["environment"]["cores"].as_u64().unwrap() >= 1);

    // The summary.
    let out = run(&[
        "lab",
        "summarize",
        lab2.to_str().unwrap(),
        "--design",
        design,
    ]);
    assert!(out.contains("## Contrasts"), "{out}");
    assert!(out.contains("le − le0 · E_L"), "{out}");
    let s: serde_json::Value = serde_json::from_str(&read(&lab2.join("SUMMARY.json"))).unwrap();
    assert_eq!(s["lab_summary"], 1);
    assert_eq!(s["cells"].as_array().unwrap().len(), 2);
    let contrasts = s["contrasts"].as_array().unwrap();
    assert_eq!(contrasts.len(), 2);
    assert_eq!(contrasts[0]["n"], 2);
    assert_eq!(contrasts[0]["window"], serde_json::json!([150, 300]));
    assert!(read(&lab2.join("SUMMARY.md")).starts_with("# cli-test"));

    // A selection runs a part and records it.
    let part = dir.join("part");
    let out = run(&[
        "lab",
        "run",
        design,
        "--out",
        part.to_str().unwrap(),
        "--cells",
        "le0",
        "--seeds",
        "1..2",
    ]);
    assert!(out.contains("1 worlds × 300 ticks"), "{out}");
    assert!(!part.join("le_summary.csv").exists());
    let summary = read(&part.join("le0_summary.csv"));
    assert_eq!(summary.lines().count(), 2);
    assert!(summary
        .lines()
        .nth(1)
        .unwrap()
        .starts_with("1,institution,"));
    let m: serde_json::Value = serde_json::from_str(&read(&part.join("RUN.json"))).unwrap();
    assert_eq!(m["selection"]["cells"], serde_json::json!(["le0"]));
    assert_eq!(m["selection"]["seeds"], serde_json::json!([1, 2]));
    // Summarising the part leaves out the cell without files.
    let out = run(&[
        "lab",
        "summarize",
        part.to_str().unwrap(),
        "--design",
        design,
    ]);
    assert!(out.contains("left out"), "{out}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 4-9's record: `cz run --policy institution --seed 42 --patches 24
/// --enact encroach --enact-at 100 --ticks 200` reaches `state_hash`
/// 0x4b945b8bade819ee — `build_world` moved to `cz-lab` without changing
/// the world it builds.
#[test]
fn cz_run_still_builds_the_institution_world_of_4_9() {
    let out = run(&[
        "run",
        "--policy",
        "institution",
        "--seed",
        "42",
        "--patches",
        "24",
        "--enact",
        "encroach",
        "--enact-at",
        "100",
        "--ticks",
        "200",
    ]);
    assert!(
        out.contains("final tick 200 hash 0x4b945b8bade819ee"),
        "{out}"
    );
}

#[test]
fn lab_usage_and_bad_designs_are_errors() {
    let out = cz().args(["lab"]).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("usage"));
    let dir = scratch("bad");
    let bad = dir.join("bad.json");
    std::fs::write(&bad, DESIGN.replace("\"patches\": 2", "\"patchs\": 2")).unwrap();
    let out = cz()
        .args([
            "lab",
            "run",
            bad.to_str().unwrap(),
            "--out",
            dir.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("patchs"));
    let _ = std::fs::remove_dir_all(&dir);
}
