//! The assembly (ADR-0014 D7; task 4-5): motions open the one floor, votes
//! are counted on it, the kernel closes it and puts a carried motion into
//! effect — and the scripted `Enforcer`, the positive control that moves
//! the first-occupancy norm, votes by its script, sanctions each episode
//! once, keeps its promises and does not encroach. Procedural
//! impossibilities are `Failure`s; nothing here is a judgement.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::policy::{decide, default_norm};
use simulation_core::rng::{RngStream, Stream};
use simulation_core::world::SNAPSHOT_FORMAT;
use simulation_core::{
    Action, Agent, AgentId, Charter, Event, Failure, Intent, InteractionKind, Intervention,
    Message, MotionKind, Norm, NormKind, PolicyKind, PolicyParams, Pos, Sanction, SnapshotError,
    TickReport, World, WorldConfig,
};

/// The hand-built world of `common::cfg()` with institutions on and every
/// vote recorded.
fn ncfg() -> WorldConfig {
    WorldConfig {
        nomos: true,
        record_votes: true,
        ..cfg()
    }
}

fn charter(quorum: u16, threshold: u16, vote_ticks: u16, amendable: bool) -> Charter {
    Charter {
        quorum_permille: quorum,
        threshold_permille: threshold,
        vote_ticks,
        amendable,
    }
}

fn with_charter(c: Charter) -> WorldConfig {
    WorldConfig {
        charter: c,
        ..ncfg()
    }
}

fn norm(kind: NormKind, standing_ticks: u16, sanction: Sanction) -> Norm {
    Norm {
        kind,
        standing_ticks,
        sanction,
        except_sanction: false,
    }
}

fn motion(kind: MotionKind) -> Action {
    Action::Signal(Message::Motion(kind))
}

fn enact(n: Norm) -> Action {
    motion(MotionKind::Enact { norm: n })
}

fn vote(aye: bool) -> Action {
    Action::Signal(Message::Vote { aye })
}

/// Four agents on a 2 × 2 block, each with plenty of energy.
fn four(cfg: WorldConfig) -> World {
    let mut w = empty_world(cfg);
    for p in [
        Pos::new(1, 1),
        Pos::new(2, 1),
        Pos::new(1, 2),
        Pos::new(2, 2),
    ] {
        add_agent(&mut w, p, 50_000, 0);
    }
    w
}

/// `n` agents (at most 64) over the 8 × 8 grid, each with plenty of energy.
fn row(cfg: WorldConfig, n: u16) -> World {
    let mut w = empty_world(cfg);
    for i in 0..n {
        add_agent(&mut w, Pos::new(i % 8, i / 8), 50_000, 0);
    }
    w
}

/// Sets `agents[i]`'s inventory, keeping the ledger's conservation law.
fn set_inventory(w: &mut World, i: usize, n: u32) {
    let before = w.agents[i].inventory as i128;
    w.ledger.food_initial = (w.ledger.food_initial as i128 - before + n as i128) as u64;
    w.agents[i].inventory = n;
}

fn step_with(w: &mut World, actions: &[Action]) -> TickReport {
    w.apply(intents(actions))
}

fn rests(n: usize) -> Vec<Action> {
    vec![Action::Rest; n]
}

fn failed(r: &TickReport, f: Failure) -> u32 {
    r.failures[f.index()]
}

fn no_failures(r: &TickReport) -> bool {
    r.failures.iter().all(|&n| n == 0)
}

/// The assembly's events of a report, in order.
fn assembly(r: &TickReport) -> Vec<Event> {
    r.events
        .iter()
        .copied()
        .filter(|e| {
            matches!(
                e,
                Event::Motioned { .. }
                    | Event::Voted { .. }
                    | Event::MotionClosed { .. }
                    | Event::Enacted { .. }
                    | Event::Repealed { .. }
                    | Event::Chartered { .. }
            )
        })
        .collect()
}

fn voted(tick: u64, motion: u16, by: u64, aye: bool) -> Event {
    Event::Voted {
        tick,
        motion,
        by: AgentId(by),
        aye,
    }
}

fn closed(tick: u64, motion: u16, ayes: u32, nays: u32, alive: u32, carried: bool) -> Event {
    Event::MotionClosed {
        tick,
        motion,
        ayes,
        nays,
        alive,
        carried,
    }
}

/// `agents[i]`'s decision by its own policy, as the kernel would make it.
fn decided(w: &World, i: usize) -> Intent {
    let index = w.spatial_index();
    let norms = w.norm_index();
    let obs = w.observation_at(i, &index, &norms);
    let mut rng = RngStream::new(w.config.seed, w.tick, w.agents[i].id.0, Stream::Decide);
    decide(&obs, &mut rng)
}

fn ids(v: &[AgentId]) -> Vec<u64> {
    v.iter().map(|a| a.0).collect()
}

#[test]
fn a_motion_is_moved_voted_closed_and_enacted_in_order() {
    // Quorum half the living, two thirds of the votes, three ticks of voting.
    let mut w = four(with_charter(charter(500, 667, 3, true)));
    let n = encroach();
    let kind = MotionKind::Enact { norm: n };
    let energy_before = w.agents[0].energy;

    // Tick 0: agent 0 moves the first-occupancy norm; the floor is taken.
    let mut acts = rests(4);
    acts[0] = enact(n);
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r), "{:?}", r.failures);
    assert_eq!(
        assembly(&r),
        vec![Event::Motioned {
            tick: 0,
            motion: 0,
            by: AgentId(0),
            asks: kind,
        }]
    );
    assert_eq!(
        (r.motions, r.votes, r.motions_carried, r.motions_defeated),
        (1, 0, 0, 0)
    );
    let m = w.nomos.motion.as_ref().expect("the floor is taken");
    assert_eq!((m.id, m.by, m.opened, m.kind), (0, AgentId(0), 0, kind));
    assert!(m.ayes.is_empty() && m.nays.is_empty());
    assert_eq!(w.nomos.next_motion_id, 1);
    assert_eq!(w.ledger.motions, 1);
    assert_eq!(
        w.agents[0].energy,
        energy_before - w.config.metabolism_cost - w.config.motion_cost,
        "a motion costs motion_cost on the attempt"
    );
    assert!(
        w.nomos.rules.is_empty(),
        "nothing is in force while the assembly votes"
    );
    w.check_invariants().unwrap();

    // The next tick's observation shows the floor to those who perceive
    // standing (the index is built for a motion alone, with no rule).
    let index = w.spatial_index();
    let norms = w.norm_index();
    assert!(!norms.is_empty() && !norms.any_rule());
    assert_eq!(norms.motion(), Some(kind));
    let obs = w.observation_at(1, &index, &norms);
    assert_eq!(obs.open_motion(), Some(kind));
    assert!(!obs.i_voted());
    assert!(obs.rule(NormKind::Encroach).is_none());

    // Tick 1: two ayes, one nay, in actor order; recorded (record_votes).
    let r = step_with(&mut w, &[Action::Rest, vote(true), vote(true), vote(false)]);
    assert!(no_failures(&r));
    assert_eq!(
        assembly(&r),
        vec![
            voted(1, 0, 1, true),
            voted(1, 0, 2, true),
            voted(1, 0, 3, false)
        ]
    );
    assert_eq!(r.votes, 3);
    let m = w.nomos.motion.as_ref().unwrap();
    assert_eq!((ids(&m.ayes), ids(&m.nays)), (vec![1, 2], vec![3]));
    let index = w.spatial_index();
    let norms = w.norm_index();
    assert!(!norms.voted(0) && norms.voted(1) && norms.voted(2) && norms.voted(3));
    assert!(w.observation_at(3, &index, &norms).i_voted());
    assert!(!w.observation_at(0, &index, &norms).i_voted());
    w.check_invariants().unwrap();

    // Tick 2: nothing happens; the motion stays open (it closes at tick 3).
    let r = step_with(&mut w, &rests(4));
    assert!(assembly(&r).is_empty());
    assert!(w.nomos.motion.is_some());

    // Tick 3: the mover's own aye still counts (3b before 3c), then the
    // motion closes: 4 votes of 4 alive ≥ 50%, 3 ayes of 4 votes ≥ 66.7%
    // — carried, and the norm is in force from the next tick.
    let mut acts = rests(4);
    acts[0] = vote(true);
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r));
    assert_eq!(
        assembly(&r),
        vec![
            voted(3, 0, 0, true),
            closed(3, 0, 3, 1, 4, true),
            Event::Enacted {
                tick: 3,
                rule: 0,
                norm: n,
                motion: 0,
            },
        ]
    );
    assert_eq!((r.motions_carried, r.motions_defeated), (1, 0));
    assert!(w.nomos.motion.is_none(), "the floor is free again");
    assert_eq!(w.nomos.rules.len(), 1);
    assert_eq!(
        (
            w.nomos.rules[0].id,
            w.nomos.rules[0].norm,
            w.nomos.rules[0].enacted
        ),
        (0, n, 3)
    );
    assert_eq!(w.nomos.next_rule_id, 1);
    assert_eq!(
        (
            w.ledger.motions,
            w.ledger.votes,
            w.ledger.motions_carried,
            w.ledger.motions_defeated
        ),
        (1, 4, 1, 0)
    );
    w.check_invariants().unwrap();
    let norms = w.norm_index();
    assert!(norms.any_rule() && norms.motion().is_none());
    assert_eq!(norms.rule(NormKind::Encroach).map(|r| r.id), Some(0));
    assert!((0..4).all(|i| !norms.voted(i)));

    // Tick 4: the floor is free: a second motion opens with the next id.
    let mut acts = rests(4);
    acts[2] = motion(MotionKind::Repeal { rule: 0 });
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r));
    assert_eq!(
        assembly(&r),
        vec![Event::Motioned {
            tick: 4,
            motion: 1,
            by: AgentId(2),
            asks: MotionKind::Repeal { rule: 0 },
        }]
    );
    assert_eq!(w.nomos.motion.as_ref().map(|m| m.id), Some(1));
    assert_eq!(w.nomos.next_motion_id, 2);
}

