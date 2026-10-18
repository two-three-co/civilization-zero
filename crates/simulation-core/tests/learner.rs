//! The Q learner (ADR-0013 3-7): the state bits, the ε-greedy choice and
//! its actuators, the kernel's credit of the previous step (reward, next
//! row's maximum, birth bonus, broken chains), inheritance, snapshots and
//! the determinism of learning worlds (serial = parallel).

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::policy::learner::{
    self, heard_last_tick, learner_state, state, state_row, FINE_STATES, Q_UNIT,
};
use simulation_core::policy::{self, scan_best};
use simulation_core::rng::{RngStream, Stream};
use simulation_core::world::SNAPSHOT_FORMAT;
use simulation_core::{
    Action, AgentId, Dir, Event, Intent, Interaction, InteractionKind, Intervention, LearnerStep,
    Message, ParamsMode, PolicyKind, PolicyTable, Pos, SnapshotError, Who, World, WorldConfig,
};

const K: usize = Action::KIND_COUNT;
const MOVE: usize = 0;
const GATHER: usize = 1;
const CONSUME: usize = 2;
const REST: usize = 3;
const TRANSFER: usize = 4;
const STORE: usize = 5;
const HARM: usize = 6;
const SIGNAL: usize = 7;

/// The hand-built world of `common::cfg()` with a chosen ε.
fn eps_cfg(eps: u16) -> WorldConfig {
    WorldConfig {
        learner_epsilon_permille: eps,
        ..cfg()
    }
}

fn learner_agent(w: &mut World, pos: Pos, energy: i64, inventory: u32) -> usize {
    let params = w.config.initial_params.clone();
    add_agent_with(w, pos, energy, inventory, PolicyKind::Learner, params)
}

fn q(w: &World, i: usize) -> &[i16] {
    match &w.agents[i].params.table {
        Some(PolicyTable::Learner { q }) => q,
        other => panic!("not a learner table: {other:?}"),
    }
}

fn set_q(w: &mut World, i: usize, row: u16, kind: usize, value: i16) {
    match w.agents[i].params.table.as_mut() {
        Some(PolicyTable::Learner { q }) => q[row as usize * K + kind] = value,
        other => panic!("not a learner table: {other:?}"),
    }
}

fn decide(w: &World, i: usize, tick: u64) -> Intent {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation(&w.agents[i], &index, &norms);
    let mut rng = RngStream::new(w.config.seed, tick, w.agents[i].id.0, Stream::Decide);
    policy::decide(&obs, &mut rng)
}

/// The fine (10-bit) state `w.agents[i]` is in right now.
fn fine_of(w: &World, i: usize) -> u16 {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let a = &w.agents[i];
    let obs = w.observation(a, &index, &norms);
    let scan = (a.inventory < w.config.inventory_capacity).then(|| scan_best(&obs));
    learner_state(&obs, scan.as_ref(), heard_last_tick(&obs))
}

