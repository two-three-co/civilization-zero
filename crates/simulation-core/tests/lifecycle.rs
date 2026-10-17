//! Birth, ageing, death causes, inheritance and lifecycle events (ADR-0008).

mod common;

use common::*;
use simulation_core::{
    Action, AgentId, DeathCause, Event, PolicyKind, PolicyParams, Pos, WorldConfig,
};

fn life_cfg() -> WorldConfig {
    WorldConfig {
        birth_threshold: Some(80_000),
        child_energy: 40_000,
        max_age: Some(50),
        ..cfg()
    }
}

#[test]
fn birth_splits_energy_and_numbers_children_in_parent_order() {
    let c = life_cfg();
    let mut w = empty_world(c.clone());
    add_agent(&mut w, Pos::new(1, 1), 90_000, 3); // id 0: rich
    add_agent(&mut w, Pos::new(2, 2), 10_000, 0); // id 1: poor
    add_agent_with(
        &mut w,
        Pos::new(3, 3),
        85_000,
        0,
        PolicyKind::Random,
        PolicyParams {
            hunger_permille: 250,
        },
    ); // id 2: rich, custom traits
    let r = w.apply(intents(&[Action::Rest; 3]));

    assert_eq!(r.births, 2);
    assert_eq!(w.ledger.births, 2);
    assert_eq!(w.next_agent_id, 5);
    let ids: Vec<u64> = w.agents.iter().map(|a| a.id.0).collect();
    assert_eq!(
        ids,
        vec![0, 1, 2, 3, 4],
        "children appended in parent-id order"
    );

    let (c3, c4) = (&w.agents[3], &w.agents[4]);
    assert_eq!(c3.pos, Pos::new(1, 1));
    assert_eq!(c4.pos, Pos::new(3, 3));
    assert_eq!((c3.energy, c4.energy), (40_000, 40_000));
    assert_eq!((c3.age, c3.inventory, c3.health), (0, 0, c.max_health));
    assert_eq!(c3.policy, PolicyKind::Forager);
    assert_eq!(c3.params, c.initial_params);
    assert_eq!(c4.policy, PolicyKind::Random, "policy kind is inherited");
    assert_eq!(
        c4.params.hunger_permille, 250,
        "params copied exactly (mutation off)"
    );

    // Parents paid exactly the child's energy plus their own metabolism;
    // inventory stays with the parent.
    assert_eq!(w.agents[0].energy, 90_000 - 40_000 - c.metabolism_cost);
    assert_eq!(w.agents[0].inventory, 3);
    assert_eq!(w.agents[1].energy, 10_000 - c.metabolism_cost);
    assert_eq!(w.agents[2].energy, 85_000 - 40_000 - c.metabolism_cost);

    assert_eq!(
        r.events,
        vec![
            Event::Born {
                tick: 0,
                child: AgentId(3),
                parent: AgentId(0),
                pos: Pos::new(1, 1),
                energy: 40_000
            },
            Event::Born {
                tick: 0,
                child: AgentId(4),
                parent: AgentId(2),
                pos: Pos::new(3, 3),
                energy: 40_000
            },
        ]
    );
    w.check_invariants().unwrap();
}

#[test]
fn newborn_acts_for_the_first_time_in_the_next_tick() {
    let mut w = empty_world(life_cfg());
    add_agent(&mut w, Pos::new(0, 0), 100_000, 0);
    let r0 = w.apply(intents(&[Action::Rest]));
    assert_eq!((r0.births, r0.actions.iter().sum::<u32>()), (1, 1));
    assert_eq!(w.agents[1].age, 0);
    let r1 = w.step();
    assert_eq!(r1.actions.iter().sum::<u32>(), 2, "both decided");
    assert_eq!(w.agents[1].age, 1);
    w.check_invariants().unwrap();
}

#[test]
fn old_age_death_is_recorded_with_cause_and_drops_food() {
    let c = life_cfg(); // max_age 50
    let mut w = empty_world(c.clone());
    let p = Pos::new(4, 4);
    let i = add_agent(&mut w, p, 30_000, 6);
    w.agents[i].age = 49;
    let r = w.apply(intents(&[Action::Rest]));
    assert_eq!(r.deaths, 1);
    assert_eq!(r.deaths_by_cause, [0, 1, 0]);
    assert_eq!(
        r.events,
        vec![Event::Died {
            tick: 0,
            agent: AgentId(0),
            cause: DeathCause::OldAge,
            pos: p,
            age: 50,
            energy_written_off: 30_000 - c.metabolism_cost,
            food_dropped: 6,
        }]
    );
    assert_eq!(w.resources.food_at(p), 6);
    assert!(w.agents.is_empty());
    assert_eq!(w.ledger.deaths_by_cause, [0, 1, 0]);
    assert_eq!(w.ledger.energy_written_off, 30_000 - c.metabolism_cost);
    w.check_invariants().unwrap();
}

