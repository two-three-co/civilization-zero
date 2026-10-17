//! Helpers shared by the integration tests: hand-built worlds driven by
//! explicit intents (bypassing policies).
#![allow(dead_code)]

use simulation_core::{
    Action, Agent, AgentId, Intent, Memory, PolicyKind, PolicyParams, Pos, World, WorldConfig,
};

/// Small grid, no regrowth, no life cycle. Each test switches on what it needs.
pub fn cfg() -> WorldConfig {
    WorldConfig {
        width: 8,
        height: 8,
        regrow_interval: 1_000_000,
        birth_threshold: None,
        max_age: None,
        ..WorldConfig::default()
    }
}

pub fn empty_world(cfg: WorldConfig) -> World {
    World::new(WorldConfig {
        initial_agents: 0,
        patch_count: 0,
        ..cfg
    })
    .unwrap()
}

pub fn add_agent(w: &mut World, pos: Pos, energy: i64, inventory: u32) -> usize {
    let params = w.config.initial_params.clone();
    add_agent_with(w, pos, energy, inventory, PolicyKind::Forager, params)
}

/// Appends an agent with the next id and keeps the ledger consistent. The
/// params are refit to the policy (a learning kind gets its blank table).
pub fn add_agent_with(
    w: &mut World,
    pos: Pos,
    energy: i64,
    inventory: u32,
    policy: PolicyKind,
    params: PolicyParams,
) -> usize {
    let params = params.refit(policy, &w.config);
    let id = AgentId(w.next_agent_id);
    w.next_agent_id += 1;
    w.agents.push(Agent {
        id,
        pos,
        energy,
        health: w.config.max_health,
        inventory,
        age: 0,
        policy,
        params,
        memory: Memory::default(),
        last_signal: None,
        recent: Vec::new(),
        last_step: None,
    });
    w.ledger.energy_initial += energy;
    w.ledger.food_initial += inventory as u64;
    w.agents.len() - 1
}

pub fn set_food(w: &mut World, p: Pos, food: u32) {
    let i = w.bounds().index(p);
    w.ledger.food_initial = w.ledger.food_initial - w.resources.food[i] as u64 + food as u64;
    w.resources.food[i] = food;
    w.resources.capacity[i] = food;
}

pub fn intents(actions: &[Action]) -> Vec<Intent> {
    actions
        .iter()
        .map(|&action| Intent {
            action,
            memory: Memory::default(),
            state: None,
        })
        .collect()
}