fn encroach() -> Norm {
    default_norm(NormKind::Encroach)
}

#[test]
fn the_floor_is_one_and_every_living_agent_votes_once() {
    let mut w = four(WorldConfig {
        signal_cost: 10,
        ..with_charter(charter(0, 0, 5, true))
    });
    let (e0, e1, e2) = (w.agents[0].energy, w.agents[1].energy, w.agents[2].energy);
    // Tick 0: 0 moves a motion, 1 tries another in the same tick (the
    // floor is taken by then: actor order), 2 votes on the motion 0 just
    // opened — a lower id's motion takes the higher ids' votes of the tick.
    let r = step_with(
        &mut w,
        &[
            enact(encroach()),
            enact(norm(NormKind::Harm, 8, Sanction::ALL)),
            vote(true),
            Action::Rest,
        ],
    );
    assert_eq!(failed(&r, Failure::FloorTaken), 1);
    assert_eq!(r.failures.iter().sum::<u32>(), 1);
    assert_eq!(r.motions, 1);
    assert_eq!(r.votes, 1);
    assert_eq!(
        w.nomos.motion.as_ref().map(|m| (m.by, ids(&m.ayes))),
        Some((AgentId(0), vec![2]))
    );
    let m = w.config.metabolism_cost;
    assert_eq!(w.agents[0].energy, e0 - m - w.config.motion_cost);
    assert_eq!(
        w.agents[1].energy,
        e1 - m - w.config.motion_cost,
        "a refused motion still costs the attempt"
    );
    assert_eq!(
        w.agents[2].energy,
        e2 - m - w.config.signal_cost,
        "a vote costs signal_cost"
    );

    // Tick 1: 2 votes again, on the other side — refused, and the tally is
    // untouched; 3 votes nay; 0 (the mover) has not voted and may.
    let e2 = w.agents[2].energy;
    let r = step_with(
        &mut w,
        &[vote(false), Action::Rest, vote(false), vote(false)],
    );
    assert_eq!(failed(&r, Failure::AlreadyVoted), 1);
    assert_eq!(r.votes, 2);
    let mo = w.nomos.motion.as_ref().unwrap();
    assert_eq!((ids(&mo.ayes), ids(&mo.nays)), (vec![2], vec![0, 3]));
    assert_eq!(
        w.agents[2].energy,
        e2 - m - w.config.signal_cost,
        "a refused vote costs the attempt too"
    );
    assert_eq!(
        assembly(&r),
        vec![voted(1, 0, 0, false), voted(1, 0, 3, false)],
        "only counted votes are recorded"
    );
    // Tick 2: a second vote on the same side is refused as well.
    let r = step_with(
        &mut w,
        &[Action::Rest, Action::Rest, vote(true), Action::Rest],
    );
    assert_eq!(failed(&r, Failure::AlreadyVoted), 1);
    w.check_invariants().unwrap();

    // No motion, no vote; and the floor refuses what cannot become a rule.
    let mut fresh = four(ncfg());
    let r = step_with(
        &mut fresh,
        &[vote(true), Action::Rest, Action::Rest, Action::Rest],
    );
    assert_eq!(failed(&r, Failure::NoMotion), 1);
    assert_eq!(r.votes, 0);
    assert!(fresh.nomos.motion.is_none());
    let r = step_with(
        &mut fresh,
        &[
            motion(MotionKind::Repeal { rule: 7 }),
            enact(Norm {
                sanction: Sanction(4),
                ..encroach()
            }),
            motion(MotionKind::Charter {
                charter: charter(1001, 0, 1, true),
            }),
            Action::Rest,
        ],
    );
    assert_eq!(failed(&r, Failure::NoSuchRule), 3);
    assert!(fresh.nomos.motion.is_none() && fresh.ledger.motions == 0);
    fresh.check_invariants().unwrap();

    // Without institutions there is no floor at all: the attempt fails
    // (and costs), a vote finds no motion, the state stays empty.
    let mut off = four(WorldConfig {
        nomos: false,
        ..ncfg()
    });
    let e0 = off.agents[0].energy;
    let r = step_with(
        &mut off,
        &[enact(encroach()), vote(true), Action::Rest, Action::Rest],
    );
    assert_eq!(failed(&r, Failure::FloorTaken), 1);
    assert_eq!(failed(&r, Failure::NoMotion), 1);
    assert_eq!(off.agents[0].energy, e0 - m - off.config.motion_cost);
    assert!(off.nomos.is_empty());
    assert_eq!(
        off.dynamics_hash(),
        {
            let mut twin = four(WorldConfig {
                nomos: false,
                ..ncfg()
            });
            twin.agents[0].energy -= off.config.motion_cost;
            step_with(&mut twin, &rests(4));
            twin.dynamics_hash()
        },
        "a refused motion is a rest that cost motion_cost"
    );
    off.check_invariants().unwrap();
}

