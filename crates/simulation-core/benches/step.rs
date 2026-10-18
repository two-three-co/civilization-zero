//! Throughput of `World::step` at several population sizes.
//! Run: `cargo bench -p simulation-core` (add `--features parallel` for rayon).

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use simulation_core::{
    AgentId, Intervention, Norm, NormIndex, NormKind, PolicyKind, Sanction, Standing, World,
    WorldConfig,
};

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

/// The first-occupancy norm of EXP-004 (ADR-0014 D11): 32 ticks of
/// standing, harm as the sanction.
fn first_occupant_rule() -> Intervention {
    Intervention::Enact {
        norm: Norm {
            kind: NormKind::Encroach,
            standing_ticks: 32,
            sanction: Sanction::HARM,
            except_sanction: false,
        },
    }
}

/// A world with `nomos` on and the first-occupancy rule in force from
/// tick 0, warmed like the others (ADR-0014 4-2: the judging, the
/// episodes and the per-tick `NormIndex` are the cost over `step/nomos`).
/// Foragers never harm, so no sanction happens; every encroachment
/// (~19 per tick, 4-1) is a violation and most of the population stands
/// accused at any time. With mutation and the learner's row count chosen
/// it is the shape of the EXP-004 world (ADR-0014 D11; the 4-3 decide
/// budget: ≤ 200 ns per agent-step with 21 features or 128 rows, rules in
/// force, agents standing accused and — among learners — sanctioning).
fn ruled_world_of(
    agents: u32,
    side: u16,
    kind: PolicyKind,
    mutation: bool,
    learner_states: u16,
) -> World {
    ruled_world_warmed(agents, side, kind, mutation, learner_states, 1_000)
}

/// `ruled_world_of` warmed for `warm` ticks: the 128-row learner is still
/// in its founding dip at tick 1,000 (EXP-002: L128 recovers between
/// ticks 1,600 and 2,900), so its gate world is warmed 3,000 ticks to
/// reach the ~1,300 agents the budget is stated for.
fn ruled_world_warmed(
    agents: u32,
    side: u16,
    kind: PolicyKind,
    mutation: bool,
    learner_states: u16,
    warm: u64,
) -> World {
    let cfg = WorldConfig {
        seed: 42,
        width: side,
        height: side,
        initial_agents: agents,
        patch_count: (side as u32 * side as u32) / 700,
        mutation_enabled: mutation,
        learner_states,
        nomos: true,
        ..WorldConfig::default()
    };
    let mut w = World::generate(cfg, |_| kind).unwrap();
    w.intervene(first_occupant_rule()).unwrap();
    w.run(warm);
    w
}

