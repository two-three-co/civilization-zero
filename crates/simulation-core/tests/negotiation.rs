//! Negotiation messages (ADR-0013 3-4): `Propose` / `Accept` / `Reject` are
//! delivered into the addressee's memory and recorded in the ledger, an
//! acceptance writes `Promised` to both parties, and nothing whatsoever
//! enforces the promise — repayment is an ordinary `Transfer`, breach is a
//! fact the ledger shows.

mod common;

use common::*;
use simulation_core::world::SNAPSHOT_FORMAT;
use simulation_core::{
    Action, AgentId, Dir, Event, EventLog, Failure, Interaction, InteractionKind, Message, Outcome,
    PolicyKind, Pos, SnapshotError, Terms, World, WorldConfig,
};

fn propose(to: u64, give: u8, want: u8, by: u16) -> Action {
    Action::Signal(Message::Propose {
        to: AgentId(to),
        give,
        want,
        by,
    })
}

fn accept(to: u64, offer_tick: u64) -> Action {
    Action::Signal(Message::Accept {
        to: AgentId(to),
        offer_tick,
    })
}

fn reject(to: u64) -> Action {
    Action::Signal(Message::Reject { to: AgentId(to) })
}

fn transfer(to: u64, n: u32) -> Action {
    Action::Transfer { to: AgentId(to), n }
}

fn note(tick: u64, other: u64, kind: InteractionKind, value: i64) -> Interaction {
    Interaction {
        tick,
        other: AgentId(other),
        kind,
        value,
    }
}

const DEAL: Terms = Terms {
    give: 2,
    want: 3,
    by: 10,
};

/// The happy path, scripted with explicit intents (an honest reciprocator):
/// A proposes to B two cells away, B accepts, A walks over and gives, B
/// gives back. Ledger and both memories hold the whole thread in order.
#[test]
fn proposal_acceptance_transfer_and_repayment_line_up_in_the_ledger_and_both_memories() {
    let c = cfg(); // signal_radius 2
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 5); // 0: A, the proposer
    add_agent(&mut w, Pos::new(4, 2), 50_000, 1); // 1: B, two cells east: in earshot, not adjacent
    let mut log = EventLog::new();
    use InteractionKind::*;

    // Tick 0: A proposes "I give 2 now, you return 3 within 10 ticks".
    let r = w.apply(intents(&[propose(1, 2, 3, 10), Action::Rest]));
    log.record(&r).unwrap();
    assert_eq!(r.proposals, 1);
    assert_eq!(r.failures.iter().sum::<u32>(), 0);
    assert_eq!(
        r.events,
        vec![Event::Proposed {
            tick: 0,
            from: AgentId(0),
            to: AgentId(1),
            give: 2,
            want: 3,
            by: 10,
        }]
    );
    assert_eq!(
        w.agents[1].recent,
        vec![note(0, 0, ProposalFrom, DEAL.pack())]
    );
    assert!(
        w.agents[0].recent.is_empty(),
        "proposing writes nothing to the proposer"
    );
    assert_eq!(
        w.agents[0].last_signal, None,
        "a proposal is not speech: last_signal untouched"
    );
    assert_eq!(Terms::unpack(w.agents[1].recent[0].value), DEAL);

    // Tick 1: B accepts the offer it remembers from tick 0.
    let r = w.apply(intents(&[Action::Rest, accept(0, 0)]));
    log.record(&r).unwrap();
    assert_eq!(r.acceptances, 1);
    assert_eq!(
        r.events,
        vec![Event::Accepted {
            tick: 1,
            from: AgentId(1),
            to: AgentId(0),
            offer_tick: 0,
            give: 2,
            want: 3,
            by: 10,
        }]
    );
    assert_eq!(
        w.agents[1].recent,
        vec![
            note(0, 0, ProposalFrom, DEAL.pack()),
            note(1, 0, Promised, 3), // B owes A the `want`
        ]
    );
    assert_eq!(
        w.agents[0].recent,
        vec![
            note(1, 1, Accepted, DEAL.pack()),
            note(1, 1, Promised, 2), // A owes B the `give`
        ]
    );

    // Tick 2: A steps next to B. Tick 3: A hands over the 2 (the `give`).
    w.apply(intents(&[Action::Move(Dir::East), Action::Rest]));
    let r = w.apply(intents(&[transfer(1, 2), Action::Rest]));
    log.record(&r).unwrap();
    assert_eq!(
        r.events,
        vec![Event::Transferred {
            tick: 3,
            from: AgentId(0),
            to: AgentId(1),
            amount: 2
        }]
    );
    // Tick 4: B returns 3 (the `want`), well before the deadline (tick 10).
    let r = w.apply(intents(&[Action::Rest, transfer(0, 3)]));
    log.record(&r).unwrap();
    assert_eq!(
        r.events,
        vec![Event::Transferred {
            tick: 4,
            from: AgentId(1),
            to: AgentId(0),
            amount: 3
        }]
    );
    assert_eq!((w.agents[0].inventory, w.agents[1].inventory), (6, 0));

    assert_eq!(
        w.agents[0].recent,
        vec![
            note(1, 1, Accepted, DEAL.pack()),
            note(1, 1, Promised, 2),
            note(3, 1, GaveTo, 2),
            note(4, 1, ReceivedFrom, 3),
        ]
    );
    assert_eq!(
        w.agents[1].recent,
        vec![
            note(0, 0, ProposalFrom, DEAL.pack()),
            note(1, 0, Promised, 3),
            note(3, 0, ReceivedFrom, 2),
            note(4, 0, GaveTo, 3),
        ]
    );
    // The ledger alone tells the story: proposed, accepted, given, repaid.
    let kinds: Vec<&str> = log
        .events()
        .iter()
        .map(|e| match e {
            Event::Proposed { .. } => "proposed",
            Event::Accepted { .. } => "accepted",
            Event::Transferred { .. } => "transferred",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        vec!["proposed", "accepted", "transferred", "transferred"]
    );
    assert_eq!(
        (
            w.ledger.proposals,
            w.ledger.acceptances,
            w.ledger.rejections,
            w.ledger.signals
        ),
        (1, 1, 0, 0),
        "negotiation is counted apart from Say"
    );
    assert_eq!(
        w.agents[0].energy,
        50_000 - 5 * c.metabolism_cost - c.signal_cost - c.move_cost - c.transfer_cost
    );
    w.check_invariants().unwrap();
}

