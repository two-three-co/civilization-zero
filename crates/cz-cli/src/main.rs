//! `cz` — run Civilization Zero worlds from the command line.
//!
//!   cz run    [--seed N] [--agents N] [--side N] [--ticks N]
//!             [--policy forager|random|mixed] [--check]
//!             [--trace PATH --every K] [--snapshot PATH]
//!             [--drought-at TICK [--drought-amount N] [--drought-interval N]]
//!             [--log PATH]
//!   cz resume --snapshot PATH [--ticks N] [--check]
//!   cz log    --path PATH [--from TICK] [--to TICK] [--limit N]
//!   cz sweep  [--seeds N] [--seed-start S] [--ticks N] [--policy ...]
//!             [--window PCT] [--sample K] [--out PREFIX] [--check]
//!
//! Timing printed by `run` covers `World::step` only (trace I/O excluded).

use serde::Serialize;
use simulation_core::{DeathCause, Event, EventLog, Intervention, PolicyKind, World, WorldConfig};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        usage();
    };
    let opts = parse_opts(rest);
    let result = match cmd.as_str() {
        "run" => cmd_run(&opts),
        "resume" => cmd_resume(&opts),
        "sweep" => cmd_sweep(&opts),
        "log" => cmd_log(&opts),
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  cz run [--seed N] [--agents N] [--side N] [--ticks N] [--policy forager|random|mixed]\n         [--check] [--trace PATH --every K] [--snapshot PATH]\n         [--drought-at TICK [--drought-amount N] [--drought-interval N]] [--log PATH]\n  cz log --path PATH [--from TICK] [--to TICK] [--limit N]\n  cz resume --snapshot PATH [--ticks N] [--check]\n  cz sweep [--seeds N] [--seed-start S] [--ticks N] [--policy ...] [--window PCT]\n           [--sample K] [--out PREFIX] [--check]"
    );
    std::process::exit(2)
}

type Opts = BTreeMap<String, String>;

fn parse_opts(args: &[String]) -> Opts {
    let mut out = Opts::new();
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        let Some(key) = a.strip_prefix("--") else {
            eprintln!("unexpected argument: {a}");
            usage();
        };
        let val = match it.peek() {
            Some(v) if !v.starts_with("--") => it.next().unwrap().clone(),
            _ => "true".to_string(),
        };
        out.insert(key.to_string(), val);
    }
    out
}

fn get<T: std::str::FromStr>(o: &Opts, k: &str, default: T) -> Result<T, String> {
    match o.get(k) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| format!("bad value for --{k}: {v}")),
    }
}

type R = Result<(), Box<dyn std::error::Error>>;

fn build_world(seed: u64, o: &Opts) -> Result<World, Box<dyn std::error::Error>> {
    let side: u16 = get(o, "side", 128)?;
    let cfg = WorldConfig {
        seed,
        width: side,
        height: side,
        initial_agents: get(o, "agents", 1_000)?,
        patch_count: get(o, "patches", ((side as u32 * side as u32) / 700).max(1))?,
        mutation_enabled: o.contains_key("mutate"),
        ..WorldConfig::default()
    };
    let policy = o.get("policy").map(String::as_str).unwrap_or("forager");
    Ok(match policy {
        "forager" => World::generate(cfg, |_| PolicyKind::Forager)?,
        "random" => World::generate(cfg, |_| PolicyKind::Random)?,
        "mixed" => World::generate(cfg, |i| {
            if i % 2 == 0 {
                PolicyKind::Forager
            } else {
                PolicyKind::Random
            }
        })?,
        p => return Err(format!("unknown policy {p}").into()),
    })
}

fn cmd_run(o: &Opts) -> R {
    let world = build_world(get(o, "seed", 42)?, o)?;
    simulate(world, o)
}

fn cmd_resume(o: &Opts) -> R {
    let path = o.get("snapshot").ok_or("--snapshot required")?;
    let world = World::from_snapshot(&std::fs::read(path)?)?;
    println!(
        "restored tick {} hash {:#018x}",
        world.tick,
        world.state_hash()
    );
    let mut o = o.clone();
    o.remove("snapshot"); // don't overwrite the input
    simulate(world, &o)
}

