//! Forager decision rules that the golden hash alone cannot pin down: which
//! cell wins and, on ties, that the lottery is uniform (ADR-0011).

mod common;

use common::*;
use simulation_core::rng::{RngStream, Stream};
use simulation_core::{policy, Action, Dir, Intent, Observation, Pos, World, WorldConfig};

fn observe(w: &World, agent_index: usize) -> Observation<'_> {
    Observation {
        agent: &w.agents[agent_index],
        resources: &w.resources,
        bounds: w.bounds(),
        config: &w.config,
    }
}

fn decide_at(w: &World, tick: u64) -> Intent {
    let mut rng = RngStream::new(w.config.seed, tick, w.agents[0].id.0, Stream::Decide);
    policy::decide(&observe(w, 0), &mut rng)
}

#[test]
fn richer_cell_beats_nearer_cell_and_nearer_breaks_food_ties() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(4, 4), 50_000, 0);
    set_food(&mut w, Pos::new(5, 4), 3); // adjacent, poorer
    set_food(&mut w, Pos::new(4, 1), 9); // three cells north, richer
    let it = decide_at(&w, 0);
    assert_eq!(it.action, Action::Move(Dir::North));
    assert_eq!(it.memory.target, Some(Pos::new(4, 1)));

    set_food(&mut w, Pos::new(5, 4), 9); // now equal food, but nearer
    let it = decide_at(&w, 0);
    assert_eq!(it.action, Action::Move(Dir::East));
    assert_eq!(it.memory.target, Some(Pos::new(5, 4)));
}

#[test]
fn tie_break_is_uniform_among_equal_cells() {
    // Four cells at distance 2 with the same food: each must win ~1/4 of
    // the time across ticks. n = 4000, p = 0.25: sd = 27.4, 5 sd = 137.
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(4, 4), 50_000, 0);
    let cells = [
        Pos::new(4, 2),
        Pos::new(6, 4),
        Pos::new(4, 6),
        Pos::new(2, 4),
    ];
    for c in cells {
        set_food(&mut w, c, 5);
    }
    let mut wins = [0u32; 4];
    for tick in 0..4_000u64 {
        let it = decide_at(&w, tick);
        let target = it.memory.target.expect("some cell is chosen");
        wins[cells.iter().position(|&c| c == target).unwrap()] += 1;
    }
    for (c, n) in cells.iter().zip(wins) {
        assert!((863..=1137).contains(&n), "{c:?} won {n} times: {wins:?}");
    }
}

#[test]
fn out_of_range_food_is_invisible() {
    let mut w = empty_world(WorldConfig {
        width: 16,
        height: 16,
        ..cfg()
    });
    add_agent(&mut w, Pos::new(8, 8), 50_000, 0);
    set_food(&mut w, Pos::new(8, 3), 20); // distance 5 > vision_radius 4
    let it = decide_at(&w, 0);
    assert_eq!(it.memory.target, None, "explores instead");
    assert!(matches!(it.action, Action::Move(_)));
}