/// Breach: B accepts, A gives, B never gives back. The deadline passes and
/// the kernel does nothing — no failure, no event, no change to anyone's
/// body or inventory. The breach is visible only by reading the ledger.
#[test]
fn a_broken_promise_changes_nothing_physical() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 5); // 0: A
    add_agent(&mut w, Pos::new(3, 2), 50_000, 0); // 1: B, adjacent
    let mut log = EventLog::new();
    log.record(&w.apply(intents(&[propose(1, 2, 3, 4), Action::Rest])))
        .unwrap();
    log.record(&w.apply(intents(&[Action::Rest, accept(0, 0)])))
        .unwrap();
    log.record(&w.apply(intents(&[transfer(1, 2), Action::Rest])))
        .unwrap();
    assert_eq!((w.agents[0].inventory, w.agents[1].inventory), (3, 2));
    let (energy_a, energy_b) = (w.agents[0].energy, w.agents[1].energy);
    let memories = (w.agents[0].recent.clone(), w.agents[1].recent.clone());

    // Ticks 3..=8: the deadline (tick 0 + 4 = tick 4) comes and goes.
    for _ in 3..=8 {
        let r = w.apply(intents(&[Action::Rest, Action::Rest]));
        log.record(&r).unwrap();
        assert!(r.events.is_empty(), "the kernel never reacts to a breach");
        assert_eq!(r.failures.iter().sum::<u32>(), 0);
    }
    assert!(w.tick > 4, "past the deadline");
    assert_eq!((w.agents[0].inventory, w.agents[1].inventory), (3, 2));
    assert_eq!(
        w.agents[0].energy,
        energy_a - 6 * c.metabolism_cost,
        "metabolism only"
    );
    assert_eq!(w.agents[1].energy, energy_b - 6 * c.metabolism_cost);
    assert_eq!(
        (w.agents[0].recent.clone(), w.agents[1].recent.clone()),
        memories,
        "no memory is written about a breach"
    );
    assert_eq!((w.agents[0].health, w.agents[1].health), (100, 100));
    w.check_invariants().unwrap();

    // What Phase 4 will do with the ledger: the acceptance names the debtor,
    // the amount and the due tick; no repayment appears by then.
    let accepted = log
        .events()
        .iter()
        .find_map(|e| match e {
            Event::Accepted {
                from,
                to,
                offer_tick,
                want,
                by,
                ..
            } => Some((*from, *to, offer_tick + *by as u64, *want)),
            _ => None,
        })
        .unwrap();
    let (debtor, creditor, due, owed) = accepted;
    assert_eq!(
        (debtor, creditor, due, owed),
        (AgentId(1), AgentId(0), 4, 3)
    );
    let repaid: u32 = log
        .range(0..due + 1)
        .iter()
        .filter_map(|e| match e {
            Event::Transferred {
                from, to, amount, ..
            } if *from == debtor && *to == creditor => Some(*amount),
            _ => None,
        })
        .sum();
    assert_eq!(
        repaid, 0,
        "the ledger shows the breach; the kernel does not care"
    );
}

