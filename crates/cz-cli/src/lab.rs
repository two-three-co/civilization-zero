//! `cz lab` and the bridge from the command line to the `cz-lab` crate
//! (ADR-0015 D2).
//!
//! - [`design_from_opts`] turns the flags of `cz run` / `cz sweep` / `cz
//!   fork` into a one-cell [`Design`]: the policy mix, the preset, the grid
//!   and founders, and every flag given explicitly as a `WorldConfig`
//!   override (so a flag left out keeps the base's value, as always).
//!   [`build_world`] is what the three commands call.
//! - [`cmd_sweep`] is `cz sweep`: the design's one cell, run seed by seed,
//!   the same console lines and the same `PREFIX_{summary,series,qpolicy}.csv`.
//! - [`cmd_lab`] is `cz lab run DESIGN.json --out DIR [--jobs N] [--cells
//!   a,b] [--seeds S..E] [--check] [--agents]` — every (cell, seed) of the
//!   design (or the selection) on `N` threads, the CSVs written in seed
//!   order whatever the thread count, `RUN.json` with the environment and
//!   one record per world, and with `--agents` the per-agent table
//!   `<cell>_agents.csv` (ADR-0015 D4, 5-2; meant for short runs) — and
//!   `cz lab summarize DIR --design DESIGN.json`, which reads the CSVs
//!   back and writes `SUMMARY.json` / `SUMMARY.md` (per-cell
//!   distributions, the paired contrasts).
//!
//! Worlds run on plain scoped threads pulling from a shared index: the
//! kernel needs no coordination between worlds, so the result does not
//! depend on the thread count, only the console order of the per-seed
//! lines does.

use crate::{decrees, get, parse_charter, parse_opts, Opts, R};
use cz_lab::{
    now_iso8601, run_cell, summarize, Base, Cell, CellRun, CellTables, Design, Environment,
    Manifest, Rows, RunOptions, Scheduled, Seeds, Selection, Table, WorldRecord, AGENTS_COLUMNS,
    DESIGN_FORMAT, LAB_RESULT_FORMAT, QPOLICY_COLUMNS, SERIES_COLUMNS, SUMMARY_COLUMNS,
};
use serde_json::{json, Map, Value};
use simulation_core::World;
use std::collections::BTreeMap;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

type Res<T> = Result<T, Box<dyn std::error::Error>>;

const LAB_USAGE: &str = "usage:\n  cz lab run DESIGN.json --out DIR [--jobs N] [--cells a,b] [--seeds S..E | S..=E | S] [--check] [--agents]\n  cz lab summarize DIR --design DESIGN.json";

/// Beyond this many ticks `--agents` only warns: ADR-0015 D4 means the
/// per-agent table for short runs (a 20,000-tick arm of 30 seeds ≈ 60 MB).
const AGENTS_TICKS_ADVISED: u64 = 5_000;

/// A flag's value parsed as `T`, or `None` when the flag is absent.
fn flag<T: std::str::FromStr>(o: &Opts, key: &str) -> Res<Option<T>> {
    match o.get(key) {
        None => Ok(None),
        Some(v) => v
            .parse::<T>()
            .map(Some)
            .map_err(|_| format!("bad value for --{key}: {v}").into()),
    }
}