fn check(world: &World) -> R {
    world
        .check_invariants()
        .map_err(|e| format!("invariant violated at tick {}: {e:?}", world.tick).into())
}

fn simulate(mut world: World, o: &Opts) -> R {
    let ticks: u64 = get(o, "ticks", 1_000)?;
    let do_check = o.contains_key("check");
    let every: u64 = get(o, "every", 5)?.max(1);
    let mut trace = o.get("trace").map(|_| Trace::new(&world));
    // A single scripted intervention, enough to eyeball a drought in the viewer.
    let drought_at: Option<u64> = o
        .get("drought-at")
        .map(|v| v.parse())
        .transpose()
        .map_err(|_| "bad value for --drought-at")?;
    let drought = Intervention::Drought {
        regrow_amount: get(o, "drought-amount", 0)?,
        regrow_interval: get(o, "drought-interval", world.config.regrow_interval)?,
    };
    let mut log = o.get("log").map(|_| EventLog::new());

    println!(
        "seed={} grid={}x{} agents={} ticks={} check={}",
        world.config.seed,
        world.config.width,
        world.config.height,
        world.agents.len(),
        ticks,
        do_check
    );
    println!(
        "{:>6} {:>6} {:>6} {:>7} {:>7} {:>9} {:>9} {:>9}",
        "tick", "alive", "births", "d.starv", "d.age", "gathered", "consumed", "food"
    );
    let mut step_time = Duration::ZERO;
    let mut agent_steps = 0u64;
    let (mut gathered, mut consumed, mut births) = (0u64, 0u64, 0u64);
    let mut deaths = [0u64; DeathCause::COUNT];
    for _ in 0..ticks {
        if let Some(t) = trace.as_mut() {
            if world.tick % every == 0 {
                t.push(&world);
            }
        }
        if drought_at == Some(world.tick) {
            let e = world.intervene(drought)?;
            println!("intervention: {}", serde_json::to_string(&e)?);
            if let Some(l) = log.as_mut() {
                l.push(e)?;
            }
        }
        agent_steps += world.agents.len() as u64;
        let t0 = Instant::now();
        let r = world.step();
        step_time += t0.elapsed();
        if let Some(l) = log.as_mut() {
            l.record(&r)?;
        }
        gathered += r.food_gathered;
        consumed += r.food_consumed;
        births += r.births as u64;
        for (acc, d) in deaths.iter_mut().zip(r.deaths_by_cause) {
            *acc += d as u64;
        }
        if do_check {
            check(&world)?;
        }
        if world.tick % 100 == 0 {
            println!(
                "{:>6} {:>6} {:>6} {:>7} {:>7} {:>9} {:>9} {:>9}",
                world.tick,
                world.agents.len(),
                births,
                deaths[DeathCause::Starvation.index()],
                deaths[DeathCause::OldAge.index()],
                gathered,
                consumed,
                world.resources.total_food()
            );
            (gathered, consumed, births, deaths) = (0, 0, 0, [0; DeathCause::COUNT]);
        }
    }
    let secs = step_time.as_secs_f64();
    println!(
        "final tick {} hash {:#018x}",
        world.tick,
        world.state_hash()
    );
    println!(
        "step time {:.3}s  {:.1} ticks/s  {:.0} ns/agent-step  ({} agent-steps)",
        secs,
        ticks as f64 / secs,
        secs * 1e9 / agent_steps.max(1) as f64,
        agent_steps
    );
    if let (Some(t), Some(path)) = (trace.as_mut(), o.get("trace")) {
        t.push(&world);
        std::fs::write(path, serde_json::to_vec(t)?)?;
        println!("trace: {} frames -> {path}", t.frames.len());
    }
    if let Some(path) = o.get("snapshot") {
        std::fs::write(path, world.to_snapshot())?;
        println!("snapshot -> {path}");
    }
    if let (Some(l), Some(path)) = (log.as_ref(), o.get("log")) {
        let mut w = std::io::BufWriter::new(std::fs::File::create(path)?);
        l.write_jsonl(&mut w)?;
        std::io::Write::flush(&mut w)?;
        println!(
            "log: {} events -> {path} ({} bytes)",
            l.len(),
            std::fs::metadata(path)?.len()
        );
    }
    Ok(())
}

