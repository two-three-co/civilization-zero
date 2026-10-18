//! `WorldConfig::enforcer_share_permille` (ADR-0014 D11, task 4-6): the
//! birth-time quota that holds the enforcers at a share of the living, so
//! that EXP-004's institution arms keep the composition `--policy
//! institution` founds (4-5 measured the forager-equivalent enforcers
//! filling the learners' founding valley: 97% enforcers by tick 300). With
//! the share `None` every child is its parent's kind, as in Phase 3. Also
//! the two readings 4-6 added to `TickReport`: births by the child's kind
//! and sanctions by the sanctioner's kind.

mod common;

use common::*;
use simulation_core::{
    Action, AgentId, Event, Intervention, Norm, NormKind, PolicyKind, PolicyTable, Pos, Sanction,
    World, WorldConfig,
};
use std::collections::{BTreeMap, BTreeSet};

const SHARE: u16 = 100;

/// The institution world of `cz --policy institution`, small: `nomos_preset`,
/// 128 rows, every tenth founder an enforcer, decrees only.
fn institution(seed: u64, share: Option<u16>) -> World {
    let cfg = WorldConfig {
        seed,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        learner_states: 128,
        enforcer_motions: false,
        enforcer_share_permille: share,
        ..WorldConfig::nomos_preset()
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

/// Kind and `hunger_permille` of every living agent.
fn roster(w: &World) -> BTreeMap<AgentId, (PolicyKind, u16)> {
    w.agents
        .iter()
        .map(|a| (a.id, (a.policy, a.params.hunger_permille)))
        .collect()
}

fn born(r: &simulation_core::TickReport) -> Vec<(AgentId, AgentId)> {
    r.events
        .iter()
        .filter_map(|e| match e {
            Event::Born { child, parent, .. } => Some((*child, *parent)),
            _ => None,
        })
        .collect()
}

fn count(w: &World, kind: PolicyKind) -> u64 {
    w.agents.iter().filter(|a| a.policy == kind).count() as u64
}

#[test]
fn the_quota_decides_every_birth_as_the_rule_says_and_children_of_the_other_kind_start_blank() {
    let mut w = institution(7, Some(SHARE));
    let (mut learners_of_enforcers, mut enforcers_of_learners) = (0u64, 0u64);
    let mut peak_share = 0u64;
    for _ in 0..1_500 {
        let before = roster(&w);
        let r = w.step();
        let births = born(&r);
        assert_eq!(births.len() as u64, u64::from(r.births));
        assert_eq!(
            r.births_by_kind.iter().map(|&b| u64::from(b)).sum::<u64>(),
            u64::from(r.births),
            "births by kind sum to the births"
        );
        // The survivors of the tick are the living less its children; the
        // rule is replayed over the births in their (parent-id) order.
        let after = roster(&w);
        let children: BTreeSet<AgentId> = births.iter().map(|(c, _)| *c).collect();
        let mut alive = after.keys().filter(|id| !children.contains(id)).count() as u64;
        let mut enforcers = after
            .iter()
            .filter(|(id, (k, _))| !children.contains(id) && *k == PolicyKind::Enforcer)
            .count() as u64;
        let mut reassigned = 0u32;
        let mut by_kind = [0u32; PolicyKind::COUNT];
        for (child, parent) in &births {
            let (parent_kind, parent_hunger) = before[parent];
            let expected = if matches!(parent_kind, PolicyKind::Learner | PolicyKind::Enforcer) {
                if enforcers * 1000 < u64::from(SHARE) * (alive + 1) {
                    PolicyKind::Enforcer
                } else {
                    PolicyKind::Learner
                }
            } else {
                parent_kind
            };
            let (kind, hunger) = after[child];
            assert_eq!(
                kind, expected,
                "tick {}: child {child:?} of {parent:?} ({parent_kind:?}) with {enforcers} enforcers among {alive}",
                r.tick
            );
            assert_eq!(hunger, parent_hunger, "the parent's scalars are kept");
            by_kind[kind.index()] += 1;
            if kind != parent_kind {
                reassigned += 1;
                let a = &w.agents[w.agents.binary_search_by_key(child, |a| a.id).unwrap()];
                assert_eq!(a.last_step, None);
                match (kind, &a.params.table) {
                    (PolicyKind::Learner, Some(PolicyTable::Learner { q })) => {
                        learners_of_enforcers += 1;
                        assert_eq!(q.len(), 128 * Action::KIND_COUNT);
                        assert!(
                            q.iter().all(|&v| v == 0),
                            "a learner born of an enforcer knows nothing"
                        );
                    }
                    (PolicyKind::Enforcer, None) => enforcers_of_learners += 1,
                    other => panic!("tick {}: {child:?} born {kind:?} carries {other:?}", r.tick),
                }
            }
            alive += 1;
            if kind == PolicyKind::Enforcer {
                enforcers += 1;
            }
        }
        assert_eq!(alive, after.len() as u64);
        assert_eq!(enforcers, count(&w, PolicyKind::Enforcer));
        assert_eq!(r.births_reassigned, reassigned);
        assert_eq!(r.births_by_kind, by_kind);
        if let Some(share) = (enforcers * 1000).checked_div(alive) {
            peak_share = peak_share.max(share);
        }
    }
    w.check_invariants().unwrap();
    // Both directions of the quota were exercised: enforcers above the
    // share (the learners' valley) bore learners, and once the learners
    // had recovered, learners bore the enforcers the share was short of.
    assert!(
        learners_of_enforcers > 0,
        "no learner was born of an enforcer in 1,500 ticks"
    );
    assert!(
        enforcers_of_learners > 0,
        "no enforcer was born of a learner in 1,500 ticks"
    );
    // The world did not become the enforcers': at the end the learners
    // hold the majority and the share is close to the quota.
    let (l, e) = (
        count(&w, PolicyKind::Learner),
        count(&w, PolicyKind::Enforcer),
    );
    assert!(l > 0 && e > 0, "learners {l}, enforcers {e}");
    let share = e * 1000 / (l + e);
    assert!(
        (50..=150).contains(&share),
        "enforcers {e} of {} living = {share}‰ at tick 1,500 (peak {peak_share}‰)",
        l + e
    );
    println!(
        "tick 1500: {l} learners, {e} enforcers ({share}‰, peak {peak_share}‰); {learners_of_enforcers} learners born of enforcers, {enforcers_of_learners} enforcers born of learners"
    );
}

#[test]
fn without_a_share_every_child_is_its_parents_kind() {
    assert_eq!(WorldConfig::default().enforcer_share_permille, None);
    assert_eq!(WorldConfig::nomos_preset().enforcer_share_permille, None);
    let mut w = institution(7, None);
    let mut births = 0u64;
    for _ in 0..600 {
        let before = roster(&w);
        let r = w.step();
        assert_eq!(r.births_reassigned, 0);
        let mut by_kind = [0u32; PolicyKind::COUNT];
        let after = roster(&w);
        for (child, parent) in born(&r) {
            assert_eq!(after[&child].0, before[&parent].0);
            by_kind[after[&child].0.index()] += 1;
            births += 1;
        }
        assert_eq!(r.births_by_kind, by_kind);
    }
    assert!(births > 0);
    // And, as 4-5 measured, the enforcers take the world over.
    let (l, e) = (
        count(&w, PolicyKind::Learner),
        count(&w, PolicyKind::Enforcer),
    );
    assert!(e > l, "without the quota: learners {l}, enforcers {e}");
}

#[test]
fn the_share_is_deterministic_and_round_trips_and_the_config_bounds_it() {
    let mut a = institution(11, Some(SHARE));
    let mut b = institution(11, Some(SHARE));
    for t in 0..300 {
        let ra = a.step();
        #[cfg(feature = "parallel")]
        let rb = b.step_parallel();
        #[cfg(not(feature = "parallel"))]
        let rb = b.step();
        assert_eq!(ra, rb);
        if t % 50 == 0 {
            a.check_invariants().unwrap();
        }
    }
    assert_eq!(a.state_hash(), b.state_hash());
    assert!(a.agents.iter().any(|x| x.policy == PolicyKind::Enforcer));
    assert!(a.agents.iter().any(|x| x.policy == PolicyKind::Learner));
    let bin = a.to_bytes();
    let json = a.to_snapshot();
    assert_eq!(World::from_bytes(&bin).unwrap(), a);
    assert_eq!(World::from_snapshot(&json).unwrap(), a);
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(v["world"]["config"]["enforcer_share_permille"], 100);
    // A config written before the field existed reads as `None` (the
    // snapshot itself is refused by its format number, as always).
    let mut old = v["world"]["config"].clone();
    old.as_object_mut()
        .unwrap()
        .remove("enforcer_share_permille");
    let loaded: WorldConfig = serde_json::from_value(old).unwrap();
    assert_eq!(loaded.enforcer_share_permille, None);
    assert_eq!(
        WorldConfig {
            enforcer_share_permille: Some(SHARE),
            ..loaded
        },
        a.config
    );
    // The share is a permille.
    let bad = WorldConfig {
        enforcer_share_permille: Some(1001),
        ..WorldConfig::nomos_preset()
    };
    assert!(bad.validate().is_err());
    assert!(WorldConfig {
        enforcer_share_permille: Some(1000),
        ..WorldConfig::nomos_preset()
    }
    .validate()
    .is_ok());
}

#[test]
fn sanctions_are_counted_by_the_sanctioners_kind() {
    let mut w = empty_world(WorldConfig {
        nomos: true,
        ..cfg()
    });
    // An enforcer, the accused and a learner in a row: both neighbours
    // can reach the accused.
    let params = w.config.initial_params.clone();
    add_agent_with(
        &mut w,
        Pos::new(0, 1),
        50_000,
        0,
        PolicyKind::Enforcer,
        params.clone(),
    );
    add_agent(&mut w, Pos::new(1, 1), 50_000, 0);
    add_agent_with(
        &mut w,
        Pos::new(2, 1),
        50_000,
        0,
        PolicyKind::Learner,
        params,
    );
    w.intervene(Intervention::Enact {
        norm: Norm {
            kind: NormKind::Encroach,
            standing_ticks: 32,
            sanction: Sanction::HARM,
            except_sanction: false,
        },
    })
    .unwrap();
    w.apply(intents(&[Action::Rest; 3]));
    // Accused since the previous tick, so a harm this tick answers it.
    w.nomos.accuse(AgentId(1), 0, w.tick - 1, AgentId(0));
    let r = w.apply(intents(&[
        Action::Harm { target: AgentId(1) },
        Action::Rest,
        Action::Harm { target: AgentId(1) },
    ]));
    assert_eq!(r.harms, 2);
    assert_eq!(r.sanctions[NormKind::Encroach.index()], 2);
    let mut expected = [0u32; PolicyKind::COUNT];
    expected[PolicyKind::Enforcer.index()] = 1;
    expected[PolicyKind::Learner.index()] = 1;
    assert_eq!(r.sanctions_by_kind, expected);
    assert_eq!(
        r.sanctions_by_kind.iter().sum::<u32>(),
        r.sanctions.iter().sum::<u32>()
    );
    // Without a rule the same harms are no one's sanctions.
    w.intervene(Intervention::Repeal { rule: 0 }).unwrap();
    let r = w.apply(intents(&[
        Action::Harm { target: AgentId(1) },
        Action::Rest,
        Action::Harm { target: AgentId(1) },
    ]));
    assert_eq!(r.harms, 2);
    assert_eq!(r.sanctions_by_kind, [0; PolicyKind::COUNT]);
    w.check_invariants().unwrap();
}