#[test]
fn rejection_is_delivered_and_recorded_without_checking_for_an_offer() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 5); // 0: A
    add_agent(&mut w, Pos::new(2, 4), 50_000, 0); // 1: B, distance 2
    w.apply(intents(&[propose(1, 1, 1, 5), Action::Rest]));
    let r = w.apply(intents(&[Action::Rest, reject(0)]));
    assert_eq!(r.rejections, 1);
    assert_eq!(
        r.events,
        vec![Event::Rejected {
            tick: 1,
            from: AgentId(1),
            to: AgentId(0)
        }]
    );
    assert_eq!(
        w.agents[0].recent,
        vec![note(1, 1, InteractionKind::Rejected, 0)]
    );
    assert_eq!(
        w.agents[1].recent.len(),
        1,
        "rejecting leaves no own memory"
    );
    // A rejection needs no prior proposal: it is just a delivered message.
    let r = w.apply(intents(&[reject(1), Action::Rest]));
    assert_eq!(r.rejections, 1);
    assert_eq!(
        w.agents[1].recent[1],
        note(2, 0, InteractionKind::Rejected, 0)
    );
    assert_eq!(w.ledger.rejections, 2);
    w.check_invariants().unwrap();
}

/// Out of earshot, nobody of that id, oneself, an offer never received, an
/// offer by a wrong tick, no memory to hold an offer, a dead proposer: each
/// is a `Failure`, costs the attempt, and leaves neither event nor memory.
#[test]
fn undeliverable_messages_fail_pay_the_attempt_and_leave_no_trace() {
    let c = WorldConfig {
        signal_cost: 50,
        ..cfg()
    };
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(1, 1), 50_000, 5); // 0
    add_agent(&mut w, Pos::new(4, 1), 50_000, 5); // 1: distance 3 from 0, beyond signal_radius 2
    add_agent(&mut w, Pos::new(2, 1), 50_000, 5); // 2: adjacent to 0, distance 2 from 1
    let r = w.apply(intents(&[
        propose(1, 1, 1, 5),  // 0: too far
        accept(2, 0),         // 1: no such offer (never received one)
        propose(99, 1, 1, 5), // 2: nobody has that id
    ]));
    assert_eq!(r.failures[Failure::OutOfEarshot.index()], 1);
    assert_eq!(r.failures[Failure::NoSuchOffer.index()], 1);
    assert_eq!(r.failures[Failure::NoSuchTarget.index()], 1);
    assert_eq!((r.proposals, r.acceptances, r.rejections), (0, 0, 0));
    assert!(r.events.is_empty(), "undelivered messages are not recorded");
    for a in &w.agents {
        assert!(a.recent.is_empty());
        assert_eq!(
            a.energy,
            50_000 - c.metabolism_cost - c.signal_cost,
            "the attempt is paid for"
        );
    }
    // Addressing oneself is `NoSuchTarget`, like Transfer and Harm.
    let r = w.apply(intents(&[reject(0), Action::Rest, Action::Rest]));
    assert_eq!(r.failures[Failure::NoSuchTarget.index()], 1);

    // Tick 2: 2 proposes to 0 (adjacent) and 0 proposes to 2 (same tick).
    let r = w.apply(intents(&[
        propose(2, 1, 1, 5),
        Action::Rest,
        propose(0, 2, 2, 5),
    ]));
    assert_eq!(r.proposals, 2);
    assert_eq!(w.agents[0].recent.len(), 1);
    assert_eq!(w.agents[2].recent.len(), 1);
    // Tick 3: accepting the right proposer at the wrong tick fails; the
    // wrong proposer fails too — the acceptor must hold exactly that offer.
    let r = w.apply(intents(&[accept(2, 1), Action::Rest, accept(1, 2)]));
    assert_eq!(r.failures[Failure::NoSuchOffer.index()], 2);
    assert_eq!(r.acceptances, 0);
    assert!(r.events.is_empty());
    // Tick 4: the right offer at the right tick succeeds for both.
    let r = w.apply(intents(&[accept(2, 2), Action::Rest, accept(0, 2)]));
    assert_eq!(r.acceptances, 2);
    assert_eq!(w.ledger.acceptances, 2);
    assert_eq!(
        w.agents[0]
            .recent
            .iter()
            .filter(|e| e.kind == InteractionKind::Promised)
            .map(|e| e.value)
            .collect::<Vec<_>>(),
        vec![2, 1],
        "actor order: 0's own acceptance first (it owes the `want` of 2's \
         offer, 2), then 2's acceptance of 0's offer (0 owes its `give`, 1)"
    );
    w.check_invariants().unwrap();

    // With no memory there is nothing to accept: memory capacity is a real
    // limit on what deals can exist (H2).
    let mut m = empty_world(WorldConfig {
        memory_slots: 0,
        ..cfg()
    });
    add_agent(&mut m, Pos::new(1, 1), 50_000, 5);
    add_agent(&mut m, Pos::new(1, 2), 50_000, 5);
    let r = m.apply(intents(&[propose(1, 1, 1, 5), Action::Rest]));
    assert_eq!(r.proposals, 1, "delivered and recorded …");
    assert!(m.agents[1].recent.is_empty(), "… but not remembered");
    let r = m.apply(intents(&[Action::Rest, accept(0, 0)]));
    assert_eq!(r.failures[Failure::NoSuchOffer.index()], 1);

    // A dead proposer: 0 proposes and starves in the same tick (phase 6 runs
    // after the memory phase, so 1 still receives the proposal), then 1's
    // acceptance finds nobody.
    let mut d = empty_world(cfg());
    add_agent(&mut d, Pos::new(1, 1), 0, 0);
    d.agents[0].health = d.config.starvation_damage;
    add_agent(&mut d, Pos::new(1, 2), 50_000, 5);
    let r = d.apply(intents(&[propose(1, 1, 1, 5), Action::Rest]));
    assert_eq!(r.proposals, 1);
    assert_eq!(r.deaths, 1);
    assert_eq!(d.agents.len(), 1);
    assert_eq!(
        d.agents[0].recent,
        vec![note(
            0,
            0,
            InteractionKind::ProposalFrom,
            Terms {
                give: 1,
                want: 1,
                by: 5
            }
            .pack()
        )]
    );
    let r = d.apply(intents(&[accept(0, 0)]));
    assert_eq!(r.failures[Failure::NoSuchTarget.index()], 1);
    assert!(r.events.is_empty());
    d.check_invariants().unwrap();
    // The outcome is reported as such when actions are recorded.
    d.config.record_actions = true;
    let r = d.apply(intents(&[accept(0, 0)]));
    assert_eq!(
        r.events,
        vec![Event::Acted {
            tick: 2,
            agent: AgentId(1),
            action: accept(0, 0),
            outcome: Outcome::Failed(Failure::NoSuchTarget),
        }]
    );
}