/// The row of its Q table that state falls into.
fn row_of(w: &World, i: usize) -> u16 {
    state_row(fine_of(w, i), w.config.learner_states)
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

#[test]
fn the_state_bits_reflect_body_sight_crowd_and_hearing() {
    let mut w = empty_world(cfg());
    w.tick = 10;
    let me = learner_agent(&mut w, Pos::new(4, 4), 30_000, 2); // hungry, upper half
    let near = learner_agent(&mut w, Pos::new(5, 4), 50_000, 0); // adjacent
    learner_agent(&mut w, Pos::new(4, 6), 50_000, 0); // distance 2: not adjacent
    set_food(&mut w, Pos::new(4, 2), 7);
    w.agents[me].recent = vec![Interaction {
        tick: 9,
        other: AgentId(1),
        kind: InteractionKind::Heard,
        value: 3,
    }];
    let all = state::HUNGRY
        | state::HAS_FOOD
        | state::FOOD_IN_SIGHT
        | state::ADJACENT
        | state::UPPER
        | state::HEARD;
    assert_eq!(fine_of(&w, me), all);
    assert!(all < FINE_STATES);
    assert_eq!(row_of(&w, me), all & 31, "32 rows drop UPPER and HEARD");

    // Four energy levels around the hunger threshold (50,000 of 100,000).
    for (energy, hungry, upper) in [
        (24_999, true, false),
        (25_000, true, true),
        (49_999, true, true),
        (50_000, false, false),
        (74_999, false, false),
        (75_000, false, true),
        (100_000, false, true),
    ] {
        w.agents[me].energy = energy;
        let s = fine_of(&w, me);
        assert_eq!(s & state::HUNGRY != 0, hungry, "energy {energy}");
        assert_eq!(s & state::UPPER != 0, upper, "energy {energy}");
    }
    w.agents[me].energy = 30_000;

    // Sight is scanned only with room in the pack; food underfoot is its own bit.
    w.agents[me].inventory = w.config.inventory_capacity;
    assert_eq!(
        fine_of(&w, me) & state::FOOD_IN_SIGHT,
        0,
        "full pack: no scan"
    );
    assert_ne!(fine_of(&w, me) & state::HAS_FOOD, 0);
    w.agents[me].inventory = 0;
    assert_eq!(fine_of(&w, me) & state::HAS_FOOD, 0);
    assert_ne!(fine_of(&w, me) & state::FOOD_IN_SIGHT, 0);
    set_food(&mut w, Pos::new(4, 4), 5);
    assert_ne!(fine_of(&w, me) & state::FOOD_HERE, 0);

    // Adjacency follows `nearest_adjacent`: same cell always, the four
    // neighbouring cells only when the social radius reaches them.
    w.agents[near].pos = Pos::new(6, 4);
    assert_eq!(
        fine_of(&w, me) & state::ADJACENT,
        0,
        "distance 2 is not adjacent"
    );
    w.agents[near].pos = Pos::new(4, 3);
    assert_ne!(fine_of(&w, me) & state::ADJACENT, 0);
    w.config.perception.social_radius = 0;
    assert_eq!(
        fine_of(&w, me) & state::ADJACENT,
        0,
        "radius 0 sees no neighbour cell"
    );
    w.agents[near].pos = Pos::new(4, 4);
    assert_ne!(
        fine_of(&w, me) & state::ADJACENT,
        0,
        "but always its own cell"
    );
    w.config.perception.social_radius = 2;

    // Hearing is last tick only.
    w.agents[me].recent[0].tick = 8;
    assert_eq!(fine_of(&w, me) & state::HEARD, 0);
    w.agents[me].recent.clear();
    assert_eq!(fine_of(&w, me) & state::HEARD, 0);
    w.agents[me].inventory = 2; // as the ledger booked it
    w.check_invariants().unwrap();
}

#[test]
fn the_greedy_choice_follows_the_row_and_actuators_fill_in_the_parameters() {
    let mut w = empty_world(eps_cfg(0));
    w.tick = 5;
    let a = learner_agent(&mut w, Pos::new(4, 4), 10_000, 3);
    let row = row_of(&w, a);
    assert_eq!(row, state::HUNGRY | state::HAS_FOOD);
    let it = decide(&w, a, 5);
    assert_eq!(it.state, Some(row), "the intent names the row decided in");

    let mut ladder = 0i16;
    let mut prefer = |w: &mut World, kind: usize| {
        ladder += 100;
        set_q(w, a, row, kind, ladder);
    };
    prefer(&mut w, CONSUME);
    assert_eq!(decide(&w, a, 5).action, Action::Consume(1));
    prefer(&mut w, REST);
    assert_eq!(decide(&w, a, 5).action, Action::Rest);
    prefer(&mut w, GATHER);
    let it = decide(&w, a, 5);
    assert_eq!(it.action, Action::Gather);
    assert_eq!(
        it.memory.target,
        Some(Pos::new(4, 4)),
        "gathering marks the cell"
    );
    prefer(&mut w, STORE);
    assert_eq!(decide(&w, a, 5).action, Action::Store(1));
    prefer(&mut w, TRANSFER);
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Rest,
        "nobody adjacent to give to"
    );
    prefer(&mut w, HARM);
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Rest,
        "nobody adjacent to harm"
    );
    prefer(&mut w, SIGNAL);
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Signal(Message::Say(0)),
        "nothing heard: symbol 0"
    );
    prefer(&mut w, MOVE);
    let it = decide(&w, a, 5);
    assert!(
        matches!(it.action, Action::Move(_)),
        "nothing in sight: explore"
    );
    assert_eq!(it.memory.target, None);

    // Someone adjacent changes the row; Transfer and Harm then aim at them.
    let other = learner_agent(&mut w, Pos::new(5, 4), 50_000, 0);
    let other_id = w.agents[other].id;
    let row2 = row_of(&w, a);
    assert_eq!(row2, row | state::ADJACENT);
    set_q(&mut w, a, row2, TRANSFER, 100);
    assert_eq!(
        decide(&w, a, 5).action,
        Action::Transfer { to: other_id, n: 1 }
    );
    set_q(&mut w, a, row2, HARM, 200);
    assert_eq!(decide(&w, a, 5).action, Action::Harm { target: other_id });

    // Food in sight: the Move actuator is the forager's rule 4.
    set_food(&mut w, Pos::new(4, 1), 9);
    let row3 = row_of(&w, a);
    assert_eq!(row3, row2 | state::FOOD_IN_SIGHT);
    set_q(&mut w, a, row3, MOVE, 100);
    let it = decide(&w, a, 5);
    assert_eq!(it.action, Action::Move(Dir::North));
    assert_eq!(it.memory.target, Some(Pos::new(4, 1)));

    // HEARD is bit 9 (ADR-0014 D6): with 32 rows it folds onto the same
    // row, and the Signal actuator echoes the symbol.
    w.agents[a].recent = vec![Interaction {
        tick: 4,
        other: other_id,
        kind: InteractionKind::Heard,
        value: 5,
    }];
    assert_eq!(fine_of(&w, a), row3 | state::HEARD);
    assert_eq!(row_of(&w, a), row3);
    set_q(&mut w, a, row3, SIGNAL, 300);
    assert_eq!(decide(&w, a, 5).action, Action::Signal(Message::Say(5)));
    // Only the full 1,024-row table keeps the bit; 128 rows (the norm bits
    // and nothing above them) fold it away like 32 do.
    w.config.learner_states = 128;
    assert_eq!(row_of(&w, a), row3);
    w.config.learner_states = 1024;
    for i in 0..w.agents.len() {
        w.agents[i].params =
            std::mem::take(&mut w.agents[i].params).refit(PolicyKind::Learner, &w.config);
    }
    assert_eq!(row_of(&w, a), row3 | state::HEARD);
    assert_eq!(q(&w, a).len(), 1024 * K);
    w.check_invariants().unwrap();
}

