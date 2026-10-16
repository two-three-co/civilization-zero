//! Throughput of `World::step` at several population sizes.
//! Run: `cargo bench -p simulation-core` (add `--features parallel` for rayon).

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use simulation_core::{World, WorldConfig};

fn warmed_world(agents: u32, side: u16) -> World {
    let cfg = WorldConfig {
        seed: 42,
        width: side,
        height: side,
        initial_agents: agents,
        patch_count: (side as u32 * side as u32) / 700,
        ..WorldConfig::default()
    };
    let mut w = World::new(cfg).unwrap();
    // Skip the initial die-off so we measure the steady state.
    w.run(200);
    w
}

fn bench_step(c: &mut Criterion) {
    let mut g = c.benchmark_group("step");
    // Density held roughly constant (~0.06 agents / cell).
    for &(agents, side) in &[(1_000u32, 128u16), (10_000, 400), (100_000, 1_280)] {
        let base = warmed_world(agents, side);
        g.throughput(Throughput::Elements(base.agents.len() as u64));
        g.bench_with_input(BenchmarkId::new("serial", agents), &base, |b, base| {
            b.iter_batched_ref(
                || base.clone(),
                |w| w.step(),
                criterion::BatchSize::LargeInput,
            )
        });
        #[cfg(feature = "parallel")]
        g.bench_with_input(
            BenchmarkId::new("parallel_decide", agents),
            &base,
            |b, base| {
                b.iter_batched_ref(
                    || base.clone(),
                    |w| w.step_parallel(),
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    g.finish();
}

fn bench_hash(c: &mut Criterion) {
    let w = warmed_world(1_000, 128);
    c.bench_function("state_hash/1000", |b| b.iter(|| w.state_hash()));
    c.bench_function("snapshot_roundtrip/1000", |b| {
        b.iter(|| World::from_snapshot(&w.to_snapshot()).unwrap())
    });
}

criterion_group!(benches, bench_step, bench_hash);
criterion_main!(benches);
