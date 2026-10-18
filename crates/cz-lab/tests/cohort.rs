//! The per-agent tracking (ADR-0015 D4, 5-2): the table agrees with the
//! ledger, the reports and the living in a generated institution world,
//! and the eleven cohort columns take the values the definitions give in
//! a scripted world where who encroached, who was sanctioned and who died
//! — and when — is known.

use cz_lab::cohort::{CohortMetrics, CohortTally, Counts};
use cz_lab::{policy_mix, AGENTS_COLUMNS};
use simulation_core::{
    Action, Agent, AgentId, DeathCause, Dir, Event, Intent, Intervention, Memory, Norm, NormKind,
    PolicyKind, PolicyParams, Pos, Sanction, TickReport, World, WorldConfig,
};

fn rule() -> Intervention {
    Intervention::Enact {
        norm: Norm {
            kind: NormKind::Encroach,
            standing_ticks: 32,
            sanction: Sanction::HARM,
            except_sanction: false,
        },
    }
}

#[test]
fn the_table_agrees_with_the_ledger_the_reports_and_the_living() {
    let mut w = World::generate(
        WorldConfig {
            seed: 7,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            ..WorldConfig::institution_preset()
        },
        policy_mix("institution").unwrap(),
    )
    .unwrap();
    w.intervene(rule()).unwrap();
    let mut t = CohortTally::new(&w, (200, 400));
    assert_eq!(t.len(), 150);
    assert_eq!(t.midpoint(), 300);
    let (mut alive_at_200, mut learners_at_200) = (0u64, 0u64);
    let (mut gathers_by_kind, mut enc_by_kind) =
        ([0u64; PolicyKind::COUNT], [0u64; PolicyKind::COUNT]);
    let mut sanctions = 0u64;
    let mut win = Counts::default();
    for _ in 0..400 {
        if w.tick == 200 {
            alive_at_200 = w.agents.len() as u64;
            learners_at_200 = w
                .agents
                .iter()
                .filter(|a| a.policy == PolicyKind::Learner)
                .count() as u64;
        }
        let r = w.step();
        t.observe(&r, &w);
        for k in 0..PolicyKind::COUNT {
            gathers_by_kind[k] += u64::from(r.gathers[k]);
            enc_by_kind[k] += u64::from(r.encroachments[k]);
        }
        sanctions += r.sanctions.iter().map(|&v| u64::from(v)).sum::<u64>();
        if r.tick >= 200 {
            win.gathers += r.gatherers.len() as u64;
            win.encroachments += r.encroachers.len() as u64;
        }
    }
    assert_eq!(t.len(), w.next_agent_id as usize, "founders + births");
    assert_eq!(
        t.iter().filter(|a| a.born > 0).count() as u64,
        w.ledger.births
    );
    assert_eq!(
        t.iter().filter(|a| !a.alive()).count() as u64,
        w.ledger.deaths
    );
    let living: Vec<_> = t.iter().filter(|a| a.alive()).map(|a| a.id).collect();
    assert_eq!(
        living,
        w.agents.iter().map(|a| a.id).collect::<Vec<_>>(),
        "the living are exactly the untallied deaths, in id order"
    );
    for a in &w.agents {
        assert_eq!(t.get(a.id).unwrap().policy, a.policy);
    }
    // Gathers and encroachments: summed over the agents they are the
    // ledger's, by kind the reports'.
    let (mut g, mut e) = ([0u64; PolicyKind::COUNT], [0u64; PolicyKind::COUNT]);
    for a in t.iter() {
        g[a.policy.index()] += a.run.gathers;
        e[a.policy.index()] += a.run.encroachments;
    }
    assert_eq!(g, gathers_by_kind);
    assert_eq!(e, enc_by_kind);
    assert_eq!(g.iter().sum::<u64>(), w.ledger.gathers);
    assert_eq!(e.iter().sum::<u64>(), w.ledger.encroachments);
    assert!(w.ledger.encroachments > 0);
    // Sanctions given = received = the ledger's.
    let given: u64 = t.iter().map(|a| a.run.sanctions_given).sum();
    let received: u64 = t.iter().map(|a| a.run.sanctions_received).sum();
    let ledger: u64 = w.ledger.sanctions.iter().sum();
    assert_eq!((given, received, sanctions), (ledger, ledger, ledger));
    assert!(
        ledger > 0,
        "the ruled world sanctioned someone in 400 ticks"
    );
    // The window's counts are the reports' from tick 200 on.
    let in_window = t.iter().fold(Counts::default(), |mut c, a| {
        let x = a.window();
        c.gathers += x.gathers;
        c.encroachments += x.encroachments;
        c
    });
    assert_eq!(
        (in_window.gathers, in_window.encroachments),
        (win.gathers, win.encroachments)
    );
    // The cohort is whoever lived when the window opened.
    assert_eq!(t.iter().filter(|a| a.cohort).count() as u64, alive_at_200);
    let m = t.metrics(&w);
    assert_eq!(m.cohort_learners, learners_at_200);
    let survivors = w
        .agents
        .iter()
        .filter(|a| a.policy == PolicyKind::Learner && t.get(a.id).unwrap().cohort)
        .count() as u64;
    assert_eq!(m.cohort_survived, survivors);
    assert!(m.cohort_survived < m.cohort_learners, "someone died");
    assert!(m.cohort_died_violence <= m.cohort_learners - m.cohort_survived);
    assert!(m.cohort_died_violence > 0, "sanctions killed a learner");
    assert!(m.sanctioned_then_died <= m.sanctioned_learners);
    assert!(m.e_learner_survivors.is_some() && m.encroach_share_removed.is_some());
    // A death carries its tick and cause.
    let dead = t.iter().find(|a| !a.alive()).expect("someone died");
    let (tick, _) = dead.died.unwrap();
    assert!(tick < 400 && tick + 1 >= dead.born);
    // One row per agent, with the header's columns, in id order.
    let rows = t.rows(7);
    assert_eq!(rows.len(), t.len());
    let width = AGENTS_COLUMNS.split(',').count();
    assert!(rows.iter().all(|r| r.split(',').count() == width));
    assert!(rows[0].starts_with("7,0,"));
    assert!(rows
        .last()
        .unwrap()
        .starts_with(&format!("7,{},", w.next_agent_id - 1)));
}