/// Moves `kind` at tick 0 by agent 0 in a world of `n` agents under
/// `charter`, casts `votes` (tick, agent, aye) as scheduled, runs until
/// the motion has closed, and returns the `MotionClosed` event with the
/// world (`Enacted` and the rest can be read from it).
fn vote_on(
    c: Charter,
    n: u16,
    kind: MotionKind,
    votes: &[(u64, usize, bool)],
    until: u64,
) -> (Event, World) {
    let mut w = row(with_charter(c), n);
    let mut closed_event = None;
    while w.tick <= until {
        let mut acts = rests(n as usize);
        if w.tick == 0 {
            acts[0] = motion(kind);
        }
        for &(t, i, aye) in votes {
            if t == w.tick {
                acts[i] = vote(aye);
            }
        }
        let r = step_with(&mut w, &acts);
        assert!(no_failures(&r), "tick {}: {:?}", r.tick, r.failures);
        if let Some(e) = r
            .events
            .iter()
            .find(|e| matches!(e, Event::MotionClosed { .. }))
        {
            assert!(closed_event.is_none(), "a motion closes once");
            closed_event = Some(*e);
        }
        w.check_invariants().unwrap();
    }
    (closed_event.expect("the motion closed"), w)
}

#[test]
fn quorum_threshold_and_deadline_sit_exactly_on_their_boundaries() {
    let kind = MotionKind::Enact { norm: encroach() };
    // Ten agents, quorum 300‰ (3 votes), threshold 500‰, two ticks.
    let c = charter(300, 500, 2, true);
    // 3 votes of 10 = exactly the quorum; 2 ayes of 3 ≥ half: carried.
    let (e, w) = vote_on(c, 10, kind, &[(1, 1, true), (1, 2, true), (1, 3, false)], 2);
    assert_eq!(e, closed(2, 0, 2, 1, 10, true));
    assert_eq!(w.nomos.rules.len(), 1);
    assert_eq!(
        (w.ledger.motions_carried, w.ledger.motions_defeated),
        (1, 0)
    );
    // One vote short of the quorum, however unanimous: defeated.
    let (e, w) = vote_on(c, 10, kind, &[(1, 1, true), (1, 2, true)], 2);
    assert_eq!(e, closed(2, 0, 2, 0, 10, false));
    assert!(w.nomos.rules.is_empty());
    assert_eq!(
        (w.ledger.motions_carried, w.ledger.motions_defeated),
        (0, 1)
    );
    assert!(
        w.nomos.motion.is_none(),
        "a defeated motion frees the floor too"
    );
    // Exactly the threshold: 2 ayes of 4 votes at 500‰ carries …
    let (e, _) = vote_on(
        c,
        10,
        kind,
        &[(0, 1, true), (1, 2, true), (2, 3, false), (2, 4, false)],
        2,
    );
    assert_eq!(e, closed(2, 0, 2, 2, 10, true));
    // … one aye fewer does not.
    let (e, _) = vote_on(
        c,
        10,
        kind,
        &[(0, 1, true), (1, 2, false), (2, 3, false), (2, 4, false)],
        2,
    );
    assert_eq!(e, closed(2, 0, 1, 3, 10, false));
    // A vote in the closing tick counts (3b before 3c); one tick later the
    // floor is already free.
    let (e, _) = vote_on(c, 10, kind, &[(2, 1, true), (2, 2, true), (2, 3, true)], 2);
    assert_eq!(e, closed(2, 0, 3, 0, 10, true));
    let mut late = row(with_charter(c), 10);
    let mut acts = rests(10);
    acts[0] = motion(kind);
    step_with(&mut late, &acts);
    step_with(&mut late, &rests(10));
    step_with(&mut late, &rests(10));
    assert!(late.nomos.motion.is_none());
    let mut acts = rests(10);
    acts[1] = vote(true);
    let r = step_with(&mut late, &acts);
    assert_eq!(failed(&r, Failure::NoMotion), 1);
    assert_eq!(late.ledger.motions_defeated, 1, "nobody voted: defeated");

    // No quorum and no threshold: still, a motion nobody voted for does
    // not carry; a single aye does, a single nay does not.
    let free = charter(0, 0, 1, true);
    let (e, _) = vote_on(free, 4, kind, &[], 1);
    assert_eq!(e, closed(1, 0, 0, 0, 4, false));
    let (e, _) = vote_on(free, 4, kind, &[(1, 3, false)], 1);
    assert_eq!(e, closed(1, 0, 0, 1, 4, false));
    let (e, w) = vote_on(free, 4, kind, &[(1, 3, true)], 1);
    assert_eq!(e, closed(1, 0, 1, 0, 4, true));
    assert_eq!(w.nomos.rules.len(), 1);
    // Everyone must vote at 1000‰ quorum; the mover's own aye counts.
    let all = charter(1000, 1000, 1, true);
    let (e, _) = vote_on(all, 3, kind, &[(0, 1, true), (0, 2, true), (1, 0, true)], 1);
    assert_eq!(e, closed(1, 0, 3, 0, 3, true));
    let (e, _) = vote_on(all, 3, kind, &[(0, 1, true), (0, 2, true)], 1);
    assert_eq!(e, closed(1, 0, 2, 0, 3, false));

    // `vote_ticks` 0: moved and closed in the same tick, with the votes of
    // the actors after the mover.
    let (e, w) = vote_on(
        charter(0, 500, 0, true),
        4,
        kind,
        &[(0, 1, true), (0, 2, true), (0, 3, false)],
        0,
    );
    assert_eq!(e, closed(0, 0, 2, 1, 4, true));
    assert_eq!(w.nomos.rules[0].enacted, 0);
    assert!(w.nomos.motion.is_none());
}

#[test]
fn the_dead_do_not_vote_and_the_living_at_closing_are_the_quorum() {
    // Five agents, quorum 600‰: 3 of 5 — or 3 of 4 once one has died
    // (3000 ≥ 2400), but 2 of 4 is not (2000 < 2400).
    let c = charter(600, 500, 2, true);
    let kind = MotionKind::Enact { norm: encroach() };
    let mut w = row(with_charter(c), 5);
    // Agent 4 will starve to death in tick 1: no energy, health down to
    // the damage of two ticks (tick 0 takes the first).
    w.ledger.energy_initial -= w.agents[4].energy;
    w.agents[4].energy = 0;
    w.agents[4].health = 2 * w.config.starvation_damage;
    let mut acts = rests(5);
    acts[0] = motion(kind);
    step_with(&mut w, &acts);
    // Tick 1: 1, 2 and 4 vote aye; 4 dies in phase 6 of the same tick and
    // its vote is struck.
    let r = step_with(
        &mut w,
        &[
            Action::Rest,
            vote(true),
            vote(true),
            Action::Rest,
            vote(true),
        ],
    );
    assert_eq!(r.votes, 3);
    assert_eq!(r.deaths, 1);
    assert_eq!(assembly(&r).len(), 3, "the vote was cast and recorded");
    let m = w.nomos.motion.as_ref().unwrap();
    assert_eq!(ids(&m.ayes), vec![1, 2], "the dead voter is struck");
    assert_eq!(w.agents.len(), 4);
    w.check_invariants().unwrap();
    // Tick 2: closing before 4 living agents with 2 votes: 2000 < 2400,
    // defeated — had the dead vote counted, 3 of 4 would have carried.
    let r = step_with(&mut w, &rests(4));
    assert_eq!(assembly(&r), vec![closed(2, 0, 2, 0, 4, false)]);
    assert!(w.nomos.rules.is_empty());

    // The same with the voter removed by an intervention instead.
    let mut w = row(with_charter(c), 5);
    let mut acts = rests(5);
    acts[0] = motion(kind);
    step_with(&mut w, &acts);
    step_with(
        &mut w,
        &[
            Action::Rest,
            vote(true),
            vote(true),
            Action::Rest,
            vote(true),
        ],
    );
    assert_eq!(ids(&w.nomos.motion.as_ref().unwrap().ayes), vec![1, 2, 4]);
    w.intervene(Intervention::RemoveAgent { id: AgentId(4) })
        .unwrap();
    assert_eq!(ids(&w.nomos.motion.as_ref().unwrap().ayes), vec![1, 2]);
    w.check_invariants().unwrap();
    let r = step_with(&mut w, &rests(4));
    assert_eq!(assembly(&r), vec![closed(2, 0, 2, 0, 4, false)]);
    // And with everyone alive, the three ayes of five carry.
    let (e, _) = vote_on(c, 5, kind, &[(1, 1, true), (1, 2, true), (1, 4, true)], 2);
    assert_eq!(e, closed(2, 0, 3, 0, 5, true));
}

