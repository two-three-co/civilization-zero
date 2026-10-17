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
    // Skip the founding transient (die-off, first birth wave) so the
    // population is in its steady band (docs/research/experiments/EXP-001).
    w.run(1_000);
    w
}

fn bench_step(c: &mut Criterion) {
    let mut g = c.benchmark_group("step");
    // Density held roughly constant (~0.06 agents / cell).
    for &(agents, side) in &[(1_000u32, 128u16), (10_000, 400), (100_000, 1_280)] {
        let base = warmed_world(agents, side);
        // One iteration = a whole regrowth period, so the measurement is not
        // biased towards the food-rich tick right after regrowth. Reported
        // time is per period; divide by TICKS for per-tick.
        const TICKS: u64 = 10;
        assert_eq!(base.config.regrow_interval, TICKS);
        g.throughput(Throughput::Elements(base.agents.len() as u64 * TICKS));
        g.bench_with_input(BenchmarkId::new("serial", agents), &base, |b, base| {
            b.iter_batched_ref(
                || base.clone(),
                |w| w.run(TICKS),
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
                    |w| {
                        for _ in 0..TICKS {
                            w.step_parallel();
                        }
                    },
                    criterion::BatchSize::LargeInput,
                )
            },
        );
    }
    g.finish();
}

/// The decide phase alone, over the ten consecutive states of one regrowth
/// period, so it can be compared with `step/serial/1000` (step − decide ≈ apply).
fn bench_decide(c: &mut Criterion) {
    let mut w = warmed_world(1_000, 128);
    let states: Vec<World> = (0..10)
        .map(|_| {
            let s = w.clone();
            w.step();
            s
        })
        .collect();
    let agent_steps: u64 = states.iter().map(|s| s.agents.len() as u64).sum();
    let mut g = c.benchmark_group("decide");
    g.throughput(Throughput::Elements(agent_steps));
    g.bench_function("serial/1000", |b| {
        b.iter(|| {
            for s in &states {
                criterion::black_box(s.decide_all());
            }
        })
    });
    g.finish();
}

fn bench_hash(c: &mut Criterion) {
    let w = warmed_world(1_000, 128);
    c.bench_function("state_hash/1000", |b| b.iter(|| w.state_hash()));
    c.bench_function("spatial_index/1000", |b| b.iter(|| w.spatial_index()));
    c.bench_function("snapshot_roundtrip/1000", |b| {
        b.iter(|| World::from_snapshot(&w.to_snapshot()).unwrap())
    });
    c.bench_function("snapshot_bin_roundtrip/1000", |b| {
        b.iter(|| World::from_bytes(&w.to_bytes()).unwrap())
    });
}

criterion_group!(benches, bench_step, bench_decide, bench_hash);
criterion_main!(benches);
