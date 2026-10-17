//! Social actions (ADR-0013 3-2): Transfer, Store, Harm, the Violence death
//! cause, their events and the optional `Acted` record.

mod common;

use common::*;
use simulation_core::{
    Action, AgentId, DeathCause, Dir, Event, Failure, Outcome, Pos, World, WorldConfig,
};

fn transfer(to: u64, n: u32) -> Action {
    Action::Transfer { to: AgentId(to), n }
}

fn harm(target: u64) -> Action {
    Action::Harm {
        target: AgentId(target),
    }
}

#[test]
fn transfer_moves_food_to_an_adjacent_agent_and_is_recorded() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 5); // 0: donor
    add_agent(&mut w, Pos::new(3, 2), 50_000, 7); // 1: adjacent, room for 3
    let r = w.apply(intents(&[transfer(1, 4), Action::Rest]));
    assert_eq!((w.agents[0].inventory, w.agents[1].inventory), (2, 10));
    assert_eq!((r.transfers, r.food_transferred), (1, 3));
    assert_eq!(
        r.events[0],
        Event::Transferred {
            tick: 0,
            from: AgentId(0),
            to: AgentId(1),
            amount: 3
        }
    );
    assert_eq!(
        w.agents[0].energy,
        50_000 - c.metabolism_cost - c.transfer_cost
    );
    assert_eq!(w.ledger.food_transferred, 3);
    w.check_invariants().unwrap();
    // Partial because the target's room, not the donor's stock, was the limit.
    let r2 = w.apply(intents(&[transfer(1, 1), Action::Consume(1)]));
    assert_eq!(r2.failures[Failure::TargetFull.index()], 0);
    assert_eq!(w.agents[1].inventory, 10); // consumed one, received one
}

#[test]
fn transfer_outcomes_cover_every_failure_and_cost_is_paid_on_attempt() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0); // 0: empty-handed
    add_agent(&mut w, Pos::new(2, 2), 50_000, 3); // 1: same cell as 0
    add_agent(&mut w, Pos::new(5, 5), 50_000, 3); // 2: far away
    add_agent(&mut w, Pos::new(2, 3), 50_000, 3); // 3: adjacent to 0 and 1, full
    w.agents[3].inventory = c.inventory_capacity;
    w.ledger.food_initial += (c.inventory_capacity - 3) as u64;
    let r = w.apply(intents(&[
        transfer(1, 1),  // 0: nothing to give
        transfer(3, 1),  // 1: target full
        transfer(0, 1),  // 2: not adjacent
        transfer(99, 1), // 3: no such agent
    ]));
    assert_eq!(r.failures[Failure::EmptyInventory.index()], 1);
    assert_eq!(r.failures[Failure::TargetFull.index()], 1);
    assert_eq!(r.failures[Failure::NotAdjacent.index()], 1);
    assert_eq!(r.failures[Failure::NoSuchTarget.index()], 1);
    assert_eq!(r.transfers, 0);
    assert!(r.events.is_empty(), "nothing moved, nothing recorded");
    for a in &w.agents {
        assert_eq!(
            a.energy,
            50_000 - c.metabolism_cost - c.transfer_cost,
            "attempt cost"
        );
    }
    // Giving to yourself is not a transfer.
    let r = w.apply(intents(&[
        transfer(0, 1),
        Action::Rest,
        Action::Rest,
        Action::Rest,
    ]));
    assert_eq!(r.failures[Failure::NoSuchTarget.index()], 1);
    w.check_invariants().unwrap();
}