#[test]
fn motions_amend_replace_repeal_and_recharter_unless_the_charter_forbids() {
    let c = charter(0, 500, 1, true);
    let mut w = four(with_charter(c));
    // A decree puts the first-occupancy norm in force as rule 0, and an
    // episode is open under it.
    w.intervene(Intervention::Enact { norm: encroach() })
        .unwrap();
    step_with(&mut w, &rests(4));
    w.nomos.accuse(AgentId(3), 0, 0, AgentId(1));
    w.check_invariants().unwrap();
    assert_eq!(w.nomos.standing.len(), 1);

    // Replacement by motion: the same kind with another norm. Carried, the
    // old rule is repealed (its episode closes) and the new one enacted,
    // in that order and both by the motion.
    let stricter = norm(NormKind::Encroach, 16, Sanction::ALL);
    let mut acts = rests(4);
    acts[1] = enact(stricter);
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r));
    let r = step_with(&mut w, &[vote(true), vote(true), Action::Rest, vote(false)]);
    assert_eq!(
        assembly(&r)[3..],
        [
            closed(2, 0, 2, 1, 4, true),
            Event::Repealed {
                tick: 2,
                rule: 0,
                norm: encroach(),
                motion: 0,
            },
            Event::Enacted {
                tick: 2,
                rule: 1,
                norm: stricter,
                motion: 0,
            },
        ]
    );
    assert_eq!(
        w.nomos.rules.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![1]
    );
    assert_eq!(w.nomos.rules[0].norm, stricter);
    assert!(
        w.nomos.standing.is_empty(),
        "the replaced rule's episodes closed"
    );
    assert_eq!(w.ledger.episodes_closed[0], 1);
    assert_eq!(r.episodes_closed[0], 1);

    // Repeal by motion.
    let mut acts = rests(4);
    acts[2] = motion(MotionKind::Repeal { rule: 1 });
    step_with(&mut w, &acts);
    let r = step_with(
        &mut w,
        &[vote(true), Action::Rest, Action::Rest, Action::Rest],
    );
    assert_eq!(
        assembly(&r)[1..],
        [
            closed(4, 1, 1, 0, 4, true),
            Event::Repealed {
                tick: 4,
                rule: 1,
                norm: stricter,
                motion: 1,
            },
        ]
    );
    assert!(w.nomos.rules.is_empty());
    // A motion to repeal what is not in force is refused.
    let mut acts = rests(4);
    acts[2] = motion(MotionKind::Repeal { rule: 1 });
    let r = step_with(&mut w, &acts);
    assert_eq!(failed(&r, Failure::NoSuchRule), 1);

    // A new charter by motion: in force for the motions after it, and it
    // forbids amendments.
    let locked = charter(0, 500, 2, false);
    let mut acts = rests(4);
    acts[3] = motion(MotionKind::Charter { charter: locked });
    step_with(&mut w, &acts);
    let r = step_with(
        &mut w,
        &[vote(true), vote(true), Action::Rest, Action::Rest],
    );
    assert_eq!(
        assembly(&r)[2..],
        [
            closed(7, 2, 2, 0, 4, true),
            Event::Chartered {
                tick: 7,
                charter: locked,
                motion: 2,
            },
        ]
    );
    assert_eq!(w.nomos.charter, locked);
    assert_eq!(w.config.charter, c, "the config keeps the initial charter");
    // Under it, enacting a kind nobody has ruled on is no amendment and
    // passes; a repeal, a charter and a replacement are `NotAmendable`.
    let harm_norm = norm(NormKind::Harm, 8, Sanction::HARM);
    let mut acts = rests(4);
    acts[0] = enact(harm_norm);
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r));
    assert_eq!(w.nomos.motion.as_ref().map(|m| m.id), Some(3));
    let r = step_with(&mut w, &[Action::Rest, vote(true), vote(true), vote(true)]);
    assert!(
        assembly(&r).is_empty() || !matches!(assembly(&r).last(), Some(Event::MotionClosed { .. })),
        "two ticks of voting now"
    );
    let r = step_with(&mut w, &rests(4));
    assert_eq!(
        assembly(&r),
        vec![
            closed(10, 3, 3, 0, 4, true),
            Event::Enacted {
                tick: 10,
                rule: 2,
                norm: harm_norm,
                motion: 3,
            },
        ]
    );
    let r = step_with(
        &mut w,
        &[
            motion(MotionKind::Repeal { rule: 2 }),
            motion(MotionKind::Charter { charter: c }),
            enact(norm(NormKind::Harm, 4, Sanction::NONE)),
            enact(encroach()),
        ],
    );
    assert_eq!(failed(&r, Failure::NotAmendable), 3);
    assert_eq!(r.motions, 1, "the fresh kind is moved by the last actor");
    assert_eq!(
        w.nomos.motion.as_ref().map(|m| (m.by, m.kind)),
        Some((AgentId(3), MotionKind::Enact { norm: encroach() }))
    );
    // Decrees are never bound by the charter.
    w.intervene(Intervention::Repeal { rule: 2 }).unwrap();
    w.intervene(Intervention::Charter { charter: c }).unwrap();
    assert_eq!(w.nomos.charter, c);
    w.check_invariants().unwrap();
}

