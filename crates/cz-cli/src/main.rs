//! `cz` — run Civilization Zero worlds from the command line.
//!
//!   cz run    [--seed N] [--agents N] [--side N] [--ticks N]
//!             [--policy forager|random|mixed|reflex|learner|mixed-learning] [--check]
//!             [--mutate | --no-mutate] [--learner-states N] [--learner-epsilon PERMILLE]
//!             [--nomos] [--learner-health-weight W] [--record-violations]
//!             [--enact KIND[:STANDING[:SANCTION[:EXCEPT]]] [--enact-at TICK]]
//!             [--repeal ID [--repeal-at TICK]] [--charter Q,TH,TICKS,AMENDABLE]
//!             [--trace PATH --every K] [--snapshot PATH]
//!             [--drought-at TICK [--drought-amount N] [--drought-interval N]]
//!             [--set-policy KIND [--set-policy-at TICK]]
//!             [--log PATH] [--record-actions]
//!   cz resume --snapshot PATH [--ticks N] [--check]
//!   cz log    --path PATH [--from TICK] [--to TICK] [--limit N]
//!   cz replay --snapshot PATH --log PATH [--to TICK]
//!   cz fork   [--seed N] [--agents N] [--side N] [--ticks N] [--at TICK] [--every K]
//!             [--scale-food PERMILLE] [--drought-amount N --drought-interval N]
//!             [--set-policy KIND] [--enact SPEC] [--repeal ID]
//!   cz sweep  [--seeds N] [--seed-start S] [--ticks N] [--policy ...]
//!             [--window PCT] [--sample K] [--out PREFIX] [--check]
//!
//! Policies: `reflex` and `mixed-learning` (forager / reflex / learner in
//! rotation) switch mutation on unless `--no-mutate` (ADR-0013 D6).
//! `--learner-states` / `--learner-epsilon` size and explore a learner's Q
//! table (3-7). `--set-policy KIND` applies `SetPolicy { who: All, params:
//! Default }` at `--set-policy-at` (default 0) and logs it like a drought.
//! `--nomos` switches the tracking of social facts on (ADR-0014 4-1: first
//! occupancy, encroachments, the audit of accepted deals) and
//! `--learner-health-weight W` adds `W × Δhealth` to a learner's reward.
//! Institutions (ADR-0014 4-2, need `--nomos`): `--enact
//! KIND[:STANDING[:SANCTION[:EXCEPT]]]` puts a norm in force by decree at
//! `--enact-at` (default 0) — KIND is `encroach`, `broken_promise` or
//! `harm`, STANDING the ticks a violator stands accused (default 32),
//! SANCTION `harm`, `reject`, `harm+reject` or `none` (default `harm`),
//! EXCEPT `true` to exempt sanctions from a harm rule (default `false`);
//! `--repeal ID` takes rule ID out of force at `--repeal-at` (default 0);
//! `--charter Q,TH,TICKS,AMENDABLE` is the assembly's initial charter;
//! `--record-violations` writes `Violated` events to the log (off by
//! default: ~21 per tick in a crowded world, 4-1). `fork` applies `--enact`
//! / `--repeal` at the fork tick, which is how "with vs without rule" is
//! compared (ADR-0014 D8). All three commands that build worlds (`run`,
//! `sweep`, `fork`) take all of these.
//!
//! `cz sweep --out PREFIX` writes `PREFIX_summary.csv` (one row per seed:
//! population band of the last `--window` percent of ticks, then that
//! window's consumption per agent-tick, transfers / harms / signals /
//! negotiation messages per tick, death causes, action mix, the Q-table or
//! weight statistics of EXP-002, since 4-1 the window's encroachment rate
//! overall and per policy kind, promises due / broken, standing food and
//! regrowth, and since 4-2 the rules in force, violations and sanctions per
//! tick, episodes closed / sanctioned and H1's S = episodes sanctioned /
//! closed and S' = sanctions / violations), `PREFIX_series.csv` (every
//! `--sample` ticks: population, food and the interval's counts — the
//! EXP-002 columns first, the 4-1 counts, then the 4-2 counts) and, with
//! learners, `PREFIX_qpolicy.csv` (per Q row, how many learners prefer each
//! kind and how often the row was visited).
//!
//! Snapshot paths ending in `.json` use the readable form; anything else is
//! the compact binary form (ADR-0006). Timing printed by `run` covers
//! `World::step` only (trace I/O excluded).