#[test]
fn epsilon_explores_uniformly_and_zero_epsilon_leaves_a_unique_maximum_alone() {
    // An adjacent agent makes every kind resolve to its own action, and the
    // row has a unique maximum (Rest), so any other kind is exploration.
    let build = |eps: u16| {
        let mut w = empty_world(eps_cfg(eps));
        let a = learner_agent(&mut w, Pos::new(4, 4), 50_000, 3);
        learner_agent(&mut w, Pos::new(5, 4), 50_000, 0);
        let row = row_of(&w, a);
        set_q(&mut w, a, row, REST, 100);
        (w, a)
    };
    let (w, a) = build(1000);
    let mut counts = [0u32; K];
    for tick in 0..8_000u64 {
        let it = decide(&w, a, tick);
        assert_eq!(it, decide(&w, a, tick), "same tick, same choice");
        counts[it.action.kind_index()] += 1;
    }
    // n = 8000, p = 1/8: mean 1000, sd 29.6, 5 sd = 148.
    for (k, n) in counts.iter().enumerate() {
        assert!(
            (852..=1148).contains(n),
            "kind {k} chosen {n} times: {counts:?}"
        );
    }

    let (w, a) = build(0);
    for tick in 0..2_000u64 {
        assert_eq!(decide(&w, a, tick).action, Action::Rest, "tick {tick}");
    }

    let (w, a) = build(500);
    let explored = (0..8_000u64)
        .filter(|&tick| decide(&w, a, tick).action != Action::Rest)
        .count();
    // Half the ticks explore, 7/8 of those away from Rest: p = 0.4375,
    // mean 3500, sd 44.4, 5 sd = 222.
    assert!((3_278..=3_722).contains(&explored), "{explored}");
}

