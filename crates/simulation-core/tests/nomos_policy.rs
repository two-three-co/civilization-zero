//! Observation of the normative facts and the learning policies that read
//! them (ADR-0014 D6, task 4-3): the `Observation` queries, the five
//! reflex norm features and the three learner norm bits, the blind
//! perception (`see_standing: false`), the Harm actuator that aims at an
//! accused neighbour first, and the contract that with `nomos` off a 64-,
//! 128- or 256-row learner decides exactly as the 32-row one.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::policy::learner::{heard_last_tick, learner_state, state, state_row};
use simulation_core::policy::{
    self, feature, forager_weights, nearest_adjacent, reflex_features, scan_best, REFLEX_FEATURES,
    REFLEX_UNIT,
};
use simulation_core::rng::{RngStream, Stream};
use simulation_core::{
    Action, AgentId, Intent, Interaction, InteractionKind, Intervention, Norm, NormIndex, NormKind,
    PolicyKind, PolicyTable, Pos, Rule, Sanction, World, WorldConfig,
};

const K: usize = Action::KIND_COUNT;
const TRANSFER: usize = 4;
const HARM: usize = 6;

/// The hand-built world of `common::cfg()` with institutions on.
fn ncfg() -> WorldConfig {
    WorldConfig {
        nomos: true,
        ..cfg()
    }
}

fn norm(kind: NormKind, standing_ticks: u16, sanction: Sanction) -> Norm {
    Norm {
        kind,
        standing_ticks,
        sanction,
        except_sanction: false,
    }
}

fn enact(w: &mut World, n: Norm) -> Rule {
    w.intervene(Intervention::Enact { norm: n }).unwrap();
    *w.nomos.rules.last().unwrap()
}

fn reflex_agent(w: &mut World, pos: Pos, energy: i64, inventory: u32) -> usize {
    let params = w.config.initial_params.clone();
    add_agent_with(w, pos, energy, inventory, PolicyKind::Reflex, params)
}

fn learner_agent(w: &mut World, pos: Pos, energy: i64, inventory: u32) -> usize {
    let params = w.config.initial_params.clone();
    add_agent_with(w, pos, energy, inventory, PolicyKind::Learner, params)
}

/// Forager weights with one kind's bias raised far above every ladder step.
fn preferring(kind: usize) -> Vec<i32> {
    let mut weights = forager_weights();
    weights[feature::BIAS * K + kind] = 8 * REFLEX_UNIT;
    weights
}

fn set_weights(w: &mut World, i: usize, weights: Vec<i32>) {
    w.agents[i].params.table = Some(PolicyTable::Reflex { weights });
}

fn set_q(w: &mut World, i: usize, row: u16, kind: usize, value: i16) {
    match w.agents[i].params.table.as_mut() {
        Some(PolicyTable::Learner { q }) => q[row as usize * K + kind] = value,
        other => panic!("not a learner table: {other:?}"),
    }
}

fn q(w: &World, i: usize) -> &[i16] {
    match &w.agents[i].params.table {
        Some(PolicyTable::Learner { q }) => q,
        other => panic!("not a learner table: {other:?}"),
    }
}

fn decide(w: &World, i: usize) -> Intent {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation_at(i, &index, &norms);
    let mut rng = RngStream::new(w.config.seed, w.tick, w.agents[i].id.0, Stream::Decide);
    policy::decide(&obs, &mut rng)
}

/// The reflex feature vector of `w.agents[i]` right now (any kind: the
/// features read the observation, not the policy).
fn features_of(w: &World, i: usize) -> [i64; REFLEX_FEATURES] {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation_at(i, &index, &norms);
    let a = &w.agents[i];
    let scan = (a.inventory < w.config.inventory_capacity).then(|| scan_best(&obs));
    reflex_features(&obs, scan.as_ref())
}

/// The fine (10-bit) learner state of `w.agents[i]` right now.
fn fine_of(w: &World, i: usize) -> u16 {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation_at(i, &index, &norms);
    let a = &w.agents[i];
    let scan = (a.inventory < w.config.inventory_capacity).then(|| scan_best(&obs));
    learner_state(&obs, scan.as_ref(), heard_last_tick(&obs))
}