use multiverse::{BranchId, Multiverse};
use serde::Serialize;
use simulation_core::policy::learner::{state, Q_UNIT};
use simulation_core::policy::{forager_weights, REFLEX_UNIT};
use simulation_core::{
    Action, Charter, DeathCause, Event, EventLog, Intervention, Norm, NormKind, ParamsMode,
    PolicyKind, PolicyTable, Sanction, TickReport, Who, World, WorldConfig,
};
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
        "replay" => cmd_replay(&opts),
        "fork" => cmd_fork(&opts),
        _ => usage(),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn usage() -> ! {
    eprintln!(
        "usage:\n  cz run [--seed N] [--agents N] [--side N] [--ticks N]\n         [--policy forager|random|mixed|reflex|learner|mixed-learning] [--mutate | --no-mutate]\n         [--learner-states N] [--learner-epsilon PERMILLE] [--nomos] [--learner-health-weight W]\n         [--enact KIND[:STANDING[:SANCTION[:EXCEPT]]] [--enact-at TICK]] [--repeal ID [--repeal-at TICK]]\n         [--charter Q,TH,TICKS,AMENDABLE] [--record-violations]\n         [--check] [--trace PATH --every K] [--snapshot PATH]\n         [--drought-at TICK [--drought-amount N] [--drought-interval N]]\n         [--set-policy KIND [--set-policy-at TICK]] [--log PATH] [--record-actions]\n  cz log --path PATH [--from TICK] [--to TICK] [--limit N]\n  cz resume --snapshot PATH [--ticks N] [--check]\n  cz replay --snapshot PATH --log PATH [--to TICK]\n  cz fork [--seed N] [--agents N] [--side N] [--ticks N] [--at TICK] [--every K]\n          [--scale-food PERMILLE] [--drought-amount N --drought-interval N] [--set-policy KIND]\n          [--enact SPEC] [--repeal ID]\n  cz sweep [--seeds N] [--seed-start S] [--ticks N] [--policy ...] [--window PCT]\n           [--sample K] [--out PREFIX] [--check]\n  (run, fork and sweep also take --nomos, --learner-health-weight W, --charter, --record-violations;\n   --enact KIND is encroach | broken_promise | harm, SANCTION is harm | reject | harm+reject | none)"
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
    let policy = o.get("policy").map(String::as_str).unwrap_or("forager");
    // Evolving policies mutate by default (ADR-0013 D6); `--no-mutate` wins.
    let evolving = matches!(policy, "reflex" | "mixed-learning");
    let cfg = WorldConfig {
        seed,
        width: side,
        height: side,
        initial_agents: get(o, "agents", 1_000)?,
        patch_count: get(o, "patches", ((side as u32 * side as u32) / 700).max(1))?,
        mutation_enabled: o.contains_key("mutate") || (evolving && !o.contains_key("no-mutate")),
        record_actions: o.contains_key("record-actions"),
        learner_states: get(o, "learner-states", 32)?,
        learner_epsilon_permille: get(o, "learner-epsilon", 50)?,
        learner_health_weight: get(o, "learner-health-weight", 0)?,
        nomos: o.contains_key("nomos"),
        record_violations: o.contains_key("record-violations"),
        charter: match o.get("charter") {
            Some(spec) => parse_charter(spec)?,
            None => Charter::default(),
        },
        ..WorldConfig::default()
    };
    Ok(match policy {
        "mixed" => World::generate(cfg, |i| {
            if i % 2 == 0 {
                PolicyKind::Forager
            } else {
                PolicyKind::Random
            }
        })?,
        "mixed-learning" => World::generate(cfg, |i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Reflex,
            _ => PolicyKind::Learner,
        })?,
        p => {
            let kind = PolicyKind::parse(p).ok_or_else(|| format!("unknown policy {p}"))?;
            World::generate(cfg, move |_| kind)?
        }
    })
}

/// `--enact KIND[:STANDING[:SANCTION[:EXCEPT]]]` as the norm it names
/// (ADR-0014 D5, D8): e.g. `encroach`, `encroach:32:harm`,
/// `harm:16:harm+reject:true`, `broken_promise:24:reject`.
fn parse_norm(spec: &str) -> Result<Norm, String> {
    let fields: Vec<&str> = spec.split(':').collect();
    if fields.len() > 4 {
        return Err(format!(
            "bad --enact {spec:?}: at most KIND:STANDING:SANCTION:EXCEPT"
        ));
    }
    let kind = NormKind::parse(fields[0]).ok_or_else(|| {
        format!(
            "bad --enact {spec:?}: KIND must be encroach, broken_promise or harm, not {:?}",
            fields[0]
        )
    })?;
    let standing_ticks = match fields.get(1) {
        None | Some(&"") => 32,
        Some(v) => v
            .parse()
            .map_err(|_| format!("bad --enact {spec:?}: STANDING {v:?} is not a tick count"))?,
    };
    let sanction = match fields.get(2) {
        None => Sanction::HARM,
        Some(v) => Sanction::parse(v).ok_or_else(|| {
            format!(
                "bad --enact {spec:?}: SANCTION {v:?} must be harm, reject, harm+reject or none"
            )
        })?,
    };
    let except_sanction = match fields.get(3) {
        None => false,
        Some(v) => parse_bool(v)
            .ok_or_else(|| format!("bad --enact {spec:?}: EXCEPT {v:?} must be true or false"))?,
    };
    Ok(Norm {
        kind,
        standing_ticks,
        sanction,
        except_sanction,
    })
}

/// `--charter Q,TH,TICKS,AMENDABLE` (permilles, ticks, true / false).
fn parse_charter(spec: &str) -> Result<Charter, String> {
    let fields: Vec<&str> = spec.split(',').collect();
    if fields.len() != 4 {
        return Err(format!(
            "bad --charter {spec:?}: expected Q,TH,TICKS,AMENDABLE"
        ));
    }
    let num = |i: usize, what: &str| -> Result<u16, String> {
        fields[i].parse().map_err(|_| {
            format!(
                "bad --charter {spec:?}: {what} {:?} is not a number",
                fields[i]
            )
        })
    };
    let charter = Charter {
        quorum_permille: num(0, "Q")?,
        threshold_permille: num(1, "TH")?,
        vote_ticks: num(2, "TICKS")?,
        amendable: parse_bool(fields[3])
            .ok_or_else(|| format!("bad --charter {spec:?}: AMENDABLE must be true or false"))?,
    };
    if !charter.is_valid() {
        return Err(format!("bad --charter {spec:?}: permilles must be <= 1000"));
    }
    Ok(charter)
}

fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "true" | "1" | "yes" | "except" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// `--enact SPEC` and `--repeal ID` as the decrees they stand for, with the
/// ticks `--enact-at` / `--repeal-at` name (default 0).
fn decrees(o: &Opts) -> Result<Vec<(u64, Intervention)>, Box<dyn std::error::Error>> {
    let mut out = Vec::new();
    if let Some(spec) = o.get("enact") {
        let norm = parse_norm(spec)?;
        out.push((get(o, "enact-at", 0)?, Intervention::Enact { norm }));
    }
    if let Some(id) = o.get("repeal") {
        let rule: u16 = id.parse().map_err(|_| format!("bad --repeal {id:?}"))?;
        out.push((get(o, "repeal-at", 0)?, Intervention::Repeal { rule }));
    }
    Ok(out)
}