/// The same world with `see_standing` off (ADR-0014 D6; EXP-004's N-blind):
/// the norm index is built and the sanctions are read, but the policies'
/// norm queries answer "no" without walking. Deciding over the same
/// states as the seeing variant isolates what the norm queries cost from
/// what a different population costs.
fn blinded(mut w: World) -> World {
    w.config.perception.see_standing = false;
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
    // The same two worlds with the first-occupancy rule in force (ADR-0014
    // 4-2, acceptance: the Nomos cost — index, judging, episodes — within
    // the step budget of ≤ 20%, D9 completion condition 2).
    for (name, kind, rows) in [
        ("nomos_rules", PolicyKind::Forager, 32),
        ("learner_nomos_rules", PolicyKind::Learner, 32),
        // The EXP-004 shape (ADR-0014 4-3): 128 rows see the norm bits.
        ("learner128_nomos_rules", PolicyKind::Learner, 128),
    ] {
        let warm = if rows == 128 { 3_000 } else { 1_000 };
        let w = ruled_world_warmed(1_000, 128, kind, false, rows, warm);
        g.throughput(Throughput::Elements(w.agents.len() as u64 * 10));
        g.bench_with_input(BenchmarkId::new(name, 1_000), &w, |b, base| {
            b.iter_batched_ref(
                || base.clone(),
                |w| w.run(10),
                criterion::BatchSize::LargeInput,
            )
        });
    }
    // The institution world of ADR-0014's completion condition 2 (4-5,
    // 4-9): `WorldConfig::institution_preset()` — `nomos_preset`, 128-row
    // learners, the enforcers held at a tenth of the living — with every
    // tenth founder an enforcer, the first-occupancy rule decreed at tick
    // 0, warmed 3,000 ticks (the valley the sanctions dig is at tick 300,
    // the population is back near 1,100 by 2,500). Enforcers sanction the
    // accused; the assembly has nothing to move with the rule in force.
    // The gate is native ≥ 1,800 tick/s. `institution_norules` is the
    // same world with no rule and no motions (EXP-004's LE0): a reference
    // for what the rule costs per agent-step, over its own population.
    for (name, w) in [
        ("institution", institution_world(1_000, 128, 3_000)),
        (
            "institution_norules",
            institution_world_without_rules(1_000, 128, 3_000),
        ),
    ] {
        eprintln!(
            "step/{name}: {} agents ({} enforcers), {} open episodes, {} sanctions, {} motions so far",
            w.agents.len(),
            w.agents
                .iter()
                .filter(|a| a.policy == PolicyKind::Enforcer)
                .count(),
            w.nomos.standing.len(),
            w.ledger.sanctions.iter().sum::<u64>(),
            w.ledger.motions
        );
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

/// `cz run --policy institution --enact encroach` after `warm` ticks
/// (ADR-0014 D8 / completion condition 2; `WorldConfig::institution_preset()`
/// since 4-9, so the enforcers stay a tenth of the living).
fn institution_world(agents: u32, side: u16, warm: u64) -> World {
    let mut w = institution_founded(agents, side, true);
    w.intervene(first_occupant_rule()).unwrap();
    w.run(warm);
    w
}

/// The same world with no rule ever in force: `enforcer_motions` off (so
/// the enforcers do not legislate it themselves) and no decree — EXP-004's
/// LE0 (`cz run --policy institution --no-enforcer-motions`).
fn institution_world_without_rules(agents: u32, side: u16, warm: u64) -> World {
    let mut w = institution_founded(agents, side, false);
    w.run(warm);
    w
}

fn institution_founded(agents: u32, side: u16, enforcer_motions: bool) -> World {
    let cfg = WorldConfig {
        seed: 42,
        width: side,
        height: side,
        initial_agents: agents,
        patch_count: (side as u32 * side as u32) / 700,
        enforcer_motions,
        ..WorldConfig::institution_preset()
    };
    World::generate(cfg, |i| {
        if i % 10 == 0 {
            PolicyKind::Enforcer
        } else {
            PolicyKind::Learner
        }
    })
    .unwrap()
}

/// Building the per-tick `NormIndex` (ADR-0014 D2; 4-2 acceptance: ≤ 30 µs
/// per tick at 1.3k agents with 2,000 open episodes). The episodes are
/// hand-built — every agent under rule 0 and the first 2,000 − n under
/// rule 1 — on the warmed forager world (~1,316 agents), so the count is
/// exact whatever the population did. `norm_index_empty` is the same
/// world with nothing in force: the cost every rule-free tick pays.
fn bench_norm_index(c: &mut Criterion) {
    let mut w = warmed_world_with(1_000, 128, PolicyKind::Forager, false, true);
    let empty = w.clone();
    w.intervene(first_occupant_rule()).unwrap();
    w.intervene(Intervention::Enact {
        norm: Norm {
            kind: NormKind::Harm,
            standing_ticks: 32,
            sanction: Sanction::ALL,
            except_sanction: true,
        },
    })
    .unwrap();
    const EPISODES: usize = 2_000;
    let n = w.agents.len();
    let under_rule_1 = EPISODES.saturating_sub(n).min(n);
    let tick = w.tick;
    let mut standing = Vec::with_capacity(EPISODES);
    for (i, a) in w.agents.iter().enumerate() {
        for rule in 0..=u16::from(i < under_rule_1) {
            standing.push(Standing {
                agent: a.id,
                rule,
                since: tick - 1,
                last: tick - 1,
                against: AgentId(0),
                violations: 1,
                sanctions: 0,
            });
        }
    }
    w.nomos.standing = standing;
    w.check_invariants()
        .expect("hand-built episodes are consistent");
    let episodes = w.nomos.standing.len();
    let mut g = c.benchmark_group("norm_index");
    g.bench_function(format!("{n}_agents_{episodes}_episodes"), |b| {
        b.iter(|| NormIndex::build(&w.agents, &w.nomos, w.bounds()))
    });
    g.bench_function(format!("{n}_agents_empty"), |b| {
        b.iter(|| NormIndex::build(&empty.agents, &empty.nomos, empty.bounds()))
    });
    // The index the institution world of completion condition 2 actually
    // builds every tick (4-9): its own episodes, its enforcers' HARMABLE
    // cells counted (4-9a).
    let inst = institution_world(1_000, 128, 3_000);
    g.bench_function(
        format!(
            "institution_{}_agents_{}_episodes",
            inst.agents.len(),
            inst.nomos.standing.len()
        ),
        |b| b.iter(|| NormIndex::build(&inst.agents, &inst.nomos, inst.bounds())),
    );
    g.finish();
}

/// The decide phase alone, over the ten consecutive states of one regrowth
/// period, so it can be compared with `step/serial/1000` (step − decide ≈ apply).
/// `reflex/1000` is the same for a reflex world with the initial (forager)
/// weights and `reflex_evolved/1000` for one whose weights have mutated for
/// 1,000 ticks (ADR-0013 3-6 budget: ≤ 150 ns per agent-step).
/// `learner/1000` is a learner world after 1,000 ticks of learning (ADR-0013
/// 3-7 budget: ≤ 200 ns per agent-step; the Q update itself is in apply).
/// `reflex_evolved_rules/1000` and `learner128_rules/1000` are the ADR-0014
/// 4-3 budget's worlds: `nomos` on, the first-occupancy rule in force since
/// tick 0, agents standing accused (and, among learners, sanctioning), 21
/// features / 128 rows reading the norm index — ≤ 250 ns per agent-step
/// (the 2026-10-18 revision, D6). `institution/3000` is the world of
/// completion condition 2 (4-5, held at the founding composition since
/// 4-9): the same with every tenth founder an enforcer, whose script reads
/// the index directly; `institution_blind/3000` decides over the same
/// states with the learners' norm queries answering "no" (the enforcers
/// read the index whatever `see_standing` says), the control the 4-9 gate
/// is read against.
fn bench_decide(c: &mut Criterion) {
    let mut g = c.benchmark_group("decide");
    type Build = fn() -> World;
    let worlds: [(&str, Build); 10] = [
        ("serial/1000", || {
            warmed_world_of(1_000, 128, PolicyKind::Forager, false)
        }),
        ("reflex/1000", || {
            warmed_world_of(1_000, 128, PolicyKind::Reflex, false)
        }),
        ("reflex_evolved/1000", || {
            warmed_world_of(1_000, 128, PolicyKind::Reflex, true)
        }),
        ("learner/1000", || {
            warmed_world_of(1_000, 128, PolicyKind::Learner, false)
        }),
        ("reflex_evolved_rules/1000", || {
            ruled_world_of(1_000, 128, PolicyKind::Reflex, true, 32)
        }),
        ("reflex_evolved_rules_blind/1000", || {
            blinded(ruled_world_of(1_000, 128, PolicyKind::Reflex, true, 32))
        }),
        ("learner128_rules/3000", || {
            ruled_world_warmed(1_000, 128, PolicyKind::Learner, false, 128, 3_000)
        }),
        ("learner128_rules_blind/3000", || {
            blinded(ruled_world_warmed(
                1_000,
                128,
                PolicyKind::Learner,
                false,
                128,
                3_000,
            ))
        }),
        ("institution/3000", || institution_world(1_000, 128, 3_000)),
        ("institution_blind/3000", || {
            blinded(institution_world(1_000, 128, 3_000))
        }),
    ];
    for (name, build) in worlds {
        let mut w = build();
        if w.config.nomos {
            eprintln!(
                "decide/{name}: {} agents, {} open episodes, {} sanctions so far, see_standing {}",
                w.agents.len(),
                w.nomos.standing.len(),
                w.ledger.sanctions.iter().sum::<u64>(),
                w.config.perception.see_standing
            );
        }
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
    // The decode alone — what a seek pays before re-executing up to
    // `snapshot_every − 1` ticks (ADR-0010 2b-4, RISKS R11) — and the
    // binary snapshot's size, for the three worlds the gates name (ADR-0013
    // / ADR-0014 completion condition 2: ≤ 1 MB for the default learner
    // world, ≤ 2 MB for the institution world).
    for (name, w) in [
        ("forager_1000", w),
        (
            "learner_1000",
            warmed_world_of(1_000, 128, PolicyKind::Learner, false),
        ),
        ("institution_3000", institution_world(1_000, 128, 3_000)),
    ] {
        let bytes = w.to_bytes();
        eprintln!(
            "snapshot_bin_decode/{name}: {} agents, {} bytes",
            w.agents.len(),
            bytes.len()
        );
        c.bench_function(&format!("snapshot_bin_decode/{name}"), |b| {
            b.iter(|| World::from_bytes(&bytes).unwrap())
        });
    }
}

criterion_group!(
    benches,
    bench_step,
    bench_decide,
    bench_hash,
    bench_norm_index
);
criterion_main!(benches);