// --- the scripted world -------------------------------------------------

const A: u64 = 0; // learner, founder of X, gathers every tick
const B: u64 = 1; // learner, steps onto X, encroaches once, is sanctioned to death
const C: u64 = 2; // learner, steps onto Y, encroaches on E until E leaves
const E: u64 = 3; // enforcer, founder of Y, leaves at tick 7
const H: u64 = 4; // enforcer beside X, sanctions B
const F: u64 = 5; // learner born old, dies of old age at tick 6

const X: Pos = Pos { x: 2, y: 2 };
const Y: Pos = Pos { x: 5, y: 5 };

/// 8 × 8, no regrowth, no births, old age only for an agent born old,
/// room in the inventory for twelve gathers, institutions on.
fn scripted_cfg() -> WorldConfig {
    WorldConfig {
        width: 8,
        height: 8,
        initial_agents: 0,
        patch_count: 0,
        regrow_interval: 1_000_000,
        birth_threshold: None,
        max_age: Some(3_007),
        inventory_capacity: 100,
        nomos: true,
        ..WorldConfig::default()
    }
}

fn add(w: &mut World, pos: Pos, policy: PolicyKind, age: u64) -> AgentId {
    let id = AgentId(w.next_agent_id);
    w.next_agent_id += 1;
    let energy = 50_000;
    w.agents.push(Agent {
        id,
        pos,
        energy,
        health: w.config.max_health,
        inventory: 0,
        age,
        policy,
        params: PolicyParams::for_kind(policy, &w.config),
        memory: Memory::default(),
        last_signal: None,
        recent: Vec::new(),
        last_step: None,
        arrived: None,
    });
    w.ledger.energy_initial += energy;
    id
}

fn stock(w: &mut World, p: Pos, food: u32) {
    let i = w.bounds().index(p);
    w.ledger.food_initial += u64::from(food);
    w.resources.food[i] = food;
    w.resources.capacity[i] = food;
}