#[test]
fn the_kernel_credits_the_previous_step_with_its_reward_and_the_next_rows_maximum() {
    let mut w = empty_world(eps_cfg(0));
    let a = learner_agent(&mut w, Pos::new(4, 4), 50_000, 3);
    let s0 = row_of(&w, a);
    assert_eq!(
        s0,
        state::HAS_FOOD,
        "fed, not in the upper half, food in the pack"
    );
    set_q(&mut w, a, s0, CONSUME, 10);
    let before = q(&w, a).to_vec();

    // Tick 0: Consume (+4,000) and metabolism (−500): reward 35. Nothing
    // to credit yet, so the table is untouched.
    w.step();
    assert_eq!(w.agents[a].energy, 53_500);
    assert_eq!(
        w.agents[a].last_step,
        Some(LearnerStep {
            state: s0,
            action: CONSUME as u8,
            reward: 35,
        })
    );
    assert_eq!(q(&w, a), &before[..]);

    // Tick 1: the same row again; the step of tick 0 is credited with
    // reward 35 and γ·max Q[s1] where s1 = s0 (its own 10).
    let s1 = row_of(&w, a);
    assert_eq!(s1, s0);
    w.step();
    let expected = learner::q_step(10, 35, 10);
    assert!(expected > 10);
    assert_eq!(q(&w, a)[s0 as usize * K + CONSUME], expected);
    assert_eq!(
        q(&w, a).iter().filter(|&&v| v != 0).count(),
        1,
        "exactly one cell changed"
    );
    assert_eq!(
        w.agents[a].last_step,
        Some(LearnerStep {
            state: s0,
            action: CONSUME as u8,
            reward: 35,
        })
    );

    // A hand-built intent carries no row: the chain ends without a credit.
    let table = q(&w, a).to_vec();
    w.apply(intents(&[Action::Rest]));
    assert_eq!(w.agents[a].last_step, None);
    assert_eq!(q(&w, a), &table[..]);
    // The next decided step starts a new chain, again without a credit.
    w.step();
    assert!(w.agents[a].last_step.is_some());
    assert_eq!(q(&w, a), &table[..]);
    w.check_invariants().unwrap();

    // Death removes the step with the body.
    let mut d = empty_world(eps_cfg(0));
    let dying = learner_agent(&mut d, Pos::new(1, 1), 0, 0);
    d.agents[dying].health = 1;
    let r = d.step();
    assert_eq!(r.deaths, 1);
    assert!(d.agents.is_empty());
    d.check_invariants().unwrap();
}

#[test]
fn the_birth_transfer_is_not_a_penalty_and_the_child_starts_fresh() {
    let c = WorldConfig {
        birth_threshold: Some(80_000),
        child_energy: 40_000,
        mutation_enabled: true,
        ..eps_cfg(0)
    };
    let mut w = empty_world(c);
    let a = learner_agent(&mut w, Pos::new(4, 4), 79_000, 1);
    let s0 = row_of(&w, a);
    set_q(&mut w, a, s0, CONSUME, 10);
    set_q(&mut w, a, 7, REST, -3); // some learned value to inherit
    let r = w.step();
    assert_eq!(r.births, 1);
    let (parent, child) = (&w.agents[0], &w.agents[1]);
    assert_eq!(parent.energy, 82_500 - 40_000);
    assert_eq!(
        parent.last_step.map(|s| s.reward),
        Some(35),
        "the 40,000 handed to the child are not a loss"
    );
    assert_eq!(child.policy, PolicyKind::Learner);
    assert_eq!(child.last_step, None, "nothing to credit for a newborn");
    assert_eq!(
        child.params.table, parent.params.table,
        "Q inherited unmutated"
    );
    assert_ne!(
        child.params.hunger_permille, parent.params.hunger_permille,
        "the scalar mutates (step 50 permille, zero draws are rare but possible: \
         the table equality above is the assertion that matters)"
    );
    w.check_invariants().unwrap();
}

#[test]
fn q_falls_monotonically_in_a_self_loop_of_resting() {
    // Alone, fed, no food anywhere: the row never changes. Every other kind
    // is pinned far below, so Rest stays greedy and the kernel feeds its own
    // value back as max Q[s']: the sequence must be the pure `q_step`
    // sequence, monotone, towards −5 · Q_UNIT · 32/3.
    let mut w = empty_world(eps_cfg(0));
    let a = learner_agent(&mut w, Pos::new(4, 4), 99_000, 0);
    let s = row_of(&w, a);
    for kind in 0..K {
        set_q(&mut w, a, s, kind, if kind == REST { 0 } else { -20_000 });
    }
    let (mut expected, mut seen) = (0i16, Vec::new());
    w.step(); // tick 0: first step, nothing credited
    for _ in 0..40 {
        assert_eq!(row_of(&w, a), s, "the row is stable");
        w.step();
        let v = q(&w, a)[s as usize * K + REST];
        expected = learner::q_step(expected, -5, expected);
        assert_eq!(v, expected);
        seen.push(v);
    }
    assert!(seen.windows(2).all(|p| p[1] <= p[0]), "{seen:?}");
    assert!(seen[0] < 0 && *seen.last().unwrap() > (-5 * Q_UNIT * 32 / 3) as i16);
    assert_eq!(
        w.agents[a].last_step,
        Some(LearnerStep {
            state: s,
            action: REST as u8,
            reward: -5,
        })
    );
}