/// The flags as a one-cell design. Only a flag that was given becomes a
/// config override, so what is left out keeps the base config's value
/// (the preset's, or what the policy mix implies). `with_schedule` puts
/// `--enact` / `--repeal` (at `--enact-at` / `--repeal-at`) on the cell's
/// schedule; `cz run` and `cz fork` apply their decrees themselves.
pub(crate) fn design_from_opts(o: &Opts, cell: &str, with_schedule: bool) -> Res<Design> {
    let side: u16 = get(o, "side", 128)?;
    let policy = o
        .get("policy")
        .cloned()
        .unwrap_or_else(|| "forager".to_string());
    let mut config = Map::new();
    if let Some(v) = flag::<i64>(o, "harm-cost")? {
        config.insert("harm_cost".into(), json!(v));
    }
    if let Some(v) = flag::<u16>(o, "learner-states")? {
        config.insert("learner_states".into(), json!(v));
    }
    if let Some(v) = flag::<u16>(o, "learner-epsilon")? {
        config.insert("learner_epsilon_permille".into(), json!(v));
    }
    if let Some(v) = flag::<u16>(o, "learner-health-weight")? {
        config.insert("learner_health_weight".into(), json!(v));
    }
    if o.contains_key("nomos") {
        config.insert("nomos".into(), json!(true));
    }
    if o.contains_key("record-violations") {
        config.insert("record_violations".into(), json!(true));
    }
    if let Some(v) = flag::<i64>(o, "motion-cost")? {
        config.insert("motion_cost".into(), json!(v));
    }
    if o.contains_key("no-enforcer-motions") {
        config.insert("enforcer_motions".into(), json!(false));
    }
    if o.contains_key("record-votes") {
        config.insert("record_votes".into(), json!(true));
    }
    if let Some(spec) = o.get("charter") {
        config.insert(
            "charter".into(),
            serde_json::to_value(parse_charter(spec)?)?,
        );
    }
    match o.get("enforcer-share").map(String::as_str) {
        // `none`: every child its parent's kind (Phase 3's rule).
        Some("none") => {
            config.insert("enforcer_share_permille".into(), Value::Null);
        }
        Some(v) => {
            let p: u16 = v
                .parse()
                .map_err(|_| format!("bad value for --enforcer-share: {v}"))?;
            config.insert("enforcer_share_permille".into(), json!(p));
        }
        None => {}
    }
    if let Some(v) = flag::<u8>(o, "memory-slots")? {
        config.insert("memory_slots".into(), json!(v));
    }
    let mut perception = Map::new();
    if let Some(v) = flag::<u16>(o, "signal-radius")? {
        perception.insert("signal_radius".into(), json!(v));
    }
    if o.contains_key("blind") {
        perception.insert("see_standing".into(), json!(false));
    }
    if !perception.is_empty() {
        config.insert("perception".into(), Value::Object(perception));
    }
    // Evolving policies mutate by default (ADR-0013 D6); `--mutate` wins
    // over `--no-mutate`.
    if o.contains_key("mutate") {
        config.insert("mutation_enabled".into(), json!(true));
    } else if o.contains_key("no-mutate") {
        config.insert("mutation_enabled".into(), json!(false));
    }
    if o.contains_key("record-actions") {
        config.insert("record_actions".into(), json!(true));
    }
    let schedule = if with_schedule {
        decrees(o)?
            .into_iter()
            .map(|(tick, intervention)| Scheduled { tick, intervention })
            .collect()
    } else {
        Vec::new()
    };
    let design = Design {
        format: DESIGN_FORMAT,
        name: cell.to_string(),
        notes: None,
        base: Base {
            policy,
            preset: o.get("preset").cloned(),
            width: Some(side),
            height: Some(side),
            agents: Some(get(o, "agents", 1_000)?),
            patches: Some(get(
                o,
                "patches",
                ((side as u32 * side as u32) / 700).max(1),
            )?),
            config: if config.is_empty() {
                Value::Null
            } else {
                Value::Object(config)
            },
        },
        // `cz run` / `cz fork` keep their own tick counts; a design only
        // needs a positive one to be well-formed.
        ticks: if with_schedule {
            get(o, "ticks", 20_000)?
        } else {
            1
        },
        window_pct: get::<u64>(o, "window", 50)?.min(100),
        sample: get::<u64>(o, "sample", 100)?.max(1),
        seeds: Seeds {
            start: get(o, "seed-start", 0)?,
            count: get(o, "seeds", 10)?,
        },
        cells: vec![Cell {
            name: cell.to_string(),
            policy: None,
            preset: None,
            config: Value::Null,
            schedule,
            seeds: None,
            sample: None,
        }],
        contrasts: Vec::new(),
    };
    design.validate()?;
    Ok(design)
}

