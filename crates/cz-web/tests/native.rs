//! The wasm surface must not change what the kernel computes. These run
//! natively; `tests/wasm.rs` runs the same kind of check inside wasm.

use cz_web::{WasmWorld, AGENT_FIELDS};
use simulation_core::{Intervention, World, WorldConfig};

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

#[test]
fn wrapper_reproduces_the_kernel_exactly() {
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100).unwrap();
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
    let mut ww = WasmWorld::new(42, 128, 128, 1000, 24, 100).unwrap();
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
    let mut ww = WasmWorld::new(7, 64, 64, 200, 5, 50).unwrap();
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
        WasmWorld::new(1, 0, 0, 1, 1, 1).is_err(),
        "invalid config is reported"
    );
}

#[test]
fn a_fork_built_from_a_snapshot_matches_multiverse_fork_and_is_reproducible() {
    use multiverse::{BranchId, Multiverse};

    let mut parent = WasmWorld::new(42, 128, 128, 1000, 24, 100).unwrap();
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
    let mut ww = WasmWorld::new(7, 64, 64, 200, 5, 50).unwrap();
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