/// `f(obs)` for `w.agents[i]`.
fn with_obs<T>(w: &World, i: usize, f: impl FnOnce(&policy::Observation<'_>) -> T) -> T {
    let index = w.spatial_index();
    let norms = w.norm_index();
    f(&w.observation_at(i, &index, &norms))
}

fn note(tick: u64, other: u64, kind: InteractionKind, value: i64) -> Interaction {
    Interaction {
        tick,
        other: AgentId(other),
        kind,
        value,
    }
}

fn body(a: &simulation_core::Agent) -> (AgentId, Pos, i64, i64, u32, u64) {
    (a.id, a.pos, a.energy, a.health, a.inventory, a.age)
}

fn bodies(w: &World) -> Vec<(AgentId, Pos, i64, i64, u32, u64)> {
    w.agents.iter().map(body).collect()
}

fn small(seed: u64) -> WorldConfig {
    WorldConfig {
        seed,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..WorldConfig::default()
    }
}

/// The scene every hand-built test below starts from, at tick 20 with the
/// first-occupancy rule in force: the observer `me` (index 0) stands on
/// (4, 4) since tick 10 beside the founder `o` (index 1, the cell's first
/// occupant, not accused); `v` (index 2) stands on the adjacent (5, 4)
/// accused under the rule since tick 19.
fn scene(kind: PolicyKind) -> (World, Rule, [usize; 3]) {
    let mut w = empty_world(ncfg());
    let add = |w: &mut World, pos, energy, inv| match kind {
        PolicyKind::Learner => learner_agent(w, pos, energy, inv),
        _ => reflex_agent(w, pos, energy, inv),
    };
    let me = add(&mut w, Pos::new(4, 4), 50_000, 3);
    let o = add(&mut w, Pos::new(4, 4), 50_000, 0);
    let v = add(&mut w, Pos::new(5, 4), 50_000, 0);
    w.tick = 20;
    w.agents[me].arrived = Some(10);
    let rule = enact(&mut w, norm(NormKind::Encroach, 32, Sanction::HARM));
    w.nomos.accuse(w.agents[v].id, rule.id, 19, w.agents[o].id);
    w.check_invariants().unwrap();
    (w, rule, [me, o, v])
}

#[test]
fn the_norm_features_read_the_index_and_blindness_zeroes_them() {
    use feature::*;
    let (mut w, rule, [me, o, v]) = scene(PolicyKind::Reflex);
    w.agents[me].recent = vec![
        note(10, 2, InteractionKind::SanctionedBy, 20), // a sanction by harm, 10 ticks ago
        note(12, 2, InteractionKind::SawSanction, 1),
    ];
    let x = features_of(&w, me);
    assert_eq!(x[OCCUPIED], 1, "the founder beside me arrived first");
    assert_eq!(x[RULE_ACTIVE], 1);
    assert_eq!(x[ACCUSED], 0, "I stand accused of nothing yet");
    assert_eq!(
        x[VIOLATOR_ADJACENT], 1,
        "V, accused and harmable, is adjacent"
    );
    assert_eq!(x[SANCTIONED_RECENTLY], 1);
    assert_eq!(x[HARMED_RECENTLY], 1, "a sanction by harm is a harm taken");
    assert_eq!(
        x[SAW_HARM_RECENTLY], 1,
        "a witnessed sanction is a witnessed harm"
    );
    assert_eq!((x[NEIGHBOURS], x[ADJACENT]), (2, 2));
    with_obs(&w, me, |obs| {
        assert!(obs.occupied());
        assert_eq!(obs.my_standing(), 0);
        assert!(!obs.accused() && !obs.i_voted());
        assert_eq!(obs.adjacent_violator(), Some(w.agents[v].id));
        assert_eq!(
            nearest_adjacent(obs),
            Some(w.agents[o].id),
            "the actuator's old target"
        );
        assert_eq!(obs.rule(NormKind::Encroach), Some(&rule));
        assert_eq!(obs.rule(NormKind::Harm), None);
        assert_eq!(obs.open_motion(), None);
    });
    // The founder: nobody arrived before it, V is adjacent to it too.
    let xo = features_of(&w, o);
    assert_eq!(
        (xo[OCCUPIED], xo[VIOLATOR_ADJACENT], xo[ACCUSED]),
        (0, 1, 0)
    );
    // V itself: accused and harmable; nobody accused stands next to it.
    let xv = features_of(&w, v);
    assert_eq!(
        (xv[OCCUPIED], xv[VIOLATOR_ADJACENT], xv[ACCUSED]),
        (0, 0, 1)
    );
    with_obs(&w, v, |obs| {
        assert_eq!(obs.my_standing(), NormIndex::ACCUSED | NormIndex::HARMABLE);
        assert!(obs.accused());
        assert_eq!(obs.adjacent_violator(), None);
    });

    // Accused myself: the flag follows the index.
    w.nomos.accuse(w.agents[me].id, rule.id, 18, w.agents[o].id);
    assert_eq!(features_of(&w, me)[ACCUSED], 1);
    with_obs(&w, me, |obs| {
        assert_eq!(obs.my_standing(), NormIndex::ACCUSED | NormIndex::HARMABLE);
        assert_eq!(
            obs.adjacent_violator(),
            Some(w.agents[v].id),
            "never oneself, even when accused"
        );
    });
    assert_eq!(
        features_of(&w, o)[VIOLATOR_ADJACENT],
        1,
        "the founder now has two"
    );

    // Blind (`see_standing: false`): the four institutional features are
    // 0, the cell's occupancy and the physical memory are not.
    w.config.perception.see_standing = false;
    let x = features_of(&w, me);
    assert_eq!(x[OCCUPIED], 1);
    assert_eq!(
        (
            x[RULE_ACTIVE],
            x[ACCUSED],
            x[VIOLATOR_ADJACENT],
            x[SANCTIONED_RECENTLY]
        ),
        (0, 0, 0, 0)
    );
    assert_eq!((x[HARMED_RECENTLY], x[SAW_HARM_RECENTLY]), (1, 1));
    with_obs(&w, me, |obs| {
        assert!(obs.occupied());
        assert_eq!(obs.my_standing(), 0);
        assert!(!obs.accused());
        assert_eq!(obs.adjacent_violator(), None);
        assert_eq!(obs.rule(NormKind::Encroach), None);
        assert_eq!(obs.open_motion(), None);
    });
    w.config.perception.see_standing = true;

    // A sanction by rejection (value 0) is remembered as a sanction but
    // was no harm.
    w.agents[me].recent = vec![note(15, 2, InteractionKind::SanctionedBy, 0)];
    let x = features_of(&w, me);
    assert_eq!((x[SANCTIONED_RECENTLY], x[HARMED_RECENTLY]), (1, 0));

    // The memory flag outlives the rule (it is the agent's past) but not
    // the horizon.
    w.intervene(Intervention::Repeal { rule: rule.id }).unwrap();
    assert!(w.norm_index().is_empty());
    let x = features_of(&w, me);
    assert_eq!(
        (x[RULE_ACTIVE], x[ACCUSED], x[VIOLATOR_ADJACENT]),
        (0, 0, 0)
    );
    assert_eq!((x[OCCUPIED], x[SANCTIONED_RECENTLY]), (1, 1));
    with_obs(&w, me, |obs| {
        assert_eq!(obs.adjacent_violator(), None);
        assert_eq!(obs.rule(NormKind::Encroach), None);
    });
    w.tick = 15 + MEMORY_HORIZON + 1;
    assert_eq!(features_of(&w, me)[SANCTIONED_RECENTLY], 0);
    w.check_invariants().unwrap();

    // Without `nomos` nothing of this exists: the five features are 0 and
    // the queries say no, whatever the perception.
    let mut off = empty_world(cfg());
    let a = reflex_agent(&mut off, Pos::new(4, 4), 50_000, 3);
    reflex_agent(&mut off, Pos::new(4, 4), 50_000, 0);
    let x = features_of(&off, a);
    assert!(x[OCCUPIED..].iter().all(|&f| f == 0), "{x:?}");
    with_obs(&off, a, |obs| {
        assert!(!obs.occupied() && !obs.accused());
        assert_eq!(obs.adjacent_violator(), None);
    });
}

#[test]
fn occupied_means_an_earlier_arrival_on_the_same_cell() {
    let mut w = empty_world(ncfg());
    w.tick = 20;
    let me = reflex_agent(&mut w, Pos::new(4, 4), 50_000, 0);
    w.agents[me].arrived = Some(10);
    let occupied = |w: &World| with_obs(w, me, |obs| obs.occupied());
    assert!(!occupied(&w), "alone");
    let o = reflex_agent(&mut w, Pos::new(4, 4), 50_000, 0);
    assert!(
        occupied(&w),
        "a founder on my cell is older than any arrival"
    );
    w.agents[o].arrived = Some(10);
    assert!(!occupied(&w), "the same tick is not earlier");
    w.agents[o].arrived = Some(11);
    assert!(!occupied(&w), "a later arrival");
    w.agents[o].arrived = Some(9);
    assert!(occupied(&w), "an earlier one");
    w.agents[o].pos = Pos::new(5, 4);
    assert!(
        !occupied(&w),
        "only my own cell counts, not an adjacent one"
    );
    w.agents[o].pos = Pos::new(4, 4);
    w.agents[me].arrived = None;
    assert!(!occupied(&w), "nothing is earlier than a founder");
    w.agents[o].arrived = None;
    assert!(!occupied(&w), "two founders");
    // The learner bit and the reflex feature are this very fact.
    w.agents[me].arrived = Some(10);
    assert_ne!(fine_of(&w, me) & state::OCCUPIED, 0);
    assert_eq!(features_of(&w, me)[feature::OCCUPIED], 1);
    assert_eq!(fine_of(&w, o) & state::OCCUPIED, 0);
    w.config.perception.see_standing = false;
    assert_ne!(fine_of(&w, me) & state::OCCUPIED, 0, "not an institution");
    w.check_invariants().unwrap();
}

#[test]
fn the_learner_bits_follow_the_norms_and_fold_by_row_count() {
    let (mut w, rule, [me, o, v]) = scene(PolicyKind::Learner);
    let base = state::HAS_FOOD | state::ADJACENT; // fed, not upper, food in the pack, O beside me
    assert_eq!(
        fine_of(&w, me),
        base | state::OCCUPIED | state::VIOLATOR_ADJACENT
    );
    assert_eq!(fine_of(&w, o), state::ADJACENT | state::VIOLATOR_ADJACENT);
    assert_eq!(fine_of(&w, v), state::ADJACENT | state::ACCUSED);
    w.nomos.accuse(w.agents[me].id, rule.id, 18, w.agents[o].id);
    let fine = fine_of(&w, me);
    assert_eq!(
        fine,
        base | state::OCCUPIED | state::VIOLATOR_ADJACENT | state::ACCUSED
    );
    // Each power of two reveals one more bit (ADR-0014 D6).
    assert_eq!(state_row(fine, 32), base, "32 rows: the Phase 3 grid");
    assert_eq!(state_row(fine, 64), base | state::OCCUPIED);
    assert_eq!(
        state_row(fine, 128),
        base | state::OCCUPIED | state::VIOLATOR_ADJACENT
    );
    assert_eq!(state_row(fine, 256), fine);
    // Blind: only the physical bit of the three survives.
    w.config.perception.see_standing = false;
    assert_eq!(fine_of(&w, me), base | state::OCCUPIED);
    assert_eq!(fine_of(&w, v), state::ADJACENT);
    w.config.perception.see_standing = true;
    // No rule in force: the same.
    w.intervene(Intervention::Repeal { rule: rule.id }).unwrap();
    assert_eq!(fine_of(&w, me), base | state::OCCUPIED);
    assert_eq!(fine_of(&w, v), state::ADJACENT);
    w.check_invariants().unwrap();
}

#[test]
fn harm_aims_at_an_accused_neighbour_first_and_otherwise_as_before() {
    let (mut w, rule, [me, o, v]) = scene(PolicyKind::Reflex);
    let (o_id, v_id) = (w.agents[o].id, w.agents[v].id);
    set_weights(&mut w, me, preferring(HARM));
    assert_eq!(
        decide(&w, me).action,
        Action::Harm { target: v_id },
        "the accused on the adjacent cell before the founder on mine"
    );
    set_weights(&mut w, me, preferring(TRANSFER));
    assert_eq!(
        decide(&w, me).action,
        Action::Transfer { to: o_id, n: 1 },
        "Transfer keeps aiming at the nearest"
    );
    set_weights(&mut w, me, preferring(HARM));

    // Blind: the old target.
    w.config.perception.see_standing = false;
    assert_eq!(decide(&w, me).action, Action::Harm { target: o_id });
    w.config.perception.see_standing = true;

    // Among the accused, the walk is `nearest_adjacent`'s: my own cell
    // first, then the lowest id of the adjacent cells.
    let x = reflex_agent(&mut w, Pos::new(4, 4), 50_000, 0); // id 3, on my cell
    let x_id = w.agents[x].id;
    assert_eq!(
        decide(&w, me).action,
        Action::Harm { target: v_id },
        "X is not accused"
    );
    w.nomos.accuse(x_id, rule.id, 19, o_id);
    assert_eq!(
        decide(&w, me).action,
        Action::Harm { target: x_id },
        "an accused on my own cell comes first"
    );
    let y = reflex_agent(&mut w, Pos::new(4, 3), 50_000, 0); // id 4, adjacent (north)
    let y_id = w.agents[y].id;
    w.nomos.accuse(y_id, rule.id, 19, o_id);
    w.agents[x].pos = Pos::new(6, 6); // X walks out of reach
    assert_eq!(
        decide(&w, me).action,
        Action::Harm { target: v_id },
        "two accused on adjacent cells: the lower id"
    );
    w.config.perception.social_radius = 0;
    assert_eq!(
        decide(&w, me).action,
        Action::Harm { target: o_id },
        "radius 0 reaches no adjacent cell: the founder beside me, as before"
    );
    w.config.perception.social_radius = 2;

    // A rule whose sanction set has no HARM leaves the accused unharmable
    // for the actuator: ordinary aim.
    w.intervene(Intervention::Repeal { rule: rule.id }).unwrap();
    let reject_only = enact(&mut w, norm(NormKind::Encroach, 32, Sanction::REJECT));
    w.nomos.accuse(v_id, reject_only.id, 19, o_id);
    assert_eq!(features_of(&w, me)[feature::VIOLATOR_ADJACENT], 0);
    assert_eq!(decide(&w, me).action, Action::Harm { target: o_id });
    assert!(with_obs(&w, v, |obs| obs.accused()));
    w.intervene(Intervention::Repeal {
        rule: reject_only.id,
    })
    .unwrap();
    assert_eq!(
        decide(&w, me).action,
        Action::Harm { target: o_id },
        "no rule"
    );
    w.check_invariants().unwrap();

    // The learner shares the actuator.
    let (mut l, _, [me, _, v]) = scene(PolicyKind::Learner);
    let row = state_row(fine_of(&l, me), 32);
    set_q(&mut l, me, row, HARM, 100);
    l.config.learner_epsilon_permille = 0;
    assert_eq!(
        decide(&l, me).action,
        Action::Harm {
            target: l.agents[v].id
        }
    );
    l.config.perception.see_standing = false;
    assert_eq!(
        decide(&l, me).action,
        Action::Harm {
            target: l.agents[1].id
        }
    );
}

#[test]
fn reflex_with_the_initial_weights_decides_as_the_forager_under_rules() {
    // The norm rows of `forager_weights()` are zero, so rules change what a
    // fresh reflex agent sees but not what it does (ADR-0014 D6).
    let cfg = WorldConfig {
        nomos: true,
        ..small(11)
    };
    let mut forager = World::generate(cfg.clone(), |_| PolicyKind::Forager).unwrap();
    let mut reflex = World::generate(cfg, |_| PolicyKind::Reflex).unwrap();
    for w in [&mut forager, &mut reflex] {
        enact(w, norm(NormKind::Encroach, 32, Sanction::HARM));
    }
    for t in 1..=300u64 {
        let a = forager.step();
        let b = reflex.step();
        assert_eq!(a.actions, b.actions, "tick {t}");
        assert_eq!(a.violations, b.violations);
    }
    assert_eq!(bodies(&forager), bodies(&reflex));
    assert_eq!(forager.resources, reflex.resources);
    assert!(reflex.ledger.violations[0] > 0 && !reflex.nomos.standing.is_empty());
    reflex.check_invariants().unwrap();
}

#[test]
fn without_nomos_wider_learner_tables_decide_exactly_as_32_rows() {
    // With `nomos` off bits 5–7 are always 0, so rows 32 and up are never
    // visited: the body of every agent, the food and the ledger of a 64-,
    // 128- or 256-row world are the 32-row world's, tick for tick, and the
    // first 32 rows of every table learn the same values (ADR-0014 D6).
    let cfg = small(42);
    let mut narrow = World::generate(cfg.clone(), |_| PolicyKind::Learner).unwrap();
    let mut wide: Vec<World> = [64u16, 128, 256]
        .into_iter()
        .map(|rows| {
            World::generate(
                WorldConfig {
                    learner_states: rows,
                    ..cfg.clone()
                },
                |_| PolicyKind::Learner,
            )
            .unwrap()
        })
        .collect();
    for t in 1..=1_000u64 {
        let a = narrow.step();
        for w in &mut wide {
            let b = w.step();
            assert_eq!(
                a.actions, b.actions,
                "{} rows, tick {t}",
                w.config.learner_states
            );
            assert_eq!(a.events, b.events);
            assert!(w
                .agents
                .iter()
                .all(|x| x.last_step.is_none_or(|s| s.state < 32)));
        }
        if t % 100 == 0 {
            for w in &wide {
                assert_eq!(bodies(&narrow), bodies(w), "tick {t}");
                assert_eq!(narrow.resources, w.resources);
                assert_eq!(narrow.ledger, w.ledger);
            }
        }
    }
    assert!(narrow.ledger.births > 0 && narrow.ledger.deaths > 0);
    for w in &wide {
        for (a, b) in narrow.agents.iter().zip(&w.agents) {
            assert_eq!(a.last_step, b.last_step);
        }
        for i in 0..narrow.agents.len() {
            let (qa, qb) = (q(&narrow, i), q(w, i));
            assert_eq!(qb.len(), w.config.learner_states as usize * K);
            assert_eq!(&qb[..32 * K], qa, "the first 32 rows learned the same");
            assert!(
                qb[32 * K..].iter().all(|&v| v == 0),
                "the rest was never visited"
            );
        }
        assert_ne!(
            narrow.dynamics_hash(),
            w.dynamics_hash(),
            "the table shape is state"
        );
        w.check_invariants().unwrap();
    }

    // With `nomos` on the picture changes: 64 rows see `OCCUPIED`, and
    // 128 rows under the first-occupancy rule see `VIOLATOR_ADJACENT`.
    let on = WorldConfig {
        nomos: true,
        ..small(42)
    };
    let mut rows_seen = [0u16; 2];
    for (k, (rows, ruled)) in [(64u16, false), (128, true)].into_iter().enumerate() {
        let mut w = World::generate(
            WorldConfig {
                learner_states: rows,
                ..on.clone()
            },
            |_| PolicyKind::Learner,
        )
        .unwrap();
        if ruled {
            enact(&mut w, norm(NormKind::Encroach, 32, Sanction::HARM));
        }
        for _ in 0..300 {
            w.step();
            for a in &w.agents {
                if let Some(s) = a.last_step {
                    rows_seen[k] = rows_seen[k].max(s.state);
                }
            }
        }
        w.check_invariants().unwrap();
    }
    assert!(
        (32..64).contains(&rows_seen[0]),
        "64 rows, no rule: OCCUPIED only ({})",
        rows_seen[0]
    );
    assert!(
        rows_seen[1] >= 64,
        "128 rows, accused neighbours: VIOLATOR_ADJACENT ({})",
        rows_seen[1]
    );
}

#[test]
fn a_blind_learner_under_rules_decides_as_without_rules() {
    // `see_standing: false` is the null model of ADR-0014 D11: the index
    // is built and the sanctions are read, but the 128-row learner's bits
    // 6–7 stay 0 and its Harm aims as before, so its decisions are those
    // of the same world with no rule in force. Seeing, they differ.
    let cfg = WorldConfig {
        nomos: true,
        learner_states: 128,
        ..small(5)
    };
    let mut plain = World::generate(cfg.clone(), |_| PolicyKind::Learner).unwrap();
    let mut blind = World::generate(
        WorldConfig {
            perception: simulation_core::Perception {
                see_standing: false,
                ..cfg.perception
            },
            ..cfg.clone()
        },
        |_| PolicyKind::Learner,
    )
    .unwrap();
    let mut seeing = World::generate(cfg, |_| PolicyKind::Learner).unwrap();
    for w in [&mut blind, &mut seeing] {
        enact(w, norm(NormKind::Encroach, 32, Sanction::HARM));
    }
    let mut diverged = false;
    for t in 1..=300u64 {
        let a = plain.step();
        let b = blind.step();
        let c = seeing.step();
        assert_eq!(a.actions, b.actions, "tick {t}");
        assert_eq!(a.harms, b.harms);
        assert_eq!(a.learner_rows, b.learner_rows);
        assert!(blind
            .agents
            .iter()
            .all(|x| x.last_step.is_none_or(|s| s.state < 64)));
        diverged |= a.actions != c.actions;
    }
    assert_eq!(bodies(&plain), bodies(&blind));
    assert_eq!(plain.resources, blind.resources);
    assert!(
        blind.ledger.sanctions[0] > 0,
        "the blind are sanctioned all the same"
    );
    assert!(diverged, "a seeing learner decides differently");
    assert!(seeing
        .agents
        .iter()
        .any(|x| x.last_step.is_some_and(|s| s.state >= 64)));
    blind.check_invariants().unwrap();
    seeing.check_invariants().unwrap();
}

fn norm_strategy() -> impl Strategy<Value = Norm> {
    (
        prop_oneof![Just(NormKind::Encroach), Just(NormKind::Harm)],
        0u16..=40,
        0u8..=3,
        any::<bool>(),
    )
        .prop_map(|(kind, standing_ticks, sanction, except_sanction)| Norm {
            kind,
            standing_ticks,
            sanction: Sanction(sanction),
            except_sanction,
        })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// Worlds whose policies read the norms are as deterministic as the
    /// rest: serial and parallel decisions agree, a twin replays to the
    /// same hash, the invariants hold and snapshots round-trip, whatever
    /// the rules, the row count, the perception and the mutation.
    #[test]
    fn norm_aware_worlds_are_deterministic(
        seed in any::<u64>(),
        norms in prop::collection::vec(norm_strategy(), 0..=2),
        rows in prop_oneof![Just(32u16), Just(64), Just(128), Just(256)],
        see_standing in any::<bool>(),
        mutation in any::<bool>(),
        ticks in 1u64..=60,
    ) {
        let cfg = WorldConfig {
            seed,
            width: 32,
            height: 32,
            initial_agents: 120,
            patch_count: 4,
            birth_threshold: Some(60_000),
            child_energy: 30_000,
            mutation_enabled: mutation,
            learner_states: rows,
            nomos: true,
            perception: simulation_core::Perception { see_standing, ..Default::default() },
            ..WorldConfig::default()
        };
        let mix = |i: u32| match i % 3 { 0 => PolicyKind::Reflex, 1 => PolicyKind::Learner, _ => PolicyKind::Forager };
        let mut a = World::generate(cfg.clone(), mix).unwrap();
        for n in &norms {
            a.intervene(Intervention::Enact { norm: *n }).unwrap();
        }
        let mut b = a.clone();
        let mut twin = a.clone();
        for _ in 0..ticks {
            let ra = a.step();
            #[cfg(feature = "parallel")]
            let rb = b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            let rb = b.step();
            prop_assert_eq!(ra, rb);
            twin.step();
        }
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.state_hash(), twin.state_hash());
        prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
        prop_assert_eq!(World::from_bytes(&a.to_bytes()).unwrap(), a);
    }
}