fn step(w: &mut World, t: &mut CohortTally, actions: &[Action]) -> TickReport {
    let intents = actions
        .iter()
        .map(|&action| Intent {
            action,
            memory: Memory::default(),
            state: None,
        })
        .collect();
    let r = w.apply(intents);
    t.observe(&r, w);
    r
}

fn died(r: &TickReport, who: u64, of: DeathCause) -> bool {
    r.events.iter().any(
        |e| matches!(e, Event::Died { agent, cause, .. } if *agent == AgentId(who) && *cause == of),
    )
}

/// Window = ticks 4 … 11, midpoint 8. A gathers on X every tick. B steps
/// onto X at tick 1 and gathers there once, at tick 4 (an encroachment on
/// A); H harms B at ticks 5 … 9 — five sanctions, B dies of violence at
/// tick 9. C steps onto Y at tick 1 and gathers there at ticks 2 … 5
/// (encroaching on E) and 8 … 11 (E left at tick 7: C is the first
/// occupant). F is born old and dies of old age at tick 6.
#[test]
fn the_cohort_columns_follow_their_definitions_in_a_scripted_world() {
    use Action::{Consume, Gather, Harm, Move, Rest};
    let harm_b = Harm { target: AgentId(B) };
    let mut w = World::new(scripted_cfg()).unwrap();
    assert_eq!(add(&mut w, X, PolicyKind::Learner, 0), AgentId(A));
    assert_eq!(
        add(&mut w, Pos::new(3, 2), PolicyKind::Learner, 0),
        AgentId(B)
    );
    assert_eq!(
        add(&mut w, Pos::new(6, 5), PolicyKind::Learner, 0),
        AgentId(C)
    );
    assert_eq!(add(&mut w, Y, PolicyKind::Enforcer, 0), AgentId(E));
    assert_eq!(
        add(&mut w, Pos::new(2, 3), PolicyKind::Enforcer, 0),
        AgentId(H)
    );
    assert_eq!(
        add(&mut w, Pos::new(0, 0), PolicyKind::Learner, 3_000),
        AgentId(F)
    );
    stock(&mut w, X, 40);
    stock(&mut w, Y, 40);
    w.intervene(rule()).unwrap();
    let mut t = CohortTally::new(&w, (4, 12));
    assert_eq!(t.midpoint(), 8);

    // Ticks 0 … 3: before the window.
    step(
        &mut w,
        &mut t,
        &[Gather, Rest, Rest, Gather, Rest, Consume(1)],
    );
    step(
        &mut w,
        &mut t,
        &[
            Gather,
            Move(Dir::West),
            Move(Dir::West),
            Rest,
            Rest,
            Consume(1),
        ],
    );
    assert_eq!((w.agents[1].pos, w.agents[2].pos), (X, Y));
    for _ in 0..2 {
        let r = step(
            &mut w,
            &mut t,
            &[Gather, Rest, Gather, Gather, Rest, Consume(1)],
        );
        assert_eq!(r.encroachers, vec![(AgentId(C), AgentId(E))]);
    }
    assert!(t.iter().all(|a| !a.cohort));
    // Tick 4: the window opens on six living agents; B and C encroach.
    let r = step(
        &mut w,
        &mut t,
        &[Gather, Gather, Gather, Gather, Rest, Consume(1)],
    );
    assert_eq!(
        r.encroachers,
        vec![(AgentId(B), AgentId(A)), (AgentId(C), AgentId(E))]
    );
    assert_eq!(t.iter().filter(|a| a.cohort).count(), 6);
    // Tick 5: the first sanction on B (accused since tick 4).
    let r = step(
        &mut w,
        &mut t,
        &[Gather, Consume(1), Gather, Gather, harm_b, Consume(1)],
    );
    assert_eq!(r.sanctions.iter().sum::<u32>(), 1);
    // Tick 6: the second; F dies of old age.
    let r = step(
        &mut w,
        &mut t,
        &[Gather, Consume(1), Rest, Gather, harm_b, Consume(1)],
    );
    assert!(died(&r, F, DeathCause::OldAge), "{:?}", r.events);
    assert_eq!(w.agents.len(), 5);
    // Tick 7: the third; E leaves Y.
    step(
        &mut w,
        &mut t,
        &[Gather, Consume(1), Rest, Move(Dir::East), harm_b],
    );
    assert_eq!(w.agents[3].pos, Pos::new(6, 5));
    // Tick 8: the second half opens; C gathers on Y alone.
    let r = step(&mut w, &mut t, &[Gather, Consume(1), Gather, Rest, harm_b]);
    assert!(r.encroachers.is_empty());
    // Tick 9: the fifth sanction kills B.
    let r = step(&mut w, &mut t, &[Gather, Consume(1), Gather, Rest, harm_b]);
    assert!(died(&r, B, DeathCause::Violence), "{:?}", r.events);
    assert_eq!(w.agents.len(), 4);
    // Ticks 10, 11.
    for _ in 0..2 {
        step(&mut w, &mut t, &[Gather, Gather, Rest, Rest]);
    }
    assert_eq!(w.tick, 12);
    assert_eq!(w.ledger.sanctions.iter().sum::<u64>(), 5);

    // The table.
    let rows = t.rows(1);
    assert_eq!(rows.len(), 6);
    assert_eq!(
        rows[A as usize],
        "1,0,learner,0,,,1,12,0,0,0,8,0,0,0,4,0,4,0,"
    );
    assert_eq!(
        rows[B as usize],
        "1,1,learner,0,9,violence,1,1,1,5,0,1,1,5,0,1,1,0,0,9"
    );
    assert_eq!(
        rows[C as usize],
        "1,2,learner,0,,,1,8,4,0,0,6,2,0,0,2,2,4,0,"
    );
    assert_eq!(
        rows[E as usize],
        "1,3,enforcer,0,,,1,6,0,0,0,3,0,0,0,3,0,0,0,"
    );
    assert_eq!(
        rows[H as usize],
        "1,4,enforcer,0,,,1,0,0,0,5,0,0,0,5,0,0,0,0,"
    );
    assert_eq!(
        rows[F as usize],
        "1,5,learner,0,6,old_age,1,0,0,0,0,0,0,0,0,0,0,0,0,"
    );
    assert_eq!(t.get(AgentId(B)).unwrap().last_sanctioned, Some((9, 32)));

    // The columns (module docs of `cz_lab::cohort`): the cohort is the
    // four learners alive at tick 4; A and C survive, B died of violence,
    // F of old age. Survivors: 2 encroachments in 14 gathers; the first
    // half 2 in 6 (A 0/4, C 2/2), the second 0 in 8; A's own change is 0,
    // C's −1. Removed: B, 1 in 1, which is one third of the learners' 3
    // window encroachments. B was sanctioned and died within 32 ticks of
    // the last sanction.
    let m = t.metrics(&w);
    assert_eq!(
        m,
        CohortMetrics {
            cohort_learners: 4,
            cohort_survived: 2,
            cohort_died_violence: 1,
            e_learner_survivors: Some(2.0 / 14.0),
            e_learner_removed: Some(1.0),
            encroach_share_removed: Some(1.0 / 3.0),
            e_survivors_first_half: Some(2.0 / 6.0),
            e_survivors_second_half: Some(0.0),
            e_survivors_delta_mean: Some(-0.5),
            sanctioned_learners: 1,
            sanctioned_then_died: 1,
        }
    );
    assert_eq!(
        m.csv(),
        "4,2,1,0.14286,1.00000,0.33333,0.33333,0.00000,-0.50000,1,1"
    );
}

/// The same script with the window after everyone's death and birth has
/// happened: a window nobody opens has no cohort and empty rates.
#[test]
fn a_window_that_never_opens_has_no_cohort() {
    let mut w = World::new(scripted_cfg()).unwrap();
    add(&mut w, X, PolicyKind::Learner, 0);
    stock(&mut w, X, 40);
    // Window (3, 3): zero ticks long, never opens.
    let mut t = CohortTally::new(&w, (3, 3));
    for _ in 0..5 {
        step(&mut w, &mut t, &[Action::Gather]);
    }
    let m = t.metrics(&w);
    assert_eq!(m, CohortMetrics::default());
    assert_eq!(m.csv(), "0,0,0,,,,,,,0,0");
    assert_eq!(t.get(AgentId(0)).unwrap().run.gathers, 5);
    assert_eq!(t.get(AgentId(0)).unwrap().window(), Counts::default());
}
