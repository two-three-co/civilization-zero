//! The wasm surface must not change what the kernel computes. These run
//! natively; `tests/wasm.rs` runs the same kind of check inside wasm.

use cz_web::{
    policy_mix, WasmWorld, AGENT_FIELDS, MARKER_FIELDS, MARKER_HARM, MARKER_SIGNAL, MARKER_TRANSFER,
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
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager").unwrap();
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
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager").unwrap();
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
    let mut ww = WasmWorld::new(7, 64, 64, 200, 5, 50, "forager").unwrap();
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
        WasmWorld::new(1, 0, 0, 1, 1, 1, "forager").is_err(),
        "invalid config is reported"
    );
}

#[test]
fn a_fork_built_from_a_snapshot_matches_multiverse_fork_and_is_reproducible() {
    use multiverse::{BranchId, Multiverse};

    let mut parent = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager").unwrap();
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
    let mut ww = WasmWorld::new(7, 64, 64, 200, 5, 50, "forager").unwrap();
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
    let rules: [(&str, Rule); 3] = [
        ("forager", |_| PolicyKind::Forager),
        ("random", |_| PolicyKind::Random),
        ("mixed", |i| {
            if i % 2 == 0 {
                PolicyKind::Forager
            } else {
                PolicyKind::Random
            }
        }),
    ];
    for (name, rule) in rules {
        let mut ww = WasmWorld::new(9, 32, 32, 200, 3, 25, name).unwrap();
        ww.step(60).unwrap();
        let mut d = World::generate(crowded(9), rule).unwrap();
        d.run(60);
        assert_eq!(ww.state_hash_hex(), hex(&d), "{name}");
        let random_walkers = ww
            .agents()
            .chunks(AGENT_FIELDS)
            .filter(|a| a[5] == 1)
            .count();
        assert_eq!(
            random_walkers,
            d.agents
                .iter()
                .filter(|a| a.policy == PolicyKind::Random)
                .count(),
            "{name}"
        );
        let ledger: Value = serde_json::from_str(&ww.ledger_json().unwrap()).unwrap();
        let social = ["transfers", "harms", "signals"]
            .iter()
            .map(|k| ledger[k].as_u64().unwrap())
            .sum::<u64>();
        if name == "forager" {
            assert_eq!(social, 0, "foragers never transfer, harm or signal");
            assert!(ww.social().is_empty());
        } else {
            assert!(social > 0, "{name}: random walkers interact");
        }
    }
    assert!(WasmWorld::new(9, 32, 32, 200, 3, 25, "learner").is_err());
    assert!(policy_mix("mixed").is_ok());
    assert!(policy_mix("").is_err());
}

#[test]
fn social_markers_are_the_ledgers_last_tick_placed_in_the_view() {
    let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random").unwrap();
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
    let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random").unwrap();
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
    assert_eq!(json, serde_json::to_string(a).unwrap());
    let v: Value = serde_json::from_str(&json).unwrap();
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
    let mut ww = WasmWorld::new(3, 32, 32, 200, 3, 25, "random").unwrap();
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
