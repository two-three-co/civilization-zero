//! Runs inside wasm32 (`wasm-pack test --node crates/cz-web`): the hash the
//! kernel reaches under wasm must equal the golden dynamics hash computed natively
//! (`simulation-core/tests/determinism.rs::GOLDEN_200`). If that constant
//! changes, change it here too.
#![cfg(target_arch = "wasm32")]

use cz_web::{WasmWorld, AGENT_FIELDS, FLAG_ACCUSED, FLAG_HARMABLE, MARKER_FIELDS};
use wasm_bindgen_test::*;

const GOLDEN_200_HEX: &str = "658ee873d1866501";

#[wasm_bindgen_test]
fn wasm_reaches_the_native_golden_hash() {
    // WorldConfig::default() has 24 patches; seed 42, 1,000 agents, 128x128.
    let mut w = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager", "").unwrap();
    w.step(200).unwrap();
    assert_eq!(w.dynamics_hash_hex(), GOLDEN_200_HEX);
    assert_eq!((w.tick(), w.frontier()), (200, 200));
}

#[wasm_bindgen_test]
fn seek_restores_the_same_state_under_wasm() {
    let mut w = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager", "").unwrap();
    w.step(250).unwrap();
    let at_250 = w.state_hash_hex();
    w.seek(130).unwrap();
    let at_130 = w.state_hash_hex();
    w.seek(250).unwrap();
    assert_eq!(w.state_hash_hex(), at_250);
    w.seek(130).unwrap();
    assert_eq!(w.state_hash_hex(), at_130);
}

/// The social surface (ADR-0013 3-9a) in a crowded world of random walkers:
/// markers, totals and one agent's memory come out well-formed, and the same
/// world built twice agrees with itself. (`tests/native.rs` checks the
/// contents against the kernel; this checks the code path under wasm.)
#[wasm_bindgen_test]
fn the_social_surface_works_under_wasm() {
    let mut w = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    assert!(w.social().is_empty(), "nothing has happened at tick 0");
    w.step(40).unwrap();

    let markers = w.social();
    assert!(!markers.is_empty());
    assert_eq!(markers.len() % MARKER_FIELDS, 0);
    assert!(markers
        .chunks(MARKER_FIELDS)
        .all(|m| (0..=2).contains(&m[0]) && m[1] == 39 && m[2] >= 0 && m[3] >= 0));

    let ledger: serde_json::Value = serde_json::from_str(&w.ledger_json().unwrap()).unwrap();
    assert!(ledger["signals"].as_u64().unwrap() > 0);
    assert!(ledger["harms"].as_u64().unwrap() > 0);

    let id = w.agents()[0] as u32;
    let a: serde_json::Value = serde_json::from_str(&w.agent_json(id).unwrap()).unwrap();
    assert_eq!(a["id"], id as u64);
    assert!(a["recent"].is_array());
    let story: Vec<serde_json::Value> =
        serde_json::from_str(&w.agent_events_json(id, 0, 40).unwrap()).unwrap();
    assert!(story.iter().all(|e| e["tick"].as_u64().unwrap() < 40));
    assert_eq!(w.agent_json(u32::MAX).unwrap(), "null");

    let mut again = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    again.step(40).unwrap();
    assert_eq!(again.state_hash_hex(), w.state_hash_hex());
    assert_eq!(again.social(), markers);

    assert!(WasmWorld::new(3, 32, 32, 200, 3, 25, "walker", "").is_err());
}

/// The learning world's golden, computed natively
/// (`tests/native.rs::the_learner_world_golden_is_pinned_for_the_wasm_check`):
/// the integer Q learner must reach the same hash under wasm. If the
/// Learner's rule changes on purpose, update both constants.
const LEARNER_GOLDEN_200_HEX: &str = "cb8bc0a9772c09a4";