#[test]
fn adjacency_is_judged_at_the_start_positions_and_transfers_resolve_in_id_order() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 2); // 0 gives to 2
    add_agent(&mut w, Pos::new(3, 3), 50_000, 2); // 1 gives to 2
    add_agent(&mut w, Pos::new(3, 2), 50_000, 9); // 2: room for 1 only, walks away this tick
    let r = w.apply(intents(&[
        transfer(2, 2),
        transfer(2, 2),
        Action::Move(Dir::East),
    ]));
    // 2 moved to (4, 2) — no longer adjacent — but the transfers used S(t).
    assert_eq!(w.agents[2].pos, Pos::new(4, 2));
    assert_eq!(w.agents[2].inventory, 10);
    assert_eq!(
        w.agents[0].inventory, 1,
        "lower id served first, got the only free slot"
    );
    assert_eq!(w.agents[1].inventory, 2, "nothing left to receive");
    assert_eq!(r.failures[Failure::TargetFull.index()], 1);
    assert_eq!(r.events.len(), 1);
    w.check_invariants().unwrap();
    // Swapping in the same tick: 0 -> 1 first (id order), then 1 -> 0.
    let mut w = empty_world(c);
    add_agent(&mut w, Pos::new(1, 1), 50_000, 1);
    add_agent(&mut w, Pos::new(1, 2), 50_000, 1);
    w.apply(intents(&[transfer(1, 1), transfer(0, 1)]));
    assert_eq!((w.agents[0].inventory, w.agents[1].inventory), (1, 1));
    assert_eq!(w.ledger.transfers, 2);
    w.check_invariants().unwrap();
}

#[test]
fn store_drops_food_on_the_own_cell_beyond_capacity() {
    let mut w = empty_world(cfg());
    let p = Pos::new(3, 3);
    set_food(&mut w, p, 2); // capacity 2
    add_agent(&mut w, p, 50_000, 8);
    let r = w.apply(intents(&[Action::Store(5)]));
    assert_eq!(w.resources.food_at(p), 7, "no capacity cap on stores");
    assert_eq!(w.agents[0].inventory, 3);
    assert_eq!(r.food_stored, 5);
    assert_eq!(
        w.agents[0].energy,
        50_000 - w.config.metabolism_cost,
        "storing is free"
    );
    w.check_invariants().unwrap();
    w.apply(intents(&[Action::Store(9)]));
    assert_eq!(w.agents[0].inventory, 0);
    let r = w.apply(intents(&[Action::Store(1)]));
    assert_eq!(r.failures[Failure::EmptyInventory.index()], 1);
    // Stored food can be gathered again (by anyone).
    let r = w.apply(intents(&[Action::Gather]));
    assert_eq!(r.food_gathered, 2);
    w.check_invariants().unwrap();
}

#[test]
fn harm_stacks_costs_energy_and_a_death_this_tick_is_violence() {
    let c = WorldConfig {
        harm_damage: 60,
        ..cfg()
    };
    let mut w = empty_world(c.clone());
    let p = Pos::new(4, 4);
    add_agent(&mut w, Pos::new(4, 3), 50_000, 0); // 0: attacker
    add_agent(&mut w, Pos::new(5, 4), 50_000, 0); // 1: attacker
    add_agent(&mut w, p, 0, 6); // 2: victim, also starving, carries food
    let r = w.apply(intents(&[harm(2), harm(2), Action::Rest]));
    assert_eq!(r.harms, 2);
    assert_eq!(r.deaths, 1);
    assert_eq!(r.deaths_by_cause[DeathCause::Violence.index()], 1);
    assert_eq!(
        r.deaths_by_cause[DeathCause::Starvation.index()],
        0,
        "violence takes precedence"
    );
    assert_eq!(w.agents.len(), 2);
    assert_eq!(
        w.resources.food_at(p),
        6,
        "the victim's food drops on the cell"
    );
    assert!(matches!(
        r.events[0],
        Event::Harmed {
            tick: 0,
            by: AgentId(0),
            target: AgentId(2),
            damage: 60
        }
    ));
    assert!(matches!(r.events[1], Event::Harmed { by: AgentId(1), .. }));
    assert!(matches!(
        r.events[2],
        Event::Died {
            cause: DeathCause::Violence,
            agent: AgentId(2),
            ..
        }
    ));
    for a in &w.agents {
        assert_eq!(a.energy, 50_000 - c.metabolism_cost - c.harm_cost);
    }
    assert_eq!(w.ledger.harms, 2);
    assert_eq!(w.ledger.deaths_by_cause, [0, 0, 1]);
    w.check_invariants().unwrap();
}

