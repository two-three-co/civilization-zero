//! Signals and kernel-written memory (ADR-0013 3-3): what neighbours see
//! and when, what the ring records, in which order, and how it is capped.

mod common;

use common::*;
use simulation_core::{
    Action, AgentId, Event, Interaction, InteractionKind, Message, PolicyKind, Pos, Signal, World,
    WorldConfig,
};

fn say(symbol: u8) -> Action {
    Action::Signal(Message::Say(symbol))
}

fn seen_signal(w: &World, observer: usize, other: u64) -> Option<Message> {
    let index = w.spatial_index();
    let norms = w.norm_index();
    w.observation(&w.agents[observer], &index, &norms)
        .neighbors()
        .into_iter()
        .find(|n| n.id == AgentId(other))
        .and_then(|n| n.signal)
}

fn note(tick: u64, other: u64, kind: InteractionKind, value: i64) -> Interaction {
    Interaction {
        tick,
        other: AgentId(other),
        kind,
        value,
    }
}

#[test]
fn a_signal_is_heard_now_and_seen_next_tick_only_within_the_signal_radius() {
    let mut w = empty_world(cfg()); // social radius 2
    w.config.perception.signal_radius = 1;
    add_agent(&mut w, Pos::new(4, 4), 50_000, 0); // 0: speaker
    add_agent(&mut w, Pos::new(5, 4), 50_000, 0); // 1: distance 1, hears
    add_agent(&mut w, Pos::new(4, 6), 50_000, 0); // 2: distance 2, a neighbour but out of earshot
    let r = w.apply(intents(&[say(3), Action::Rest, Action::Rest]));
    assert_eq!(r.signals, 1);
    assert_eq!(
        w.agents[0].last_signal,
        Some(Signal {
            tick: 0,
            message: Message::Say(3)
        })
    );
    assert_eq!(
        w.agents[1].recent,
        vec![note(0, 0, InteractionKind::Heard, 3)]
    );
    assert!(w.agents[2].recent.is_empty(), "too far to hear");
    assert!(
        w.agents[0].recent.is_empty(),
        "speakers do not hear themselves"
    );
    assert!(
        !r.events.iter().any(|e| matches!(e, Event::Signaled { .. })),
        "not recorded by default"
    );

    // Tick 1: the signal is fresh, so the neighbour within earshot sees it.
    assert_eq!(seen_signal(&w, 1, 0), Some(Message::Say(3)));
    assert_eq!(
        seen_signal(&w, 2, 0),
        None,
        "a neighbour, but beyond signal_radius"
    );
    w.apply(intents(&[Action::Rest; 3]));
    // Tick 2: stale. The agent's own record stays; only the observation ages.
    assert_eq!(seen_signal(&w, 1, 0), None);
    assert_eq!(w.agents[0].last_signal.map(|s| s.tick), Some(0));

    w.config.record_signals = true;
    let r = w.apply(intents(&[say(7), Action::Rest, Action::Rest]));
    assert!(r.events.contains(&Event::Signaled {
        tick: 2,
        agent: AgentId(0),
        message: Message::Say(7)
    }));
    assert_eq!(
        w.agents[0].energy,
        50_000 - 3 * w.config.metabolism_cost,
        "signals are free by default"
    );
    w.check_invariants().unwrap();
}

#[test]
fn memory_records_interactions_in_order_of_occurrence_and_caps_at_memory_slots() {
    let mut w = empty_world(WorldConfig {
        memory_slots: 3,
        harm_damage: 10,
        ..cfg()
    });
    add_agent(&mut w, Pos::new(2, 2), 50_000, 5); // 0
    add_agent(&mut w, Pos::new(3, 2), 50_000, 0); // 1: adjacent to 0
    add_agent(&mut w, Pos::new(2, 4), 50_000, 0); // 2: two cells from 0, three from 1
                                                  // Tick 0: 0 gives 2 food to 1, 1 harms 0, 2 says 5.
    let r = w.apply(intents(&[
        Action::Transfer {
            to: AgentId(1),
            n: 2,
        },
        Action::Harm { target: AgentId(0) },
        say(5),
    ]));
    assert_eq!((r.transfers, r.harms, r.signals), (1, 1, 1));
    use InteractionKind::*;
    assert_eq!(
        w.agents[0].recent,
        vec![
            note(0, 1, GaveTo, 2),
            note(0, 1, HarmedBy, 10),
            note(0, 2, Heard, 5)
        ]
    );
    assert_eq!(
        w.agents[1].recent,
        vec![note(0, 0, ReceivedFrom, 2), note(0, 0, Harmed, 10)],
        "too far from 2 to hear it"
    );
    assert_eq!(
        w.agents[2].recent,
        vec![note(0, 1, SawHarm, 0)],
        "a witness within social_radius of the victim, and deaf to itself"
    );
    w.check_invariants().unwrap();

    // Three more things heard push the oldest entries out: the cap is 3.
    for s in [1u8, 2, 3] {
        w.apply(intents(&[Action::Rest, Action::Rest, say(s)]));
    }
    assert_eq!(
        w.agents[0]
            .recent
            .iter()
            .map(|e| (e.kind, e.value))
            .collect::<Vec<_>>(),
        vec![(Heard, 1), (Heard, 2), (Heard, 3)]
    );
    assert_eq!(w.agents[0].recent[0].tick, 1);

    // memory_slots = 0 turns memory off.
    w.config.memory_slots = 0;
    w.apply(intents(&[Action::Rest, Action::Rest, say(9)]));
    assert!(w.agents[0].recent.is_empty());
}