/// Messages reach exactly `signal_radius`; the proposer may be beyond
/// adjacency, and the deal is judged at the tick's start positions.
#[test]
fn delivery_uses_signal_radius_at_start_positions() {
    let mut w = empty_world(cfg());
    w.config.perception.signal_radius = 1;
    add_agent(&mut w, Pos::new(2, 2), 50_000, 5); // 0
    add_agent(&mut w, Pos::new(2, 4), 50_000, 0); // 1: distance 2
                                                  // 1 walks towards 0 this tick, but the proposal is judged at S(t).
    let r = w.apply(intents(&[propose(1, 1, 1, 5), Action::Move(Dir::North)]));
    assert_eq!(r.failures[Failure::OutOfEarshot.index()], 1);
    assert_eq!(w.agents[1].pos, Pos::new(2, 3));
    // Now adjacent: delivered.
    let r = w.apply(intents(&[propose(1, 1, 1, 5), Action::Rest]));
    assert_eq!(r.proposals, 1);
    // 0 walks away as 1 accepts: still judged at S(t), so it is delivered.
    let r = w.apply(intents(&[Action::Move(Dir::West), accept(0, 1)]));
    assert_eq!(r.acceptances, 1);
    assert_eq!(w.agents[0].pos, Pos::new(1, 2));
    // From here (distance 2) a second acceptance is out of earshot, although
    // the offer is still remembered.
    let r = w.apply(intents(&[Action::Rest, accept(0, 1)]));
    assert_eq!(r.failures[Failure::OutOfEarshot.index()], 1);
    w.check_invariants().unwrap();
}

