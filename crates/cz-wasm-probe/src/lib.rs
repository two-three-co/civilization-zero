//! Smallest possible surface for driving the kernel from JavaScript without
//! wasm-bindgen: a C ABI over one thread-local world. It exists to answer
//! ADR-0009's questions (does the kernel build and run under wasm32, is the
//! state hash identical to native, how fast is it) and is not the product API.
//!
//! Build: `cargo build -p cz-wasm-probe --target wasm32-unknown-unknown --release`
//! Harness: `viewer/wasm-probe.html`.

use simulation_core::{World, WorldConfig};
use std::cell::RefCell;

thread_local! {
    static WORLD: RefCell<Option<World>> = const { RefCell::new(None) };
}

/// Same derivation as `cz run`, so hashes can be compared with the CLI.
pub fn config(seed: u64, agents: u32, side: u16) -> WorldConfig {
    WorldConfig {
        seed,
        width: side,
        height: side,
        initial_agents: agents,
        patch_count: ((side as u32 * side as u32) / 700).max(1),
        ..WorldConfig::default()
    }
}

/// Creates the world. Returns the number of agents, or 0 if the config is invalid.
#[no_mangle]
pub extern "C" fn probe_init(seed: u64, agents: u32, side: u16) -> u32 {
    WORLD.with(|w| {
        let world = World::new(config(seed, agents, side)).ok();
        let n = world.as_ref().map_or(0, |w| w.agents.len() as u32);
        *w.borrow_mut() = world;
        n
    })
}

/// Advances `ticks` ticks. Returns the number of living agents.
#[no_mangle]
pub extern "C" fn probe_step(ticks: u32) -> u32 {
    WORLD.with(|w| {
        let mut guard = w.borrow_mut();
        let world = guard.as_mut().expect("probe_init first");
        world.run(ticks as u64);
        world.agents.len() as u32
    })
}

#[no_mangle]
pub extern "C" fn probe_tick() -> u64 {
    WORLD.with(|w| w.borrow().as_ref().map_or(0, |w| w.tick))
}

#[no_mangle]
pub extern "C" fn probe_hash() -> u64 {
    WORLD.with(|w| w.borrow().as_ref().map_or(0, |w| w.state_hash()))
}

/// Size of a JSON snapshot of the current world, in bytes (ADR-0009 sizing).
#[no_mangle]
pub extern "C" fn probe_snapshot_len() -> u32 {
    WORLD.with(|w| {
        w.borrow()
            .as_ref()
            .map_or(0, |w| w.to_snapshot().len() as u32)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The native side of the ADR-0009 comparison: the harness prints this
    /// value for (42, 1000, 128, 1000 ticks) when run under wasm32.
    #[test]
    fn native_reference_hash() {
        probe_init(42, 1000, 128);
        probe_step(1000);
        println!("native hash after 1000 ticks: {:#018x}", probe_hash());
        assert_eq!(probe_tick(), 1000);
    }
}