#[test]
fn forager_worlds_neither_signal_nor_remember() {
    let mut w = World::new(WorldConfig {
        seed: 42,
        width: 48,
        height: 48,
        initial_agents: 150,
        patch_count: 6,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(200);
    assert!(w
        .agents
        .iter()
        .all(|a| a.recent.is_empty() && a.last_signal.is_none()));
    assert_eq!(w.ledger.signals, 0);
}

#[test]
fn memories_survive_snapshots_and_are_part_of_the_dynamics_hash() {
    let mut w = World::generate(
        WorldConfig {
            seed: 9,
            width: 24,
            height: 24,
            initial_agents: 60,
            patch_count: 2,
            ..WorldConfig::default()
        },
        |_| PolicyKind::Random,
    )
    .unwrap();
    w.run(60);
    assert!(
        w.agents.iter().any(|a| !a.recent.is_empty()),
        "random walkers interact"
    );
    assert!(
        w.agents.iter().any(|a| a.last_signal.is_some()),
        "random walkers talk"
    );
    assert_eq!(World::from_bytes(&w.to_bytes()).unwrap(), w);
    assert_eq!(World::from_snapshot(&w.to_snapshot()).unwrap(), w);
    println!(
        "snapshot with memories: {} bytes for {} agents",
        w.to_bytes().len(),
        w.agents.len()
    );
    let h = w.dynamics_hash();
    let mut forgetful = w.clone();
    let i = forgetful
        .agents
        .iter()
        .position(|a| !a.recent.is_empty())
        .unwrap();
    forgetful.agents[i].recent.clear();
    assert_ne!(forgetful.dynamics_hash(), h, "memory is state");
    let mut quiet = w.clone();
    let j = quiet
        .agents
        .iter()
        .position(|a| a.last_signal.is_some())
        .unwrap();
    quiet.agents[j].last_signal = None;
    assert_ne!(quiet.dynamics_hash(), h, "the last signal is state");
}

use proptest::prelude::*;

/// Half random walkers, half foragers, with harm free so the walkers stay
/// alive long enough to give, take, hurt, watch and talk.
fn mixed(seed: u64, social: u16, signal: u16, slots: u8) -> World {
    let mut cfg = WorldConfig {
        seed,
        width: 24,
        height: 24,
        initial_agents: 80,
        patch_count: 3,
        harm_cost: 0,
        memory_slots: slots,
        ..WorldConfig::default()
    };
    cfg.perception.social_radius = social;
    cfg.perception.signal_radius = signal;
    World::generate(cfg, |i| {
        if i % 2 == 0 {
            PolicyKind::Random
        } else {
            PolicyKind::Forager
        }
    })
    .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 32, .. ProptestConfig::default() })]

    /// What agents remember is a function of the history alone: two worlds
    /// with the same config agree entry for entry whether they decide
    /// serially or in parallel, and every ring respects its cap and its
    /// oldest-first order.
    #[test]
    fn memories_are_deterministic_capped_and_ordered(
        seed in any::<u64>(),
        social in 0u16..=4,
        signal in 0u16..=4,
        slots in 0u8..=12,
        ticks in 1u64..=60,
    ) {
        let mut a = mixed(seed, social, signal, slots);
        let mut b = mixed(seed, social, signal, slots);
        for _ in 0..ticks {
            a.step();
            #[cfg(feature = "parallel")]
            b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            b.step();
        }
        prop_assert_eq!(&a.agents, &b.agents);
        prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
        for ag in &a.agents {
            prop_assert!(ag.recent.len() <= slots as usize);
            prop_assert!(ag.recent.windows(2).all(|w| w[0].tick <= w[1].tick), "oldest first");
            prop_assert!(ag.recent.iter().all(|e| e.other != ag.id), "nobody interacts with themselves");
            prop_assert!(ag.recent.iter().all(|e| e.tick < a.tick));
            prop_assert!(ag.recent.iter().filter(|e| e.kind == InteractionKind::Heard).all(|e| (0..8).contains(&e.value)));
            if let Some(s) = ag.last_signal {
                prop_assert!(s.tick < a.tick);
            }
        }
    }
}