/// 3-9c under wasm: a world of learners reaches the native hash, and
/// "fixed vs learning" — a fork of a forager world with every agent
/// switched by `SetPolicy` through `intervene` — is reproducible and
/// reports its agents as learners (policy code 3). `tests/native.rs`
/// checks the contents against `Multiverse::fork`; this checks the code
/// path under wasm.
#[wasm_bindgen_test]
fn learning_worlds_run_under_wasm_and_the_compare_fork_is_reproducible() {
    let codes =
        |w: &WasmWorld| -> Vec<i32> { w.agents().chunks(AGENT_FIELDS).map(|a| a[5]).collect() };
    let mut learners = WasmWorld::new(42, 128, 128, 1000, 24, 100, "learner", "").unwrap();
    learners.step(200).unwrap();
    assert_eq!(learners.dynamics_hash_hex(), LEARNER_GOLDEN_200_HEX);
    assert!(codes(&learners).iter().all(|&c| c == 3));

    let mut parent = WasmWorld::new(3, 32, 32, 200, 3, 25, "forager", "").unwrap();
    parent.step(100).unwrap();
    parent.seek(60).unwrap();
    let bytes = parent.snapshot_bytes();
    let fork = || {
        let mut b = WasmWorld::from_snapshot(&bytes, 25).unwrap();
        b.intervene(r#"{"op":"set_policy","who":"all","kind":"Learner","params":"default"}"#)
            .unwrap();
        b.step(80).unwrap();
        b
    };
    let (b, c) = (fork(), fork());
    assert_eq!(b.state_hash_hex(), c.state_hash_hex());
    assert!(codes(&b).iter().all(|&c| c == 3));
    assert!(codes(&parent).iter().all(|&c| c == 0));
    let ledger: serde_json::Value = serde_json::from_str(&b.ledger_json().unwrap()).unwrap();
    assert!(
        ledger["transfers"].as_u64().unwrap()
            + ledger["harms"].as_u64().unwrap()
            + ledger["signals"].as_u64().unwrap()
            > 0,
        "blank learners try every kind"
    );
    let a: serde_json::Value =
        serde_json::from_str(&b.agent_json(b.agents()[0] as u32).unwrap()).unwrap();
    assert_eq!(
        a["params"]["table"]["learner"]["q"]
            .as_array()
            .unwrap()
            .len(),
        32 * 8
    );

    let mixed = WasmWorld::new(3, 32, 32, 200, 3, 25, "mixed-learning", "").unwrap();
    let mut seen: Vec<i32> = codes(&mixed);
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen,
        vec![0, 2, 3],
        "forager / reflex / learner by founding index"
    );
}

/// Negotiation threads (ADR-0013 3-9b) under wasm: in the crowded random
/// world the deals finder and the pair thread agree with each other and
/// with the ledger, and the same world built twice reads the same.
/// (`tests/native.rs` checks the judgement itself against hand-built
/// ledgers and the kernel; this checks the code path under wasm.)
#[wasm_bindgen_test]
fn negotiation_threads_work_under_wasm() {
    let mut w = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    w.step(40).unwrap();
    let deals: Vec<serde_json::Value> =
        serde_json::from_str(&w.deals_json(0, 40).unwrap()).unwrap();
    let ledger: serde_json::Value = serde_json::from_str(&w.ledger_json().unwrap()).unwrap();
    assert_eq!(
        deals.len() as u64,
        ledger["acceptances"].as_u64().unwrap(),
        "one deal per acceptance"
    );
    assert!(deals
        .iter()
        .all(|d| ["open", "fulfilled", "broken"].contains(&d["status"].as_str().unwrap())));

    let d = &deals[0];
    let (p, q) = (
        d["proposer"].as_u64().unwrap() as u32,
        d["acceptor"].as_u64().unwrap() as u32,
    );
    let json = w.thread_json(p, q, 0, 40).unwrap();
    let t: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        (
            t["a"].as_u64(),
            t["b"].as_u64(),
            t["from"].as_u64(),
            t["now"].as_u64()
        ),
        (Some(p as u64), Some(q as u64), Some(0), Some(40))
    );
    assert!(t["deals"].as_array().unwrap().contains(d));
    let entries = t["entries"].as_array().unwrap();
    assert!(entries
        .iter()
        .any(|e| e["kind"] == "accepted" && e["offer_tick"] == d["offer_tick"]));
    assert!(
        entries
            .windows(2)
            .all(|w| w[0]["tick"].as_u64() <= w[1]["tick"].as_u64()),
        "ledger order"
    );
    // Read at the view: `to` past the view is capped, and an earlier view is shorter.
    assert_eq!(w.thread_json(p, q, 0, 1000).unwrap(), json);
    w.seek(20).unwrap();
    let early: serde_json::Value =
        serde_json::from_str(&w.thread_json(p, q, 0, 40).unwrap()).unwrap();
    assert_eq!(early["now"], 20);
    assert!(early["entries"].as_array().unwrap().len() <= entries.len());
    w.seek(40).unwrap();

    let mut again = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    again.step(40).unwrap();
    assert_eq!(again.thread_json(p, q, 0, 40).unwrap(), json);
    assert_eq!(
        again.deals_json(0, 40).unwrap(),
        w.deals_json(0, 40).unwrap()
    );
}

