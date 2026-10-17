//! Other-agent observation (ADR-0013 3-1): neighbours come back nearest
//! first and by id within a distance, never include the observer, respect
//! the perception radius and its information limits, and agree with a
//! brute-force scan for any world.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::{Agent, AgentId, Perception, PolicyKind, Pos, World, WorldConfig};

fn neighbors_of(w: &World, i: usize) -> Vec<(u32, u64)> {
    let index = w.spatial_index();
    w.observation(&w.agents[i], &index)
        .neighbors()
        .iter()
        .map(|n| (n.distance, n.id.0))
        .collect()
}

#[test]
fn nearest_first_then_by_id_and_never_self() {
    let mut w = empty_world(WorldConfig {
        perception: Perception {
            social_radius: 2,
            signal_radius: 2,
            ..Perception::default()
        },
        ..cfg()
    });
    add_agent(&mut w, Pos::new(4, 4), 1_000, 0); // 0: observer
    add_agent(&mut w, Pos::new(4, 4), 1_000, 0); // 1: same cell, distance 0
    add_agent(&mut w, Pos::new(6, 4), 1_000, 0); // 2: distance 2
    add_agent(&mut w, Pos::new(4, 3), 1_000, 0); // 3: distance 1
    add_agent(&mut w, Pos::new(5, 5), 1_000, 0); // 4: distance 2
    add_agent(&mut w, Pos::new(7, 4), 1_000, 0); // 5: distance 3, out of radius
    add_agent(&mut w, Pos::new(3, 4), 1_000, 0); // 6: distance 1
    assert_eq!(
        neighbors_of(&w, 0),
        vec![(0, 1), (1, 3), (1, 6), (2, 2), (2, 4)]
    );
    // From agent 2's point of view (distance 2 from 0, 1 and 4; 3 from 3 and 6).
    assert_eq!(neighbors_of(&w, 2), vec![(1, 5), (2, 0), (2, 1), (2, 4)]);
}

#[test]
fn radius_zero_sees_only_the_same_cell_and_the_grid_edge_is_clipped() {
    let mut w = empty_world(WorldConfig {
        perception: Perception {
            social_radius: 0,
            signal_radius: 2,
            ..Perception::default()
        },
        ..cfg()
    });
    add_agent(&mut w, Pos::new(0, 0), 1_000, 0);
    add_agent(&mut w, Pos::new(0, 0), 1_000, 0);
    add_agent(&mut w, Pos::new(1, 0), 1_000, 0);
    assert_eq!(neighbors_of(&w, 0), vec![(0, 1)]);
    w.config.perception.social_radius = 5; // corner: the diamond is clipped, no panic
    assert_eq!(neighbors_of(&w, 0), vec![(0, 1), (1, 2)]);
}

#[test]
fn perception_limits_what_is_visible() {
    let mut w = empty_world(cfg());
    add_agent(&mut w, Pos::new(2, 2), 1_000, 0);
    let params = w.config.initial_params.clone();
    add_agent_with(&mut w, Pos::new(3, 2), 7_000, 4, PolicyKind::Random, params);
    let look = |w: &World| {
        let index = w.spatial_index();
        w.observation(&w.agents[0], &index).neighbors()[0]
    };
    let n = look(&w); // defaults: inventory yes, policy and energy no
    assert_eq!((n.id, n.pos, n.distance), (AgentId(1), Pos::new(3, 2), 1));
    assert_eq!(n.inventory, Some(4));
    assert_eq!((n.policy, n.energy), (None, None));

    w.config.perception = Perception {
        social_radius: 2,
        signal_radius: 2,
        see_policy: true,
        see_inventory: false,
        see_energy: true,
    };
    let n = look(&w);
    assert_eq!(n.policy, Some(PolicyKind::Random));
    assert_eq!(n.energy, Some(7_000));
    assert_eq!(n.inventory, None);
}

#[test]
fn perception_is_part_of_the_config_and_older_configs_still_load() {
    let w = World::new(WorldConfig {
        seed: 1,
        width: 8,
        height: 8,
        initial_agents: 3,
        patch_count: 1,
        ..WorldConfig::default()
    })
    .unwrap();
    let json = serde_json::to_string(&w.config).unwrap();
    assert!(json.contains("\"perception\""), "{json}");
    // A config written before the field existed loads with the default.
    let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
    v.as_object_mut().unwrap().remove("perception");
    let back: WorldConfig = serde_json::from_value(v).unwrap();
    assert_eq!(back, w.config);
    assert_eq!(back.perception, Perception::default());
    // Perception is part of the state hash but not of the dynamics hash.
    let mut custom = w.clone();
    custom.config.perception.social_radius = 3;
    assert_ne!(w.state_hash(), custom.state_hash());
    assert_eq!(w.dynamics_hash(), custom.dynamics_hash());
    assert_eq!(World::from_snapshot(&w.to_snapshot()).unwrap(), w);
    assert_eq!(World::from_bytes(&w.to_bytes()).unwrap(), w);
    assert!(WorldConfig {
        perception: Perception {
            social_radius: 33,
            signal_radius: 2,
            ..Perception::default()
        },
        ..WorldConfig::default()
    }
    .validate()
    .is_err());
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, .. ProptestConfig::default() })]

    /// The index agrees with a brute-force scan for any placement, radius
    /// and grid shape, including many agents on one cell.
    #[test]
    fn index_matches_brute_force(
        width in 1u16..12,
        height in 1u16..12,
        n in 0usize..40,
        radius in 0u16..6,
        seed in any::<u64>(),
    ) {
        let mut w = empty_world(WorldConfig {
            width, height,
            perception: Perception { social_radius: radius, ..Perception::default() },
            ..cfg()
        });
        let mut x = seed;
        for _ in 0..n {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let px = ((x >> 33) % width as u64) as u16;
            let py = ((x >> 13) % height as u64) as u16;
            add_agent(&mut w, Pos::new(px, py), 1_000, 0);
        }
        let index = w.spatial_index();
        prop_assert_eq!(index.len(), w.agents.len());
        for (i, me) in w.agents.iter().enumerate() {
            let got: Vec<(u32, u64)> = w.observation(me, &index).neighbors().iter().map(|nb| (nb.distance, nb.id.0)).collect();
            let mut expected: Vec<(u32, u64)> = w.agents.iter()
                .filter(|a: &&Agent| a.id != me.id)
                .map(|a| (me.pos.manhattan(a.pos), a.id.0))
                .filter(|&(d, _)| d <= radius as u32)
                .collect();
            expected.sort_unstable();
            prop_assert_eq!(got, expected, "agent {}", i);
        }
    }
}