/// The world the flags describe, at tick 0 with nothing applied.
pub(crate) fn build_world(seed: u64, o: &Opts) -> Res<World> {
    let design = design_from_opts(o, "run", false)?;
    Ok(design.build_world(&design.cells[0], seed)?)
}

/// How a cell's files are named: `cz sweep --out PREFIX` writes
/// `PREFIX_summary.csv`; `cz lab run --out DIR` writes
/// `DIR/<cell>_summary.csv`.
pub(crate) enum Naming {
    Prefix(String),
    Dir(String),
}

impl Naming {
    fn prefix(&self, cell: &str) -> String {
        match self {
            Naming::Prefix(p) => p.clone(),
            Naming::Dir(d) => format!("{d}/{cell}"),
        }
    }
}

fn write_csv(path: &str, header: &str, rows: &[String]) -> Res<()> {
    let mut s =
        String::with_capacity(header.len() + 1 + rows.iter().map(|r| r.len() + 1).sum::<usize>());
    s.push_str(header);
    s.push('\n');
    for r in rows {
        s.push_str(r);
        s.push('\n');
    }
    std::fs::write(path, s)?;
    Ok(())
}

/// Runs every world of the selection on `jobs` threads, prints `cz
/// sweep`'s lines, writes each cell's CSVs (if `naming`), and returns the
/// manifest. `label_cells` puts the cell's name in front of each line
/// (several cells run at once, so the lines interleave).
pub(crate) fn run_design(
    design: &Design,
    selection: &Selection,
    jobs: usize,
    options: RunOptions,
    naming: Option<&Naming>,
    label_cells: bool,
) -> Res<Manifest> {
    let list = design.jobs(selection.cells.as_deref(), selection.seeds)?;
    if list.is_empty() {
        return Err("the selection covers no world".into());
    }
    let mut environment = environment();
    let started = now_iso8601();
    let t0 = Instant::now();
    let check = options.check;
    let label = |cell: &str| {
        if label_cells {
            format!("{cell:<12} ")
        } else {
            String::new()
        }
    };
    println!("{}{}", label("cell"), CellRun::console_header());
    type Slot = Mutex<Option<Result<(CellRun, Rows), cz_lab::Error>>>;
    let results: Vec<Slot> = (0..list.len()).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let workers = jobs.max(1).min(list.len());
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= list.len() {
                    break;
                }
                let (ci, seed) = list[i];
                let cell = &design.cells[ci];
                let mut rows = Rows::default();
                let res = run_cell(design, cell, seed, &options, &mut rows);
                if let Ok(run) = &res {
                    println!("{}{}", label(&cell.name), run.console_line());
                }
                *results[i].lock().expect("no poisoned slot") = Some(res.map(|r| (r, rows)));
            });
        }
    });
    let mut worlds = Vec::with_capacity(list.len());
    let mut i = 0;
    for (ci, cell) in design.cells.iter().enumerate() {
        let mut rows = Rows::default();
        let mut runs: Vec<CellRun> = Vec::new();
        while i < list.len() && list[i].0 == ci {
            let slot = results[i].lock().expect("no poisoned slot").take();
            let (run, r) = slot
                .expect("every job ran")
                .map_err(|e| format!("cell {:?} seed {}: {e}", cell.name, list[i].1))?;
            rows.summary.extend(r.summary);
            rows.series.extend(r.series);
            rows.qpolicy.extend(r.qpolicy);
            rows.agents.extend(r.agents);
            runs.push(run);
            i += 1;
        }
        if runs.is_empty() {
            continue;
        }
        let n = runs.len() as f64;
        let grand = runs.iter().map(|r| r.win_mean).sum::<f64>() / n;
        let sd = (runs
            .iter()
            .map(|r| (r.win_mean - grand).powi(2))
            .sum::<f64>()
            / n)
            .sqrt();
        let extinctions = runs.iter().filter(|r| r.extinct_at.is_some()).count();
        println!(
            "{}window = last {}% of {} ticks; across {} seeds: mean of window means {grand:.1}, sd {sd:.1}, cv {:.3}, extinct {extinctions}/{}",
            label(&cell.name),
            design.window_pct,
            design.ticks,
            runs.len(),
            if grand > 0.0 { sd / grand } else { f64::NAN },
            runs.len()
        );
        if let Some(naming) = naming {
            let prefix = naming.prefix(&cell.name);
            write_csv(
                &format!("{prefix}_summary.csv"),
                SUMMARY_COLUMNS,
                &rows.summary,
            )?;
            write_csv(
                &format!("{prefix}_series.csv"),
                SERIES_COLUMNS,
                &rows.series,
            )?;
            let mut wrote = format!("wrote {prefix}_summary.csv and {prefix}_series.csv");
            if !rows.qpolicy.is_empty() {
                write_csv(
                    &format!("{prefix}_qpolicy.csv"),
                    QPOLICY_COLUMNS,
                    &rows.qpolicy,
                )?;
                wrote.push_str(&format!(" and {prefix}_qpolicy.csv"));
            }
            if !rows.agents.is_empty() {
                write_csv(
                    &format!("{prefix}_agents.csv"),
                    AGENTS_COLUMNS,
                    &rows.agents,
                )?;
                wrote.push_str(&format!(" and {prefix}_agents.csv"));
            }
            println!("{}{wrote}", label(&cell.name));
        }
        worlds.extend(runs.iter().map(WorldRecord::of));
    }
    environment.load_at_end = load();
    Ok(Manifest {
        lab_result: LAB_RESULT_FORMAT,
        design: design.clone(),
        selection: selection.clone(),
        environment,
        jobs: workers,
        check,
        started,
        finished: now_iso8601(),
        wall_ms: t0.elapsed().as_millis() as u64,
        worlds,
    })
}