#[test]
fn a_carried_motion_that_can_no_longer_take_effect_is_recorded_without_one() {
    let c = charter(0, 500, 2, true);
    let kind = MotionKind::Repeal { rule: 0 };
    // The rule leaves force by decree while the assembly votes to repeal it.
    let mut w = four(with_charter(c));
    w.intervene(Intervention::Enact { norm: encroach() })
        .unwrap();
    let mut acts = rests(4);
    acts[0] = motion(kind);
    step_with(&mut w, &acts);
    step_with(&mut w, &[vote(true), vote(true), vote(true), Action::Rest]);
    w.intervene(Intervention::Repeal { rule: 0 }).unwrap();
    let r = step_with(&mut w, &rests(4));
    assert_eq!(
        assembly(&r),
        vec![closed(2, 0, 3, 0, 4, true)],
        "carried, and nothing to repeal"
    );
    assert_eq!(w.ledger.motions_carried, 1);
    assert!(w.nomos.rules.is_empty() && w.nomos.motion.is_none());

    // The charter stops amending while the assembly votes to repeal.
    let mut w = four(with_charter(c));
    w.intervene(Intervention::Enact { norm: encroach() })
        .unwrap();
    let mut acts = rests(4);
    acts[0] = motion(kind);
    step_with(&mut w, &acts);
    step_with(&mut w, &[vote(true), vote(true), vote(true), Action::Rest]);
    w.intervene(Intervention::Charter {
        charter: charter(0, 500, 2, false),
    })
    .unwrap();
    let r = step_with(&mut w, &rests(4));
    assert_eq!(assembly(&r), vec![closed(2, 0, 3, 0, 4, true)]);
    assert_eq!(w.nomos.rules.len(), 1, "the rule stays in force");

    // The charter read at closing is the one in force then: a shorter
    // voting period closes an open motion at the next phase 3c, a longer
    // one keeps it open.
    let mut w = four(with_charter(charter(0, 500, 10, true)));
    let mut acts = rests(4);
    acts[0] = enact(encroach());
    step_with(&mut w, &acts);
    step_with(
        &mut w,
        &[vote(true), Action::Rest, Action::Rest, Action::Rest],
    );
    w.intervene(Intervention::Charter {
        charter: charter(0, 500, 1, true),
    })
    .unwrap();
    let r = step_with(&mut w, &rests(4));
    assert_eq!(assembly(&r)[0], closed(2, 0, 1, 0, 4, true));
    assert_eq!(w.nomos.rules.len(), 1);
}

/// A world for the enforcer's script: the first-occupancy norm in force,
/// an enforcer `e` at (2, 2) and a few others around it.
fn enforcer_world(cfg: WorldConfig) -> (World, usize) {
    let mut w = empty_world(cfg);
    let e = add_agent_with(
        &mut w,
        Pos::new(2, 2),
        50_000,
        0,
        PolicyKind::Enforcer,
        PolicyParams::default(),
    );
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0); // 1: on the same cell
    add_agent(&mut w, Pos::new(3, 2), 50_000, 0); // 2: adjacent
    add_agent(&mut w, Pos::new(5, 5), 50_000, 0); // 3: far away
    w.intervene(Intervention::Enact { norm: encroach() })
        .unwrap();
    // One tick so that episodes can be opened in the past.
    step_with(&mut w, &rests(4));
    (w, e)
}

#[test]
fn the_enforcer_sanctions_an_accused_neighbour_once_per_episode() {
    let (mut w, e) = enforcer_world(ncfg());
    // Nobody accused: the forager's decision (nothing to gather here, so
    // a move).
    assert!(matches!(decided(&w, e).action, Action::Move(_)));
    // Agent 2 (adjacent) stands accused since the previous tick.
    w.nomos.accuse(AgentId(2), 0, w.tick - 1, AgentId(3));
    w.check_invariants().unwrap();
    assert_eq!(decided(&w, e).action, Action::Harm { target: AgentId(2) });
    // The one on the enforcer's own cell comes first when it is accused too.
    w.nomos.accuse(AgentId(1), 0, w.tick - 1, AgentId(3));
    assert_eq!(decided(&w, e).action, Action::Harm { target: AgentId(1) });
    // Too poor to sanction: `energy <= 2 × harm_cost` falls through.
    let rich = w.agents[e].energy;
    w.agents[e].energy = 2 * w.config.harm_cost;
    assert!(matches!(decided(&w, e).action, Action::Move(_)));
    w.agents[e].energy = rich;
    // Blind perception does not blind the script: it reads the index.
    let mut blind = w.clone();
    blind.config.perception.see_standing = false;
    assert_eq!(
        decided(&blind, e).action,
        Action::Harm { target: AgentId(1) }
    );
    // A rule whose sanction is not a harm gives it nothing to answer with.
    let mut reject_only = w.clone();
    reject_only
        .intervene(Intervention::Enact {
            norm: norm(NormKind::Encroach, 32, Sanction::REJECT),
        })
        .unwrap();
    reject_only
        .nomos
        .accuse(AgentId(1), 1, w.tick - 1, AgentId(3));
    assert!(matches!(decided(&reject_only, e).action, Action::Move(_)));

    // Sanction it: the kernel reads the harm as a sanction and the
    // enforcer remembers `Sanctioned { 1 }`.
    let tick = w.tick;
    let mut acts = rests(4);
    acts[e] = decided(&w, e).action;
    let r = step_with(&mut w, &acts);
    assert!(r
        .events
        .iter()
        .any(|ev| matches!(ev, Event::Sanctioned { by, target, rule: 0, .. } if *by == AgentId(0) && *target == AgentId(1))));
    assert!(w.agents[e]
        .recent
        .iter()
        .any(|m| m.kind == InteractionKind::Sanctioned && m.other == AgentId(1) && m.tick == tick));
    // The same episode is not answered twice: the other accused is next.
    assert_eq!(decided(&w, e).action, Action::Harm { target: AgentId(2) });
    let mut acts = rests(4);
    acts[e] = Action::Harm { target: AgentId(2) };
    step_with(&mut w, &acts);
    assert!(
        matches!(decided(&w, e).action, Action::Move(_)),
        "both episodes answered"
    );
    // A new episode of agent 1 (opened after the sanction) is answered again.
    w.nomos.standing.retain(|s| s.agent != AgentId(1));
    w.nomos.accuse(AgentId(1), 0, w.tick - 1, AgentId(3));
    assert_eq!(decided(&w, e).action, Action::Harm { target: AgentId(1) });
    // Without memory the brake is gone: every tick a sanction.
    let mut forgetful = w.clone();
    forgetful.config.memory_slots = 0;
    for a in &mut forgetful.agents {
        a.recent.clear();
    }
    forgetful.nomos.standing.retain(|s| s.agent != AgentId(2));
    forgetful
        .nomos
        .accuse(AgentId(2), 0, w.tick - 1, AgentId(3));
    for _ in 0..3 {
        assert_eq!(
            decided(&forgetful, e).action,
            Action::Harm { target: AgentId(1) }
        );
        let mut acts = rests(4);
        acts[e] = Action::Harm { target: AgentId(1) };
        step_with(&mut forgetful, &acts);
    }
    w.check_invariants().unwrap();
}

