//! `cz` — run Civilization Zero worlds from the command line.
//!
//!   cz run    [--seed N] [--agents N] [--side N] [--ticks N] [--preset nomos]
//!             [--policy forager|random|mixed|reflex|learner|mixed-learning|institution] [--check]
//!             [--mutate | --no-mutate] [--learner-states N] [--learner-epsilon PERMILLE]
//!             [--harm-cost COST] [--nomos] [--learner-health-weight W] [--record-violations]
//!             [--motion-cost COST] [--no-enforcer-motions] [--record-votes]
//!             [--enforcer-share PERMILLE|none] [--blind] [--signal-radius R] [--memory-slots N]
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
//!             [--window PCT] [--sample K] [--out PREFIX] [--check] [--jobs N]
//!   cz lab run DESIGN.json --out DIR [--jobs N] [--cells a,b] [--seeds S..E] [--check]
//!   cz lab summarize DIR --design DESIGN.json
//!
//! Policies: `reflex` and `mixed-learning` (forager / reflex / learner in
//! rotation) switch mutation on unless `--no-mutate` (ADR-0013 D6).
//! `--learner-states` / `--learner-epsilon` size and explore a learner's Q
//! table (3-7). `--harm-cost COST` is the energy a Harm attempt costs
//! (default 2,000; an axis of EXP-003's grid, ADR-0014 D10). `--set-policy KIND` applies `SetPolicy { who: All, params:
//! Default }` at `--set-policy-at` (default 0) and logs it like a drought.
//! `--nomos` switches the tracking of social facts on (ADR-0014 4-1: first
//! occupancy, encroachments, the audit of accepted deals) and
//! `--learner-health-weight W` adds `W × Δhealth` to a learner's reward.
//! `--preset nomos` starts from `WorldConfig::nomos_preset()` instead of
//! the default config (ADR-0014 D10: the institution world's baseline,
//! fixed by EXP-003 — `nomos` on and its ε / Harm cost / health term);
//! every other flag still overrides the field it names.
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
//! `sweep`, `fork`) take all of these. The assembly (ADR-0014 D7, 4-5):
//! `--policy institution` is the institution world — learners (128 rows)
//! with every tenth founder an `Enforcer`, on `--preset nomos`, the
//! enforcers held at 100‰ of the living by the birth-time quota
//! (`WorldConfig::institution_preset()`, ADR-0014 completion condition 2;
//! 4-9) unless a preset, `--learner-states` or `--enforcer-share` says
//! otherwise — `--motion-cost COST` is
//! what a motion attempt costs (default 2,000), `--no-enforcer-motions`
//! keeps the enforcers from moving the first-occupancy norm themselves
//! (EXP-004 decrees it instead, D11), and `--record-votes` writes a
//! `Voted` event per vote (motions, closings and their effects are always
//! logged). EXP-004's arms (ADR-0014 D11, 4-6): `--enforcer-share
//! PERMILLE` holds the enforcers at that share of the living by a
//! birth-time quota (`WorldConfig::enforcer_share_permille`; a learner's or
//! an enforcer's child is born whichever kind keeps the share; `none` is
//! Phase 3's rule, every child its parent's kind), `--blind`
//! switches `perception.see_standing` off (the learners' norm features read
//! 0: the blind null model), `--signal-radius R` and `--memory-slots N` set
//! the perception's signal radius and the memory's slots (the no-communication
//! and no-memory null models at 0).
//!
//! `cz sweep` is a one-cell experiment of the `cz-lab` crate (ADR-0015
//! D2): the flags become a `Design`, `cz-lab` runs each seed and writes the
//! rows, and the files are the same as before. `cz lab run` runs a design
//! file (cells × seeds, several worlds at once with `--jobs`, the CSVs in
//! seed order whatever the thread count, plus `RUN.json`); `cz lab
//! summarize` reads the CSVs back into per-cell distributions and the
//! design's paired contrasts (`SUMMARY.json` / `SUMMARY.md`). See
//! `lab.rs` and the `cz-lab` crate.
//!
//! `cz sweep --out PREFIX` writes `PREFIX_summary.csv` (one row per seed:
//! population band of the last `--window` percent of ticks, then that
//! window's consumption per agent-tick, transfers / harms / signals /
//! negotiation messages per tick, death causes, action mix, the Q-table or
//! weight statistics of EXP-002, since 4-1 the window's encroachment rate
//! overall and per policy kind, promises due / broken, standing food and
//! regrowth, and since 4-2 the rules in force, violations and sanctions per
//! tick, episodes closed / sanctioned and H1's S = episodes sanctioned /
//! closed and S' = sanctions / violations, and since 4-4 the Harm cost),
//! `PREFIX_series.csv` (every
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
use simulation_core::{
    Charter, DeathCause, Event, EventLog, Intervention, Norm, NormKind, ParamsMode, PolicyKind,
    Sanction, Who, World,
};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