/// Summarises a ledger file and prints the events of a tick range.
fn cmd_log(o: &Opts) -> R {
    let path = o.get("path").ok_or("--path required")?;
    let file = std::fs::File::open(path)?;
    let log = EventLog::read_jsonl(std::io::BufReader::new(file))?;
    let from: u64 = get(o, "from", 0)?;
    let to: u64 = get(o, "to", u64::MAX)?;
    let limit: usize = get(o, "limit", 50)?;
    let mut counts: BTreeMap<&str, u64> = BTreeMap::new();
    for e in log.events() {
        let kind = match e {
            Event::Born { .. } => "born",
            Event::Died { .. } => "died",
            Event::Intervened { .. } => "intervened",
        };
        *counts.entry(kind).or_default() += 1;
    }
    let summary: Vec<String> = counts.iter().map(|(k, v)| format!("{k}={v}")).collect();
    println!(
        "{path}: {} events, last tick {}, {}",
        log.len(),
        log.last_tick().map_or("-".to_string(), |t| t.to_string()),
        summary.join(" ")
    );
    let slice = log.range(from..to);
    println!(
        "ticks {from}..{to}: {} events (showing up to {limit})",
        slice.len()
    );
    for e in slice.iter().take(limit) {
        println!("{}", serde_json::to_string(e)?);
    }
    Ok(())
}

/// Runs several seeds and summarises population dynamics over the last
/// `--window` percent of ticks. Writes `<out>_summary.csv` (one row per seed)
/// and `<out>_series.csv` (population every `--sample` ticks).
fn cmd_sweep(o: &Opts) -> R {
    let seeds: u64 = get(o, "seeds", 10)?;
    let seed_start: u64 = get(o, "seed-start", 0)?;
    let ticks: u64 = get(o, "ticks", 20_000)?;
    let window_pct: u64 = get(o, "window", 50)?.min(100);
    let sample: u64 = get(o, "sample", 100)?.max(1);
    let do_check = o.contains_key("check");
    let policy = o.get("policy").map(String::as_str).unwrap_or("forager");
    let win_start = ticks - ticks * window_pct / 100;

    let mut summary = String::from(
        "seed,policy,final_alive,extinct_at,win_mean,win_min,win_max,win_max_over_min,births,deaths_starvation,deaths_old_age,ticks_per_s\n",
    );
    let mut series = String::from("seed,tick,alive,food\n");
    println!(
        "{:>5} {:>7} {:>8} {:>9} {:>7} {:>7} {:>7} {:>8} {:>8} {:>7} {:>8}",
        "seed",
        "final",
        "extinct",
        "win_mean",
        "min",
        "max",
        "max/min",
        "births",
        "d.starv",
        "d.age",
        "ticks/s"
    );
    let mut means = Vec::new();
    for s in 0..seeds {
        let seed = seed_start + s;
        let mut world = build_world(seed, o)?;
        let (mut sum, mut min, mut max, mut count) = (0u64, u64::MAX, 0u64, 0u64);
        let mut extinct_at: Option<u64> = None;
        let t0 = Instant::now();
        for _ in 0..ticks {
            if world.tick % sample == 0 {
                let _ = writeln!(
                    series,
                    "{seed},{},{},{}",
                    world.tick,
                    world.agents.len(),
                    world.resources.total_food()
                );
            }
            world.step();
            if do_check {
                check(&world)?;
            }
            let alive = world.agents.len() as u64;
            if world.tick > win_start {
                sum += alive;
                min = min.min(alive);
                max = max.max(alive);
                count += 1;
            }
            if extinct_at.is_none() && alive == 0 {
                extinct_at = Some(world.tick);
            }
        }
        let secs = t0.elapsed().as_secs_f64();
        let mean = sum as f64 / count.max(1) as f64;
        let ratio = if min > 0 {
            max as f64 / min as f64
        } else {
            f64::INFINITY
        };
        let l = &world.ledger;
        let extinct = extinct_at.map_or("-".to_string(), |t| t.to_string());
        means.push(mean);
        println!(
            "{:>5} {:>7} {:>8} {:>9.1} {:>7} {:>7} {:>8.3} {:>8} {:>8} {:>7} {:>8.0}",
            seed,
            world.agents.len(),
            extinct,
            mean,
            min,
            max,
            ratio,
            l.births,
            l.deaths_by_cause[DeathCause::Starvation.index()],
            l.deaths_by_cause[DeathCause::OldAge.index()],
            ticks as f64 / secs
        );
        let _ = writeln!(
            summary,
            "{seed},{policy},{},{extinct},{mean:.2},{min},{max},{ratio:.4},{},{},{},{:.0}",
            world.agents.len(),
            l.births,
            l.deaths_by_cause[DeathCause::Starvation.index()],
            l.deaths_by_cause[DeathCause::OldAge.index()],
            ticks as f64 / secs
        );
    }
    let n = means.len() as f64;
    let grand = means.iter().sum::<f64>() / n;
    let sd = (means.iter().map(|m| (m - grand).powi(2)).sum::<f64>() / n).sqrt();
    println!(
        "window = last {window_pct}% of {ticks} ticks; across {seeds} seeds: mean of window means {grand:.1}, sd {sd:.1}, cv {:.3}",
        if grand > 0.0 { sd / grand } else { f64::NAN }
    );
    if let Some(prefix) = o.get("out") {
        std::fs::write(format!("{prefix}_summary.csv"), summary)?;
        std::fs::write(format!("{prefix}_series.csv"), series)?;
        println!("wrote {prefix}_summary.csv and {prefix}_series.csv");
    }
    Ok(())
}

