//! Throughput of `World::step` at several population sizes.
//! Run: `cargo bench -p simulation-core` (add `--features parallel` for rayon).

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use simulation_core::{PolicyKind, World, WorldConfig};

fn warmed_world(agents: u32, side: u16) -> World {
    warmed_world_of(agents, side, PolicyKind::Forager, false)
}

/// `mutation` switches weight / scalar mutation on, so after the warm-up a
/// reflex population carries evolved (dense, non-zero) weights.
fn warmed_world_of(agents: u32, side: u16, kind: PolicyKind, mutation: bool) -> World {
    warmed_world_with(agents, side, kind, mutation, false)
}

/// `nomos` switches the tracking of social facts on (ADR-0014 4-1: first
/// occupancy, encroachment, the deal audit) — no rules, so the decisions
/// of a forager or learner world are the same as without it.
fn warmed_world_with(
    agents: u32,
    side: u16,
    kind: PolicyKind,
    mutation: bool,
    nomos: bool,
) -> World {
    let cfg = WorldConfig {
        seed: 42,
        width: side,
        height: side,
        initial_agents: agents,
        patch_count: (side as u32 * side as u32) / 700,
        mutation_enabled: mutation,
        nomos,
        ..WorldConfig::default()
    };
    let mut w = World::generate(cfg, |_| kind).unwrap();
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
    // A reflex world of the same size (ADR-0013 3-6): the same dynamics as
    // the forager world with the initial weights, so the population is the
    // same; the cost of features + scores + lottery is the difference.
    let reflex = warmed_world_of(1_000, 128, PolicyKind::Reflex, false);
    g.throughput(Throughput::Elements(reflex.agents.len() as u64 * 10));
    g.bench_with_input(BenchmarkId::new("reflex", 1_000), &reflex, |b, base| {
        b.iter_batched_ref(
            || base.clone(),
            |w| w.run(10),
            criterion::BatchSize::LargeInput,
        )
    });
    // A learner world (ADR-0013 3-7) after 1,000 ticks of learning: its own
    // population, decide with the Q lookup and apply with the Q update.
    let learner = warmed_world_of(1_000, 128, PolicyKind::Learner, false);
    g.throughput(Throughput::Elements(learner.agents.len() as u64 * 10));
    g.bench_with_input(BenchmarkId::new("learner", 1_000), &learner, |b, base| {
        b.iter_batched_ref(
            || base.clone(),
            |w| w.run(10),
            criterion::BatchSize::LargeInput,
        )
    });
    // The same two worlds with the social-fact tracking on (ADR-0014 4-1,
    // acceptance: step ≤ +10% with `nomos` on and no rules). Same
    // decisions and populations as `serial` / `learner`; the difference is
    // the encroachment judgement, the audit and the `arrived` bookkeeping.
    for (name, kind) in [
        ("nomos", PolicyKind::Forager),
        ("learner_nomos", PolicyKind::Learner),
    ] {
        let w = warmed_world_with(1_000, 128, kind, false, true);
        g.throughput(Throughput::Elements(w.agents.len() as u64 * 10));
        g.bench_with_input(BenchmarkId::new(name, 1_000), &w, |b, base| {
            b.iter_batched_ref(
                || base.clone(),
                |w| w.run(10),
                criterion::BatchSize::LargeInput,
            )
        });
    }
    g.finish();
}

/// The decide phase alone, over the ten consecutive states of one regrowth
/// period, so it can be compared with `step/serial/1000` (step − decide ≈ apply).
/// `reflex/1000` is the same for a reflex world with the initial (forager)
/// weights and `reflex_evolved/1000` for one whose weights have mutated for
/// 1,000 ticks (ADR-0013 3-6 budget: ≤ 150 ns per agent-step).
/// `learner/1000` is a learner world after 1,000 ticks of learning (ADR-0013
/// 3-7 budget: ≤ 200 ns per agent-step; the Q update itself is in apply).
fn bench_decide(c: &mut Criterion) {
    let mut g = c.benchmark_group("decide");
    for (name, kind, mutation) in [
        ("serial/1000", PolicyKind::Forager, false),
        ("reflex/1000", PolicyKind::Reflex, false),
        ("reflex_evolved/1000", PolicyKind::Reflex, true),
        ("learner/1000", PolicyKind::Learner, false),
    ] {
        let mut w = warmed_world_of(1_000, 128, kind, mutation);
        let states: Vec<World> = (0..10)
            .map(|_| {
                let s = w.clone();
                w.step();
                s
            })
            .collect();
        let agent_steps: u64 = states.iter().map(|s| s.agents.len() as u64).sum();
        g.throughput(Throughput::Elements(agent_steps));
        g.bench_function(name, |b| {
            b.iter(|| {
                for s in &states {
                    criterion::black_box(s.decide_all());
                }
            })
        });
    }
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