#[test]
fn forager_worlds_never_negotiate_and_keep_the_golden_hash() {
    let mut w = World::new(WorldConfig {
        seed: 42,
        ..WorldConfig::default()
    })
    .unwrap();
    w.run(200);
    assert_eq!(w.dynamics_hash(), 0x658e_e873_d186_6501);
    assert_eq!(
        (
            w.ledger.proposals,
            w.ledger.acceptances,
            w.ledger.rejections
        ),
        (0, 0, 0)
    );
}

/// Random walkers now propose, accept and reject, so a random world has
/// every negotiation kind in its memories and ledger — and its snapshots
/// carry them, in both encodings, with the hash seeing them.
#[test]
fn random_worlds_negotiate_and_snapshots_carry_the_deals() {
    let mut w = World::generate(
        WorldConfig {
            seed: 9,
            width: 24,
            height: 24,
            initial_agents: 60,
            patch_count: 2,
            harm_cost: 0,
            ..WorldConfig::default()
        },
        |_| PolicyKind::Random,
    )
    .unwrap();
    let mut log = EventLog::new();
    for _ in 0..120 {
        let r = w.step();
        log.record(&r).unwrap();
        if let Err(e) = w.check_invariants() {
            panic!("tick {}: {e:?}", w.tick);
        }
    }
    let l = &w.ledger;
    assert!(
        l.proposals > 0 && l.acceptances > 0 && l.rejections > 0,
        "proposals {} acceptances {} rejections {}",
        l.proposals,
        l.acceptances,
        l.rejections
    );
    let has = |k: InteractionKind| {
        w.agents
            .iter()
            .any(|a| a.recent.iter().any(|e| e.kind == k))
    };
    use InteractionKind::*;
    assert!(has(ProposalFrom) && has(Accepted) && has(Rejected) && has(Promised));

    // Every acceptance in the ledger answers a proposal that was delivered,
    // with the same terms: the kernel checked the acceptor's memory.
    let events = log.events();
    for (i, e) in events.iter().enumerate() {
        if let Event::Accepted {
            from,
            to,
            offer_tick,
            give,
            want,
            by,
            ..
        } = e
        {
            assert!(
                events[..i].contains(&Event::Proposed {
                    tick: *offer_tick,
                    from: *to,
                    to: *from,
                    give: *give,
                    want: *want,
                    by: *by,
                }),
                "{e:?} answers no delivered proposal"
            );
        }
    }

    // Snapshots: both encodings round-trip, the format is 6, and a promise
    // is part of the dynamics hash.
    let bin = w.to_bytes();
    assert_eq!(World::from_bytes(&bin).unwrap(), w);
    let json = w.to_snapshot();
    assert_eq!(World::from_snapshot(&json).unwrap(), w);
    assert!(std::str::from_utf8(&json)
        .unwrap()
        .starts_with(&format!("{{\"format\":{SNAPSHOT_FORMAT}")));
    assert_eq!(SNAPSHOT_FORMAT, 6);
    println!(
        "random snapshot with deals: {} bytes binary, {} bytes json, {} agents",
        bin.len(),
        json.len(),
        w.agents.len()
    );
    let h = w.dynamics_hash();
    let mut amnesiac = w.clone();
    let a = amnesiac
        .agents
        .iter_mut()
        .find(|a| a.recent.iter().any(|e| e.kind == Promised))
        .unwrap();
    a.recent.retain(|e| e.kind != Promised);
    assert_ne!(amnesiac.dynamics_hash(), h, "a promise is state");

    // A format-5 snapshot is refused by its number alone.
    let mut old: serde_json::Value = serde_json::from_slice(&json).unwrap();
    old["format"] = serde_json::json!(5);
    assert_eq!(
        World::from_snapshot(&serde_json::to_vec(&old).unwrap()).err(),
        Some(SnapshotError::Format(5))
    );
}