/// `cz sweep`: the flags' one cell, every seed, the files under `--out`.
pub(crate) fn cmd_sweep(o: &Opts) -> R {
    let design = design_from_opts(o, "sweep", true)?;
    let jobs: usize = get(o, "jobs", 1)?;
    let naming = o.get("out").map(|p| Naming::Prefix(p.clone()));
    run_design(
        &design,
        &Selection::default(),
        jobs,
        RunOptions {
            check: o.contains_key("check"),
            agents: false,
        },
        naming.as_ref(),
        false,
    )?;
    Ok(())
}

/// `cz lab run | summarize`.
pub(crate) fn cmd_lab(args: &[String]) -> R {
    let (sub, path, rest) = match args {
        [sub, path, rest @ ..] if !path.starts_with("--") => (sub.as_str(), path.as_str(), rest),
        _ => return Err(LAB_USAGE.into()),
    };
    let o = parse_opts(rest);
    match sub {
        "run" => lab_run(path, &o),
        "summarize" => lab_summarize(path, &o),
        _ => Err(LAB_USAGE.into()),
    }
}

/// `S..E` (end exclusive), `S..=E` (inclusive) or `S` alone.
fn parse_seed_range(spec: &str) -> Res<(u64, u64)> {
    let bad = || -> Box<dyn std::error::Error> {
        format!("bad --seeds {spec:?}: expected S..E, S..=E or S").into()
    };
    let num = |s: &str| s.trim().parse::<u64>().map_err(|_| bad());
    if let Some((a, b)) = spec.split_once("..=") {
        let (a, b) = (num(a)?, num(b)?);
        return Ok((a, b + 1));
    }
    if let Some((a, b)) = spec.split_once("..") {
        return Ok((num(a)?, num(b)?));
    }
    let a = num(spec)?;
    Ok((a, a + 1))
}