// --- the count predicate behind `VIOLATOR_ADJACENT` (ADR-0014 4-9) -------

/// `f(obs)` for an agent observed "as if" through `World::observation`: a
/// copy, or a stranger, that need not be in `w.agents`.
fn with_obs_of<T>(
    w: &World,
    a: &simulation_core::Agent,
    f: impl FnOnce(&policy::Observation<'_>) -> T,
) -> T {
    let index = w.spatial_index();
    let norms = w.norm_index();
    f(&w.observation(a, &index, &norms))
}

/// `violator_adjacent()` of `obs`, checked against the walk it stands for.
fn agreed(obs: &policy::Observation<'_>) -> bool {
    let fast = obs.violator_adjacent();
    assert_eq!(
        fast,
        obs.adjacent_violator().is_some(),
        "the count predicate and the walk disagree for {:?} at {:?}",
        obs.agent.id,
        obs.agent.pos
    );
    fast
}

/// `violator_adjacent()` of `w.agents[i]`, checked against the walk, the
/// reflex feature and the learner bit that read it.
fn violator_adjacent(w: &World, i: usize) -> bool {
    let fast = with_obs(w, i, agreed);
    assert_eq!(features_of(w, i)[feature::VIOLATOR_ADJACENT], fast as i64);
    assert_eq!(fine_of(w, i) & state::VIOLATOR_ADJACENT != 0, fast);
    fast
}

#[test]
fn the_count_predicate_agrees_with_the_walk_in_every_corner() {
    let (mut w, rule, [me, o, v]) = scene(PolicyKind::Reflex);
    let (me_id, o_id) = (w.agents[me].id, w.agents[o].id);
    // The scene: V, accused and harmable, on the cell east of mine.
    assert!(violator_adjacent(&w, me));
    assert!(violator_adjacent(&w, o));
    assert!(!violator_adjacent(&w, v), "nobody accused stands next to V");
    let norms = w.norm_index();
    assert_eq!(norms.harmable_on(Pos::new(5, 4)), 1);
    assert_eq!(norms.harmable_on(Pos::new(4, 4)), 0);

    // Accused myself: counted on my cell, never my own violator.
    w.nomos.accuse(me_id, rule.id, 18, o_id);
    assert_eq!(w.norm_index().harmable_on(Pos::new(4, 4)), 1);
    w.agents[v].pos = Pos::new(7, 7); // V walks out of reach
    assert!(!violator_adjacent(&w, me), "my own flag does not count");
    assert!(
        violator_adjacent(&w, o),
        "but it counts for the founder beside me"
    );
    assert!(!violator_adjacent(&w, v));

    // Two accused on my cell: me and X.
    let x = reflex_agent(&mut w, Pos::new(4, 4), 50_000, 0);
    let x_id = w.agents[x].id;
    assert!(!violator_adjacent(&w, me), "X is not accused yet");
    w.nomos.accuse(x_id, rule.id, 19, o_id);
    assert_eq!(w.norm_index().harmable_on(Pos::new(4, 4)), 2);
    assert!(violator_adjacent(&w, me));
    assert!(violator_adjacent(&w, x));

    // Radius 0 reaches my own cell only.
    w.agents[x].pos = Pos::new(4, 3); // X north of me, still accused
    w.config.perception.social_radius = 0;
    assert!(
        !violator_adjacent(&w, me),
        "X is adjacent but beyond the social radius"
    );
    assert!(
        violator_adjacent(&w, o),
        "I stand accused on O's cell whatever the radius"
    );
    w.config.perception.social_radius = 1;
    assert!(violator_adjacent(&w, me));
    w.config.perception.social_radius = 2;

    // Blind: nothing, and agreed.
    w.config.perception.see_standing = false;
    assert!(!violator_adjacent(&w, me) && !violator_adjacent(&w, o));
    w.config.perception.see_standing = true;

    // A rule whose sanction set has no HARM: accused, not harmable, not
    // counted.
    w.intervene(Intervention::Repeal { rule: rule.id }).unwrap();
    assert!(!violator_adjacent(&w, me) && !violator_adjacent(&w, o));
    let reject_only = enact(&mut w, norm(NormKind::Encroach, 32, Sanction::REJECT));
    w.nomos.accuse(x_id, reject_only.id, 19, o_id);
    w.nomos.accuse(me_id, reject_only.id, 19, o_id);
    assert!(with_obs(&w, x, |obs| obs.accused()));
    assert_eq!(w.norm_index().harmable_on(Pos::new(4, 3)), 0);
    assert!(!violator_adjacent(&w, me) && !violator_adjacent(&w, o));
    w.intervene(Intervention::Repeal {
        rule: reject_only.id,
    })
    .unwrap();
    let rule = enact(&mut w, norm(NormKind::Encroach, 32, Sanction::ALL));
    w.nomos.accuse(me_id, rule.id, 19, o_id);
    w.nomos.accuse(x_id, rule.id, 19, o_id);
    assert!(violator_adjacent(&w, me) && violator_adjacent(&w, o));
    w.check_invariants().unwrap();

    // Observed "as if" (`World::observation`): a copy of me — accused —
    // standing on another cell is a stranger there, so that cell's count
    // is not reduced for it, exactly as the walk skips nobody there.
    let y = reflex_agent(&mut w, Pos::new(1, 1), 50_000, 0);
    let y_id = w.agents[y].id;
    w.nomos.accuse(y_id, rule.id, 19, o_id);
    let mut copy = w.agents[me].clone();
    copy.pos = Pos::new(1, 1); // alone with Y, who is accused
    assert!(with_obs_of(&w, &copy, agreed));
    copy.pos = Pos::new(1, 0); // north of Y: adjacent
    assert!(with_obs_of(&w, &copy, agreed));
    copy.pos = Pos::new(7, 0); // nobody near
    assert!(!with_obs_of(&w, &copy, agreed));
    // A copy still on my own cell is myself: X beside me counts, I do not.
    copy.pos = Pos::new(4, 4);
    w.agents[x].pos = Pos::new(4, 4);
    assert!(with_obs_of(&w, &copy, agreed));
    w.agents[x].pos = Pos::new(7, 7);
    assert!(
        !with_obs_of(&w, &copy, agreed),
        "alone with O, who stands accused of nothing"
    );
    // A stranger (an id no agent carries) on my cell sees me, the accused.
    let mut stranger = w.agents[me].clone();
    stranger.id = AgentId(999);
    assert!(with_obs_of(&w, &stranger, agreed));
    stranger.pos = Pos::new(7, 0);
    assert!(!with_obs_of(&w, &stranger, agreed));
    w.check_invariants().unwrap();
}

/// Over a crowded world under the first-occupancy rule — reflex agents
/// mutating, 128-row learners exploring, so that harms, sanctions and
/// accused neighbours all happen — the count predicate and the walk agree
/// for every agent at every tick, and so do the serial and the parallel
/// decisions that read it.
#[test]
fn the_count_predicate_agrees_with_the_walk_over_a_ruled_world() {
    let cfg = WorldConfig {
        nomos: true,
        mutation_enabled: true,
        learner_states: 128,
        ..small(7)
    };
    let mix = |i: u32| {
        if i % 2 == 0 {
            PolicyKind::Reflex
        } else {
            PolicyKind::Learner
        }
    };
    let mut w = World::generate(cfg, mix).unwrap();
    w.intervene(Intervention::Enact {
        norm: norm(NormKind::Encroach, 32, Sanction::HARM),
    })
    .unwrap();
    let (mut seen, mut checked) = (0usize, 0usize);
    for _ in 0..300 {
        let index = w.spatial_index();
        let norms = w.norm_index();
        for i in 0..w.agents.len() {
            let obs = w.observation_at(i, &index, &norms);
            seen += agreed(&obs) as usize;
            checked += 1;
        }
        let serial = w.decide_all_with(&index, &norms);
        #[cfg(feature = "parallel")]
        assert_eq!(w.decide_all_parallel_with(&index, &norms), serial);
        w.apply_with(serial, &index, &norms);
    }
    assert!(
        seen > 0,
        "no accused neighbour in 300 ticks: the test exercises nothing"
    );
    assert!(
        w.ledger.sanctions.iter().sum::<u64>() > 0,
        "nobody sanctioned anyone"
    );
    w.check_invariants().unwrap();
    eprintln!("{seen} of {checked} observations had an accused neighbour within reach");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// The count predicate (`violator_adjacent`) is the walk
    /// (`adjacent_violator`) for every agent of every tick, whatever the
    /// rules, the perception, the social radius and the row count — and
    /// serial and parallel decisions, which read it, agree.
    #[test]
    fn the_count_predicate_is_the_walk_everywhere(
        seed in any::<u64>(),
        norms in prop::collection::vec(norm_strategy(), 1..=2),
        rows in prop_oneof![Just(32u16), Just(128)],
        see_standing in any::<bool>(),
        social_radius in 0u16..=2,
        mutation in any::<bool>(),
        ticks in 1u64..=40,
    ) {
        let cfg = WorldConfig {
            seed,
            width: 24,
            height: 24,
            initial_agents: 100,
            patch_count: 3,
            birth_threshold: Some(60_000),
            child_energy: 30_000,
            mutation_enabled: mutation,
            learner_states: rows,
            nomos: true,
            perception: simulation_core::Perception {
                see_standing,
                social_radius,
                ..Default::default()
            },
            ..WorldConfig::default()
        };
        let mix = |i: u32| match i % 3 { 0 => PolicyKind::Reflex, 1 => PolicyKind::Learner, _ => PolicyKind::Forager };
        let mut a = World::generate(cfg, mix).unwrap();
        for n in &norms {
            a.intervene(Intervention::Enact { norm: *n }).unwrap();
        }
        let mut b = a.clone();
        for _ in 0..ticks {
            let index = a.spatial_index();
            let norms = a.norm_index();
            for i in 0..a.agents.len() {
                let obs = a.observation_at(i, &index, &norms);
                prop_assert_eq!(
                    obs.violator_adjacent(),
                    obs.adjacent_violator().is_some(),
                    "agent {:?} at tick {}",
                    a.agents[i].id,
                    a.tick
                );
            }
            let ra = a.step();
            #[cfg(feature = "parallel")]
            let rb = b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            let rb = b.step();
            prop_assert_eq!(ra, rb);
        }
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
    }
}
