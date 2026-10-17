//! Scenario tests for action semantics and conflict resolution.

mod common;

use common::*;
use simulation_core::{Action, Dir, Failure, Pos, WorldConfig};

#[test]
fn contended_gather_never_double_allocates() {
    let mut w = empty_world(cfg());
    let p = Pos::new(3, 3);
    set_food(&mut w, p, 3); // gather_amount = 2, so only 1.5 agents can be served
    for _ in 0..5 {
        add_agent(&mut w, p, 50_000, 0);
    }
    let r = w.apply(intents(&[Action::Gather; 5]));
    assert_eq!(r.food_gathered, 3);
    assert_eq!(w.resources.food_at(p), 0);
    assert_eq!(w.total_inventory(), 3);
    assert_eq!(r.failures[Failure::LostContention.index()], 3);
    w.check_invariants().unwrap();
}

#[test]
fn contention_winner_depends_on_seed_not_on_vector_order() {
    // Same seed, same agents => same winner, regardless of how often we run.
    let run = |seed| {
        let mut w = empty_world(WorldConfig { seed, ..cfg() });
        let p = Pos::new(1, 1);
        set_food(&mut w, p, 2);
        for _ in 0..4 {
            add_agent(&mut w, p, 50_000, 0);
        }
        w.apply(intents(&[Action::Gather; 4]));
        w.agents.iter().position(|a| a.inventory == 2).unwrap()
    };
    assert_eq!(run(1), run(1));
    let winners: std::collections::BTreeSet<_> = (0..32).map(run).collect();
    assert!(
        winners.len() > 1,
        "lottery should not always favour the same slot"
    );
}

#[test]
fn gather_on_empty_cell_is_no_resource_not_contention() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(0, 0), 50_000, 0);
    let r = w.apply(intents(&[Action::Gather]));
    assert_eq!(r.failures[Failure::NoResource.index()], 1);
    // Gather cost is paid even on failure.
    assert_eq!(
        w.agents[0].energy,
        50_000 - w.config.metabolism_cost - w.config.gather_cost
    );
}

#[test]
fn move_into_wall_fails_and_costs_only_metabolism() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(0, 0), 50_000, 0);
    let r = w.apply(intents(&[Action::Move(Dir::West)]));
    assert_eq!(r.failures[Failure::OutOfBounds.index()], 1);
    assert_eq!(w.agents[0].pos, Pos::new(0, 0));
    assert_eq!(w.agents[0].energy, 50_000 - w.config.metabolism_cost);
}

#[test]
fn move_succeeds_and_costs_move() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(0, 0), 50_000, 0);
    w.apply(intents(&[Action::Move(Dir::East)]));
    assert_eq!(w.agents[0].pos, Pos::new(1, 0));
    assert_eq!(
        w.agents[0].energy,
        50_000 - w.config.metabolism_cost - w.config.move_cost
    );
}

#[test]
fn gather_uses_pre_move_positions() {
    // A gathers at p while B moves onto p in the same tick: B cannot gather
    // this tick, and A's gather is resolved at p.
    let mut w = empty_world(cfg());
    let p = Pos::new(2, 2);
    set_food(&mut w, p, 10);
    add_agent(&mut w, p, 50_000, 0);
    add_agent(&mut w, Pos::new(1, 2), 50_000, 0);
    w.apply(intents(&[Action::Gather, Action::Move(Dir::East)]));
    assert_eq!(w.agents[0].inventory, 2);
    assert_eq!(w.agents[1].pos, p);
    assert_eq!(w.agents[1].inventory, 0);
}

#[test]
fn consume_converts_food_and_respects_energy_cap() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    // Room for exactly one food unit.
    let start = c.max_energy - c.energy_per_food - 1;
    add_agent(&mut w, Pos::new(0, 0), start, 5);
    w.apply(intents(&[Action::Consume(5)]));
    assert_eq!(w.agents[0].inventory, 4);
    assert_eq!(
        w.agents[0].energy,
        start + c.energy_per_food - c.metabolism_cost
    );
    w.check_invariants().unwrap();
}

#[test]
fn consume_with_no_room_fails_energy_full() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(0, 0), c.max_energy, 1);
    let r = w.apply(intents(&[Action::Consume(1)]));
    assert_eq!(r.failures[Failure::EnergyFull.index()], 1);
    assert_eq!(w.agents[0].inventory, 1);
}

#[test]
fn consume_with_empty_inventory_fails() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(0, 0), 10_000, 0);
    let r = w.apply(intents(&[Action::Consume(1)]));
    assert_eq!(r.failures[Failure::EmptyInventory.index()], 1);
}

#[test]
fn starvation_kills_and_drops_inventory_on_cell() {
    let c = WorldConfig {
        starvation_damage: 60,
        ..cfg()
    };
    let mut w = empty_world(c.clone());
    let p = Pos::new(4, 4);
    add_agent(&mut w, p, 0, 0);
    add_agent(&mut w, p, 0, 7);
    let r1 = w.apply(intents(&[Action::Rest, Action::Rest]));
    assert_eq!(r1.deaths, 0); // health 100 -> 40
    let r2 = w.apply(intents(&[Action::Rest, Action::Rest]));
    assert_eq!(r2.deaths, 2);
    assert!(w.agents.is_empty());
    assert_eq!(w.resources.food_at(p), 7);
    w.check_invariants().unwrap();
}

#[test]
fn rest_heals_only_with_energy() {
    let c = cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(0, 0), 50_000, 0);
    w.agents[0].health = 50;
    w.apply(intents(&[Action::Rest]));
    assert_eq!(w.agents[0].health, 50 + c.rest_heal);
}

#[test]
fn regrowth_respects_capacity() {
    let c = WorldConfig {
        regrow_interval: 1,
        regrow_amount: 5,
        ..cfg()
    };
    let mut w = empty_world(c);
    let p = Pos::new(0, 0);
    set_food(&mut w, p, 3);
    w.resources.food[0] = 1;
    w.ledger.food_initial -= 2;
    w.apply(vec![]);
    assert_eq!(w.resources.food_at(p), 3);
    w.check_invariants().unwrap();
}