fn lab_run(design_path: &str, o: &Opts) -> R {
    let text = std::fs::read_to_string(design_path).map_err(|e| format!("{design_path}: {e}"))?;
    let design = Design::from_json(&text)?;
    let dir = o.get("out").ok_or("--out DIR required")?;
    let jobs: usize = get(o, "jobs", 1)?;
    let cells: Option<Vec<String>> = o.get("cells").map(|s| {
        s.split(',')
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .map(String::from)
            .collect()
    });
    let seeds = o.get("seeds").map(|s| parse_seed_range(s)).transpose()?;
    let selection = Selection { cells, seeds };
    let list = design.jobs(selection.cells.as_deref(), selection.seeds)?;
    let options = RunOptions {
        check: o.contains_key("check"),
        agents: o.contains_key("agents"),
    };
    if options.agents && design.ticks > AGENTS_TICKS_ADVISED {
        eprintln!(
            "warning: --agents writes one row per agent ever alive; ADR-0015 D4 means it for runs of at most {AGENTS_TICKS_ADVISED} ticks (this design runs {} — expect large <cell>_agents.csv files)",
            design.ticks
        );
    }
    std::fs::create_dir_all(dir)?;
    let env = environment();
    println!(
        "cz lab run {}: {} worlds × {} ticks ({} cells{}{}), jobs {}, check {}{}",
        design.name,
        list.len(),
        design.ticks,
        design.cells.len(),
        selection
            .cells
            .as_ref()
            .map_or(String::new(), |c| format!(", cells {}", c.join(","))),
        selection
            .seeds
            .map_or(String::new(), |(a, b)| format!(", seeds {a}..{b}")),
        jobs.max(1),
        options.check,
        if options.agents { ", agents" } else { "" }
    );
    println!(
        "commit {} ({} modified), {}, {} cores, {}, {}, binary sha256 {}, load {}",
        env.commit.as_deref().unwrap_or("?"),
        env.worktree_modified
            .map_or("?".to_string(), |n| n.to_string()),
        env.machine.as_deref().unwrap_or("?"),
        env.cores.map_or("?".to_string(), |n| n.to_string()),
        env.os.as_deref().unwrap_or("?"),
        env.rustc.as_deref().unwrap_or("rustc ?"),
        env.binary_sha256
            .as_deref()
            .map_or("?", |s| &s[..s.len().min(16)]),
        env.load_at_start.as_deref().unwrap_or("?")
    );
    let manifest = run_design(
        &design,
        &selection,
        jobs,
        options,
        Some(&Naming::Dir(dir.clone())),
        true,
    )?;
    let path = format!("{dir}/RUN.json");
    std::fs::write(&path, manifest.to_json())?;
    let secs = manifest.wall_ms as f64 / 1e3;
    println!(
        "wrote {path}: {} worlds, {} world·ticks, wall {secs:.1} s ({:.0} world·ticks/s), load {}",
        manifest.worlds.len(),
        manifest.world_ticks(),
        manifest.world_ticks() as f64 / secs.max(1e-9),
        manifest.environment.load_at_end.as_deref().unwrap_or("?")
    );
    Ok(())
}