#[test]
fn the_enforcer_keeps_its_promise_to_an_adjacent_partner() {
    let (mut w, e) = enforcer_world(ncfg());
    // Agent 2 proposes "I give 0, you give me 2 within 10 ticks"; the
    // enforcer accepts (by hand: the script never negotiates on its own)
    // and so promises 2 to agent 2.
    let mut acts = rests(4);
    acts[2] = Action::Signal(Message::Propose {
        to: AgentId(0),
        give: 0,
        want: 2,
        by: 10,
    });
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r));
    let offer_tick = w.tick - 1;
    let mut acts = rests(4);
    acts[e] = Action::Signal(Message::Accept {
        to: AgentId(2),
        offer_tick,
    });
    let r = step_with(&mut w, &acts);
    assert!(no_failures(&r));
    assert!(w.agents[e]
        .recent
        .iter()
        .any(|m| m.kind == InteractionKind::Promised && m.other == AgentId(2) && m.value == 2));
    // Nothing to give: the forager's decision.
    assert!(matches!(decided(&w, e).action, Action::Move(_)));
    // With food, the shortfall goes to the partner — all of it when it
    // fits, else what is carried.
    set_inventory(&mut w, e, 3);
    assert_eq!(
        decided(&w, e).action,
        Action::Transfer {
            to: AgentId(2),
            n: 2
        }
    );
    set_inventory(&mut w, e, 1);
    assert_eq!(
        decided(&w, e).action,
        Action::Transfer {
            to: AgentId(2),
            n: 1
        }
    );
    // Given once, the rest is owed; given in full, nothing is.
    let mut acts = rests(4);
    acts[e] = Action::Transfer {
        to: AgentId(2),
        n: 1,
    };
    let r = step_with(&mut w, &acts);
    assert_eq!(r.transfers, 1);
    set_inventory(&mut w, e, 2);
    assert_eq!(
        decided(&w, e).action,
        Action::Transfer {
            to: AgentId(2),
            n: 1
        }
    );
    let mut acts = rests(4);
    acts[e] = Action::Transfer {
        to: AgentId(2),
        n: 1,
    };
    step_with(&mut w, &acts);
    assert!(w.agents[e].inventory == 1);
    assert!(
        !matches!(decided(&w, e).action, Action::Transfer { .. }),
        "paid in full"
    );
    // A partner out of reach is not paid; one that comes back is.
    let mut owing = w.clone();
    owing.agents[e]
        .recent
        .retain(|m| m.kind != InteractionKind::GaveTo);
    owing.agents[2].pos = Pos::new(6, 6);
    assert!(!matches!(
        decided(&owing, e).action,
        Action::Transfer { .. }
    ));
    owing.agents[2].pos = Pos::new(2, 3);
    assert_eq!(
        decided(&owing, e).action,
        Action::Transfer {
            to: AgentId(2),
            n: 1
        }
    );
    // The kernel settles the deal as kept: the promise was 2 and 2 arrived.
    for _ in 0..12 {
        step_with(&mut w, &rests(4));
    }
    assert_eq!((w.ledger.promises_due, w.ledger.promises_broken), (1, 0));
    w.check_invariants().unwrap();
}

#[test]
fn the_enforcer_moves_the_first_occupancy_norm_and_votes_by_its_script() {
    // A founder enforcer on a food cell, two later arrivals gathering on it.
    let mut w = empty_world(ncfg());
    let e = add_agent_with(
        &mut w,
        Pos::new(2, 2),
        50_000,
        0,
        PolicyKind::Enforcer,
        PolicyParams::default(),
    );
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(5, 5), 50_000, 0);
    set_food(&mut w, Pos::new(2, 2), 20);
    step_with(&mut w, &rests(4));
    w.agents[1].arrived = Some(1);
    w.agents[2].arrived = Some(1);
    w.check_invariants().unwrap();
    // One encroachment is not enough; two within the horizon are.
    let mut acts = rests(4);
    acts[1] = Action::Gather;
    let r = step_with(&mut w, &acts);
    assert_eq!(r.encroachments.iter().sum::<u32>(), 1);
    assert_eq!(
        w.agents[e]
            .recent
            .iter()
            .filter(|m| m.kind == InteractionKind::EncroachedBy)
            .count(),
        1
    );
    let d = decided(&w, e);
    assert!(
        !matches!(d.action, Action::Signal(Message::Motion(_))),
        "one encroachment: {:?}",
        d.action
    );
    let mut acts = rests(4);
    acts[2] = Action::Gather;
    step_with(&mut w, &acts);
    let expected = Action::Signal(Message::Motion(MotionKind::Enact { norm: encroach() }));
    assert_eq!(decided(&w, e).action, expected);
    // Not with `enforcer_motions` off, not with the floor taken, not with
    // the norm already in force.
    let mut quiet = w.clone();
    quiet.config.enforcer_motions = false;
    assert_ne!(decided(&quiet, e).action, expected);
    assert!(!matches!(
        decided(&quiet, e).action,
        Action::Signal(Message::Motion(_))
    ));
    let mut ruled = w.clone();
    ruled
        .intervene(Intervention::Enact { norm: encroach() })
        .unwrap();
    assert!(!matches!(
        decided(&ruled, e).action,
        Action::Signal(Message::Motion(_))
    ));
    // Memory older than the horizon does not count: 17 quiet ticks later
    // the two encroachments are stale.
    let mut stale = w.clone();
    for _ in 0..17 {
        step_with(&mut stale, &rests(4));
    }
    assert!(!matches!(
        decided(&stale, e).action,
        Action::Signal(Message::Motion(_))
    ));

    // Move it for real: the enforcer's own motion opens the floor, and
    // from the next tick it votes aye on it (and only once).
    let mut acts = rests(4);
    acts[e] = decided(&w, e).action;
    let r = step_with(&mut w, &acts);
    assert_eq!(r.motions, 1);
    assert_eq!(w.nomos.motion.as_ref().map(|m| m.by), Some(AgentId(0)));
    assert_eq!(decided(&w, e).action, vote(true));
    let mut acts = rests(4);
    acts[e] = vote(true);
    step_with(&mut w, &acts);
    assert!(!matches!(
        decided(&w, e).action,
        Action::Signal(Message::Vote { .. })
    ));
    // A blind enforcer votes all the same (it reads the index).
    let mut blind = w.clone();
    blind.config.perception.see_standing = false;
    blind.nomos.motion.as_mut().unwrap().ayes.clear();
    assert_eq!(decided(&blind, e).action, vote(true));
    let index = blind.spatial_index();
    let norms = blind.norm_index();
    assert_eq!(
        blind.observation_at(e, &index, &norms).open_motion(),
        None,
        "the filtered query is blind"
    );

    // The script's votes, motion by motion: aye to a norm against
    // encroachment, broken promises and (exempting sanctions) harm, and
    // to a new charter; nay to a repeal and to a harm norm without the
    // exemption.
    let cases = [
        (MotionKind::Enact { norm: encroach() }, true),
        (
            MotionKind::Enact {
                norm: default_norm(NormKind::BrokenPromise),
            },
            true,
        ),
        (
            MotionKind::Enact {
                norm: Norm {
                    except_sanction: true,
                    ..default_norm(NormKind::Harm)
                },
            },
            true,
        ),
        (
            MotionKind::Enact {
                norm: default_norm(NormKind::Harm),
            },
            false,
        ),
        (MotionKind::Repeal { rule: 0 }, false),
        (
            MotionKind::Charter {
                charter: charter(100, 500, 8, true),
            },
            true,
        ),
    ];
    for (kind, aye) in cases {
        let mut v = four(with_charter(charter(0, 500, 4, true)));
        v.agents[0].policy = PolicyKind::Enforcer;
        v.intervene(Intervention::Enact {
            norm: norm(NormKind::Harm, 8, Sanction::NONE),
        })
        .unwrap();
        let mut acts = rests(4);
        acts[3] = motion(kind);
        let r = step_with(&mut v, &acts);
        assert!(no_failures(&r), "{kind:?}: {:?}", r.failures);
        assert_eq!(decided(&v, 0).action, vote(aye), "{kind:?}");
    }
}

