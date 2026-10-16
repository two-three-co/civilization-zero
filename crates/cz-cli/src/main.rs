//! `cz` — run Civilization Zero worlds from the command line.
//!
//!   cz run    [--seed N] [--agents N] [--side N] [--ticks N]
//!             [--policy forager|random|mixed] [--check]
//!             [--trace PATH --every K] [--snapshot PATH]
//!   cz resume --snapshot PATH [--ticks N] [--check]
//!
//! Timing printed by `run` covers `World::step` only (trace I/O excluded).

use serde::Serialize;
use simulation_core::{PolicyKind, World, WorldConfig};
use std::collections::BTreeMap;
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
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  cz run [--seed N] [--agents N] [--side N] [--ticks N] [--policy forager|random|mixed]\n         [--check] [--trace PATH --every K] [--snapshot PATH]\n  cz resume --snapshot PATH [--ticks N] [--check]"
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

fn cmd_run(o: &Opts) -> R {
    let side: u16 = get(o, "side", 128)?;
    let cfg = WorldConfig {
        seed: get(o, "seed", 42)?,
        width: side,
        height: side,
        initial_agents: get(o, "agents", 1_000)?,
        patch_count: get(o, "patches", ((side as u32 * side as u32) / 700).max(1))?,
        ..WorldConfig::default()
    };
    let policy = o.get("policy").map(String::as_str).unwrap_or("forager");
    let world = match policy {
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
    };
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

fn simulate(mut world: World, o: &Opts) -> R {
    let ticks: u64 = get(o, "ticks", 1_000)?;
    let check = o.contains_key("check");
    let every: u64 = get(o, "every", 5)?.max(1);
    let mut trace = o.get("trace").map(|_| Trace::new(&world));

    println!(
        "seed={} grid={}x{} agents={} ticks={} check={}",
        world.config.seed,
        world.config.width,
        world.config.height,
        world.agents.len(),
        ticks,
        check
    );
    println!(
        "{:>6} {:>6} {:>7} {:>9} {:>9} {:>9}",
        "tick", "alive", "deaths", "gathered", "consumed", "food"
    );
    let mut step_time = Duration::ZERO;
    let mut agent_steps = 0u64;
    let (mut gathered, mut consumed, mut deaths) = (0u64, 0u64, 0u64);
    for _ in 0..ticks {
        if let Some(t) = trace.as_mut() {
            if world.tick % every == 0 {
                t.push(&world);
            }
        }
        agent_steps += world.agents.len() as u64;
        let t0 = Instant::now();
        let r = world.step();
        step_time += t0.elapsed();
        gathered += r.food_gathered;
        consumed += r.food_consumed;
        deaths += r.deaths as u64;
        if check {
            world
                .check_invariants()
                .map_err(|e| format!("invariant violated at tick {}: {e:?}", world.tick))?;
        }
        if world.tick % 100 == 0 {
            println!(
                "{:>6} {:>6} {:>7} {:>9} {:>9} {:>9}",
                world.tick,
                world.agents.len(),
                deaths,
                gathered,
                consumed,
                world.resources.total_food()
            );
            (gathered, consumed, deaths) = (0, 0, 0);
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
    /// Flat `[id, x, y, energy, inventory, policy, health]` per agent.
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
            format: 1,
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
        let mut agents = Vec::with_capacity(w.agents.len() * 7);
        for a in &w.agents {
            agents.extend_from_slice(&[
                a.id.0 as i64,
                a.pos.x as i64,
                a.pos.y as i64,
                a.energy,
                a.inventory as i64,
                matches!(a.policy, PolicyKind::Random) as i64,
                a.health,
            ]);
        }
        self.frames.push(Frame {
            tick: w.tick,
            food: hex_bytes(&w.resources.food),
            agents,
        });
    }
}
