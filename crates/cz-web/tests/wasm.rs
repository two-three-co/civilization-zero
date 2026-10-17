//! Runs inside wasm32 (`wasm-pack test --node crates/cz-web`): the hash the
//! kernel reaches under wasm must equal the golden dynamics hash computed natively
//! (`simulation-core/tests/determinism.rs::GOLDEN_200`). If that constant
//! changes, change it here too.
#![cfg(target_arch = "wasm32")]

use cz_web::{WasmWorld, MARKER_FIELDS};
use wasm_bindgen_test::*;

const GOLDEN_200_HEX: &str = "658ee873d1866501";

#[wasm_bindgen_test]
fn wasm_reaches_the_native_golden_hash() {
    // WorldConfig::default() has 24 patches; seed 42, 1,000 agents, 128x128.
    let mut w = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager").unwrap();
    w.step(200).unwrap();
    assert_eq!(w.dynamics_hash_hex(), GOLDEN_200_HEX);
    assert_eq!((w.tick(), w.frontier()), (200, 200));
}

#[wasm_bindgen_test]
fn seek_restores_the_same_state_under_wasm() {
    let mut w = WasmWorld::new(42, 128, 128, 1000, 24, 100, "forager").unwrap();
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
    let mut w = WasmWorld::new(3, 32, 32, 200, 3, 25, "random").unwrap();
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

    let mut again = WasmWorld::new(3, 32, 32, 200, 3, 25, "random").unwrap();
    again.step(40).unwrap();
    assert_eq!(again.state_hash_hex(), w.state_hash_hex());
    assert_eq!(again.social(), markers);

    assert!(WasmWorld::new(3, 32, 32, 200, 3, 25, "learner").is_err());
}