#[test]
fn set_policy_clears_the_pending_step_and_a_kept_table_resumes_learning() {
    let cfg = small(7);
    let mut w = World::generate(cfg, |_| PolicyKind::Learner).unwrap();
    w.run(20);
    let id = w.agents[3].id;
    assert!(w.agents[3].last_step.is_some());
    let learned = w.agents[3].params.table.clone();
    assert!(matches!(&learned, Some(PolicyTable::Learner { q }) if q.iter().any(|&v| v != 0)));

    w.intervene(Intervention::SetPolicy {
        who: Who::Id(id),
        kind: PolicyKind::Learner,
        params: ParamsMode::Keep,
    })
    .unwrap();
    assert_eq!(w.agents[3].last_step, None, "the switch breaks the chain");
    assert_eq!(
        w.agents[3].params.table, learned,
        "but keeps what was learned"
    );
    w.step();
    assert!(w.agents[3].last_step.is_some(), "and learning resumes");

    w.intervene(Intervention::SetPolicy {
        who: Who::All,
        kind: PolicyKind::Forager,
        params: ParamsMode::Default,
    })
    .unwrap();
    assert!(w.agents.iter().all(|a| a.last_step.is_none()));
    w.run(5);
    assert!(w.agents.iter().all(|a| a.last_step.is_none()));
    w.check_invariants().unwrap();
}

#[test]
fn snapshots_carry_the_pending_step_in_both_encodings_and_reject_what_does_not_fit() {
    let mut w = World::generate(
        WorldConfig {
            seed: 9,
            width: 24,
            height: 24,
            initial_agents: 60,
            patch_count: 2,
            ..WorldConfig::default()
        },
        |i| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Reflex,
            _ => PolicyKind::Learner,
        },
    )
    .unwrap();
    w.run(50);
    assert!(w
        .agents
        .iter()
        .any(|a| a.policy == PolicyKind::Learner && a.last_step.is_some()));
    let bin = w.to_bytes();
    let json = w.to_snapshot();
    assert_eq!(World::from_bytes(&bin).unwrap(), w);
    assert_eq!(World::from_snapshot(&json).unwrap(), w);
    assert_eq!(SNAPSHOT_FORMAT, 12);
    assert!(std::str::from_utf8(&json)
        .unwrap()
        .starts_with("{\"format\":12"));
    println!(
        "mixed-learning snapshot after learning: {} bytes binary, {} bytes json, {} agents",
        bin.len(),
        json.len(),
        w.agents.len()
    );

    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    let mut old = v.clone();
    old["format"] = serde_json::json!(11);
    assert_eq!(
        World::from_snapshot(&serde_json::to_vec(&old).unwrap()).err(),
        Some(SnapshotError::Format(11))
    );
    let agents = v["world"]["agents"].as_array().unwrap();
    let learner_ix = agents
        .iter()
        .position(|a| a["policy"] == "Learner" && !a["last_step"].is_null())
        .unwrap();
    let forager_ix = agents
        .iter()
        .position(|a| a["policy"] == "Forager")
        .unwrap();
    assert!(v["world"]["agents"][forager_ix]["last_step"].is_null());
    let step = &v["world"]["agents"][learner_ix]["last_step"];
    assert!(step["state"].is_u64() && step["action"].is_u64() && step["reward"].is_i64());
    let invalid = |mutate: &dyn Fn(&mut serde_json::Value)| {
        let mut bad = v.clone();
        mutate(&mut bad);
        matches!(
            World::from_snapshot(&serde_json::to_vec(&bad).unwrap()),
            Err(SnapshotError::Invalid(_))
        )
    };
    // A row beyond the table, an action beyond the kinds, a step on a kind
    // that does not learn, an ε above 1000: all refused before the hash.
    assert!(invalid(
        &|b| b["world"]["agents"][learner_ix]["last_step"]["state"] = 32.into()
    ));
    assert!(invalid(
        &|b| b["world"]["agents"][learner_ix]["last_step"]["action"] = 8.into()
    ));
    assert!(invalid(&|b| {
        b["world"]["agents"][forager_ix]["last_step"] =
            serde_json::json!({"state": 0, "action": 0, "reward": 0})
    }));
    assert!(invalid(
        &|b| b["world"]["config"]["learner_epsilon_permille"] = 1001.into()
    ));
    // A config written before 3-7 still loads, with the default ε.
    let mut c: serde_json::Value = serde_json::to_value(WorldConfig::default()).unwrap();
    c.as_object_mut()
        .unwrap()
        .remove("learner_epsilon_permille");
    let c: WorldConfig = serde_json::from_value(c).unwrap();
    assert_eq!(c.learner_epsilon_permille, 50);
    assert!(WorldConfig {
        learner_epsilon_permille: 1001,
        ..WorldConfig::default()
    }
    .validate()
    .is_err());
}