fn lab_summarize(dir: &str, o: &Opts) -> R {
    let design_path = o.get("design").ok_or("--design DESIGN.json required")?;
    let text = std::fs::read_to_string(design_path).map_err(|e| format!("{design_path}: {e}"))?;
    let mut design = Design::from_json(&text)?;
    let mut tables = BTreeMap::new();
    let mut missing = Vec::new();
    for cell in &design.cells {
        let summary = format!("{dir}/{}_summary.csv", cell.name);
        let series = format!("{dir}/{}_series.csv", cell.name);
        match (
            std::fs::read_to_string(&summary),
            std::fs::read_to_string(&series),
        ) {
            (Ok(s), Ok(r)) => {
                tables.insert(
                    cell.name.clone(),
                    CellTables {
                        summary: Table::parse(&s).map_err(|e| format!("{summary}: {e}"))?,
                        series: Table::parse(&r).map_err(|e| format!("{series}: {e}"))?,
                    },
                );
            }
            _ => missing.push(cell.name.clone()),
        }
    }
    if tables.is_empty() {
        return Err(format!(
            "{dir}: no <cell>_summary.csv / <cell>_series.csv of {}",
            design.name
        )
        .into());
    }
    if !missing.is_empty() {
        println!(
            "cells without files in {dir}, left out (with their contrasts): {}",
            missing.join(", ")
        );
        design.cells.retain(|c| tables.contains_key(&c.name));
        design
            .contrasts
            .retain(|c| tables.contains_key(&c.treat) && tables.contains_key(&c.control));
    }
    let summary = summarize(&design, &tables)?;
    let md = summary.to_markdown();
    std::fs::write(format!("{dir}/SUMMARY.json"), summary.to_json())?;
    std::fs::write(format!("{dir}/SUMMARY.md"), &md)?;
    print!("{md}");
    println!(
        "\nwrote {dir}/SUMMARY.json and {dir}/SUMMARY.md ({} cells, {} contrasts)",
        summary.cells.len(),
        summary.contrasts.len()
    );
    Ok(())
}

/// Standard output of a command, trimmed; `None` when it cannot run or
/// fails.
fn sh(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Load averages from `uptime` (`load averages: a b c` on macOS, `load
/// average: a, b, c` on Linux).
fn load() -> Option<String> {
    sh("uptime", &[]).and_then(|s| {
        s.split("load average")
            .nth(1)
            .map(|t| t.trim_start_matches(['s', ':', ' ']).trim().to_string())
            .and_then(non_empty)
    })
}

/// What the manifest records about where and with what the run happened;
/// every field best effort.
fn environment() -> Environment {
    let commit = sh("git", &["rev-parse", "HEAD"]).and_then(non_empty);
    let worktree_modified = sh("git", &["status", "--porcelain"]).map(|s| s.lines().count());
    let machine = sh("sysctl", &["-n", "machdep.cpu.brand_string"])
        .and_then(non_empty)
        .or_else(|| {
            std::fs::read_to_string("/proc/cpuinfo").ok().and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("model name"))
                    .and_then(|l| l.split(':').nth(1))
                    .map(|v| v.trim().to_string())
            })
        });
    let os = sh("sw_vers", &["-productVersion"])
        .and_then(non_empty)
        .map(|v| format!("macOS {v}"))
        .or_else(|| sh("uname", &["-sr"]).and_then(non_empty));
    let cores = std::thread::available_parallelism().ok().map(|n| n.get());
    let memory_gb = sh("sysctl", &["-n", "hw.memsize"])
        .and_then(|s| s.parse::<u64>().ok())
        .map(|b| b >> 30)
        .or_else(|| {
            std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
                s.lines()
                    .find(|l| l.starts_with("MemTotal"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|kb| kb.parse::<u64>().ok())
                    .map(|kb| kb >> 20)
            })
        });
    let rustc = option_env!("CZ_RUSTC_VERSION")
        .map(String::from)
        .and_then(non_empty);
    let binary_sha256 = std::env::current_exe().ok().and_then(|p| {
        let p = p.to_string_lossy().to_string();
        sh("shasum", &["-a", "256", &p])
            .or_else(|| sh("sha256sum", &[&p]))
            .and_then(|s| s.split_whitespace().next().map(String::from))
    });
    Environment {
        commit,
        worktree_modified,
        machine,
        os,
        cores,
        memory_gb,
        rustc,
        binary_sha256,
        load_at_start: load(),
        load_at_end: None,
    }
}