/// `--set-policy KIND` as the intervention it stands for.
fn set_policy_opt(o: &Opts) -> Result<Option<Intervention>, Box<dyn std::error::Error>> {
    o.get("set-policy")
        .map(|k| {
            let kind = PolicyKind::parse(k).ok_or_else(|| format!("unknown policy {k}"))?;
            Ok(Intervention::SetPolicy {
                who: Who::All,
                kind,
                params: ParamsMode::Default,
            })
        })
        .transpose()
}

fn cmd_run(o: &Opts) -> R {
    let world = build_world(get(o, "seed", 42)?, o)?;
    simulate(world, o)
}

fn load_snapshot(path: &str) -> Result<World, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    Ok(if path.ends_with(".json") {
        World::from_snapshot(&bytes)?
    } else {
        World::from_bytes(&bytes)?
    })
}

fn save_snapshot(path: &str, world: &World) -> R {
    let (bytes, kind) = if path.ends_with(".json") {
        (world.to_snapshot(), "json")
    } else {
        (world.to_bytes(), "binary")
    };
    std::fs::write(path, &bytes)?;
    println!("snapshot -> {path} ({} bytes, {kind})", bytes.len());
    Ok(())
}

fn cmd_resume(o: &Opts) -> R {
    let path = o.get("snapshot").ok_or("--snapshot required")?;
    let world = load_snapshot(path)?;
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
    // One scripted policy switch (ADR-0013 D7): every agent to KIND at a tick.
    let set_policy = set_policy_opt(o)?;
    let set_policy_at: u64 = get(o, "set-policy-at", 0)?;
    // Decrees (ADR-0014 D8): a rule put in force or taken out at a tick.
    let decrees = decrees(o)?;
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
        "{:>6} {:>6} {:>6} {:>7} {:>6} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>9} {:>9} {:>9}",
        "tick",
        "alive",
        "births",
        "d.starv",
        "d.age",
        "d.viol",
        "xfer",
        "harm",
        "sig",
        "prop",
        "acc",
        "rej",
        "gathered",
        "consumed",
        "food"
    );
    let mut step_time = Duration::ZERO;
    let mut agent_steps = 0u64;
    let (mut gathered, mut consumed, mut births) = (0u64, 0u64, 0u64);
    let (mut xfers, mut harms, mut sigs) = (0u64, 0u64, 0u64);
    let (mut props, mut accs, mut rejs) = (0u64, 0u64, 0u64);
    let mut deaths = [0u64; DeathCause::COUNT];
    // Social facts over the whole run (ADR-0014 4-1), by the gatherer's kind.
    let mut gathers = [0u64; PolicyKind::COUNT];
    let mut encroachments = [0u64; PolicyKind::COUNT];
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
        if let Some(iv) = set_policy.filter(|_| set_policy_at == world.tick) {
            let e = world.intervene(iv)?;
            println!("intervention: {}", serde_json::to_string(&e)?);
            if let Some(l) = log.as_mut() {
                l.push(e)?;
            }
        }
        for (at, iv) in &decrees {
            if *at == world.tick {
                let e = world.intervene(*iv)?;
                println!("intervention: {}", serde_json::to_string(&e)?);
                if let Some(l) = log.as_mut() {
                    l.push(e)?;
                }
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
        xfers += r.transfers as u64;
        harms += r.harms as u64;
        sigs += r.signals as u64;
        props += r.proposals as u64;
        accs += r.acceptances as u64;
        rejs += r.rejections as u64;
        for (acc, d) in deaths.iter_mut().zip(r.deaths_by_cause) {
            *acc += d as u64;
        }
        for k in 0..PolicyKind::COUNT {
            gathers[k] += u64::from(r.gathers[k]);
            encroachments[k] += u64::from(r.encroachments[k]);
        }
        if do_check {
            check(&world)?;
        }
        if world.tick % 100 == 0 {
            println!(
                "{:>6} {:>6} {:>6} {:>7} {:>6} {:>6} {:>5} {:>5} {:>5} {:>5} {:>5} {:>5} {:>9} {:>9} {:>9}",
                world.tick,
                world.agents.len(),
                births,
                deaths[DeathCause::Starvation.index()],
                deaths[DeathCause::OldAge.index()],
                deaths[DeathCause::Violence.index()],
                xfers,
                harms,
                sigs,
                props,
                accs,
                rejs,
                gathered,
                consumed,
                world.resources.total_food()
            );
            (gathered, consumed, births, xfers, harms, sigs) = (0, 0, 0, 0, 0, 0);
            (props, accs, rejs, deaths) = (0, 0, 0, [0; DeathCause::COUNT]);
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
    if world.config.nomos {
        // The run's social facts (ADR-0014 4-1): what a rule would have
        // called violations, had there been one.
        let l = &world.ledger;
        let rate = |e: u64, g: u64| if g > 0 { e as f64 / g as f64 } else { 0.0 };
        let mut by_kind = String::new();
        for k in PolicyKind::ALL {
            let (g, e) = (gathers[k.index()], encroachments[k.index()]);
            if g > 0 {
                let _ = write!(by_kind, " {}={e}/{g}={:.4}", k.name(), rate(e, g));
            }
        }
        println!(
            "nomos: gathers {} encroachments {} ({:.4}/gather, {:.2}/tick){}; promises due {} broken {} open {}",
            l.gathers,
            l.encroachments,
            rate(l.encroachments, l.gathers),
            l.encroachments as f64 / ticks.max(1) as f64,
            by_kind,
            l.promises_due,
            l.promises_broken,
            world.nomos.deals.len()
        );
        // Institutions (ADR-0014 4-2): the rules in force and H1's
        // readings S = episodes sanctioned / closed, S' = sanctions /
        // violations, from the ledger alone.
        let rules: Vec<String> = world
            .nomos
            .rules
            .iter()
            .map(|r| {
                format!(
                    "#{} {}:{}:{}{} since {}",
                    r.id,
                    r.norm.kind.name(),
                    r.norm.standing_ticks,
                    r.norm.sanction.name(),
                    if r.norm.except_sanction {
                        ":except"
                    } else {
                        ""
                    },
                    r.enacted
                )
            })
            .collect();
        let per_kind = |a: &[u64; NormKind::COUNT]| {
            NormKind::ALL
                .iter()
                .map(|k| format!("{}={}", k.name(), a[k.index()]))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let (violations, sanctions) = (
            l.violations.iter().sum::<u64>(),
            l.sanctions.iter().sum::<u64>(),
        );
        let (closed, sanctioned) = (
            l.episodes_closed.iter().sum::<u64>(),
            l.episodes_sanctioned.iter().sum::<u64>(),
        );
        println!(
            "rules: {}; accused now {}; violations {} ({}); sanctions {} ({}); episodes closed {} sanctioned {}; S {:.4} S' {:.4}",
            if rules.is_empty() {
                "none".to_string()
            } else {
                rules.join(", ")
            },
            world.nomos.standing.len(),
            violations,
            per_kind(&l.violations),
            sanctions,
            per_kind(&l.sanctions),
            closed,
            sanctioned,
            rate(sanctioned, closed),
            rate(sanctions, violations)
        );
    }
    if let (Some(t), Some(path)) = (trace.as_mut(), o.get("trace")) {
        t.push(&world);
        std::fs::write(path, serde_json::to_vec(t)?)?;
        println!("trace: {} frames -> {path}", t.frames.len());
    }
    if let Some(path) = o.get("snapshot") {
        save_snapshot(path, &world)?;
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

/// Restores a snapshot and re-executes it against a ledger (ADR-0012):
/// prints the restore time and the hash reached.
fn cmd_replay(o: &Opts) -> R {
    let snap = o.get("snapshot").ok_or("--snapshot required")?;
    let log_path = o.get("log").ok_or("--log required")?;
    let mut world = load_snapshot(snap)?;
    let log = EventLog::read_jsonl(std::io::BufReader::new(std::fs::File::open(log_path)?))?;
    let default_to = log
        .last_tick()
        .map_or(world.tick, |t| t + 1)
        .max(world.tick);
    let to: u64 = get(o, "to", default_to)?;
    let from = world.tick;
    println!(
        "snapshot at tick {from} (hash {:#018x}), ledger {} events up to tick {}",
        world.state_hash(),
        log.len(),
        log.last_tick().map_or("-".to_string(), |t| t.to_string())
    );
    let t0 = Instant::now();
    let stats = multiverse::replay(&mut world, &log, to)?;
    let dt = t0.elapsed().as_secs_f64();
    println!(
        "replayed {from} -> {to}: {} interventions applied, {} events verified, {:.1} ms ({:.0} ticks/s)",
        stats.interventions,
        stats.events_checked,
        dt * 1e3,
        (to - from) as f64 / dt.max(1e-9)
    );
    println!(
        "final tick {} hash {:#018x}",
        world.tick,
        world.state_hash()
    );
    Ok(())
}

/// Runs a root world, forks it twice at `--at` with the same interventions
/// and shows that both branches reach the same hash (ADR-0010 2a-5).
fn cmd_fork(o: &Opts) -> R {
    let ticks: u64 = get(o, "ticks", 2_000)?;
    let at: u64 = get(o, "at", ticks / 2)?;
    let every: u64 = get(o, "every", 500)?;
    let mut interventions = Vec::new();
    if let Some(p) = o.get("scale-food") {
        interventions.push(Intervention::ScaleFood {
            permille: p.parse().map_err(|_| "bad --scale-food")?,
        });
    }
    if o.contains_key("drought-amount") || o.contains_key("drought-interval") {
        interventions.push(Intervention::Drought {
            regrow_amount: get(o, "drought-amount", 0)?,
            regrow_interval: get(o, "drought-interval", 10)?,
        });
    }
    if let Some(iv) = set_policy_opt(o)? {
        interventions.push(iv);
    }
    // Decrees are applied at the fork tick whatever `--enact-at` says:
    // "with vs without rule" (ADR-0014 D8).
    for (_, iv) in decrees(o)? {
        interventions.push(iv);
    }
    if interventions.is_empty() {
        interventions.push(Intervention::ScaleFood { permille: 500 });
    }
    let root = build_world(get(o, "seed", 42)?, o)?;
    let mut mv = Multiverse::new(root, every);
    let r = BranchId(0);

    let t0 = Instant::now();
    mv.step(r, ticks)?;
    println!(
        "root: {ticks} ticks in {:.1} ms, {} snapshots every {every}, ledger {} events",
        t0.elapsed().as_secs_f64() * 1e3,
        mv.branch(r)?.snapshot_ticks().count(),
        mv.branch(r)?.log().len()
    );
    for t in [at.saturating_sub(every / 2), at, ticks] {
        let t0 = Instant::now();
        let w = mv.state_at(r, t)?;
        println!(
            "state_at({t:>6}): {:>7.2} ms  alive {:>5}  hash {:#018x}",
            t0.elapsed().as_secs_f64() * 1e3,
            w.agents.len(),
            w.state_hash()
        );
    }

    println!("fork at {at} with {interventions:?}");
    let t0 = Instant::now();
    let a = mv.fork(r, at, &interventions)?;
    let fork_ms = t0.elapsed().as_secs_f64() * 1e3;
    let b = mv.fork(r, at, &interventions)?;
    mv.step(a, ticks - at)?;
    mv.step(b, ticks - at)?;
    println!(
        "{:>6} {:>6} {:>9} {:>6} {:>6} {:>18}",
        "branch", "parent", "forked_at", "tick", "alive", "hash"
    );
    for br in mv.branches() {
        println!(
            "{:>6} {:>6} {:>9} {:>6} {:>6} {:#018x}",
            br.id().0,
            br.parent().map_or("-".to_string(), |p| p.0.to_string()),
            br.forked_at().map_or("-".to_string(), |t| t.to_string()),
            br.head().tick,
            br.head().agents.len(),
            br.head().state_hash()
        );
    }
    let (ha, hb, hr) = (
        mv.branch(a)?.head().state_hash(),
        mv.branch(b)?.head().state_hash(),
        mv.branch(r)?.head().state_hash(),
    );
    println!(
        "fork took {fork_ms:.2} ms; branches 1 and 2 {}; root differs from forks: {}",
        if ha == hb { "MATCH" } else { "MISMATCH" },
        hr != ha
    );
    if ha != hb {
        return Err("fork is not reproducible".into());
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
            Event::Transferred { .. } => "transferred",
            Event::Harmed { .. } => "harmed",
            Event::Acted { .. } => "acted",
            Event::Signaled { .. } => "signaled",
            Event::Proposed { .. } => "proposed",
            Event::Accepted { .. } => "accepted",
            Event::Rejected { .. } => "rejected",
            Event::Sanctioned { .. } => "sanctioned",
            Event::Violated { .. } => "violated",
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

/// Counts over a stretch of ticks: one sampling interval of the series, or
/// the analysis window of the summary (EXP-002, ADR-0013 3-8).
#[derive(Clone, Default)]
struct Tally {
    ticks: u64,
    /// Agents alive at the start of each tick, summed: the decisions made.
    agent_ticks: u64,
    gathered: u64,
    consumed: u64,
    births: u64,
    transfers: u64,
    harms: u64,
    signals: u64,
    proposals: u64,
    acceptances: u64,
    rejections: u64,
    deaths: [u64; DeathCause::COUNT],
    /// Decisions by action kind (`Action::kind_index`), successful or not.
    actions: [u64; Action::KIND_COUNT],
    /// Social facts (ADR-0014 4-1), by the gatherer's `PolicyKind`.
    gathers: [u64; PolicyKind::COUNT],
    encroachments: [u64; PolicyKind::COUNT],
    promises_due: u64,
    promises_broken: u64,
    /// Food regrown (`Ledger.food_regrown` deltas).
    regrown: u64,
    /// Standing food after each tick, summed (mean = `/ ticks`).
    food_standing: u64,
    /// Learner row visits (`TickReport.learner_rows`, summed).
    rows: Vec<u64>,
    /// Institutions (ADR-0014 4-2), summed over the kinds.
    violations: u64,
    sanctions: u64,
    episodes_closed: u64,
    episodes_sanctioned: u64,
}

impl Tally {
    /// Column names of `counts_csv`: the EXP-002 columns first (unchanged
    /// in name, order and meaning), then the 4-1 counts, then the 4-2 counts.
    const COUNT_COLUMNS: &'static str = "gathered,consumed,births,transfers,harms,signals,proposals,acceptances,rejections,deaths_starvation,deaths_old_age,deaths_violence,gathers,encroachments,promises_due,promises_broken,regrown,gathers_forager,encroachments_forager,gathers_random,encroachments_random,gathers_reflex,encroachments_reflex,gathers_learner,encroachments_learner,violations,sanctions,episodes_closed,episodes_sanctioned";

    /// Adds one tick: `alive_before` agents decided, `regrown` food units
    /// grew, `standing` is the food on the ground afterwards.
    fn add(&mut self, alive_before: usize, r: &TickReport, regrown: u64, standing: u64) {
        self.ticks += 1;
        self.agent_ticks += alive_before as u64;
        self.gathered += r.food_gathered;
        self.consumed += r.food_consumed;
        self.births += u64::from(r.births);
        self.transfers += u64::from(r.transfers);
        self.harms += u64::from(r.harms);
        self.signals += u64::from(r.signals);
        self.proposals += u64::from(r.proposals);
        self.acceptances += u64::from(r.acceptances);
        self.rejections += u64::from(r.rejections);
        for (acc, d) in self.deaths.iter_mut().zip(r.deaths_by_cause) {
            *acc += u64::from(d);
        }
        for (acc, a) in self.actions.iter_mut().zip(r.actions) {
            *acc += u64::from(a);
        }
        for k in 0..PolicyKind::COUNT {
            self.gathers[k] += u64::from(r.gathers[k]);
            self.encroachments[k] += u64::from(r.encroachments[k]);
        }
        self.promises_due += r.promises_settled.len() as u64;
        self.promises_broken += r.promises_settled.iter().filter(|d| !d.kept()).count() as u64;
        self.regrown += regrown;
        self.food_standing += standing;
        self.violations += r.violations.iter().map(|&v| u64::from(v)).sum::<u64>();
        self.sanctions += r.sanctions.iter().map(|&v| u64::from(v)).sum::<u64>();
        self.episodes_closed += r.episodes_closed.iter().map(|&v| u64::from(v)).sum::<u64>();
        self.episodes_sanctioned += r
            .episodes_sanctioned
            .iter()
            .map(|&v| u64::from(v))
            .sum::<u64>();
        if !r.learner_rows.is_empty() {
            if self.rows.len() < r.learner_rows.len() {
                self.rows.resize(r.learner_rows.len(), 0);
            }
            for (acc, v) in self.rows.iter_mut().zip(&r.learner_rows) {
                *acc += u64::from(*v);
            }
        }
    }

    fn per_tick(&self, n: u64) -> f64 {
        n as f64 / self.ticks.max(1) as f64
    }

    fn per_agent_tick(&self, n: u64) -> f64 {
        n as f64 / self.agent_ticks.max(1) as f64
    }

    fn gathers_total(&self) -> u64 {
        self.gathers.iter().sum()
    }

    fn encroachments_total(&self) -> u64 {
        self.encroachments.iter().sum()
    }

    /// `encroachments / gathers` of one kind, `None` when that kind gathered
    /// nothing (written as an empty CSV cell).
    fn encroach_rate(&self, kind: PolicyKind) -> Option<f64> {
        let (g, e) = (self.gathers[kind.index()], self.encroachments[kind.index()]);
        (g > 0).then(|| e as f64 / g as f64)
    }

    fn counts_csv(&self) -> String {
        let mut s = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            self.gathered,
            self.consumed,
            self.births,
            self.transfers,
            self.harms,
            self.signals,
            self.proposals,
            self.acceptances,
            self.rejections,
            self.deaths[DeathCause::Starvation.index()],
            self.deaths[DeathCause::OldAge.index()],
            self.deaths[DeathCause::Violence.index()],
            self.gathers_total(),
            self.encroachments_total(),
            self.promises_due,
            self.promises_broken,
            self.regrown,
        );
        for k in 0..PolicyKind::COUNT {
            let _ = write!(s, ",{},{}", self.gathers[k], self.encroachments[k]);
        }
        let _ = write!(
            s,
            ",{},{},{},{}",
            self.violations, self.sanctions, self.episodes_closed, self.episodes_sanctioned
        );
        s
    }

    /// H1's S: episodes sanctioned / episodes closed (`None` without a
    /// closed episode).
    fn s(&self) -> Option<f64> {
        (self.episodes_closed > 0)
            .then(|| self.episodes_sanctioned as f64 / self.episodes_closed as f64)
    }

    /// H1's S': sanctions / violations (`None` without a violation).
    fn s_prime(&self) -> Option<f64> {
        (self.violations > 0).then(|| self.sanctions as f64 / self.violations as f64)
    }
}

/// Population mean of the living learners' Q tables, cell by cell (fixed
/// point). `None` without learners. All tables have `learner_states × 8`
/// cells (`check_invariants`).
fn mean_q_table(world: &World) -> Option<Vec<f64>> {
    let mut sum: Vec<f64> = Vec::new();
    let mut n = 0u32;
    for a in &world.agents {
        if let Some(PolicyTable::Learner { q }) = &a.params.table {
            if sum.is_empty() {
                sum = vec![0.0; q.len()];
            }
            for (s, &v) in sum.iter_mut().zip(q) {
                *s += f64::from(v);
            }
            n += 1;
        }
    }
    if n == 0 {
        return None;
    }
    for s in &mut sum {
        *s /= f64::from(n);
    }
    Some(sum)
}

/// Mean |Q| per cell over the living learners, in reward units (one unit =
/// 100 milli-energy): how far from the blank table the population has moved.
fn q_abs_mean(world: &World) -> Option<f64> {
    let (mut sum, mut cells) = (0.0f64, 0u64);
    for a in &world.agents {
        if let Some(PolicyTable::Learner { q }) = &a.params.table {
            sum += q.iter().map(|&v| f64::from(v).abs()).sum::<f64>();
            cells += q.len() as u64;
        }
    }
    (cells > 0).then(|| sum / cells as f64 / Q_UNIT as f64)
}

/// Mean |w − w₀| per weight over the living reflex agents, in units of 1.0
/// (`REFLEX_UNIT`), with w₀ the forager-equivalent weights every reflex
/// lineage started from: how far mutation has carried the population.
fn reflex_weight_drift(world: &World) -> Option<f64> {
    let w0 = forager_weights();
    let (mut sum, mut cells) = (0.0f64, 0u64);
    for a in &world.agents {
        if let Some(PolicyTable::Reflex { weights }) = &a.params.table {
            sum += weights
                .iter()
                .zip(&w0)
                .map(|(&w, &b)| f64::from(w - b).abs())
                .sum::<f64>();
            cells += weights.len() as u64;
        }
    }
    (cells > 0).then(|| sum / cells as f64 / f64::from(REFLEX_UNIT))
}

/// Mean `hunger_permille` of the living (the one scalar mutation moves).
fn hunger_mean(world: &World) -> Option<f64> {
    let n = world.agents.len();
    (n > 0).then(|| {
        world
            .agents
            .iter()
            .map(|a| f64::from(a.params.hunger_permille))
            .sum::<f64>()
            / n as f64
    })
}

/// Per Q row, how many living learners would pick each action kind greedily
/// (the row's unique maximum); the last column counts rows tied at the top,
/// which the kernel breaks by lot.
fn greedy_counts(world: &World, rows: usize) -> Vec<[u32; Action::KIND_COUNT + 1]> {
    let mut out = vec![[0u32; Action::KIND_COUNT + 1]; rows];
    for a in &world.agents {
        if let Some(PolicyTable::Learner { q }) = &a.params.table {
            for (row, cells) in q.chunks_exact(Action::KIND_COUNT).enumerate().take(rows) {
                let best = cells.iter().copied().max().unwrap_or(0);
                let mut winners = cells.iter().enumerate().filter(|(_, &v)| v == best);
                let first = winners.next().map_or(0, |(k, _)| k);
                let col = if winners.next().is_some() {
                    Action::KIND_COUNT
                } else {
                    first
                };
                out[row][col] += 1;
            }
        }
    }
    out
}

fn opt_csv(v: Option<f64>, digits: usize) -> String {
    v.map_or(String::new(), |x| format!("{x:.digits$}"))
}

/// Runs several seeds and summarises each over the last `--window` percent of
/// ticks. Writes `<out>_summary.csv` (one row per seed: the EXP-001 columns,
/// then the window's counts per tick and per agent-tick, death causes, action
/// mix and the table statistics of EXP-002), `<out>_series.csv` (every
/// `--sample` ticks and at the end: population, food and the counts of the
/// interval since the previous row) and, when learners are alive at the end
/// of any seed, `<out>_qpolicy.csv` (per Q row, how many living learners
/// prefer each action kind; the row's bits are those of
/// `policy::learner::state`, folded above `log2(learner_states)`).
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
        "seed,policy,final_alive,extinct_at,win_mean,win_min,win_max,win_max_over_min,births,deaths_starvation,deaths_old_age,deaths_violence,mutation,learner_states,learner_epsilon,win_ticks,win_agent_ticks,win_consumed_per_agent_tick,win_gathered_per_agent_tick,win_births_per_tick,win_transfers_per_tick,win_harms_per_tick,win_signals_per_tick,win_proposals_per_tick,win_acceptances_per_tick,win_rejections_per_tick,win_deaths_starvation,win_deaths_old_age,win_deaths_violence,win_act_move,win_act_gather,win_act_consume,win_act_rest,win_act_transfer,win_act_store,win_act_harm,win_act_signal,hunger_permille_mean,q_abs_mean,q_delta_window,reflex_weight_drift,nomos,learner_health_weight,win_gathers_per_tick,win_encroachments_per_tick,win_encroach_rate,win_encroach_rate_forager,win_encroach_rate_random,win_encroach_rate_reflex,win_encroach_rate_learner,win_promises_due_per_tick,win_promises_broken_per_tick,win_food_standing_mean,win_regrown_per_tick,rules,win_violations_per_tick,win_sanctions_per_tick,win_episodes_closed,win_episodes_sanctioned,win_s,win_s_prime,ticks_per_s\n",
    );
    let decrees = decrees(o)?;
    let mut series = format!("seed,tick,alive,food,{}\n", Tally::COUNT_COLUMNS);
    let mut qpolicy = String::from(
        "seed,row,hungry,has_food,food_here,food_in_sight,adjacent,upper,heard,learners,move,gather,consume,rest,transfer,store,harm,signal,tie,visits,visits_window\n",
    );
    let mut any_learners = false;
    println!(
        "{:>5} {:>7} {:>8} {:>9} {:>7} {:>7} {:>7} {:>8} {:>8} {:>7} {:>7} {:>8} {:>7} {:>7} {:>7} {:>7} {:>8}",
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
        "d.viol",
        "cons/at",
        "xfer/t",
        "harm/t",
        "sig/t",
        "encr/g",
        "ticks/s"
    );
    let mut means = Vec::new();
    let mut extinctions = 0u64;
    for s in 0..seeds {
        let seed = seed_start + s;
        let mut world = build_world(seed, o)?;
        let (mutation, states, epsilon, nomos, health_weight) = (
            world.config.mutation_enabled,
            world.config.learner_states,
            world.config.learner_epsilon_permille,
            world.config.nomos,
            world.config.learner_health_weight,
        );
        let (mut sum, mut min, mut max, mut count) = (0u64, u64::MAX, 0u64, 0u64);
        let mut extinct_at: Option<u64> = None;
        let mut interval = Tally::default();
        let mut window = Tally::default();
        // Row visits over the whole run (the window's are in `window`).
        let mut run = Tally::default();
        let mut q_at_window_start: Option<Vec<f64>> = None;
        let t0 = Instant::now();
        for _ in 0..ticks {
            if world.tick % sample == 0 {
                let _ = writeln!(
                    series,
                    "{seed},{},{},{},{}",
                    world.tick,
                    world.agents.len(),
                    world.resources.total_food(),
                    interval.counts_csv()
                );
                interval = Tally::default();
            }
            if world.tick == win_start {
                q_at_window_start = mean_q_table(&world);
            }
            for (at, iv) in &decrees {
                if *at == world.tick {
                    world.intervene(*iv)?;
                }
            }
            let alive_before = world.agents.len();
            let regrown_before = world.ledger.food_regrown;
            let r = world.step();
            if do_check {
                check(&world)?;
            }
            let regrown = world.ledger.food_regrown - regrown_before;
            let standing = world.resources.total_food();
            interval.add(alive_before, &r, regrown, standing);
            run.add(alive_before, &r, regrown, standing);
            let alive = world.agents.len() as u64;
            if world.tick > win_start {
                window.add(alive_before, &r, regrown, standing);
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
        let _ = writeln!(
            series,
            "{seed},{},{},{},{}",
            world.tick,
            world.agents.len(),
            world.resources.total_food(),
            interval.counts_csv()
        );
        let mean = sum as f64 / count.max(1) as f64;
        let ratio = if min > 0 {
            max as f64 / min as f64
        } else {
            f64::INFINITY
        };
        let l = &world.ledger;
        let extinct = extinct_at.map_or("-".to_string(), |t| t.to_string());
        extinctions += u64::from(extinct_at.is_some());
        means.push(mean);
        let q_delta = match (q_at_window_start, mean_q_table(&world)) {
            (Some(a), Some(b)) if !a.is_empty() && a.len() == b.len() => Some(
                a.iter().zip(&b).map(|(x, y)| (x - y).abs()).sum::<f64>()
                    / a.len() as f64
                    / Q_UNIT as f64,
            ),
            _ => None,
        };
        let cons = window.per_agent_tick(window.consumed);
        let (xfer, harm, sig) = (
            window.per_tick(window.transfers),
            window.per_tick(window.harms),
            window.per_tick(window.signals),
        );
        let encroach_rate = {
            let g = window.gathers_total();
            if g > 0 {
                window.encroachments_total() as f64 / g as f64
            } else {
                0.0
            }
        };
        println!(
            "{:>5} {:>7} {:>8} {:>9.1} {:>7} {:>7} {:>8.3} {:>8} {:>8} {:>7} {:>7} {:>8.4} {:>7.2} {:>7.2} {:>7.2} {:>7.4} {:>8.0}",
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
            l.deaths_by_cause[DeathCause::Violence.index()],
            cons,
            xfer,
            harm,
            sig,
            encroach_rate,
            ticks as f64 / secs
        );
        let mut row = format!(
            "{seed},{policy},{},{extinct},{mean:.2},{min},{max},{ratio:.4},{},{},{},{},{},{states},{epsilon},{},{},{cons:.5},{:.5},{:.4},{xfer:.4},{harm:.4},{sig:.4},{:.4},{:.4},{:.4},{},{},{}",
            world.agents.len(),
            l.births,
            l.deaths_by_cause[DeathCause::Starvation.index()],
            l.deaths_by_cause[DeathCause::OldAge.index()],
            l.deaths_by_cause[DeathCause::Violence.index()],
            u8::from(mutation),
            window.ticks,
            window.agent_ticks,
            window.per_agent_tick(window.gathered),
            window.per_tick(window.births),
            window.per_tick(window.proposals),
            window.per_tick(window.acceptances),
            window.per_tick(window.rejections),
            window.deaths[DeathCause::Starvation.index()],
            window.deaths[DeathCause::OldAge.index()],
            window.deaths[DeathCause::Violence.index()],
        );
        for n in window.actions {
            let _ = write!(row, ",{:.5}", window.per_agent_tick(n));
        }
        let _ = write!(
            row,
            ",{},{},{},{}",
            opt_csv(hunger_mean(&world), 1),
            opt_csv(q_abs_mean(&world), 3),
            opt_csv(q_delta, 3),
            opt_csv(reflex_weight_drift(&world), 4),
        );
        // 4-1: the window's social facts, standing food and regrowth.
        let _ = write!(
            row,
            ",{},{health_weight},{:.4},{:.4},{encroach_rate:.5}",
            u8::from(nomos),
            window.per_tick(window.gathers_total()),
            window.per_tick(window.encroachments_total()),
        );
        for k in PolicyKind::ALL {
            let _ = write!(row, ",{}", opt_csv(window.encroach_rate(k), 5));
        }
        let _ = write!(
            row,
            ",{:.4},{:.4},{:.1},{:.4}",
            window.per_tick(window.promises_due),
            window.per_tick(window.promises_broken),
            window.per_tick(window.food_standing),
            window.per_tick(window.regrown),
        );
        // 4-2: the rules in force at the end, the window's violations and
        // sanctions, and H1's S / S'.
        let _ = writeln!(
            row,
            ",{},{:.4},{:.4},{},{},{},{},{:.0}",
            world.nomos.rules.len(),
            window.per_tick(window.violations),
            window.per_tick(window.sanctions),
            window.episodes_closed,
            window.episodes_sanctioned,
            opt_csv(window.s(), 5),
            opt_csv(window.s_prime(), 5),
            ticks as f64 / secs
        );
        summary.push_str(&row);
        if world.agents.iter().any(|a| a.policy == PolicyKind::Learner) {
            any_learners = true;
            for (r, counts) in greedy_counts(&world, usize::from(states))
                .iter()
                .enumerate()
            {
                let bit = |b: u16| u8::from(r as u16 & b != 0);
                let learners: u32 = counts.iter().sum();
                let mut line = format!(
                    "{seed},{r},{},{},{},{},{},{},{},{learners}",
                    bit(state::HUNGRY),
                    bit(state::HAS_FOOD),
                    bit(state::FOOD_HERE),
                    bit(state::FOOD_IN_SIGHT),
                    bit(state::ADJACENT),
                    bit(state::UPPER),
                    bit(state::HEARD)
                );
                for c in counts {
                    let _ = write!(line, ",{c}");
                }
                // Visits (ADR-0014 D10): decisions made in this row over the
                // whole run and over the window.
                let _ = write!(
                    line,
                    ",{},{}",
                    run.rows.get(r).copied().unwrap_or(0),
                    window.rows.get(r).copied().unwrap_or(0)
                );
                qpolicy.push_str(&line);
                qpolicy.push('\n');
            }
        }
    }
    let n = means.len() as f64;
    let grand = means.iter().sum::<f64>() / n;
    let sd = (means.iter().map(|m| (m - grand).powi(2)).sum::<f64>() / n).sqrt();
    println!(
        "window = last {window_pct}% of {ticks} ticks; across {seeds} seeds: mean of window means {grand:.1}, sd {sd:.1}, cv {:.3}, extinct {extinctions}/{seeds}",
        if grand > 0.0 { sd / grand } else { f64::NAN }
    );
    if let Some(prefix) = o.get("out") {
        std::fs::write(format!("{prefix}_summary.csv"), summary)?;
        std::fs::write(format!("{prefix}_series.csv"), series)?;
        let mut wrote = format!("wrote {prefix}_summary.csv and {prefix}_series.csv");
        if any_learners {
            std::fs::write(format!("{prefix}_qpolicy.csv"), qpolicy)?;
            let _ = write!(wrote, " and {prefix}_qpolicy.csv");
        }
        println!("{wrote}");
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