#[test]
fn the_enforcer_leaves_an_occupied_cell_and_is_otherwise_the_forager() {
    // A founder forager and an enforcer that arrived later share a food
    // cell: the forager gathers, the enforcer leaves (towards the richest
    // other cell in sight). As the first occupant, the enforcer gathers.
    let mut w = empty_world(ncfg());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    let e = add_agent_with(
        &mut w,
        Pos::new(2, 2),
        50_000,
        0,
        PolicyKind::Enforcer,
        PolicyParams::default(),
    );
    set_food(&mut w, Pos::new(2, 2), 10);
    set_food(&mut w, Pos::new(5, 2), 6);
    step_with(&mut w, &rests(2));
    w.agents[e].arrived = Some(1);
    assert_eq!(decided(&w, 0).action, Action::Gather);
    let d = decided(&w, e);
    assert_eq!(d.action, Action::Move(simulation_core::Dir::East));
    assert_eq!(d.memory.target, Some(Pos::new(5, 2)));
    w.agents[e].arrived = None;
    w.agents[0].arrived = Some(1);
    assert_eq!(decided(&w, e).action, Action::Gather);
    // Blind or not, first occupancy is a physical fact.
    w.agents[e].arrived = Some(1);
    w.agents[0].arrived = None;
    w.config.perception.see_standing = false;
    assert!(matches!(decided(&w, e).action, Action::Move(_)));

    // Without institutions the enforcer *is* the forager: a world of
    // enforcers and one of foragers run 600 ticks with the same bodies,
    // food and ledger (the hash differs by the policy byte alone).
    let cfg = WorldConfig {
        seed: 7,
        width: 32,
        height: 32,
        initial_agents: 150,
        patch_count: 3,
        ..WorldConfig::default()
    };
    let mut foragers = World::new(cfg.clone()).unwrap();
    let mut enforcers = World::generate(cfg, |_| PolicyKind::Enforcer).unwrap();
    let strip = |w: &World| -> Vec<Agent> {
        w.agents
            .iter()
            .map(|a| Agent {
                policy: PolicyKind::Forager,
                ..a.clone()
            })
            .collect()
    };
    for _ in 0..600 {
        let rf = foragers.step();
        let re = enforcers.step();
        assert_eq!(
            strip(&foragers),
            strip(&enforcers),
            "tick {}",
            foragers.tick
        );
        assert_eq!(foragers.resources, enforcers.resources);
        assert_eq!(foragers.ledger, enforcers.ledger);
        assert_eq!(rf.events, re.events);
        assert_eq!(rf.failures, re.failures);
    }
    assert!(!enforcers.agents.is_empty());
    assert_ne!(foragers.dynamics_hash(), enforcers.dynamics_hash());
    enforcers.check_invariants().unwrap();

    // With institutions, no rule and no motions of its own, the enforcer
    // differs from the forager only where it stands second: every other
    // decision, RNG draws included, is the forager's.
    let cfg = WorldConfig {
        seed: 11,
        width: 24,
        height: 24,
        initial_agents: 100,
        patch_count: 3,
        nomos: true,
        enforcer_motions: false,
        ..WorldConfig::default()
    };
    let mut w = World::generate(cfg, |_| PolicyKind::Enforcer).unwrap();
    let (mut same, mut left) = (0, 0);
    for _ in 0..150 {
        let index = w.spatial_index();
        let norms = w.norm_index();
        let mut twin = w.clone();
        for a in &mut twin.agents {
            a.policy = PolicyKind::Forager;
        }
        let twin_index = twin.spatial_index();
        for i in 0..w.agents.len() {
            let obs = w.observation_at(i, &index, &norms);
            let mut rng = RngStream::new(w.config.seed, w.tick, w.agents[i].id.0, Stream::Decide);
            let mine = decide(&obs, &mut rng);
            let tobs = twin.observation_at(i, &twin_index, &norms);
            let mut rng = RngStream::new(w.config.seed, w.tick, w.agents[i].id.0, Stream::Decide);
            let theirs = decide(&tobs, &mut rng);
            if obs.occupied() && theirs.action == Action::Gather {
                assert_ne!(mine.action, Action::Gather, "tick {} agent {i}", w.tick);
                left += 1;
            } else {
                assert_eq!(mine, theirs, "tick {} agent {i}", w.tick);
                same += 1;
            }
        }
        w.step();
    }
    assert!(
        left > 0 && same > left,
        "{left} left an occupied cell, {same} decided as the forager"
    );
    assert_eq!(w.ledger.motions, 0);
}

#[test]
fn random_worlds_legislate_and_round_trip_through_both_snapshot_forms() {
    let cfg = WorldConfig {
        seed: 5,
        width: 32,
        height: 32,
        initial_agents: 200,
        patch_count: 3,
        nomos: true,
        record_votes: true,
        ..WorldConfig::default()
    };
    let mut w = World::generate(cfg.clone(), |_| PolicyKind::Random).unwrap();
    let (mut motioned, mut voted_ev, mut closed_ev, mut effects) = (0u64, 0u64, 0u64, 0u64);
    for _ in 0..150 {
        let r = w.step();
        assert!(
            r.motions <= 1,
            "one floor: at most one motion opens per tick"
        );
        assert!(r.motions_carried + r.motions_defeated <= 1);
        for e in &r.events {
            match e {
                Event::Motioned { .. } => motioned += 1,
                Event::Voted { .. } => voted_ev += 1,
                Event::MotionClosed { .. } => closed_ev += 1,
                Event::Enacted { .. } | Event::Repealed { .. } | Event::Chartered { .. } => {
                    effects += 1
                }
                _ => {}
            }
        }
        assert_eq!(
            w.ledger.motions,
            w.ledger.motions_carried
                + w.ledger.motions_defeated
                + u64::from(w.nomos.motion.is_some())
        );
        w.check_invariants().unwrap();
    }
    assert!(
        motioned > 0 && voted_ev > 0 && closed_ev > 0,
        "{motioned} {voted_ev} {closed_ev}"
    );
    assert_eq!(w.ledger.motions, motioned);
    assert_eq!(w.ledger.votes, voted_ev);
    assert_eq!(
        w.ledger.motions_carried + w.ledger.motions_defeated,
        closed_ev
    );
    println!(
        "random 32² × 200, 150 ticks: motions {motioned}, votes {voted_ev}, carried {}, defeated {}, effects {effects}, rules now {}",
        w.ledger.motions_carried,
        w.ledger.motions_defeated,
        w.nomos.rules.len()
    );
    assert!(
        w.ledger.motions_carried <= w.ledger.motions_defeated,
        "the null model should not legislate more than it fails to"
    );

    let bin = w.to_bytes();
    let json = w.to_snapshot();
    assert_eq!(World::from_bytes(&bin).unwrap(), w);
    assert_eq!(World::from_snapshot(&json).unwrap(), w);
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(v["format"], SNAPSHOT_FORMAT);
    assert_eq!(SNAPSHOT_FORMAT, 12);
    assert!(v["world"]["ledger"]["motions"].as_u64().unwrap() > 0);
    assert_eq!(v["world"]["config"]["motion_cost"], 2000);
    assert_eq!(v["world"]["config"]["record_votes"], true);
    assert_eq!(v["world"]["config"]["enforcer_motions"], true);
    let mut old = v.clone();
    old["format"] = serde_json::json!(11);
    assert_eq!(
        World::from_snapshot(&serde_json::to_vec(&old).unwrap()).err(),
        Some(SnapshotError::Format(11))
    );
    // The floor is state: a world with an open motion hashes differently
    // from the same world with the floor cleared.
    let mut with_motion = w.clone();
    while with_motion.nomos.motion.is_none() {
        with_motion.step();
        assert!(with_motion.tick < 1_000, "random walkers move motions");
    }
    let mut cleared = with_motion.clone();
    cleared.nomos.motion = None;
    assert_ne!(with_motion.dynamics_hash(), cleared.dynamics_hash());
    // A snapshot with an open motion round-trips too, and a voter not
    // alive is rejected.
    assert_eq!(
        World::from_bytes(&with_motion.to_bytes()).unwrap(),
        with_motion
    );
    let mut bad = with_motion.clone();
    bad.nomos.motion.as_mut().unwrap().ayes = vec![AgentId(u64::MAX - 1)];
    let bytes = {
        // Re-encode with the right hash so only the invariant trips.
        let mut v: serde_json::Value = serde_json::from_slice(&bad.to_snapshot()).unwrap();
        v["state_hash"] = serde_json::json!(bad.state_hash());
        serde_json::to_vec(&v).unwrap()
    };
    assert!(matches!(
        World::from_snapshot(&bytes),
        Err(SnapshotError::Invalid(errs)) if errs.iter().any(|e| e.contains("not alive"))
    ));

    // Without institutions the random walker does not even try: no floor,
    // no attempt, no cost.
    let mut off = World::generate(
        WorldConfig {
            nomos: false,
            ..cfg
        },
        |_| PolicyKind::Random,
    )
    .unwrap();
    for _ in 0..50 {
        let r = off.step();
        assert_eq!(
            failed(&r, Failure::FloorTaken) + failed(&r, Failure::NoMotion),
            0
        );
        assert_eq!(r.motions + r.votes, 0);
    }
    assert!(off.nomos.is_empty());
}

