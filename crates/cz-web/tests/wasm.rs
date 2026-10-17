//! Runs inside wasm32 (`wasm-pack test --node crates/cz-web`): the hash the
//! kernel reaches under wasm must equal the golden dynamics hash computed natively
//! (`simulation-core/tests/determinism.rs::GOLDEN_200`). If that constant
//! changes, change it here too.
#![cfg(target_arch = "wasm32")]

use cz_web::WasmWorld;
use wasm_bindgen_test::*;

const GOLDEN_200_HEX: &str = "b7b914705736f95f";

#[wasm_bindgen_test]
fn wasm_reaches_the_native_golden_hash() {
    // WorldConfig::default() has 24 patches; seed 42, 1,000 agents, 128x128.
    let mut w = WasmWorld::new(42, 128, 128, 1000, 24, 100).unwrap();
    w.step(200).unwrap();
    assert_eq!(w.dynamics_hash_hex(), GOLDEN_200_HEX);
    assert_eq!((w.tick(), w.frontier()), (200, 200));
}

#[wasm_bindgen_test]
fn seek_restores_the_same_state_under_wasm() {
    let mut w = WasmWorld::new(42, 128, 128, 1000, 24, 100).unwrap();
    w.step(250).unwrap();
    let at_250 = w.state_hash_hex();
    w.seek(130).unwrap();
    let at_130 = w.state_hash_hex();
    w.seek(250).unwrap();
    assert_eq!(w.state_hash_hex(), at_250);
    w.seek(130).unwrap();
    assert_eq!(w.state_hash_hex(), at_130);
}