/// The ruled world's golden (`dynamics_hash`), computed natively
/// (`tests/native.rs::institutions::the_ruled_world_golden_is_pinned_for_the_wasm_check`):
/// seed 3, 32², 200 random walkers, 3 patches, `{"nomos":true}`
/// (`WorldConfig::nomos_preset()`), the first-occupancy norm decreed at
/// tick 0 (standing 32, sanction harm), 40 ticks. If the kernel's rule
/// changes on purpose, update both files (4-5 did: the random walkers
/// draw sixteen actions since, ADR-0014 D7).
const RULED_HASH_40: &str = "1e2a77fc40cf7156";
const RULED_VIOLATIONS_40: u64 = 4;
const RULED_SANCTIONS_40: u64 = 5;
const RULED_ACCUSED_40: u64 = 3;

const ENCROACH_JSON: &str = r#"{"op":"enact","norm":{"kind":"encroach","standing_ticks":32,"sanction":1,"except_sanction":false}}"#;

/// Institutions (ADR-0014 D12, 4-7) under wasm: a decreed world reaches
/// the native hash and counts, the readers come out well-formed and agree
/// with each other and with the ledger, a seek reads at the view, and a
/// fork from a snapshot continues the rule ids. (`tests/native.rs` checks
/// the readers against hand-built ledgers and the kernel; this checks the
/// code path under wasm.)
#[wasm_bindgen_test]
fn institutions_read_the_same_under_wasm() {
    let value = |s: String| -> serde_json::Value { serde_json::from_str(&s).unwrap() };
    let mut w = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", r#"{"nomos":true}"#).unwrap();
    let decree = w.intervene(ENCROACH_JSON).unwrap();
    assert!(decree.contains(r#""op":"enact""#), "{decree}");
    w.step(40).unwrap();
    assert_eq!(w.dynamics_hash_hex(), RULED_HASH_40);
    let ledger = value(w.ledger_json().unwrap());
    assert_eq!(ledger["violations"][0], RULED_VIOLATIONS_40);
    assert_eq!(ledger["sanctions"][0], RULED_SANCTIONS_40);

    let inst = value(w.institution_json().unwrap());
    assert_eq!(inst["nomos"], true);
    assert_eq!(inst["record_violations"], false);
    assert_eq!(inst["rules"][0]["id"], 0);
    assert_eq!(inst["rules"][0]["norm"]["kind"], "encroach");
    assert_eq!(inst["accused"]["episodes"], RULED_ACCUSED_40);
    assert_eq!(inst["accused"]["by_kind"][0], RULED_ACCUSED_40);

    let rows = w.agents();
    assert_eq!(rows.len() % AGENT_FIELDS, 0);
    assert_eq!(AGENT_FIELDS, 9);
    let accused: Vec<i32> = rows
        .chunks(AGENT_FIELDS)
        .filter(|a| a[8] & FLAG_ACCUSED != 0)
        .map(|a| a[0])
        .collect();
    assert_eq!(
        accused.len() as u64,
        inst["accused"]["agents"].as_u64().unwrap()
    );
    assert!(rows
        .chunks(AGENT_FIELDS)
        .all(|a| (a[8] & FLAG_HARMABLE != 0) == (a[8] & FLAG_ACCUSED != 0)));
    let a = value(w.agent_json(accused[0] as u32).unwrap());
    assert_eq!(a["standing"][0]["rule"], 0);
    assert_eq!(a["standing"][0]["norm"], "encroach");
    // Random walkers vote (4-5): the inspector's `voted` is the flag's.
    let voted_flag = rows
        .chunks(AGENT_FIELDS)
        .find(|r| r[0] == accused[0])
        .map(|r| r[8] & cz_web::FLAG_VOTED != 0)
        .unwrap();
    assert_eq!(a["voted"], voted_flag);

    // The decree first; whatever the walkers moved and closed follows it
    // in the same list (4-5), each by its motion.
    let t = value(w.timeline_json(0, 40).unwrap());
    let entries = t["entries"].as_array().unwrap();
    assert!(!entries.is_empty());
    assert_eq!(entries[0]["kind"], "enacted");
    assert_eq!(entries[0]["rule"], 0);
    assert_eq!(entries[0]["by"], "decree");
    assert_eq!(
        entries.iter().filter(|e| e["by"] == "decree").count(),
        1,
        "one decree"
    );
    assert!(entries[1..].iter().all(|e| {
        matches!(
            e["kind"].as_str().unwrap(),
            "motioned" | "motion_closed" | "enacted" | "repealed" | "chartered"
        )
    }));

    let v = value(w.violations_json(0, 40).unwrap());
    assert_eq!(v["source"], "standing");
    assert_eq!(
        v["entries"].as_array().unwrap().len() as u64,
        RULED_ACCUSED_40
    );
    assert!(v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["kind"] == "accused"));

    let s = value(w.sanctions_json(0, 40).unwrap());
    let entries = s["entries"].as_array().unwrap();
    assert_eq!(entries.len() as u64, RULED_SANCTIONS_40);
    assert!(entries
        .iter()
        .all(|e| e["via"] == "harm" && e["norm"] == "encroach"));
    assert!(
        entries
            .iter()
            .filter(|e| !e["episode"].is_null())
            .all(|e| e["episode"]["open"] == true && e["episode"]["source"] == "standing"),
        "not recorded: only open episodes are named"
    );

    // Read at the view after a seek.
    w.seek(20).unwrap();
    assert_eq!(value(w.sanctions_json(0, 40).unwrap())["now"], 20);
    assert_eq!(w.agents().len() % AGENT_FIELDS, 0);
    let early = value(w.institution_json().unwrap());
    assert_eq!(early["rules"][0]["id"], 0);
    w.seek(40).unwrap();
    assert_eq!(value(w.institution_json().unwrap()), inst);

    // Recorded violations come from the ledger, one per judged violation.
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
    assert_eq!(
        r.dynamics_hash_hex(),
        RULED_HASH_40,
        "recording changes no state"
    );
    let v = value(r.violations_json(0, 40).unwrap());
    assert_eq!(v["source"], "ledger");
    assert_eq!(
        v["entries"].as_array().unwrap().len() as u64,
        RULED_VIOLATIONS_40
    );
    assert!(v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["kind"] == "violated" && e["grounds"]["fact"] == "encroachment"));
    let s = value(r.sanctions_json(0, 40).unwrap());
    assert!(s["entries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| !e["episode"].is_null()));

    // A fork continues the ids: the snapshot carries rule 0, the next decree is rule 1.
    let bytes = w.snapshot_bytes();
    let mut fork = WasmWorld::from_snapshot(&bytes, 25).unwrap();
    fork.intervene(r#"{"op":"enact","norm":{"kind":"harm","standing_ticks":8,"sanction":2,"except_sanction":true}}"#).unwrap();
    fork.step(5).unwrap();
    let t = value(fork.timeline_json(0, 100).unwrap());
    assert_eq!(t["entries"].as_array().unwrap().len(), 1);
    assert_eq!(t["entries"][0]["rule"], 1);
    assert_eq!(t["entries"][0]["replaced"], serde_json::Value::Null);
    let inst = value(fork.institution_json().unwrap());
    assert_eq!(inst["rules"].as_array().unwrap().len(), 2);

    // Without institutions a decree is refused and everything reads empty.
    let mut plain = WasmWorld::new(3, 32, 32, 200, 3, 25, "random", "").unwrap();
    assert!(plain.intervene(ENCROACH_JSON).is_err());
    assert_eq!(value(plain.institution_json().unwrap())["nomos"], false);
    assert!(plain.agents().chunks(AGENT_FIELDS).all(|a| a[8] == 0));
    assert!(WasmWorld::new(3, 32, 32, 200, 3, 25, "random", r#"{"bogus":1}"#).is_err());
}