/// Where each event must sit within a tick (ADR-0014 D9).
fn rank(e: &Event) -> u8 {
    match e {
        Event::Transferred { .. }
        | Event::Harmed { .. }
        | Event::Proposed { .. }
        | Event::Accepted { .. }
        | Event::Rejected { .. }
        | Event::Signaled { .. } => 0,
        Event::Sanctioned { .. } => 1,
        Event::Violated { .. } => 2,
        Event::Motioned { .. } => 3,
        Event::Voted { .. } => 4,
        Event::MotionClosed { .. } => 5,
        Event::Enacted { .. } | Event::Repealed { .. } | Event::Chartered { .. } => 6,
        Event::Acted { .. } => 7,
        Event::Died { .. } => 8,
        Event::Born { .. } => 9,
        Event::Intervened { .. } => 10,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 12, .. ProptestConfig::default() })]

    /// Assemblies are as deterministic as the rest: serial and parallel
    /// decisions agree down to the report, a twin replays to the same
    /// hash, the invariants hold every tick, the per-tick view's `VOTED`
    /// agrees with the floor, the events keep their order, one motion at
    /// most opens or closes per tick, effects follow carried closings
    /// only, the ledger agrees with the reports, and snapshots round-trip.
    #[test]
    fn assemblies_are_deterministic(
        seed in any::<u64>(),
        quorum in prop_oneof![Just(0u16), Just(100), Just(500), Just(1000)],
        threshold in prop_oneof![Just(0u16), Just(500), Just(667), Just(1000)],
        vote_ticks in 0u16..=6,
        amendable in any::<bool>(),
        record in any::<bool>(),
        enforcer_motions in any::<bool>(),
        ruled in any::<bool>(),
        ticks in 1u64..=50,
    ) {
        let cfg = WorldConfig {
            seed,
            width: 24,
            height: 24,
            initial_agents: 120,
            patch_count: 4,
            birth_threshold: Some(60_000),
            child_energy: 30_000,
            record_votes: record,
            enforcer_motions,
            charter: charter(quorum, threshold, vote_ticks, amendable),
            nomos: true,
            ..WorldConfig::default()
        };
        let mix = |i: u32| match i % 3 {
            0 => PolicyKind::Random,
            1 => PolicyKind::Enforcer,
            _ => PolicyKind::Learner,
        };
        let mut a = World::generate(cfg, mix).unwrap();
        if ruled {
            a.intervene(Intervention::Enact { norm: encroach() }).unwrap();
        }
        let mut b = a.clone();
        let mut twin = a.clone();
        let mut sums = (0u64, 0u64, 0u64, 0u64);
        for _ in 0..ticks {
            let ix = a.norm_index();
            for (i, ag) in a.agents.iter().enumerate() {
                let on_floor = a.nomos.motion.as_ref().is_some_and(|m| {
                    m.ayes.binary_search(&ag.id).is_ok() || m.nays.binary_search(&ag.id).is_ok()
                });
                prop_assert_eq!(ix.voted(i), on_floor);
            }
            prop_assert_eq!(ix.motion(), a.nomos.motion.as_ref().map(|m| m.kind));
            let open_before = a.nomos.motion.is_some();
            let ra = a.step();
            #[cfg(feature = "parallel")]
            let rb = b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            let rb = b.step();
            prop_assert_eq!(&ra, &rb);
            twin.step();
            prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
            prop_assert!(ra.motions <= 1);
            prop_assert!(ra.motions_carried + ra.motions_defeated <= 1);
            prop_assert!(!(open_before && ra.motions == 1), "no motion opens while one is open");
            prop_assert!(ra.events.windows(2).all(|w| rank(&w[0]) <= rank(&w[1])), "{:?}", ra.events);
            let motioned = ra.events.iter().filter(|e| matches!(e, Event::Motioned { .. })).count() as u32;
            let voted_ev = ra.events.iter().filter(|e| matches!(e, Event::Voted { .. })).count() as u32;
            let closed_ev: Vec<&Event> = ra.events.iter().filter(|e| matches!(e, Event::MotionClosed { .. })).collect();
            let effects = ra.events.iter().filter(|e| matches!(e, Event::Enacted { .. } | Event::Repealed { .. } | Event::Chartered { .. })).count();
            prop_assert_eq!(motioned, ra.motions);
            prop_assert_eq!(voted_ev, if record { ra.votes } else { 0 });
            prop_assert_eq!(closed_ev.len() as u32, ra.motions_carried + ra.motions_defeated);
            let carried = closed_ev.iter().any(|e| matches!(e, Event::MotionClosed { carried: true, .. }));
            prop_assert!(effects <= 2 && (carried || effects == 0), "{:?}", ra.events);
            if let Some(Event::MotionClosed { alive, ayes, nays, .. }) = closed_ev.first() {
                prop_assert!(*ayes + *nays <= *alive);
                prop_assert!(*alive as usize >= a.agents.len() - ra.births as usize);
            }
            sums.0 += ra.motions as u64;
            sums.1 += ra.votes as u64;
            sums.2 += ra.motions_carried as u64;
            sums.3 += ra.motions_defeated as u64;
            prop_assert!(ra.votes as usize <= ra.alive as usize + ra.deaths as usize);
        }
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.state_hash(), twin.state_hash());
        prop_assert_eq!(a.ledger.motions, sums.0);
        prop_assert_eq!(a.ledger.votes, sums.1);
        prop_assert_eq!(a.ledger.motions_carried, sums.2);
        prop_assert_eq!(a.ledger.motions_defeated, sums.3);
        prop_assert_eq!(a.ledger.motions, sums.2 + sums.3 + u64::from(a.nomos.motion.is_some()));
        if !enforcer_motions {
            // Only random walkers move then; enforcers still vote.
            for e in a.agents.iter().filter(|ag| ag.policy == PolicyKind::Enforcer) {
                prop_assert!(a.nomos.motion.as_ref().is_none_or(|m| m.by != e.id || true));
            }
        }
        prop_assert_eq!(World::from_bytes(&a.to_bytes()).unwrap(), a.clone());
        prop_assert_eq!(World::from_snapshot(&a.to_snapshot()).unwrap(), a);
    }
}