/// Compact trace for the 2D viewer (viewer/index.html).
#[derive(Serialize)]
struct Trace {
    format: u32,
    width: u16,
    height: u16,
    max_energy: i64,
    inventory_capacity: u32,
    /// Static per-cell capacity, hex-encoded bytes (clamped to 255).
    capacity: String,
    frames: Vec<Frame>,
}

#[derive(Serialize)]
struct Frame {
    tick: u64,
    /// Hex-encoded per-cell food, row-major, clamped to 255.
    food: String,
    /// Flat `[id, x, y, energy, inventory, policy, health, age]` per agent.
    agents: Vec<i64>,
}

fn hex_bytes(v: &[u32]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(v.len() * 2);
    for &x in v {
        let b = x.min(255) as usize;
        s.push(H[b >> 4] as char);
        s.push(H[b & 15] as char);
    }
    s
}

impl Trace {
    fn new(w: &World) -> Self {
        Self {
            format: 2,
            width: w.config.width,
            height: w.config.height,
            max_energy: w.config.max_energy,
            inventory_capacity: w.config.inventory_capacity,
            capacity: hex_bytes(&w.resources.capacity),
            frames: Vec::new(),
        }
    }

    fn push(&mut self, w: &World) {
        if self.frames.last().is_some_and(|f| f.tick == w.tick) {
            return;
        }
        let mut agents = Vec::with_capacity(w.agents.len() * 8);
        for a in &w.agents {
            agents.extend_from_slice(&[
                a.id.0 as i64,
                a.pos.x as i64,
                a.pos.y as i64,
                a.energy,
                a.inventory as i64,
                matches!(a.policy, PolicyKind::Random) as i64,
                a.health,
                a.age as i64,
            ]);
        }
        self.frames.push(Frame {
            tick: w.tick,
            food: hex_bytes(&w.resources.food),
            agents,
        });
    }
}