mod lab;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = args.split_first() else {
        usage();
    };
    // `cz lab SUB PATH [--flags]` has positional arguments; the rest are
    // flags only.
    if cmd == "lab" {
        if let Err(e) = lab::cmd_lab(rest) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }
    let opts = parse_opts(rest);
    let result = match cmd.as_str() {
        "run" => cmd_run(&opts),
        "resume" => cmd_resume(&opts),
        "sweep" => lab::cmd_sweep(&opts),
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
        "usage:\n  cz run [--seed N] [--agents N] [--side N] [--ticks N] [--preset nomos]\n         [--policy forager|random|mixed|reflex|learner|mixed-learning|institution] [--mutate | --no-mutate]\n         [--learner-states N] [--learner-epsilon PERMILLE] [--harm-cost COST] [--nomos] [--learner-health-weight W]\n         [--enact KIND[:STANDING[:SANCTION[:EXCEPT]]] [--enact-at TICK]] [--repeal ID [--repeal-at TICK]]\n         [--charter Q,TH,TICKS,AMENDABLE] [--record-violations] [--motion-cost COST] [--no-enforcer-motions] [--record-votes]\n         [--enforcer-share PERMILLE|none] [--blind] [--signal-radius R] [--memory-slots N]\n         [--check] [--trace PATH --every K] [--snapshot PATH]\n         [--drought-at TICK [--drought-amount N] [--drought-interval N]]\n         [--set-policy KIND [--set-policy-at TICK]] [--log PATH] [--record-actions]\n  cz log --path PATH [--from TICK] [--to TICK] [--limit N]\n  cz resume --snapshot PATH [--ticks N] [--check]\n  cz replay --snapshot PATH --log PATH [--to TICK]\n  cz fork [--seed N] [--agents N] [--side N] [--ticks N] [--at TICK] [--every K]\n          [--scale-food PERMILLE] [--drought-amount N --drought-interval N] [--set-policy KIND]\n          [--enact SPEC] [--repeal ID]\n  cz sweep [--seeds N] [--seed-start S] [--ticks N] [--policy ...] [--window PCT]\n           [--sample K] [--out PREFIX] [--check] [--jobs N]\n  cz lab run DESIGN.json --out DIR [--jobs N] [--cells a,b] [--seeds S..E] [--check]\n  cz lab summarize DIR --design DESIGN.json\n  (run, fork and sweep also take --preset nomos, --harm-cost COST, --nomos, --learner-health-weight W, --charter, --record-violations,\n   --motion-cost, --no-enforcer-motions, --record-votes, --enforcer-share, --blind, --signal-radius, --memory-slots;\n   --policy institution = learners (128 rows) + every tenth founder an enforcer on --preset nomos, enforcers held at 100‰ (--enforcer-share overrides);\n   --enact KIND is encroach | broken_promise | harm, SANCTION is harm | reject | harm+reject | none)"
    );
    std::process::exit(2)
}

pub(crate) type Opts = BTreeMap<String, String>;

pub(crate) fn parse_opts(args: &[String]) -> Opts {
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

pub(crate) fn get<T: std::str::FromStr>(o: &Opts, k: &str, default: T) -> Result<T, String> {
    match o.get(k) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| format!("bad value for --{k}: {v}")),
    }
}

pub(crate) type R = Result<(), Box<dyn std::error::Error>>;

/// The world the flags describe (ADR-0015 D2): the flags become a
/// one-cell `cz-lab` `Design` (`lab::design_from_opts`) and `cz-lab` builds
/// the world by the rules that used to live here — the preset, what the
/// policy mix implies, then every flag given explicitly as an override.
fn build_world(seed: u64, o: &Opts) -> Result<World, Box<dyn std::error::Error>> {
    lab::build_world(seed, o)
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
pub(crate) fn parse_charter(spec: &str) -> Result<Charter, String> {
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
pub(crate) fn decrees(o: &Opts) -> Result<Vec<(u64, Intervention)>, Box<dyn std::error::Error>> {
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
        // The assembly (ADR-0014 D7, 4-5): what was moved, carried and
        // defeated over the run, and the floor as it stands.
        println!(
            "assembly: motions {} (carried {}, defeated {}), votes {}, charter {}/{}/{}/{}, floor {}",
            l.motions,
            l.motions_carried,
            l.motions_defeated,
            l.votes,
            world.nomos.charter.quorum_permille,
            world.nomos.charter.threshold_permille,
            world.nomos.charter.vote_ticks,
            if world.nomos.charter.amendable {
                "amendable"
            } else {
                "not amendable"
            },
            match &world.nomos.motion {
                None => "free".to_string(),
                Some(m) => format!(
                    "motion #{} by {} since tick {} ({} ayes, {} nays)",
                    m.id,
                    m.by.0,
                    m.opened,
                    m.ayes.len(),
                    m.nays.len()
                ),
            }
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
            Event::Motioned { .. } => "motioned",
            Event::Voted { .. } => "voted",
            Event::MotionClosed { .. } => "motion_closed",
            Event::Enacted { .. } => "enacted",
            Event::Repealed { .. } => "repealed",
            Event::Chartered { .. } => "chartered",
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