#[test]
fn starvation_takes_precedence_over_old_age() {
    let mut w = empty_world(WorldConfig {
        starvation_damage: 100,
        ..life_cfg()
    });
    let i = add_agent(&mut w, Pos::new(0, 0), 0, 0);
    w.agents[i].age = 49;
    let r = w.apply(intents(&[Action::Rest]));
    assert_eq!(r.deaths_by_cause, [1, 0, 0]);
    assert!(matches!(
        r.events[0],
        Event::Died {
            cause: DeathCause::Starvation,
            ..
        }
    ));
}

#[test]
fn the_dead_do_not_reproduce() {
    // Energy above the threshold, but max_age is reached in the same tick:
    // Death runs before Birth.
    let mut w = empty_world(life_cfg());
    let i = add_agent(&mut w, Pos::new(0, 0), 100_000, 0);
    w.agents[i].age = 49;
    let r = w.apply(intents(&[Action::Rest]));
    assert_eq!((r.deaths, r.births), (1, 0));
    assert!(w.agents.is_empty());
    assert_eq!(w.next_agent_id, 1, "no id was handed out");
    w.check_invariants().unwrap();
}

#[test]
fn birth_is_off_when_threshold_is_none() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(0, 0), 100_000, 0);
    let r = w.apply(intents(&[Action::Rest]));
    assert_eq!(r.births, 0);
    assert_eq!(w.agents.len(), 1);
}

#[test]
fn mutation_off_copies_exactly_and_on_perturbs_deterministically() {
    let base = WorldConfig {
        mutation_step_permille: 100,
        ..life_cfg()
    };
    let run = |mutation_enabled: bool, seed: u64| {
        let mut w = empty_world(WorldConfig {
            seed,
            mutation_enabled,
            ..base.clone()
        });
        for k in 0..20u16 {
            add_agent(&mut w, Pos::new(k % 8, k / 8), 100_000, 0);
        }
        w.apply(intents(&[Action::Rest; 20]));
        w.agents[20..]
            .iter()
            .map(|a| a.params.hunger_permille)
            .collect::<Vec<_>>()
    };
    assert!(run(false, 1).iter().all(|&h| h == 500));
    let on = run(true, 1);
    assert_eq!(on.len(), 20);
    assert_eq!(
        on,
        run(true, 1),
        "mutation is a function of (seed, tick, child id)"
    );
    assert!(on.iter().any(|&h| h != 500), "some child should differ");
    assert!(on.iter().all(|&h| (400..=600).contains(&h)), "{on:?}");
    assert_ne!(on, run(true, 2));
}

#[test]
fn invalid_lifecycle_config_is_rejected() {
    let bad = [
        WorldConfig {
            birth_threshold: Some(10),
            child_energy: 20,
            ..WorldConfig::default()
        },
        WorldConfig {
            birth_threshold: Some(200_000),
            ..WorldConfig::default()
        },
        WorldConfig {
            max_age: Some(0),
            ..WorldConfig::default()
        },
        WorldConfig {
            mutation_step_permille: 1001,
            ..WorldConfig::default()
        },
        WorldConfig {
            initial_params: PolicyParams {
                hunger_permille: 1001,
            },
            ..WorldConfig::default()
        },
    ];
    for c in bad {
        assert!(c.validate().is_err(), "{c:?}");
    }
    assert!(WorldConfig::default().validate().is_ok());
}

#[test]
fn lifecycle_events_serialise_as_tagged_records() {
    let e = Event::Born {
        tick: 7,
        child: AgentId(12),
        parent: AgentId(3),
        pos: Pos::new(1, 2),
        energy: 40_000,
    };
    let json = serde_json::to_string(&e).unwrap();
    assert_eq!(
        json,
        r#"{"kind":"born","tick":7,"child":12,"parent":3,"pos":{"x":1,"y":2},"energy":40000}"#
    );
    let back: Event = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e);
}