use proptest::prelude::*;

/// Random walkers (who negotiate) among foragers, harm free so they live.
fn mixed(seed: u64, signal: u16, slots: u8) -> World {
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

    /// Deals are a function of the history alone: serial and parallel
    /// decision agree memory for memory and event for event, rings respect
    /// their cap, and every negotiation memory is well formed.
    #[test]
    fn negotiation_memories_are_deterministic_capped_and_well_formed(
        seed in any::<u64>(),
        signal in 0u16..=4,
        slots in 0u8..=12,
        ticks in 1u64..=60,
    ) {
        let mut a = mixed(seed, signal, slots);
        let mut b = mixed(seed, signal, slots);
        let (mut log_a, mut log_b) = (EventLog::new(), EventLog::new());
        for _ in 0..ticks {
            log_a.record(&a.step()).unwrap();
            #[cfg(feature = "parallel")]
            log_b.record(&b.step_parallel()).unwrap();
            #[cfg(not(feature = "parallel"))]
            log_b.record(&b.step()).unwrap();
        }
        prop_assert_eq!(&a.agents, &b.agents);
        prop_assert_eq!(&a.ledger, &b.ledger);
        prop_assert_eq!(log_a.events(), log_b.events());
        prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
        use InteractionKind::*;
        for ag in &a.agents {
            prop_assert!(ag.recent.len() <= slots as usize);
            prop_assert!(ag.recent.windows(2).all(|w| w[0].tick <= w[1].tick), "oldest first");
            for e in &ag.recent {
                prop_assert!(e.other != ag.id, "nobody deals with themselves");
                match e.kind {
                    ProposalFrom | Accepted => {
                        let t = Terms::unpack(e.value);
                        prop_assert!(t.give < 3 && t.want < 3 && (1..=16).contains(&t.by), "{t:?}");
                        prop_assert_eq!(t.pack(), e.value, "no bits beyond the terms");
                    }
                    Rejected => prop_assert_eq!(e.value, 0),
                    Promised => prop_assert!((0..3).contains(&e.value), "{}", e.value),
                    _ => {}
                }
            }
        }
        if slots == 0 {
            prop_assert_eq!(a.ledger.acceptances, 0, "nothing can be accepted without memory");
        }
        let delivered = a.ledger.proposals + a.ledger.acceptances + a.ledger.rejections;
        let recorded = log_a.events().iter().filter(|e| matches!(e,
            Event::Proposed { .. } | Event::Accepted { .. } | Event::Rejected { .. })).count() as u64;
        prop_assert_eq!(delivered, recorded, "every delivered message is in the ledger, nothing else is");
    }
}
