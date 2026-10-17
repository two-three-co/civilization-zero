//! The wasm surface must not change what the kernel computes. These run
//! natively; `tests/wasm.rs` runs the same kind of check inside wasm.

use cz_web::{
    evolves, policy_code, policy_mix, WasmWorld, AGENT_FIELDS, MARKER_FIELDS, MARKER_HARM,
    MARKER_SIGNAL, MARKER_TRANSFER, POLICY_MIXES,
};
use serde_json::Value;
use simulation_core::{AgentId, Event, Intervention, PolicyKind, World, WorldConfig};

fn direct(seed: u64, ticks: u64) -> World {
    let mut w = World::new(WorldConfig {
        seed,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(ticks);
    w
}

fn hex(w: &World) -> String {
    format!("{:016x}", w.state_hash())
}

/// A small, crowded world: with random walkers in it, transfers, harms and
/// signals happen every tick, so the social surface has something to show.
fn crowded(seed: u64) -> WorldConfig {
    WorldConfig {
        seed,
        width: 32,
        height: 32,
        initial_agents: 200,
        patch_count: 3,
        ..WorldConfig::default()
    }
}

#[test]
fn wrapper_reproduces_the_kernel_exactly() {
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager", "").unwrap();
    ww.step(200).unwrap();
    let d = direct(42, 200);
    assert_eq!(ww.state_hash_hex(), hex(&d));
    assert_eq!(
        (ww.tick(), ww.frontier(), ww.alive()),
        (200, 200, d.agents.len() as u32)
    );
    assert_eq!(ww.food_total() as u64, d.resources.total_food());
    assert_eq!(ww.agents().len(), d.agents.len() * AGENT_FIELDS);
    let food = ww.food();
    assert_eq!(food.len(), 128 * 128);
    assert_eq!(
        food.iter().map(|&f| f as u64).sum::<u64>(),
        d.resources.total_food(),
        "no cell above 255 in this world, so the clamped copy sums exactly"
    );
    assert_eq!(World::from_bytes(&ww.snapshot_bytes()).unwrap(), d);
}

#[test]
fn seek_back_then_step_rejoins_the_head() {
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager", "").unwrap();
    ww.step(300).unwrap();
    ww.seek(120).unwrap();
    assert_eq!((ww.tick(), ww.frontier()), (120, 300));
    assert_eq!(ww.state_hash_hex(), hex(&direct(42, 120)));

    ww.step(50).unwrap(); // re-execution behind the head
    assert_eq!((ww.tick(), ww.frontier()), (170, 300));
    assert_eq!(ww.state_hash_hex(), hex(&direct(42, 170)));

    ww.step(200).unwrap(); // 130 to rejoin the head, then 70 new ticks
    assert_eq!((ww.tick(), ww.frontier()), (370, 370));
    assert_eq!(ww.state_hash_hex(), hex(&direct(42, 370)));

    ww.seek(500).unwrap(); // forward past the head extends it
    assert_eq!((ww.tick(), ww.frontier()), (500, 500));
    assert_eq!(ww.state_hash_hex(), hex(&direct(42, 500)));
    ww.seek(0).unwrap();
    assert_eq!(ww.state_hash_hex(), hex(&direct(42, 0)));
}

#[test]
fn interventions_apply_at_the_head_only_and_are_logged() {
    let mut ww = WasmWorld::new(7, 64, 64, 200, 5, 50, "forager", "").unwrap();
    ww.step(100).unwrap();
    let event = ww
        .intervene(r#"{"op":"scale_food","permille":500}"#)
        .unwrap();
    assert!(
        event.starts_with(r#"{"kind":"intervened","tick":100,"#),
        "{event}"
    );
    ww.step(100).unwrap();

    let mut d = World::new(WorldConfig {
        seed: 7,
        width: 64,
        height: 64,
        initial_agents: 200,
        patch_count: 5,
        ..WorldConfig::default()
    })
    .unwrap();
    d.run(100);
    d.intervene(Intervention::ScaleFood { permille: 500 })
        .unwrap();
    d.run(100);
    assert_eq!(ww.state_hash_hex(), hex(&d));

    let at_100 = ww.events_at(100).unwrap();
    assert!(at_100.contains(r#""op":"scale_food""#), "{at_100}");

    ww.seek(150).unwrap();
    assert!(ww
        .intervene(r#"{"op":"scale_food","permille":500}"#)
        .is_err());
    assert!(ww.intervene("not json").is_err());
    assert!(
        WasmWorld::new(1, 0, 0, 1, 1, 1, "forager", "").is_err(),
        "invalid config is reported"
    );
}

#[test]
fn a_fork_built_from_a_snapshot_matches_multiverse_fork_and_is_reproducible() {
    use multiverse::{BranchId, Multiverse};

    let mut parent = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager", "").unwrap();
    parent.step(400).unwrap();
    parent.seek(300).unwrap(); // the view is the state tick 300 starts with
    let bytes = parent.snapshot_bytes();

    let fork = || {
        let mut child = WasmWorld::from_snapshot(&bytes, 50).unwrap();
        child
            .intervene(r#"{"op":"scale_food","permille":500}"#)
            .unwrap();
        child.step(150).unwrap();
        child
    };
    let (b, c) = (fork(), fork());
    assert_eq!((b.origin(), b.tick(), b.frontier()), (300, 450, 450));
    assert_eq!(
        b.state_hash_hex(),
        c.state_hash_hex(),
        "same fork twice, same history"
    );

    // Bit-identical to forking inside one Multiverse.
    let mut mv = Multiverse::new(
        World::new(WorldConfig {
            seed: 42,
            ..WorldConfig::default()
        })
        .unwrap(),
        100,
    );
    mv.step(BranchId(0), 400).unwrap();
    let f = mv
        .fork(
            BranchId(0),
            300,
            &[Intervention::ScaleFood { permille: 500 }],
        )
        .unwrap();
    mv.step(f, 150).unwrap();
    assert_eq!(b.state_hash_hex(), hex(mv.branch(f).unwrap().head()));

    // The parent is untouched and its own history differs from the fork's.
    parent.seek(450).unwrap();
    assert_eq!(parent.state_hash_hex(), hex(&direct(42, 450)));
    assert_ne!(parent.state_hash_hex(), b.state_hash_hex());

    // A fork seeks within its own history only.
    let mut d = WasmWorld::from_snapshot(&bytes, 50).unwrap();
    assert!(d.seek(299).is_err(), "before the origin");
    d.seek(300).unwrap();
    assert_eq!(d.state_hash_hex(), hex(&direct(42, 300)));
    assert_eq!(d.interventions_json().unwrap(), "[]");
    assert_eq!(
        b.interventions_json()
            .unwrap()
            .matches("scale_food")
            .count(),
        1
    );

    assert!(WasmWorld::from_snapshot(b"not a snapshot", 50).is_err());
}

#[test]
fn intervention_history_comes_from_the_ledger() {
    let mut ww = WasmWorld::new(7, 64, 64, 200, 5, 50, "forager", "").unwrap();
    assert_eq!(ww.interventions_json().unwrap(), "[]");
    ww.step(60).unwrap();
    ww.intervene(r#"{"op":"drought","regrow_amount":0,"regrow_interval":10}"#)
        .unwrap();
    ww.step(40).unwrap();
    ww.intervene(r#"{"op":"spawn_agents","n":3,"pos":{"x":1,"y":2},"policy":"Random"}"#)
        .unwrap();
    let history = ww.interventions_json().unwrap();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&history).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0]["tick"], 60);
    assert_eq!(parsed[0]["intervention"]["op"], "drought");
    assert_eq!(parsed[1]["tick"], 100);
    assert_eq!(parsed[1]["intervention"]["n"], 3);
    // Seeking back does not change what the ledger holds.
    ww.seek(10).unwrap();
    assert_eq!(ww.interventions_json().unwrap(), history);
}

// --- ADR-0013 3-9a: policy mix and the social surface ---

type Rule = fn(u32) -> PolicyKind;

#[test]
fn the_policy_mix_is_the_cli_rule_and_foragers_alone_are_socially_inert() {
    // `cz run --policy NAME`'s assignment, and its mutation default: the
    // evolving mixes mutate unless `--no-mutate` (ADR-0013 D6).
    let rules: [(&str, Rule, bool); 6] = [
        ("forager", |_| PolicyKind::Forager, false),
        ("random", |_| PolicyKind::Random, false),
        (
            "mixed",
            |i| {
                if i % 2 == 0 {
                    PolicyKind::Forager
                } else {
                    PolicyKind::Random
                }
            },
            false,
        ),
        ("reflex", |_| PolicyKind::Reflex, true),
        ("learner", |_| PolicyKind::Learner, false),
        (
            "mixed-learning",
            |i| match i % 3 {
                0 => PolicyKind::Forager,
                1 => PolicyKind::Reflex,
                _ => PolicyKind::Learner,
            },
            true,
        ),
    ];
    let mut hashes = Vec::new();
    for (name, rule, mutates) in rules {
        assert_eq!(evolves(name), mutates, "{name}");
        let mut ww = WasmWorld::new(9, 32, 32, 200, 3, 25, name, "").unwrap();
        ww.step(60).unwrap();
        let mut d = World::generate(
            WorldConfig {
                mutation_enabled: mutates,
                ..crowded(9)
            },
            rule,
        )
        .unwrap();
        d.run(60);
        assert_eq!(ww.state_hash_hex(), hex(&d), "{name}");
        hashes.push(ww.state_hash_hex());
        // The policy column tells all four kinds apart (3-9c).
        for kind in PolicyKind::ALL {
            let code = policy_code(kind);
            assert_eq!(
                ww.agents()
                    .chunks(AGENT_FIELDS)
                    .filter(|a| a[5] == code)
                    .count(),
                d.agents.iter().filter(|a| a.policy == kind).count(),
                "{name}: {kind:?}"
            );
        }
        let ledger: Value = serde_json::from_str(&ww.ledger_json().unwrap()).unwrap();
        let social = ["transfers", "harms", "signals"]
            .iter()
            .map(|k| ledger[k].as_u64().unwrap())
            .sum::<u64>();
        match name {
            "forager" => {
                assert_eq!(social, 0, "foragers never transfer, harm or signal");
                assert!(ww.social().is_empty());
            }
            // Reflex starts from forager-equivalent weights (3-6); whether
            // a mutated child has interacted by tick 60 is not promised.
            "reflex" => {}
            _ => assert!(
                social > 0,
                "{name}: random walkers and blank learners interact"
            ),
        }
    }
    hashes.sort();
    hashes.dedup();
    assert_eq!(hashes.len(), 6, "six mixes, six worlds");
    for name in POLICY_MIXES {
        assert!(policy_mix(name).is_ok(), "{name}");
    }
    assert!(WasmWorld::new(9, 32, 32, 200, 3, 25, "walker", "").is_err());
    assert!(policy_mix("").is_err());
    assert_eq!(
        PolicyKind::ALL.map(policy_code),
        [0, 1, 2, 3],
        "forager / random / reflex / learner, the kernel's order"
    );
}

/// 3-9c: "fixed vs learning" is a fork plus `SetPolicy`, and the page does
/// it with the same `fromSnapshot` + `intervene(json)` as any other fork.
/// Done twice it reaches the same hash, equal to `Multiverse::fork` with the
/// same intervention; the fork reports every agent as a learner with its Q
/// table and the kernel's record of its last step, and the ledger totals
/// the curves are drawn from carry on from the parent's.
#[test]
fn set_policy_crosses_the_boundary_and_the_compare_fork_is_reproducible() {
    use multiverse::{BranchId, Multiverse};
    use simulation_core::{ParamsMode, Who};

    let parse = |s: String| -> Value { serde_json::from_str(&s).unwrap() };
    let codes =
        |w: &WasmWorld| -> Vec<i32> { w.agents().chunks(AGENT_FIELDS).map(|a| a[5]).collect() };
    let mut parent = WasmWorld::new(9, 32, 32, 200, 3, 25, "forager", "").unwrap();
    parent.step(100).unwrap();
    parent.seek(60).unwrap();
    let bytes = parent.snapshot_bytes();
    let ledger_at_60 = parent.ledger_json().unwrap();
    let switch = r#"{"op":"set_policy","who":"all","kind":"Learner","params":"default"}"#;

    let fork = || {
        let mut b = WasmWorld::from_snapshot(&bytes, 25).unwrap();
        assert_eq!(
            b.ledger_json().unwrap(),
            ledger_at_60,
            "the ledger is state"
        );
        let event = b.intervene(switch).unwrap();
        assert!(
            event.starts_with(r#"{"kind":"intervened","tick":60,"#)
                && event.contains(r#""op":"set_policy""#),
            "{event}"
        );
        assert!(
            codes(&b).iter().all(|&c| c == 3),
            "everyone switched at once"
        );
        b.step(100).unwrap();
        b
    };
    let (b, c) = (fork(), fork());
    assert_eq!(
        b.state_hash_hex(),
        c.state_hash_hex(),
        "same compare twice, same history"
    );
    assert_eq!((b.origin(), b.tick(), b.frontier()), (60, 160, 160));
    assert!(
        codes(&b).iter().all(|&c| c == 3),
        "children inherit the kind"
    );
    assert_eq!(
        b.interventions_json()
            .unwrap()
            .matches("set_policy")
            .count(),
        1
    );

    let mut mv = Multiverse::new(
        World::generate(crowded(9), |_| PolicyKind::Forager).unwrap(),
        25,
    );
    mv.step(BranchId(0), 100).unwrap();
    let f = mv
        .fork(
            BranchId(0),
            60,
            &[Intervention::SetPolicy {
                who: Who::All,
                kind: PolicyKind::Learner,
                params: ParamsMode::Default,
            }],
        )
        .unwrap();
    mv.step(f, 100).unwrap();
    assert_eq!(b.state_hash_hex(), hex(mv.branch(f).unwrap().head()));

    // A is untouched: still foragers, a different history from tick 60 on.
    parent.seek(160).unwrap();
    assert!(codes(&parent).iter().all(|&c| c == 0));
    assert_ne!(parent.state_hash_hex(), b.state_hash_hex());

    // What the inspector shows for a learner: its policy, the Q table (32
    // rows × 8 kinds, no longer blank) and the kernel-written last step.
    let id = b.agents()[0] as u32;
    let a = parse(b.agent_json(id).unwrap());
    assert_eq!(a["policy"], "Learner");
    let q = a["params"]["table"]["learner"]["q"].as_array().unwrap();
    assert_eq!(q.len(), 32 * 8);
    assert!(
        q.iter().any(|v| v.as_i64().unwrap() != 0),
        "100 ticks taught it something"
    );
    let step = &a["last_step"];
    assert!(step["state"].as_u64().unwrap() < 32, "{step}");
    assert!(step["action"].as_u64().unwrap() < 8, "{step}");
    assert!(step["reward"].is_i64(), "{step}");
    let p = parse(parent.agent_json(parent.agents()[0] as u32).unwrap());
    assert_eq!(
        (&p["params"]["table"], &p["last_step"]),
        (&Value::Null, &Value::Null)
    );

    // The curves' inputs are in the ledger, and learners with a blank table
    // try every kind, so the fork's social totals grow from the parent's.
    let (lb, lp) = (parse(b.ledger_json().unwrap()), parse(ledger_at_60));
    for k in [
        "food_consumed",
        "transfers",
        "harms",
        "signals",
        "births",
        "deaths",
    ] {
        assert!(
            lb[k].as_u64().unwrap() >= lp[k].as_u64().unwrap(),
            "{k}: {lb} vs {lp}"
        );
    }
    assert!(lb["food_consumed"].as_u64().unwrap() > lp["food_consumed"].as_u64().unwrap());
    assert!(
        lb["transfers"].as_u64().unwrap()
            + lb["harms"].as_u64().unwrap()
            + lb["signals"].as_u64().unwrap()
            > 0
    );
    assert_eq!(
        lp["transfers"].as_u64().unwrap()
            + lp["harms"].as_u64().unwrap()
            + lp["signals"].as_u64().unwrap(),
        0
    );

    // The other shapes of the record pass through unchanged; an invalid one
    // is rejected without touching the state; a `Fraction` draw is a
    // function of the snapshot, so the same fraction picks the same agents.
    let fraction = r#"{"op":"set_policy","who":{"fraction":{"permille":500}},"kind":"Reflex","params":"keep"}"#;
    let mut w = WasmWorld::from_snapshot(&bytes, 25).unwrap();
    let before = w.state_hash_hex();
    assert!(w
        .intervene(r#"{"op":"set_policy","who":{"fraction":{"permille":1001}},"kind":"Reflex","params":"keep"}"#)
        .is_err());
    assert!(w
        .intervene(r#"{"op":"set_policy","who":"all","kind":"Walker","params":"default"}"#)
        .is_err());
    assert_eq!(w.state_hash_hex(), before);
    w.intervene(fraction).unwrap();
    let half = codes(&w);
    let reflex = half.iter().filter(|&&c| c == 2).count();
    assert!(
        reflex > 0 && reflex < half.len(),
        "{reflex} of {}",
        half.len()
    );
    let mut again = WasmWorld::from_snapshot(&bytes, 25).unwrap();
    again.intervene(fraction).unwrap();
    assert_eq!(codes(&again), half);
    let first = w.agents()[0] as u32;
    w.intervene(&format!(
        r#"{{"op":"set_policy","who":{{"id":{first}}},"kind":"Random","params":"default"}}"#
    ))
    .unwrap();
    assert_eq!(w.agents()[5], 1);
    assert_eq!(&codes(&w)[1..], &half[1..]);
    w.step(20).unwrap();
    assert_eq!(
        w.interventions_json()
            .unwrap()
            .matches("set_policy")
            .count(),
        2
    );
    // Behind the head it is a fork, as for every intervention.
    w.seek(70).unwrap();
    assert!(w.intervene(switch).is_err());
}

/// The learning world's golden (ADR-0013 completion criterion 4): the
/// default world with every founder a `Learner`, 200 ticks, as
/// `tests/wasm.rs` checks under wasm. Pinned here natively so the two
/// cannot drift apart silently; if the Learner's rule changes on purpose,
/// update both and STATUS's golden table.
pub const LEARNER_GOLDEN_200_HEX: &str = "cb8bc0a9772c09a4";

#[test]
fn the_learner_world_golden_is_pinned_for_the_wasm_check() {
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100, "learner", "").unwrap();
    ww.step(200).unwrap();
    let mut d = World::generate(
        WorldConfig {
            seed: 42,
            ..WorldConfig::default()
        },
        |_| PolicyKind::Learner,
    )
    .unwrap();
    d.run(200);
    assert_eq!(ww.state_hash_hex(), hex(&d));
    assert_eq!(
        ww.dynamics_hash_hex(),
        format!("{:016x}", d.dynamics_hash())
    );
    assert_eq!(ww.dynamics_hash_hex(), LEARNER_GOLDEN_200_HEX);
    assert_ne!(
        ww.dynamics_hash_hex(),
        "658ee873d1866501",
        "not the forager's"
    );
    assert!(ww.agents().chunks(AGENT_FIELDS).all(|a| a[5] == 3));
}

#[test]
fn social_markers_are_the_ledgers_last_tick_placed_in_the_view() {
    let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    let mut d = World::generate(crowded(3), |_| PolicyKind::Random).unwrap();
    assert!(ww.social().is_empty(), "nothing has happened before tick 0");
    let mut per_tick = vec![Vec::new()];
    let mut seen = [false; 3];
    for t in 1..=60u64 {
        let report = d.step();
        ww.step(1).unwrap();
        let markers = ww.social();
        assert_eq!(markers.len() % MARKER_FIELDS, 0);

        // What the kernel itself reported for the tick, placed at the
        // positions the tick ended with (the view's).
        let pos = |id: AgentId| d.agents.iter().find(|a| a.id == id).map(|a| a.pos);
        let mut expected = Vec::new();
        for e in &report.events {
            let (kind, actor, other, value) = match e {
                Event::Transferred {
                    from, to, amount, ..
                } => (MARKER_TRANSFER, *from, *to, *amount as i32),
                Event::Harmed {
                    by, target, damage, ..
                } => (MARKER_HARM, *by, *target, *damage as i32),
                _ => continue,
            };
            let (x, y, x2, y2) = match (pos(actor), pos(other)) {
                (Some(p), Some(q)) => (p.x as i32, p.y as i32, q.x as i32, q.y as i32),
                (Some(p), None) | (None, Some(p)) => (p.x as i32, p.y as i32, -1, -1),
                (None, None) => continue,
            };
            expected.extend_from_slice(&[kind, (t - 1) as i32, x, y, x2, y2, value]);
        }
        for a in &d.agents {
            if let Some(s) = a.last_signal.filter(|s| s.tick == t - 1) {
                expected.extend_from_slice(&[
                    MARKER_SIGNAL,
                    (t - 1) as i32,
                    a.pos.x as i32,
                    a.pos.y as i32,
                    -1,
                    -1,
                    s.message.value() as i32,
                ]);
            }
        }
        assert_eq!(markers, expected, "tick {t}");
        for m in markers.chunks(MARKER_FIELDS) {
            seen[m[0] as usize] = true;
        }
        per_tick.push(markers);
    }
    assert_eq!(seen, [true; 3], "transfers, harms and signals all occurred");

    // The totals are the view's ledger.
    let ledger: Value = serde_json::from_str(&ww.ledger_json().unwrap()).unwrap();
    assert_eq!(ledger["transfers"], d.ledger.transfers);
    assert_eq!(ledger["food_transferred"], d.ledger.food_transferred);
    assert_eq!(ledger["harms"], d.ledger.harms);
    assert_eq!(ledger["signals"], d.ledger.signals);
    assert_eq!(ledger["births"], d.ledger.births);
    assert_eq!(ledger["deaths_by_cause"][2], d.ledger.deaths_by_cause[2]);

    // Markers are a function of the view, not of how it was reached.
    for t in [60u32, 37, 25, 1, 0] {
        ww.seek(t).unwrap();
        assert_eq!(ww.social(), per_tick[t as usize], "after seek to {t}");
    }

    // A fork's ledger starts at its origin (fromSnapshot has no past), so
    // at the origin only the signals, which live in the state, remain.
    ww.seek(40).unwrap();
    let child = WasmWorld::from_snapshot(&ww.snapshot_bytes(), 25).unwrap();
    let signals_only: Vec<i32> = per_tick[40]
        .chunks(MARKER_FIELDS)
        .filter(|m| m[0] == MARKER_SIGNAL)
        .flatten()
        .copied()
        .collect();
    assert_eq!(child.social(), signals_only);
    assert!(!signals_only.is_empty());
}

#[test]
fn agent_json_is_the_views_agent_with_its_memory_and_last_signal() {
    let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    ww.step(40).unwrap();
    let mut d = World::generate(crowded(3), |_| PolicyKind::Random).unwrap();
    d.run(40);
    assert_eq!(ww.alive() as usize, d.agents.len());

    let a = d
        .agents
        .iter()
        .find(|a| !a.recent.is_empty() && a.last_signal.is_some())
        .expect("in a crowded random world someone remembers something and has spoken");
    let json = ww.agent_json(a.id.0 as u32).unwrap();
    let v: Value = serde_json::from_str(&json).unwrap();
    // The kernel's `Agent` as serde renders it, plus the 4-7 fields (no
    // institutions here: accused of nothing, no motion to vote on).
    let mut expected = serde_json::to_value(a).unwrap();
    expected["standing"] = Value::Array(vec![]);
    expected["voted"] = Value::Bool(false);
    assert_eq!(v, expected);
    assert_eq!(v["id"], a.id.0);
    assert_eq!(v["pos"]["x"], a.pos.x as u64);
    assert_eq!(v["policy"], "Random");
    assert!(v["last_signal"]["tick"].is_u64());
    let recent = v["recent"].as_array().unwrap();
    assert_eq!(recent.len(), a.recent.len());
    assert!(recent.iter().all(|r| {
        r["tick"].is_u64() && r["other"].is_u64() && r["kind"].is_string() && r["value"].is_i64()
    }));
    assert!(
        recent
            .windows(2)
            .all(|p| p[0]["tick"].as_u64() <= p[1]["tick"].as_u64()),
        "oldest first"
    );

    // Absent is `null`, not an error.
    assert_eq!(ww.agent_json(u32::MAX).unwrap(), "null");

    // The view decides: at tick 0 nobody has interacted yet.
    ww.seek(0).unwrap();
    let v0: Value = serde_json::from_str(&ww.agent_json(a.id.0 as u32).unwrap()).unwrap();
    assert_eq!(v0["recent"], Value::Array(vec![]));
    assert_eq!(v0["last_signal"], Value::Null);
    ww.seek(40).unwrap();
    assert_eq!(ww.agent_json(a.id.0 as u32).unwrap(), json);
}

#[test]
fn agent_events_json_filters_the_ledger_by_agent_and_window() {
    let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    ww.step(60).unwrap();
    let all: Vec<Value> = (0..60)
        .flat_map(|t| serde_json::from_str::<Vec<Value>>(&ww.events_at(t).unwrap()).unwrap())
        .collect();
    // Which fields name agents depends on the kind: a proposal's `by` is a
    // deadline, a harm's `by` is the attacker.
    let mentions = |e: &Value, id: u64| {
        let keys: &[&str] = match e["kind"].as_str().unwrap() {
            "born" => &["child", "parent"],
            "died" | "acted" | "signaled" => &["agent"],
            "transferred" | "proposed" | "accepted" | "rejected" => &["from", "to"],
            "harmed" => &["by", "target"],
            "intervened" => {
                return e["intervention"]["op"] == "remove_agent" && e["intervention"]["id"] == id
            }
            k => panic!("unexpected event kind {k}"),
        };
        keys.iter().any(|k| e[k] == id)
    };
    let id = (0..200u64)
        .max_by_key(|&id| all.iter().filter(|e| mentions(e, id)).count())
        .unwrap();
    let want: Vec<&Value> = all.iter().filter(|e| mentions(e, id)).collect();
    assert!(want.len() > 1);
    let got: Vec<Value> =
        serde_json::from_str(&ww.agent_events_json(id as u32, 0, 60).unwrap()).unwrap();
    assert_eq!(got.iter().collect::<Vec<_>>(), want);

    // Half-open window; empty and inverted windows are empty.
    let tail: Vec<Value> =
        serde_json::from_str(&ww.agent_events_json(id as u32, 50, 60).unwrap()).unwrap();
    assert_eq!(
        tail.len(),
        want.iter()
            .filter(|e| e["tick"].as_u64().unwrap() >= 50)
            .count()
    );
    assert!(tail
        .iter()
        .all(|e| (50..60).contains(&e["tick"].as_u64().unwrap())));
    assert_eq!(ww.agent_events_json(id as u32, 60, 60).unwrap(), "[]");
    assert_eq!(ww.agent_events_json(id as u32, 60, 10).unwrap(), "[]");

    // The ledger outlives the agent: its death is there, its memory is not.
    let dead = all
        .iter()
        .find(|e| e["kind"] == "died")
        .expect("random walkers die within 60 ticks of a crowded world");
    let dead_id = dead["agent"].as_u64().unwrap();
    let story: Vec<Value> =
        serde_json::from_str(&ww.agent_events_json(dead_id as u32, 0, 60).unwrap()).unwrap();
    assert!(story.iter().any(|e| e["kind"] == "died"));
    assert_eq!(ww.agent_json(dead_id as u32).unwrap(), "null");
}

// --- ADR-0013 3-9b: negotiation threads ---

mod threads {
    use cz_web::thread::{deals, thread, Answer, Deal, Entry, Role, Status};
    use cz_web::WasmWorld;
    use serde_json::Value;
    use simulation_core::{AgentId, Event};

    const A: AgentId = AgentId(0);
    const B: AgentId = AgentId(1);

    fn proposed(tick: u64, from: AgentId, to: AgentId, give: u8, want: u8, by: u16) -> Event {
        Event::Proposed {
            tick,
            from,
            to,
            give,
            want,
            by,
        }
    }

    fn accepted(
        tick: u64,
        from: AgentId,
        to: AgentId,
        offer_tick: u64,
        give: u8,
        want: u8,
        by: u16,
    ) -> Event {
        Event::Accepted {
            tick,
            from,
            to,
            offer_tick,
            give,
            want,
            by,
        }
    }

    fn rejected(tick: u64, from: AgentId, to: AgentId) -> Event {
        Event::Rejected { tick, from, to }
    }

    fn transferred(tick: u64, from: AgentId, to: AgentId, amount: u32) -> Event {
        Event::Transferred {
            tick,
            from,
            to,
            amount,
        }
    }

    fn harmed(tick: u64, by: AgentId, target: AgentId) -> Event {
        Event::Harmed {
            tick,
            by,
            target,
            damage: 20,
        }
    }

    /// The honest reciprocator of `simulation-core/tests/negotiation.rs`,
    /// as the ledger records it: A offers 2 for 3 within 10 ticks, B
    /// accepts, A gives, B repays. Read as a thread it is one deal,
    /// fulfilled at the repayment; read earlier it is still open.
    #[test]
    fn the_happy_path_is_one_deal_fulfilled_at_the_repayment() {
        let log = [
            proposed(0, A, B, 2, 3, 10),
            accepted(1, B, A, 0, 2, 3, 10),
            transferred(3, A, B, 2),
            transferred(4, B, A, 3),
        ];
        let t = thread(&log, A, B, 0, 5);
        assert_eq!((t.a, t.b, t.from, t.now), (A, B, 0, 5));
        assert_eq!(
            t.deals,
            vec![Deal {
                proposer: A,
                acceptor: B,
                offer_tick: 0,
                accepted_tick: 1,
                deadline: 10,
                give: 2,
                want: 3,
                given: 2,
                repaid: 3,
                status: Status::Fulfilled,
                settled_tick: Some(4),
                broken_by: vec![],
            }]
        );
        assert_eq!(
            t.entries,
            vec![
                Entry::Proposed {
                    tick: 0,
                    from: A,
                    to: B,
                    give: 2,
                    want: 3,
                    by: 10,
                    deadline: 10,
                    answer: Answer::Accepted,
                    deal: Some(0),
                },
                Entry::Accepted {
                    tick: 1,
                    from: B,
                    to: A,
                    offer_tick: 0,
                    give: 2,
                    want: 3,
                    by: 10,
                    deadline: 10,
                    deal: 0,
                },
                Entry::Transferred {
                    tick: 3,
                    from: A,
                    to: B,
                    amount: 2,
                    deal: Some(0),
                    role: Some(Role::Give),
                },
                Entry::Transferred {
                    tick: 4,
                    from: B,
                    to: A,
                    amount: 3,
                    deal: Some(0),
                    role: Some(Role::Repay),
                },
            ]
        );
        // The same thread read from B's side is the same thread.
        let mirrored = thread(&log, B, A, 0, 5);
        assert_eq!(
            (mirrored.entries, mirrored.deals),
            (t.entries.clone(), t.deals.clone())
        );

        // Read at tick 4 (the repayment not yet known): open, 2 given.
        let early = thread(&log[..3], A, B, 0, 4);
        let d = &early.deals[0];
        assert_eq!(
            (d.status, d.given, d.repaid, d.settled_tick),
            (Status::Open, 2, 0, None)
        );
        assert!(matches!(
            early.entries[0],
            Entry::Proposed {
                answer: Answer::Accepted,
                ..
            }
        ));

        // `deals` over any-pair events agrees with the thread's reading.
        assert_eq!(deals(&log, 5), t.deals);
    }

    /// `a_broken_promise_changes_nothing_physical` in the kernel's tests:
    /// A gives, B never repays, the deadline (tick 4) passes. The thread
    /// says so — open until the deadline, broken by B after it — while the
    /// kernel, as that test shows, does nothing at all.
    #[test]
    fn a_breach_is_open_until_the_deadline_and_then_broken_by_the_debtor() {
        let log = [
            proposed(0, A, B, 2, 3, 4),
            accepted(1, B, A, 0, 2, 3, 4),
            transferred(2, A, B, 2),
        ];
        for now in 3..=4 {
            let d = &thread(&log, A, B, 0, now).deals[0];
            assert_eq!((d.status, d.deadline), (Status::Open, 4), "now {now}");
            assert!(d.broken_by.is_empty());
        }
        let d = &thread(&log, A, B, 0, 5).deals[0];
        assert_eq!((d.status, d.given, d.repaid), (Status::Broken, 2, 0));
        assert_eq!(d.broken_by, vec![B], "the acceptor owed the `want`");
        // A late repayment does not mend it: the window closed at tick 4.
        let late = [log.as_slice(), &[transferred(9, B, A, 3)]].concat();
        let t = thread(&late, A, B, 0, 10);
        assert_eq!((t.deals[0].status, t.deals[0].repaid), (Status::Broken, 0));
        assert!(matches!(
            t.entries[3],
            Entry::Transferred {
                deal: None,
                role: None,
                ..
            }
        ));
        // Had A never given either, both would be named.
        let neither = [proposed(0, A, B, 2, 3, 4), accepted(1, B, A, 0, 2, 3, 4)];
        assert_eq!(thread(&neither, A, B, 0, 5).deals[0].broken_by, vec![A, B]);
        // A deal with nothing to deliver is settled by the acceptance itself.
        let nothing = [proposed(0, A, B, 0, 0, 4), accepted(1, B, A, 0, 0, 0, 4)];
        let d = &thread(&nothing, A, B, 0, 2).deals[0];
        assert_eq!((d.status, d.settled_tick), (Status::Fulfilled, Some(1)));
    }

    /// A rejection names no offer; the thread attributes it to the latest
    /// earlier offer from the rejected party. Unanswered offers are pending
    /// until their deadline, then lapsed. Harms between the pair are part
    /// of the thread; everything else in the ledger is not.
    #[test]
    fn rejections_are_linked_to_the_latest_earlier_offer_and_other_events_are_ignored() {
        let c = AgentId(2);
        let log = [
            proposed(0, A, B, 1, 1, 5),
            proposed(1, B, A, 2, 2, 3), // B's own offer the other way
            proposed(2, A, B, 3, 3, 20),
            rejected(3, B, A), // declines A's latest: tick 2
            harmed(3, A, B),
            rejected(4, A, B),          // declines B's: tick 1
            proposed(5, A, c, 1, 1, 1), // another pair
            transferred(6, c, A, 4),    // another pair
            rejected(6, B, A),          // declines tick 2 again (no newer offer)
        ];
        let t = thread(&log, A, B, 0, 7);
        assert!(t.deals.is_empty());
        let kinds: Vec<&str> = t
            .entries
            .iter()
            .map(|e| match e {
                Entry::Proposed { .. } => "proposed",
                Entry::Accepted { .. } => "accepted",
                Entry::Rejected { .. } => "rejected",
                Entry::Transferred { .. } => "transferred",
                Entry::Harmed { .. } => "harmed",
            })
            .collect();
        assert_eq!(
            kinds,
            ["proposed", "proposed", "proposed", "rejected", "harmed", "rejected", "rejected"]
        );
        let answers: Vec<Answer> = t
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Proposed { answer, .. } => Some(*answer),
                _ => None,
            })
            .collect();
        // Tick 0: never answered, deadline 5 passed → lapsed. Tick 1 (B's):
        // rejected at 4. Tick 2: rejected at 3 (and 6).
        assert_eq!(
            answers,
            [Answer::Lapsed, Answer::Rejected, Answer::Rejected]
        );
        let linked: Vec<Option<u64>> = t
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Rejected { offer_tick, .. } => Some(*offer_tick),
                _ => None,
            })
            .collect();
        assert_eq!(linked, [Some(2), Some(1), Some(2)]);
        // Pending while the deadline is ahead; a rejection with no earlier
        // offer links to nothing.
        let t = thread(&[proposed(0, A, B, 1, 1, 5), rejected(1, A, B)], A, B, 0, 2);
        assert!(matches!(
            t.entries[0],
            Entry::Proposed {
                answer: Answer::Pending,
                ..
            }
        ));
        assert!(matches!(
            t.entries[1],
            Entry::Rejected {
                offer_tick: None,
                ..
            }
        ));
    }

    /// Two deals between the same pair whose windows overlap: D5's sums are
    /// per deal (a transfer inside both windows counts for both), and each
    /// transfer is shown against the oldest deal that still owed on that
    /// side — or against none, when every promise it could serve was
    /// already kept (a gift, as far as the ledger can tell).
    #[test]
    fn overlapping_deals_are_judged_independently_and_transfers_are_attributed_in_order() {
        let log = [
            proposed(0, A, B, 2, 1, 10),
            accepted(1, B, A, 0, 2, 1, 10),
            transferred(4, A, B, 2), // deal 0's give, in full
            proposed(5, A, B, 1, 1, 10),
            accepted(6, B, A, 5, 1, 1, 10),
            transferred(7, A, B, 1), // deal 0 is given in full → deal 1's give
            transferred(8, B, A, 1), // repays deal 0 (the oldest owed)…
            transferred(9, B, A, 1), // …deal 1 was repaid by tick 8 too: owed by nobody
        ];
        let t = thread(&log, A, B, 0, 10);
        assert_eq!(t.deals.len(), 2);
        // Deal 0's window (0..=10) holds all four transfers; deal 1's (5..=15)
        // holds the last three.
        assert_eq!((t.deals[0].given, t.deals[0].repaid), (3, 2));
        assert_eq!((t.deals[1].given, t.deals[1].repaid), (1, 2));
        assert!(t.deals.iter().all(|d| d.status == Status::Fulfilled));
        assert_eq!(
            (t.deals[0].settled_tick, t.deals[1].settled_tick),
            (Some(8), Some(8))
        );
        let tags: Vec<(Option<usize>, Option<Role>)> = t
            .entries
            .iter()
            .filter_map(|e| match e {
                Entry::Transferred { deal, role, .. } => Some((*deal, *role)),
                _ => None,
            })
            .collect();
        assert_eq!(
            tags,
            [
                (Some(0), Some(Role::Give)),
                (Some(1), Some(Role::Give)),
                (Some(0), Some(Role::Repay)),
                (None, None),
            ]
        );
    }

    /// In a crowded world of random walkers the thread of a pair is exactly
    /// the pair's slice of what `agent_events_json` reports for either
    /// agent, read at the view's tick: seeking back shortens it and can
    /// reopen a deal, `to` beyond the view is capped, and `deals_json`
    /// over the same window lists every acceptance the pair threads hold.
    #[test]
    fn thread_json_is_the_pairs_slice_of_the_ledger_read_at_the_view() {
        let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
        ww.step(60).unwrap();
        let all: Vec<Value> = (0..60)
            .flat_map(|t| serde_json::from_str::<Vec<Value>>(&ww.events_at(t).unwrap()).unwrap())
            .collect();
        let pair_of = |e: &Value| -> Option<(u64, u64)> {
            let (x, y) = match e["kind"].as_str().unwrap() {
                "transferred" | "proposed" | "accepted" | "rejected" => ("from", "to"),
                "harmed" => ("by", "target"),
                _ => return None,
            };
            let (x, y) = (e[x].as_u64().unwrap(), e[y].as_u64().unwrap());
            Some((x.min(y), x.max(y)))
        };
        // The pair with the most acceptances, then the most events.
        let accepted_pairs: Vec<(u64, u64)> = all
            .iter()
            .filter(|e| e["kind"] == "accepted")
            .filter_map(pair_of)
            .collect();
        assert!(!accepted_pairs.is_empty(), "random walkers accept offers");
        let (a, b) = *accepted_pairs
            .iter()
            .max_by_key(|p| {
                (
                    accepted_pairs.iter().filter(|q| q == p).count(),
                    all.iter().filter(|e| pair_of(e) == Some(**p)).count(),
                )
            })
            .unwrap();
        let want: Vec<&Value> = all.iter().filter(|e| pair_of(e) == Some((a, b))).collect();

        let t: Value =
            serde_json::from_str(&ww.thread_json(a as u32, b as u32, 0, 60).unwrap()).unwrap();
        assert_eq!(
            (
                t["a"].as_u64(),
                t["b"].as_u64(),
                t["from"].as_u64(),
                t["now"].as_u64()
            ),
            (Some(a), Some(b), Some(0), Some(60))
        );
        let entries = t["entries"].as_array().unwrap();
        assert_eq!(entries.len(), want.len());
        for (got, exp) in entries.iter().zip(&want) {
            assert_eq!(got["kind"], exp["kind"]);
            assert_eq!(got["tick"], exp["tick"]);
            for k in [
                "from",
                "to",
                "by",
                "target",
                "amount",
                "give",
                "want",
                "offer_tick",
                "damage",
            ] {
                if !exp[k].is_null() {
                    assert_eq!(got[k], exp[k], "{k} of {exp}");
                }
            }
        }
        // Every entry is in one of the two agents' own ledger views.
        let story_a: Vec<Value> =
            serde_json::from_str(&ww.agent_events_json(a as u32, 0, 60).unwrap()).unwrap();
        assert!(want.iter().all(|e| story_a.contains(e)));

        // Deals: one per acceptance, the terms copied from it, and the sums
        // recomputed here from the pair's transfers.
        let deals = t["deals"].as_array().unwrap();
        let acceptances: Vec<&&Value> = want.iter().filter(|e| e["kind"] == "accepted").collect();
        assert_eq!(deals.len(), acceptances.len());
        for (d, acc) in deals.iter().zip(&acceptances) {
            let (acceptor, proposer) = (acc["from"].as_u64().unwrap(), acc["to"].as_u64().unwrap());
            let offer = acc["offer_tick"].as_u64().unwrap();
            let deadline = offer + acc["by"].as_u64().unwrap();
            assert_eq!(
                (d["proposer"].as_u64(), d["acceptor"].as_u64()),
                (Some(proposer), Some(acceptor))
            );
            assert_eq!(
                (
                    d["offer_tick"].as_u64(),
                    d["accepted_tick"].as_u64(),
                    d["deadline"].as_u64()
                ),
                (Some(offer), acc["tick"].as_u64(), Some(deadline))
            );
            let sum = |from: u64, to: u64| -> u64 {
                want.iter()
                    .filter(|e| e["kind"] == "transferred" && e["from"] == from && e["to"] == to)
                    .filter(|e| (offer..=deadline).contains(&e["tick"].as_u64().unwrap()))
                    .map(|e| e["amount"].as_u64().unwrap())
                    .sum()
            };
            assert_eq!(d["given"].as_u64().unwrap(), sum(proposer, acceptor));
            assert_eq!(d["repaid"].as_u64().unwrap(), sum(acceptor, proposer));
            let full = d["given"].as_u64() >= acc["give"].as_u64()
                && d["repaid"].as_u64() >= acc["want"].as_u64();
            let expected = if full {
                "fulfilled"
            } else if 60 > deadline {
                "broken"
            } else {
                "open"
            };
            assert_eq!(d["status"], expected, "{d}");
            assert_eq!(
                d["broken_by"].as_array().unwrap().is_empty(),
                expected != "broken"
            );
        }
        // The proposal each deal came from is marked accepted.
        assert!(entries
            .iter()
            .filter(|e| e["kind"] == "proposed")
            .filter(|e| deals
                .iter()
                .any(|d| d["offer_tick"] == e["tick"] && d["proposer"] == e["from"]))
            .all(|e| e["answer"] == "accepted"));

        // Read at the view: seeking back cuts the thread at the view's tick
        // and judges open deadlines there; `to` beyond the view is capped.
        let first_deal_tick = deals[0]["accepted_tick"].as_u64().unwrap();
        ww.seek(first_deal_tick as u32 + 1).unwrap();
        let early: Value =
            serde_json::from_str(&ww.thread_json(a as u32, b as u32, 0, 1000).unwrap()).unwrap();
        assert_eq!(early["now"], first_deal_tick + 1);
        assert!(early["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["tick"].as_u64().unwrap() <= first_deal_tick));
        let d0 = &early["deals"][0];
        assert_eq!(d0["accepted_tick"], first_deal_tick);
        let status0 = if d0["given"].as_u64() >= d0["give"].as_u64()
            && d0["repaid"].as_u64() >= d0["want"].as_u64()
        {
            "fulfilled"
        } else if first_deal_tick + 1 > d0["deadline"].as_u64().unwrap() {
            "broken"
        } else {
            "open"
        };
        assert_eq!(d0["status"], status0);
        ww.seek(60).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&ww.thread_json(a as u32, b as u32, 0, 60).unwrap())
                .unwrap(),
            t,
            "a function of the view, not of how it was reached"
        );
        for (from, to) in [(60, 60), (60, 10), (70, 90)] {
            let v: Value =
                serde_json::from_str(&ww.thread_json(a as u32, b as u32, from, to).unwrap())
                    .unwrap();
            assert_eq!(
                v["entries"],
                Value::Array(vec![]),
                "{from}..{to} at the view 60 is empty"
            );
            assert!(v["from"].as_u64() <= v["now"].as_u64() && v["now"].as_u64() <= Some(60));
        }

        // `deals_json`: every acceptance of the window, each equal to the
        // deal its own pair thread holds.
        let found: Vec<Value> = serde_json::from_str(&ww.deals_json(0, 60).unwrap()).unwrap();
        assert_eq!(
            found.len(),
            all.iter().filter(|e| e["kind"] == "accepted").count()
        );
        for d in &found {
            let (p, q) = (
                d["proposer"].as_u64().unwrap() as u32,
                d["acceptor"].as_u64().unwrap() as u32,
            );
            let pt: Value = serde_json::from_str(&ww.thread_json(p, q, 0, 60).unwrap()).unwrap();
            assert!(pt["deals"].as_array().unwrap().contains(d), "{d}");
        }
        assert!(found
            .windows(2)
            .all(|w| w[0]["accepted_tick"].as_u64() <= w[1]["accepted_tick"].as_u64()));
    }

    /// The page's worlds are 128², 1,000 agents, 23 patches (as `cz` and
    /// the kernel worker compute them), snapshots every 250. In the seed
    /// the page defaults to (42) no deal is kept in order — proposal,
    /// acceptance, the give, the repayment — before the random walkers die
    /// out, which is why the page takes `?seed=`. In seed 23, in both the
    /// `mixed` and the `random` world, #909 and #363 hold one: offered at
    /// tick 17 (2 for 1 by tick 33), accepted at 19, given at 19, repaid at
    /// 20. TUTORIAL.md names it; if this test fails the tutorial is stale.
    #[test]
    fn seed_23_holds_the_kept_deal_the_tutorial_names_and_seed_42_none_in_order() {
        let page_world = |seed: u32, mix: &str, ticks: u32| {
            let mut ww = WasmWorld::new(seed, 128, 128, 1000, 23, 250, mix, "").unwrap();
            ww.step(ticks).unwrap();
            ww
        };
        // A deal kept in order: food on both sides, accepted by the
        // deadline, the give after the acceptance, the repayment after the give.
        let in_order = |ww: &WasmWorld, ticks: u32| -> Vec<Value> {
            let found: Vec<Value> =
                serde_json::from_str(&ww.deals_json(0, ticks).unwrap()).unwrap();
            found
                .into_iter()
                .filter(|d| {
                    d["status"] == "fulfilled"
                        && d["give"].as_u64() > Some(0)
                        && d["want"].as_u64() > Some(0)
                })
                .filter(|d| {
                    let (p, q) = (
                        d["proposer"].as_u64().unwrap() as u32,
                        d["acceptor"].as_u64().unwrap() as u32,
                    );
                    let (acc, deadline) = (
                        d["accepted_tick"].as_u64().unwrap(),
                        d["deadline"].as_u64().unwrap(),
                    );
                    let t: Value =
                        serde_json::from_str(&ww.thread_json(p, q, 0, ticks).unwrap()).unwrap();
                    let xfer = |from: u32, after: u64| {
                        t["entries"].as_array().unwrap().iter().find_map(|e| {
                            let tick = e["tick"].as_u64().unwrap();
                            (e["kind"] == "transferred"
                                && e["from"] == from
                                && tick >= after
                                && tick <= deadline)
                                .then_some(tick)
                        })
                    };
                    acc <= deadline && xfer(p, acc).and_then(|g| xfer(q, g)).is_some()
                })
                .collect()
        };
        for mix in ["mixed", "random"] {
            let ww = page_world(42, mix, 300);
            assert!(
                in_order(&ww, 300).is_empty(),
                "seed 42 {mix}: the tutorial says there is none"
            );
        }
        for mix in ["mixed", "random"] {
            let ww = page_world(23, mix, 100);
            let kept = in_order(&ww, 100);
            let d = kept
                .iter()
                .find(|d| d["proposer"] == 909 && d["acceptor"] == 363 && d["offer_tick"] == 17)
                .unwrap_or_else(|| {
                    panic!("seed 23 {mix}: the tutorial's deal is missing from {kept:?}")
                });
            assert_eq!(
                (
                    d["accepted_tick"].as_u64(),
                    d["deadline"].as_u64(),
                    d["give"].as_u64(),
                    d["want"].as_u64()
                ),
                (Some(19), Some(33), Some(2), Some(1))
            );
            assert_eq!(
                (
                    d["given"].as_u64(),
                    d["repaid"].as_u64(),
                    d["settled_tick"].as_u64()
                ),
                (Some(2), Some(1), Some(20))
            );
            // The thread reads in order around it.
            let t: Value =
                serde_json::from_str(&ww.thread_json(909, 363, 0, 100).unwrap()).unwrap();
            let rows: Vec<(u64, &str, u64)> = t["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    let who = if e["kind"] == "harmed" {
                        &e["by"]
                    } else {
                        &e["from"]
                    };
                    (
                        e["tick"].as_u64().unwrap(),
                        e["kind"].as_str().unwrap(),
                        who.as_u64().unwrap(),
                    )
                })
                .collect();
            let i = rows
                .iter()
                .position(|r| *r == (17, "proposed", 909))
                .unwrap();
            assert_eq!(
                &rows[i..i + 5],
                &[
                    (17, "proposed", 909),
                    (18, "transferred", 909),
                    (19, "accepted", 363),
                    (19, "transferred", 909),
                    (20, "transferred", 363),
                ],
                "{mix}"
            );
            let deals = t["deals"].as_array().unwrap();
            assert_eq!(
                deals.len(),
                5,
                "{mix}: three broken, then this one kept, then its late second acceptance"
            );
            assert_eq!(deals[3]["status"], "fulfilled");
            assert_eq!(deals[3]["settled_tick"], 20);
            assert_eq!(
                deals[4]["offer_tick"], 17,
                "the same offer accepted again at tick 32"
            );
            assert_eq!(deals[4]["accepted_tick"], 32);
            assert!(deals[..3].iter().all(|d| d["status"] == "broken"));
            // The thread is identical in the two worlds this far (the pair's
            // neighbourhood is all random walkers); the worlds are not.
            if mix == "random" {
                let mixed = page_world(23, "mixed", 100);
                assert_eq!(
                    mixed.thread_json(909, 363, 0, 100).unwrap(),
                    ww.thread_json(909, 363, 0, 100).unwrap()
                );
                assert_ne!(mixed.state_hash_hex(), ww.state_hash_hex());
            }
        }
    }
}

/// Institutions (ADR-0014 D12, 4-7): the readers of `cz_web::institution`
/// against hand-built ledgers and states, then through the wrapper against
/// a ruled world run natively — at the head, after a seek, and in a fork.
mod institutions {
    use cz_web::institution::{
        institution, sanctions, standing_of, timeline, violations, voted, Accused, By, Entry,
        Episode, Origin, Sanction, Source, Violation,
    };
    use cz_web::{
        WasmWorld, WorldOptions, AGENT_FIELDS, FLAG_ACCUSED, FLAG_HARMABLE, FLAG_REJECTABLE,
        FLAG_VOTED,
    };
    use serde_json::Value;
    use simulation_core::{
        AgentId, Charter, Event, EventLog, Grounds, Intervention, Motion, MotionKind, Nomos, Norm,
        NormKind, Pos, Rule, Sanction as SanctionSet, Standing, Via, World, WorldConfig,
    };

    fn norm(kind: NormKind, standing_ticks: u16, sanction: SanctionSet) -> Norm {
        Norm {
            kind,
            standing_ticks,
            sanction,
            except_sanction: false,
        }
    }

    fn enact(tick: u64, norm: Norm) -> Event {
        Event::Intervened {
            tick,
            intervention: Intervention::Enact { norm },
        }
    }

    fn repeal(tick: u64, rule: u16) -> Event {
        Event::Intervened {
            tick,
            intervention: Intervention::Repeal { rule },
        }
    }

    fn sanctioned(tick: u64, by: u64, target: u64, rule: u16) -> Event {
        Event::Sanctioned {
            tick,
            by: AgentId(by),
            target: AgentId(target),
            rule,
            via: Via::Harm,
        }
    }

    fn violated(tick: u64, agent: u64, rule: u16, against: u64) -> Event {
        Event::Violated {
            tick,
            agent: AgentId(agent),
            rule,
            norm: NormKind::Encroach,
            against: AgentId(against),
            grounds: Grounds::Encroachment {
                cell: Pos::new(1, 2),
                amount: 1,
            },
        }
    }

    fn standing(
        agent: u64,
        rule: u16,
        since: u64,
        last: u64,
        against: u64,
        violations: u16,
        sanctions: u16,
    ) -> Standing {
        Standing {
            agent: AgentId(agent),
            rule,
            since,
            last,
            against: AgentId(against),
            violations,
            sanctions,
        }
    }

    const ENCROACH: Norm = Norm {
        kind: NormKind::Encroach,
        standing_ticks: 32,
        sanction: SanctionSet::HARM,
        except_sanction: false,
    };
    const ENCROACH_JSON: &str = r#"{"op":"enact","norm":{"kind":"encroach","standing_ticks":32,"sanction":1,"except_sanction":false}}"#;
    const HARM_JSON: &str = r#"{"op":"enact","norm":{"kind":"harm","standing_ticks":8,"sanction":2,"except_sanction":true}}"#;

    /// Decrees replayed over the origin: ids count on from the origin's
    /// `next_rule_id`, a replacement names the rule it took out of force,
    /// a repeal names what the rule said, and the window is `from..now`.
    #[test]
    fn the_timeline_recovers_rule_ids_from_the_origin_and_the_decrees() {
        let encroach = norm(NormKind::Encroach, 16, SanctionSet::HARM);
        let harm = norm(NormKind::Harm, 8, SanctionSet::REJECT);
        let promise = norm(NormKind::BrokenPromise, 4, SanctionSet::ALL);
        let encroach2 = norm(NormKind::Encroach, 64, SanctionSet::NONE);
        let origin = Origin {
            rules: vec![
                Rule {
                    id: 3,
                    norm: encroach,
                    enacted: 10,
                },
                Rule {
                    id: 5,
                    norm: harm,
                    enacted: 12,
                },
            ],
            next_rule_id: 6,
            standing: Vec::new(),
        };
        let c = Charter {
            quorum_permille: 500,
            ..Charter::default()
        };
        let log = [
            Event::Born {
                tick: 20,
                child: AgentId(9),
                parent: AgentId(1),
                pos: Pos::new(0, 0),
                energy: 1,
            },
            enact(20, promise),
            enact(25, encroach2),
            repeal(30, 5),
            Event::Intervened {
                tick: 30,
                intervention: Intervention::Charter { charter: c },
            },
            repeal(31, 99),
            enact(40, harm),
        ];
        let t = timeline(&log, &origin, 0, 100);
        assert_eq!((t.from, t.now), (0, 100));
        assert_eq!(
            t.entries,
            vec![
                Entry::Enacted {
                    tick: 20,
                    rule: 6,
                    norm: promise,
                    replaced: None,
                    by: By::Decree
                },
                Entry::Enacted {
                    tick: 25,
                    rule: 7,
                    norm: encroach2,
                    replaced: Some(3),
                    by: By::Decree
                },
                Entry::Repealed {
                    tick: 30,
                    rule: 5,
                    norm: Some(harm),
                    by: By::Decree
                },
                Entry::Chartered {
                    tick: 30,
                    charter: c,
                    by: By::Decree
                },
                Entry::Repealed {
                    tick: 31,
                    rule: 99,
                    norm: None,
                    by: By::Decree
                },
                Entry::Enacted {
                    tick: 40,
                    rule: 8,
                    norm: harm,
                    replaced: None,
                    by: By::Decree
                },
            ]
        );
        // The window keeps `from <= tick < now`; the replay still starts
        // at the origin, so ids do not depend on the window.
        let w = timeline(&log, &origin, 25, 31);
        assert_eq!(
            w.entries.iter().map(Entry::tick).collect::<Vec<_>>(),
            vec![25, 30, 30]
        );
        assert!(matches!(w.entries[0], Entry::Enacted { rule: 7, .. }));
        // A fresh world: the first decree is rule 0.
        let fresh = timeline(&[enact(0, encroach)], &Origin::default(), 0, 1);
        assert!(matches!(
            fresh.entries[..],
            [Entry::Enacted {
                tick: 0,
                rule: 0,
                replaced: None,
                ..
            }]
        ));
        let json = serde_json::to_string(&fresh).unwrap();
        assert!(
            json.contains(r#""kind":"enacted""#) && json.contains(r#""by":"decree""#),
            "{json}"
        );
    }

    /// Recorded, the violations are the ledger's `Violated` of the window;
    /// not recorded, they are the episodes open now, newest `last` first.
    #[test]
    fn violations_come_from_the_ledger_when_recorded_and_from_the_open_episodes_otherwise() {
        let mut nomos = Nomos::default();
        nomos.enact(norm(NormKind::Encroach, 8, SanctionSet::HARM), 0);
        nomos.enact(norm(NormKind::Harm, 4, SanctionSet::ALL), 0);
        nomos.standing = vec![
            standing(2, 0, 10, 12, 7, 2, 1),
            standing(5, 0, 3, 12, 9, 4, 2),
            standing(5, 1, 11, 11, 2, 1, 0),
        ];
        let log = [
            violated(9, 2, 0, 7),
            violated(12, 5, 0, 9),
            violated(12, 2, 0, 7),
            violated(13, 5, 0, 9),
        ];
        let recorded = violations(&log, &nomos, true, 10, 13);
        assert_eq!(
            (recorded.source, recorded.from, recorded.now),
            (Source::Ledger, 10, 13)
        );
        assert_eq!(
            recorded.entries,
            vec![
                Violation::Violated {
                    tick: 12,
                    agent: AgentId(5),
                    rule: 0,
                    norm: NormKind::Encroach,
                    against: AgentId(9),
                    grounds: Grounds::Encroachment {
                        cell: Pos::new(1, 2),
                        amount: 1
                    },
                },
                Violation::Violated {
                    tick: 12,
                    agent: AgentId(2),
                    rule: 0,
                    norm: NormKind::Encroach,
                    against: AgentId(7),
                    grounds: Grounds::Encroachment {
                        cell: Pos::new(1, 2),
                        amount: 1
                    },
                },
            ]
        );
        let json = serde_json::to_string(&recorded).unwrap();
        assert!(
            json.contains(r#""kind":"violated""#) && json.contains(r#""fact":"encroachment""#),
            "{json}"
        );

        let open = violations(&log, &nomos, false, 10, 13);
        assert_eq!(open.source, Source::Standing);
        let accused = |agent: u64,
                       rule: u16,
                       since: u64,
                       last: u64,
                       until: u64,
                       against: u64,
                       v: u16,
                       s: u16| {
            Violation::Accused(Accused {
                agent: AgentId(agent),
                rule,
                norm: if rule == 0 {
                    NormKind::Encroach
                } else {
                    NormKind::Harm
                },
                since,
                last,
                until,
                against: AgentId(against),
                violations: v,
                sanctions: s,
            })
        };
        assert_eq!(
            open.entries,
            vec![
                accused(5, 0, 3, 12, 20, 9, 4, 2),
                accused(2, 0, 10, 12, 20, 7, 2, 1),
                accused(5, 1, 11, 11, 15, 2, 1, 0),
            ],
            "newest last first, older since first among equals"
        );
        let json = serde_json::to_string(&open).unwrap();
        assert!(
            json.contains(r#""kind":"accused""#) && json.contains(r#""until":20"#),
            "{json}"
        );

        // The inspector's reading of one agent: ascending by rule.
        let mine = standing_of(&nomos, AgentId(5));
        assert_eq!(mine.iter().map(|a| a.rule).collect::<Vec<_>>(), vec![0, 1]);
        assert_eq!(mine[1].until, 15);
        assert!(standing_of(&nomos, AgentId(4)).is_empty());
        assert!(!voted(&nomos, AgentId(5)));
        nomos.motion = Some(Motion {
            id: 0,
            by: AgentId(2),
            opened: 12,
            kind: MotionKind::Repeal { rule: 1 },
            ayes: vec![AgentId(5)],
            nays: vec![AgentId(9)],
        });
        assert!(voted(&nomos, AgentId(5)) && voted(&nomos, AgentId(9)));
        assert!(!voted(&nomos, AgentId(2)), "moving is not voting");
    }

    /// A sanction is tied to the episode open at its tick: the `Standing`
    /// still open at the view, else the episode rebuilt from the recorded
    /// violations (runs within `standing_ticks` of each other), else none.
    #[test]
    fn sanctions_are_tied_to_the_episode_they_answered() {
        let rule = norm(NormKind::Encroach, 4, SanctionSet::HARM);
        let log = [
            enact(0, rule),
            violated(5, 2, 0, 7),
            violated(6, 2, 0, 7),
            sanctioned(7, 9, 2, 0),
            sanctioned(7, 8, 2, 0),
            violated(12, 2, 0, 8), // 6 + 4 < 12: a new episode
            sanctioned(13, 9, 2, 0),
            violated(20, 3, 0, 1),
            sanctioned(21, 9, 3, 0),
        ];
        let mut nomos = Nomos::default();
        nomos.enact(rule, 0);
        nomos.standing = vec![standing(3, 0, 20, 20, 1, 1, 1)];
        let origin = Origin::default();

        let read = sanctions(&log, &origin, &nomos, true, 0, 25);
        assert_eq!((read.from, read.now), (0, 25));
        let brief: Vec<(u64, u64, Option<Episode>)> = read
            .entries
            .iter()
            .map(|s| (s.tick, s.by.0, s.episode))
            .collect();
        let closed = |since: u64, last: u64, against: u64, v: u16, s: u16| {
            Some(Episode {
                since,
                last,
                against: AgentId(against),
                violations: v,
                sanctions: s,
                open: false,
                source: Source::Ledger,
            })
        };
        assert_eq!(
            brief,
            vec![
                (7, 9, closed(5, 6, 7, 2, 2)),
                (7, 8, closed(5, 6, 7, 2, 2)),
                (13, 9, closed(12, 12, 8, 1, 1)),
                (
                    21,
                    9,
                    Some(Episode {
                        since: 20,
                        last: 20,
                        against: AgentId(1),
                        violations: 1,
                        sanctions: 1,
                        open: true,
                        source: Source::Standing,
                    })
                ),
            ]
        );
        assert!(read
            .entries
            .iter()
            .all(|s| s.norm == Some(NormKind::Encroach) && s.via == Via::Harm && s.rule == 0));

        // Not recorded: only the open episode can be named.
        let bare = sanctions(&log, &origin, &nomos, false, 0, 25);
        assert_eq!(
            bare.entries
                .iter()
                .map(|s| s.episode.map(|e| e.since))
                .collect::<Vec<_>>(),
            vec![None, None, None, Some(20)]
        );
        // The window keeps `from <= tick < now`; the rebuild still reads
        // the ledger from the origin.
        let window = sanctions(&log, &origin, &nomos, true, 13, 21);
        assert_eq!(
            window.entries.iter().map(|s| s.tick).collect::<Vec<_>>(),
            vec![13]
        );
        assert_eq!(window.entries[0].episode, closed(12, 12, 8, 1, 1));
        // Read at tick 8: the first episode is still open — but the view's
        // state is what says so, so with a `nomos` of that tick it is
        // `standing`, and with the wrong (later) state it is rebuilt.
        let mut at_8 = Nomos::default();
        at_8.enact(rule, 0);
        at_8.standing = vec![standing(2, 0, 5, 6, 7, 2, 2)];
        let early = sanctions(&log, &origin, &at_8, true, 0, 8);
        assert_eq!(early.entries.len(), 2);
        assert!(early
            .entries
            .iter()
            .all(|s| s.episode.is_some_and(|e| e.open)));

        // A fork: the ledger begins at tick 7, the episode it continues
        // comes from the origin's standing.
        let forked = Origin {
            rules: nomos.rules.clone(),
            next_rule_id: 1,
            standing: vec![standing(2, 0, 5, 6, 7, 2, 0)],
        };
        let fork_log = &log[3..];
        let read = sanctions(fork_log, &forked, &nomos, true, 7, 25);
        assert_eq!(read.entries[0].episode, closed(5, 6, 7, 2, 2));
        assert_eq!(read.entries[2].episode, closed(12, 12, 8, 1, 1));
        let json = serde_json::to_string(&read).unwrap();
        assert!(
            json.contains(r#""via":"harm""#) && json.contains(r#""source":"ledger""#),
            "{json}"
        );
    }

    /// The crowded random world with the first-occupancy norm in force:
    /// random walkers gather where others arrived earlier and harm their
    /// neighbours, so violations, episodes and sanctions appear within a
    /// few ticks.
    fn ruled_config(record_violations: bool) -> WorldConfig {
        WorldConfig {
            seed: 3,
            width: 32,
            height: 32,
            initial_agents: 200,
            patch_count: 3,
            record_violations,
            ..WorldConfig::nomos_preset()
        }
    }

    /// Runs `w` for `ticks`, recording every tick into `log` as the
    /// multiverse does.
    fn run_logged(w: &mut World, log: &mut EventLog, ticks: u64) {
        for _ in 0..ticks {
            let report = w.step();
            log.record(&report).unwrap();
        }
    }

    fn value(json: &str) -> Value {
        serde_json::from_str(json).unwrap()
    }

    /// Through the wrapper, every reader equals the expectation built from
    /// the natively recorded ledger and the native state: at the head,
    /// after a seek (read at the view's tick), and in a fork built from a
    /// snapshot (whose ledger starts at the fork tick).
    #[test]
    fn a_ruled_world_reads_the_same_through_the_wrapper_after_a_seek_and_in_a_fork() {
        let mut d =
            World::generate(ruled_config(true), cz_web::policy_mix("random").unwrap()).unwrap();
        let mut log = EventLog::new();
        log.push(d.intervene(Intervention::Enact { norm: ENCROACH }).unwrap())
            .unwrap();
        run_logged(&mut d, &mut log, 30);
        let d30 = d.clone();
        run_logged(&mut d, &mut log, 30);

        let options = r#"{"nomos":true,"record_violations":true}"#;
        let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", options).unwrap();
        let event = ww.intervene(ENCROACH_JSON).unwrap();
        assert!(
            event.starts_with(r#"{"kind":"intervened","tick":0,"intervention":{"op":"enact""#),
            "{event}"
        );
        ww.step(60).unwrap();
        assert_eq!(ww.state_hash_hex(), format!("{:016x}", d.state_hash()));
        let total = |counter: &[u64]| counter.iter().sum::<u64>();
        assert!(
            total(&d.ledger.violations) > 0
                && total(&d.ledger.sanctions) > 0
                && !d.nomos.standing.is_empty(),
            "the world must have something to read: {:?} {:?} {}",
            d.ledger.violations,
            d.ledger.sanctions,
            d.nomos.standing.len()
        );

        // The institution is the state.
        let inst = value(&ww.institution_json().unwrap());
        assert_eq!(inst, serde_json::to_value(institution(&d)).unwrap());
        assert_eq!(inst["nomos"], true);
        assert_eq!(inst["record_violations"], true);
        assert_eq!(inst["rules"].as_array().unwrap().len(), 1);
        assert_eq!(inst["rules"][0]["id"], 0);
        assert_eq!(inst["rules"][0]["norm"]["kind"], "encroach");
        assert_eq!(inst["rules"][0]["enacted"], 0);
        assert_eq!(inst["motion"], Value::Null);
        assert_eq!(
            inst["accused"]["episodes"].as_u64().unwrap() as usize,
            d.nomos.standing.len()
        );
        assert_eq!(inst["accused"]["by_kind"][0], inst["accused"]["episodes"]);
        assert_eq!(inst["accused"]["by_kind"][1], 0);
        assert_eq!(inst["charter"]["quorum_permille"], 100);

        // The flags column is the kernel's NormIndex.
        let rows = ww.agents();
        assert_eq!(rows.len(), d.agents.len() * AGENT_FIELDS);
        let norms = d.norm_index();
        let mut accused_seen = 0;
        for (i, row) in rows.chunks(AGENT_FIELDS).enumerate() {
            assert_eq!(row[0] as u64, d.agents[i].id.0);
            let open = !d.nomos.standing_of(d.agents[i].id).is_empty();
            assert_eq!(row[8] & FLAG_ACCUSED != 0, open);
            assert_eq!(row[8] & FLAG_ACCUSED != 0, norms.accused(i));
            assert_eq!(row[8] & FLAG_HARMABLE != 0, open, "the rule allows harm");
            assert_eq!(row[8] & FLAG_REJECTABLE, 0);
            assert_eq!(row[8] & FLAG_VOTED, 0);
            accused_seen += open as usize;
        }
        assert_eq!(
            accused_seen,
            inst["accused"]["agents"].as_u64().unwrap() as usize
        );

        // The inspector: the agent plus its episodes.
        let s0 = d.nomos.standing[0];
        let a = value(&ww.agent_json(s0.agent.0 as u32).unwrap());
        assert_eq!(a["id"], s0.agent.0);
        assert!(a.get("arrived").is_some() && a.get("recent").is_some());
        assert_eq!(a["voted"], false);
        assert_eq!(
            a["standing"],
            serde_json::to_value(standing_of(&d.nomos, s0.agent)).unwrap()
        );
        assert_eq!(a["standing"][0]["since"], s0.since);
        assert_eq!(a["standing"][0]["until"], s0.last + 32);
        let free = d
            .agents
            .iter()
            .find(|a| d.nomos.standing_of(a.id).is_empty())
            .unwrap();
        let f = value(&ww.agent_json(free.id.0 as u32).unwrap());
        assert_eq!(f["standing"], Value::Array(vec![]));

        // The ledger readers equal the expectations from the recorded log.
        let expect_timeline = timeline(log.events(), &Origin::default(), 0, 60);
        assert_eq!(
            value(&ww.timeline_json(0, 60).unwrap()),
            serde_json::to_value(&expect_timeline).unwrap()
        );
        assert_eq!(expect_timeline.entries.len(), 1);
        let expect_violations = violations(log.range(0..60), &d.nomos, true, 0, 60);
        assert_eq!(
            value(&ww.violations_json(0, 60).unwrap()),
            serde_json::to_value(&expect_violations).unwrap()
        );
        assert_eq!(
            expect_violations.entries.len() as u64,
            total(&d.ledger.violations),
            "every judged violation is recorded"
        );
        let expect_sanctions = sanctions(log.events(), &Origin::default(), &d.nomos, true, 0, 60);
        assert_eq!(
            value(&ww.sanctions_json(0, 60).unwrap()),
            serde_json::to_value(&expect_sanctions).unwrap()
        );
        assert_eq!(
            expect_sanctions.entries.len() as u64,
            total(&d.ledger.sanctions),
            "every sanction is recorded"
        );
        assert!(
            expect_sanctions.entries.iter().all(|s| s.episode.is_some()),
            "recorded, every sanction finds its episode"
        );
        let (open, closed): (Vec<&Sanction>, Vec<&Sanction>) = expect_sanctions
            .entries
            .iter()
            .partition(|s| s.episode.unwrap().open);
        assert!(
            !open.is_empty() && !closed.is_empty(),
            "{} / {}",
            open.len(),
            closed.len()
        );
        assert!(open
            .iter()
            .all(|s| s.episode.unwrap().source == Source::Standing));
        assert!(closed
            .iter()
            .all(|s| s.episode.unwrap().source == Source::Ledger));
        // The rebuild agrees with the kernel's own record: read with an
        // empty state every episode is rebuilt from the ledger, and the
        // ones the kernel still holds open come out identical.
        let rebuilt = sanctions(
            log.events(),
            &Origin::default(),
            &Nomos::default(),
            true,
            0,
            60,
        );
        for (a, b) in expect_sanctions.entries.iter().zip(&rebuilt.entries) {
            let (ea, eb) = (a.episode.unwrap(), b.episode.unwrap());
            assert_eq!(
                (ea.since, ea.last, ea.against, ea.violations, ea.sanctions),
                (eb.since, eb.last, eb.against, eb.violations, eb.sanctions),
                "sanction at {} on {}",
                a.tick,
                a.target.0
            );
            assert_eq!((eb.open, eb.source), (false, Source::Ledger));
        }
        // A window: `from` bounds the entries, not the rebuild.
        let late = value(&ww.sanctions_json(50, 60).unwrap());
        assert_eq!(late["from"], 50);
        assert!(late["entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["tick"].as_u64().unwrap() >= 50 && !s["episode"].is_null()));
        // The pair thread is untouched by the new events (3-9b).
        let s = &expect_sanctions.entries[0];
        let t = value(
            &ww.thread_json(s.by.0 as u32, s.target.0 as u32, 0, 60)
                .unwrap(),
        );
        assert!(t["entries"].as_array().unwrap().iter().all(|e| {
            ["proposed", "accepted", "rejected", "transferred", "harmed"]
                .contains(&e["kind"].as_str().unwrap())
        }));
        assert!(t["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "harmed" && e["tick"] == s.tick));

        // After a seek everything is read at the view's tick.
        ww.seek(30).unwrap();
        assert_eq!(ww.tick(), 30);
        assert_eq!(
            value(&ww.institution_json().unwrap()),
            serde_json::to_value(institution(&d30)).unwrap()
        );
        let rows = ww.agents();
        assert_eq!(rows.len(), d30.agents.len() * AGENT_FIELDS);
        for (i, row) in rows.chunks(AGENT_FIELDS).enumerate() {
            assert_eq!(
                row[8] & FLAG_ACCUSED != 0,
                !d30.nomos.standing_of(d30.agents[i].id).is_empty()
            );
        }
        let v30 = value(&ww.violations_json(0, 60).unwrap());
        assert_eq!(v30["now"], 30);
        assert_eq!(
            v30,
            serde_json::to_value(violations(log.range(0..30), &d30.nomos, true, 0, 30)).unwrap()
        );
        assert_eq!(
            value(&ww.sanctions_json(0, 60).unwrap()),
            serde_json::to_value(sanctions(
                log.range(0..30),
                &Origin::default(),
                &d30.nomos,
                true,
                0,
                30
            ))
            .unwrap()
        );
        assert_eq!(value(&ww.timeline_json(0, 60).unwrap())["now"], 30);
        ww.seek(60).unwrap();
        assert_eq!(
            value(&ww.sanctions_json(0, 60).unwrap()),
            serde_json::to_value(&expect_sanctions).unwrap()
        );

        // A fork at tick 40: its ledger starts there, so the rule in force
        // and the episodes open at the fork come from the snapshot.
        ww.seek(40).unwrap();
        let bytes = ww.snapshot_bytes();
        let mut fork = WasmWorld::from_snapshot(&bytes, 25).unwrap();
        let mut df = World::from_bytes(&bytes).unwrap();
        let mut flog = EventLog::new();
        for json in [HARM_JSON, ENCROACH_JSON] {
            fork.intervene(json).unwrap();
            let iv: Intervention = serde_json::from_str(json).unwrap();
            flog.push(df.intervene(iv).unwrap()).unwrap();
        }
        let origin = Origin::of(&World::from_bytes(&bytes).unwrap().nomos);
        assert_eq!(origin.next_rule_id, 1);
        assert_eq!(origin.rules.len(), 1);
        let t = value(&fork.timeline_json(0, 100).unwrap());
        assert_eq!(t["from"], 0);
        assert_eq!(t["now"], 40);
        let entries = t["entries"].as_array().unwrap();
        assert_eq!(
            entries.len(),
            0,
            "decrees at the fork tick are read at now 41+"
        );
        fork.step(20).unwrap();
        run_logged(&mut df, &mut flog, 20);
        assert_eq!(fork.state_hash_hex(), format!("{:016x}", df.state_hash()));
        let t = value(&fork.timeline_json(0, 100).unwrap());
        assert_eq!(
            t,
            serde_json::to_value(timeline(flog.events(), &origin, 0, 60)).unwrap()
        );
        let entries = t["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(
            (
                &entries[0]["kind"],
                &entries[0]["rule"],
                &entries[0]["replaced"]
            ),
            (&Value::from("enacted"), &Value::from(1), &Value::Null)
        );
        assert_eq!(
            (&entries[1]["rule"], &entries[1]["replaced"]),
            (&Value::from(2), &Value::from(0))
        );
        let inst = value(&fork.institution_json().unwrap());
        assert_eq!(
            inst["rules"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        let fs = sanctions(flog.events(), &origin, &df.nomos, true, 40, 60);
        assert_eq!(
            value(&fork.sanctions_json(40, 60).unwrap()),
            serde_json::to_value(&fs).unwrap()
        );
        assert!(fs.entries.iter().all(|s| s.episode.is_some()));
        // Replacing the rule closed its episodes: nobody is accused under
        // rule 0 after the fork tick, and the new rule's episodes start
        // after it.
        assert!(fs.entries.iter().all(|s| s.rule != 0));
        assert!(fs.entries.iter().all(|s| s.episode.unwrap().since >= 40));
        assert!(
            !fs.entries.is_empty(),
            "the fork sanctions under its new rules"
        );
        assert!(d.nomos.rule(0).is_some(), "the parent keeps rule 0");
    }

    /// Decrees cross the boundary as the ledger's JSON and need a world
    /// with institutions; options are checked.
    #[test]
    fn decrees_cross_the_boundary_and_need_institutions() {
        assert_eq!(WorldOptions::parse("").unwrap(), WorldOptions::default());
        assert_eq!(
            WorldOptions::parse(" {} ").unwrap(),
            WorldOptions::default()
        );
        assert_eq!(
            WorldOptions::parse(r#"{"nomos":true}"#).unwrap(),
            WorldOptions {
                nomos: true,
                record_violations: false
            }
        );
        assert!(WorldOptions::parse(r#"{"nomo":true}"#).is_err());
        assert!(WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "nonsense").is_err());

        let mut plain = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
        let err = plain.intervene(ENCROACH_JSON).unwrap_err();
        assert!(err.contains("nomos is off"), "{err}");
        let inst = value(&plain.institution_json().unwrap());
        assert_eq!(inst["nomos"], false);
        assert_eq!(inst["rules"], Value::Array(vec![]));
        assert_eq!(inst["accused"]["episodes"], 0);
        assert!(plain.agents().chunks(AGENT_FIELDS).all(|a| a[8] == 0));
        let v = value(&plain.violations_json(0, 10).unwrap());
        assert_eq!(
            (&v["source"], &v["entries"]),
            (&Value::from("standing"), &Value::Array(vec![]))
        );
        assert_eq!(
            value(&plain.sanctions_json(0, 10).unwrap())["entries"],
            Value::Array(vec![])
        );

        let mut ruled =
            WasmWorld::new(3, 32, 32, 200, 3, 25, "random", r#"{"nomos":true}"#).unwrap();
        ruled.intervene(ENCROACH_JSON).unwrap();
        let again = ruled.intervene(HARM_JSON).unwrap();
        assert!(
            again.contains(r#""op":"enact""#) && again.contains(r#""kind":"harm""#),
            "{again}"
        );
        let repealed = ruled.intervene(r#"{"op":"repeal","rule":0}"#).unwrap();
        assert!(repealed.contains(r#""op":"repeal""#), "{repealed}");
        assert!(
            ruled.intervene(r#"{"op":"repeal","rule":0}"#).is_err(),
            "already repealed"
        );
        let chartered = ruled
            .intervene(r#"{"op":"charter","charter":{"quorum_permille":250,"threshold_permille":500,"vote_ticks":16,"amendable":false}}"#)
            .unwrap();
        assert!(chartered.contains(r#""op":"charter""#), "{chartered}");
        assert!(ruled
            .intervene(r#"{"op":"charter","charter":{"quorum_permille":2500,"threshold_permille":500,"vote_ticks":16,"amendable":false}}"#)
            .is_err());
        let inst = value(&ruled.institution_json().unwrap());
        assert_eq!(inst["charter"]["quorum_permille"], 250);
        assert_eq!(inst["charter"]["amendable"], false);
        assert_eq!(inst["rules"].as_array().unwrap().len(), 1);
        assert_eq!(inst["rules"][0]["id"], 1);
        ruled.step(1).unwrap();
        let t = value(&ruled.timeline_json(0, 1).unwrap());
        let kinds: Vec<&str> = t["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["kind"].as_str().unwrap())
            .collect();
        assert_eq!(kinds, vec!["enacted", "enacted", "repealed", "chartered"]);
        assert_eq!(t["entries"][2]["norm"]["kind"], "encroach");
        // The history is still the plain `Intervened` list.
        let h = value(&ruled.interventions_json().unwrap());
        assert_eq!(h.as_array().unwrap().len(), 4);
    }

    /// The ruled world's `dynamics_hash` and counts, computed natively, which
    /// `tests/wasm.rs::institutions_read_the_same_under_wasm` must reach
    /// under wasm (the world is the one above, violations not recorded).
    #[test]
    fn the_ruled_world_golden_is_pinned_for_the_wasm_check() {
        let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", r#"{"nomos":true}"#).unwrap();
        ww.intervene(ENCROACH_JSON).unwrap();
        ww.step(40).unwrap();
        let ledger = value(&ww.ledger_json().unwrap());
        let inst = value(&ww.institution_json().unwrap());
        let s = value(&ww.sanctions_json(0, 40).unwrap());
        let v = value(&ww.violations_json(0, 40).unwrap());
        assert_eq!(
            (
                ww.dynamics_hash_hex().as_str(),
                ledger["violations"][0].as_u64().unwrap(),
                ledger["sanctions"][0].as_u64().unwrap(),
                inst["accused"]["episodes"].as_u64().unwrap(),
                s["entries"].as_array().unwrap().len() as u64,
                v["entries"].as_array().unwrap().len() as u64,
            ),
            (
                RULED_HASH_40,
                RULED_VIOLATIONS_40,
                RULED_SANCTIONS_40,
                RULED_ACCUSED_40,
                RULED_SANCTIONS_40,
                RULED_ACCUSED_40
            )
        );
        let mut d =
            World::generate(ruled_config(false), cz_web::policy_mix("random").unwrap()).unwrap();
        d.intervene(Intervention::Enact { norm: ENCROACH }).unwrap();
        d.run(40);
        assert_eq!(format!("{:016x}", d.dynamics_hash()), RULED_HASH_40);
        // Recording the violations adds ledger events, not state.
        let mut r = WasmWorld::new(
            3,
            32,
            32,
            200,
            3,
            25,
            "random",
            r#"{"nomos":true,"record_violations":true}"#,
        )
        .unwrap();
        r.intervene(ENCROACH_JSON).unwrap();
        r.step(40).unwrap();
        assert_eq!(r.dynamics_hash_hex(), RULED_HASH_40);
        assert_ne!(
            r.state_hash_hex(),
            ww.state_hash_hex(),
            "the config differs"
        );
    }

    /// Pinned by the test above; `tests/wasm.rs` holds the same constants.
    const RULED_HASH_40: &str = "33babd3d304e5d92";
    const RULED_VIOLATIONS_40: u64 = 9;
    const RULED_SANCTIONS_40: u64 = 11;
    const RULED_ACCUSED_40: u64 = 6;
}