#[test]
fn harm_short_of_death_leaves_health_and_fails_when_not_adjacent() {
    let c = cfg(); // harm_damage 20
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(1, 1), 50_000, 0);
    add_agent(&mut w, Pos::new(1, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(6, 6), 50_000, 0);
    let r = w.apply(intents(&[harm(1), Action::Rest, harm(0)]));
    // Harmed by 20, then its own Rest in the same tick healed 1.
    assert_eq!(w.agents[1].health, c.max_health - 20 + 1);
    assert_eq!(r.failures[Failure::NotAdjacent.index()], 1);
    assert_eq!(
        w.agents[2].energy,
        50_000 - c.metabolism_cost - c.harm_cost,
        "attempt cost"
    );
    assert_eq!(r.deaths, 0);
    // Resting heals one point per tick (one tick here), so the wound is
    // slow to close.
    w.apply(intents(&[Action::Rest; 3]));
    assert_eq!(w.agents[1].health, c.max_health - 20 + 2);
    w.check_invariants().unwrap();
}

#[test]
fn acted_is_recorded_for_every_agent_only_when_enabled() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(0, 1), 50_000, 2);
    add_agent(&mut w, Pos::new(1, 1), 50_000, 0);
    // Tick 0: a transfer, and agent 1 steps west onto agent 0's cell.
    let r = w.apply(intents(&[transfer(1, 1), Action::Move(Dir::West)]));
    assert!(!r.events.iter().any(|e| matches!(e, Event::Acted { .. })));
    assert_eq!(w.agents[1].pos, Pos::new(0, 1));

    w.config.record_actions = true;
    let r = w.apply(intents(&[transfer(1, 1), Action::Move(Dir::West)]));
    let acted: Vec<&Event> = r
        .events
        .iter()
        .filter(|e| matches!(e, Event::Acted { .. }))
        .collect();
    assert_eq!(acted.len(), 2);
    assert_eq!(
        *acted[0],
        Event::Acted {
            tick: 1,
            agent: AgentId(0),
            action: transfer(1, 1),
            outcome: Outcome::Ok
        }
    );
    assert_eq!(
        *acted[1],
        Event::Acted {
            tick: 1,
            agent: AgentId(1),
            action: Action::Move(Dir::West),
            outcome: Outcome::Failed(Failure::OutOfBounds)
        }
    );
    // The record round-trips through the ledger's JSON form.
    let json = serde_json::to_string(acted[0]).unwrap();
    assert!(
        json.contains(r#""kind":"acted""#) && json.contains(r#""Transfer":{"to":1,"n":1}"#),
        "{json}"
    );
    assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), *acted[0]);
}

#[test]
fn random_worlds_with_social_actions_keep_every_invariant() {
    let mut w = World::generate(
        WorldConfig {
            seed: 5,
            width: 32,
            height: 32,
            initial_agents: 120,
            patch_count: 3,
            record_actions: true,
            ..WorldConfig::default()
        },
        |_| simulation_core::PolicyKind::Random,
    )
    .unwrap();
    let (mut transfers, mut harms, mut acted) = (0u64, 0u64, 0u64);
    for _ in 0..400 {
        let r = w.step();
        transfers += r.transfers as u64;
        harms += r.harms as u64;
        acted += r
            .events
            .iter()
            .filter(|e| matches!(e, Event::Acted { .. }))
            .count() as u64;
        if let Err(e) = w.check_invariants() {
            panic!("tick {}: {e:?}", w.tick);
        }
    }
    assert!(
        transfers > 0 && harms > 0,
        "random walkers exercised the social actions"
    );
    assert!(acted > 0);
    assert!(
        w.ledger.deaths_by_cause[DeathCause::Violence.index()] > 0,
        "some died of violence"
    );
}