#[test]
fn children_inherit_the_credited_table_unmutated_and_rows_follow_learner_states() {
    // 1,024 rows, not 128: with `nomos` off the norm bits 5–7 are always 0
    // (ADR-0014 D6), so only `UPPER` / `HEARD` can take a row past 31.
    for (states, seed) in [(8u16, 3u64), (1024, 4)] {
        let cfg = WorldConfig {
            learner_states: states,
            mutation_enabled: true,
            birth_threshold: Some(60_000),
            child_energy: 30_000,
            ..small(seed)
        };
        let mut w = World::generate(cfg, |_| PolicyKind::Learner).unwrap();
        let mut births = 0;
        let mut max_row = 0u16;
        for _ in 0..300 {
            let r = w.step();
            for e in &r.events {
                if let Event::Born { child, parent, .. } = e {
                    let at = |id: AgentId| {
                        let i = w.agents.binary_search_by_key(&id, |x| x.id).unwrap();
                        &w.agents[i]
                    };
                    let (p, c) = (at(*parent), at(*child));
                    assert_eq!(p.params.table, c.params.table, "Q travels unmutated");
                    assert_eq!(c.last_step, None);
                    assert!(p.last_step.is_some());
                    births += 1;
                }
            }
            for a in &w.agents {
                if let Some(s) = a.last_step {
                    assert!(s.state < states);
                    max_row = max_row.max(s.state);
                }
            }
        }
        assert!(births > 0, "{states} states: generations passed");
        if states == 1024 {
            assert!(max_row >= 256, "the UPPER bit is visited: {max_row}");
        } else {
            assert!((4..8).contains(&max_row), "{max_row}");
        }
        assert_eq!(World::from_bytes(&w.to_bytes()).unwrap(), w);
        w.check_invariants().unwrap();
    }
}

#[test]
fn forager_worlds_carry_no_learner_state_and_keep_the_golden_hash() {
    let mut w = World::new(WorldConfig {
        seed: 42,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(200);
    assert!(w.agents.iter().all(|a| a.last_step.is_none()));
    assert_eq!(w.dynamics_hash(), 0x658e_e873_d186_6501);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// Learning worlds are as deterministic as fixed ones: serial and
    /// parallel decisions agree, a twin replays to the same hash, the
    /// invariants hold whatever ε, the row count or mutation.
    #[test]
    fn learner_worlds_are_deterministic(
        seed in any::<u64>(),
        eps in 0u16..=1000,
        states in prop_oneof![Just(1u16), Just(2), Just(8), Just(32), Just(128), Just(300)],
        mutation in any::<bool>(),
        ticks in 1u64..=80,
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
            learner_states: states,
            learner_epsilon_permille: eps,
            ..WorldConfig::default()
        };
        let mut a = World::generate(cfg.clone(), |i| if i % 4 == 0 { PolicyKind::Forager } else { PolicyKind::Learner }).unwrap();
        let mut b = World::generate(cfg.clone(), |i| if i % 4 == 0 { PolicyKind::Forager } else { PolicyKind::Learner }).unwrap();
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
        prop_assert!(a.agents.iter().all(|x| x.last_step.is_some() == (x.policy == PolicyKind::Learner && x.age > 0)));
        prop_assert_eq!(World::from_bytes(&a.to_bytes()).unwrap(), a);
    }
}
