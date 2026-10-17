//! Binary snapshots (ADR-0006 revision, ADR-0010 2a-6).

use simulation_core::world::SNAPSHOT_MAGIC;
use simulation_core::{SnapshotError, World, WorldConfig};

fn warmed() -> World {
    let mut w = World::new(WorldConfig {
        seed: 42,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(1_000);
    w
}

#[test]
fn binary_roundtrip_is_lossless_and_under_100kb() {
    let w = warmed();
    let bytes = w.to_bytes();
    assert!(bytes.starts_with(&SNAPSHOT_MAGIC));
    let back = World::from_bytes(&bytes).unwrap();
    assert_eq!(back, w);
    println!(
        "binary snapshot: {} bytes for {} agents on {}x{} (json: {} bytes)",
        bytes.len(),
        w.agents.len(),
        w.config.width,
        w.config.height,
        w.to_snapshot().len()
    );
    assert!(bytes.len() <= 100_000, "{} bytes", bytes.len());
    let (mut a, mut b) = (w, back);
    a.run(100);
    b.run(100);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn corrupted_or_foreign_bytes_are_rejected() {
    let mut w = World::new(WorldConfig {
        seed: 3,
        width: 24,
        height: 24,
        initial_agents: 40,
        patch_count: 2,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(30);
    let bytes = w.to_bytes();

    // JSON handed to the binary reader: wrong magic.
    assert!(matches!(
        World::from_bytes(&w.to_snapshot()),
        Err(SnapshotError::Decode(_))
    ));
    // Truncated.
    assert!(World::from_bytes(&bytes[..bytes.len() / 2]).is_err());
    // One byte flipped anywhere in the payload: decode, invariants or the
    // hash must catch it.
    for at in [
        SNAPSHOT_MAGIC.len() + 1,
        bytes.len() / 3,
        bytes.len() / 2,
        bytes.len() - 2,
    ] {
        let mut bad = bytes.clone();
        bad[at] ^= 0x5a;
        assert!(
            World::from_bytes(&bad).is_err(),
            "flip at {at} went unnoticed"
        );
    }
    // Still fine untouched.
    assert_eq!(World::from_bytes(&bytes).unwrap(), w);
}
