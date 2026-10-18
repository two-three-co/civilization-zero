//! Institutions (ADR-0014 D2, D4, D5, D8; task 4-2): rules in force turn
//! the social facts of 4-1 into violations, violators stand accused for a
//! while, harms and rejections that reach the accused are read as
//! sanctions — and none of it moves a unit of food or health.

mod common;

use common::*;
use proptest::prelude::*;
use simulation_core::world::SNAPSHOT_FORMAT;
use simulation_core::{
    Action, AgentId, Charter, DeathCause, Dir, Event, Grounds, Interaction, InteractionKind,
    Intervention, InterventionError, Ledger, Message, Motion, MotionKind, Norm, NormIndex,
    NormKind, PolicyKind, PolicyParams, Pos, Rule, Sanction, SnapshotError, Standing, TickReport,
    Via, World, WorldConfig,
};

const ENCROACH: usize = 0;
const PROMISE: usize = 1;
const HARM: usize = 2;

/// The hand-built world of `common::cfg()` with institutions on.
fn ncfg() -> WorldConfig {
    WorldConfig {
        nomos: true,
        ..cfg()
    }
}

/// … and every violation recorded.
fn rcfg() -> WorldConfig {
    WorldConfig {
        record_violations: true,
        ..ncfg()
    }
}

fn norm(kind: NormKind, standing_ticks: u16, sanction: Sanction, except: bool) -> Norm {
    Norm {
        kind,
        standing_ticks,
        sanction,
        except_sanction: except,
    }
}

fn enact(w: &mut World, n: Norm) -> Rule {
    w.intervene(Intervention::Enact { norm: n }).unwrap();
    *w.nomos.rules.last().unwrap()
}

fn harm(target: u64) -> Action {
    Action::Harm {
        target: AgentId(target),
    }
}

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

fn standing(
    agent: u64,
    rule: u16,
    since: u64,
    last: u64,
    against: u64,
    v: u16,
    s: u16,
) -> Standing {
    Standing {
        agent: AgentId(agent),
        rule,
        since,
        last,
        against: AgentId(against),
        violations: v,
        sanctions: s,
    }
}

fn sanctioned(tick: u64, by: u64, target: u64, rule: u16, via: Via) -> Event {
    Event::Sanctioned {
        tick,
        by: AgentId(by),
        target: AgentId(target),
        rule,
        via,
    }
}

fn violated(
    tick: u64,
    agent: u64,
    rule: u16,
    norm: NormKind,
    against: u64,
    grounds: Grounds,
) -> Event {
    Event::Violated {
        tick,
        agent: AgentId(agent),
        rule,
        norm,
        against: AgentId(against),
        grounds,
    }
}

fn by_kind(k: usize, n: u32) -> [u32; NormKind::COUNT] {
    let mut a = [0; NormKind::COUNT];
    a[k] = n;
    a
}

/// The physical part of an agent: everything a rule must not touch.
fn body(w: &World) -> Vec<(AgentId, Pos, i64, i64, u32, u64, PolicyParams)> {
    w.agents
        .iter()
        .map(|a| {
            (
                a.id,
                a.pos,
                a.energy,
                a.health,
                a.inventory,
                a.age,
                a.params.clone(),
            )
        })
        .collect()
}

/// The ledger with the institutional readings blanked: what must agree
/// between a world with rules and one without.
fn physical(l: &Ledger) -> Ledger {
    Ledger {
        violations: [0; NormKind::COUNT],
        sanctions: [0; NormKind::COUNT],
        episodes_closed: [0; NormKind::COUNT],
        episodes_sanctioned: [0; NormKind::COUNT],
        encroachments: 0,
        promises_due: 0,
        promises_broken: 0,
        interventions: 0,
        ..l.clone()
    }
}

/// Events minus the institutional readings.
fn physical_events(r: &TickReport) -> Vec<Event> {
    r.events
        .iter()
        .copied()
        .filter(|e| !matches!(e, Event::Sanctioned { .. } | Event::Violated { .. }))
        .collect()
}

/// Memory with the sanction kinds read back as the harm kinds they stand
/// in for, so a rule world's memory can be compared with a rule-free one.
fn as_harm(kind: InteractionKind) -> InteractionKind {
    match kind {
        InteractionKind::Sanctioned => InteractionKind::Harmed,
        InteractionKind::SanctionedBy => InteractionKind::HarmedBy,
        InteractionKind::SawSanction => InteractionKind::SawHarm,
        k => k,
    }
}

#[test]
fn an_encroachment_is_a_violation_under_a_rule_and_a_harm_on_the_accused_is_a_sanction() {
    use Action::{Gather, Move, Rest};
    // A (0) founder on the food cell, C (1) founder next to it, W (2) a
    // witness two cells from A's cell.
    let script: Vec<Vec<Action>> = vec![
        vec![Rest, Move(Dir::West), Rest], // 0: C steps onto A's cell
        vec![Rest, Gather, Rest],          // 1: C gathers where A was first
        vec![harm(1), Rest, Rest],         // 2: A harms C
        vec![Rest, Rest, Rest],            // 3
        vec![Rest, Rest, Rest],            // 4: C's accusation ends here
        vec![harm(1), Rest, Rest],         // 5: A harms C again
    ];
    let run = |c: WorldConfig, rule: Option<Norm>| -> (World, Vec<TickReport>) {
        let mut w = empty_world(c);
        add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
        add_agent(&mut w, Pos::new(3, 2), 50_000, 0);
        add_agent(&mut w, Pos::new(2, 4), 50_000, 0);
        set_food(&mut w, Pos::new(2, 2), 40);
        if let Some(n) = rule {
            enact(&mut w, n);
        }
        let reports = script.iter().map(|s| w.apply(intents(s))).collect();
        (w, reports)
    };
    let rule = norm(NormKind::Encroach, 3, Sanction::HARM, false);
    let (w, r) = run(rcfg(), Some(rule));
    assert_eq!(
        w.nomos.rules,
        vec![Rule {
            id: 0,
            norm: rule,
            enacted: 0
        }]
    );

    // Tick 1: the encroachment of 4-1 is now a violation of rule 0 —
    // recorded, counted, and C stands accused from the next tick.
    assert_eq!(r[1].encroachments, [1, 0, 0, 0, 0]);
    assert_eq!(r[1].violations, by_kind(ENCROACH, 1));
    assert_eq!(
        r[1].events,
        vec![violated(
            1,
            1,
            0,
            NormKind::Encroach,
            0,
            Grounds::Encroachment {
                cell: Pos::new(2, 2),
                amount: 2
            }
        )]
    );
    assert_eq!(w.ledger.violations, [1, 0, 0]);
    // Tick 2: A's harm reaches an accused agent under a rule that lets a
    // harm answer: a sanction, after the harm it reads, and remembered as
    // one by actor, target and witness.
    assert_eq!(
        r[2].events,
        vec![
            Event::Harmed {
                tick: 2,
                by: AgentId(0),
                target: AgentId(1),
                damage: 20
            },
            sanctioned(2, 0, 1, 0, Via::Harm),
        ]
    );
    assert_eq!(r[2].sanctions, by_kind(ENCROACH, 1));
    assert_eq!(
        r[2].violations, [0; 3],
        "no harm rule: the harm is nobody's violation"
    );
    assert_eq!(
        w.agents[1].health,
        100 - 2 * 20 + 4,
        "two harms taken and four ticks of rest, as physics says"
    );
    // Ticks 3–4: accused through tick 1 + 3 = 4, closed at the end of 4.
    assert_eq!(r[3].episodes_closed, [0; 3]);
    assert_eq!(r[4].episodes_closed, by_kind(ENCROACH, 1));
    assert_eq!(r[4].episodes_sanctioned, by_kind(ENCROACH, 1));
    // Tick 5: no longer accused — an ordinary harm.
    assert_eq!(
        r[5].events,
        vec![Event::Harmed {
            tick: 5,
            by: AgentId(0),
            target: AgentId(1),
            damage: 20
        }]
    );
    assert_eq!(r[5].sanctions, [0; 3]);
    assert_eq!(
        w.agents[0].recent,
        vec![
            note(1, 1, InteractionKind::EncroachedBy, 2),
            note(2, 1, InteractionKind::Sanctioned, 20),
            note(5, 1, InteractionKind::Harmed, 20),
        ]
    );
    assert_eq!(
        w.agents[1].recent,
        vec![
            note(2, 0, InteractionKind::SanctionedBy, 20),
            note(5, 0, InteractionKind::HarmedBy, 20),
        ]
    );
    assert_eq!(
        w.agents[2].recent,
        vec![
            note(2, 0, InteractionKind::SawSanction, 1),
            note(5, 0, InteractionKind::SawHarm, 1),
        ]
    );
    assert!(w.nomos.standing.is_empty());
    assert_eq!(w.ledger.sanctions, [1, 0, 0]);
    assert_eq!(w.ledger.episodes_closed, [1, 0, 0]);
    assert_eq!(w.ledger.episodes_sanctioned, [1, 0, 0]);
    w.check_invariants().unwrap();

    // The episode as the state carries it between ticks 1 and 4, and the
    // per-tick view built from it.
    let mut mid = empty_world(rcfg());
    add_agent(&mut mid, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut mid, Pos::new(3, 2), 50_000, 0);
    add_agent(&mut mid, Pos::new(2, 4), 50_000, 0);
    set_food(&mut mid, Pos::new(2, 2), 40);
    enact(&mut mid, rule);
    let ix = mid.norm_index();
    assert!(
        !ix.is_empty(),
        "a rule in force builds a view even before any episode"
    );
    assert!((0..3).all(|i| ix.flags(i) == 0));
    mid.apply(intents(&script[0]));
    mid.apply(intents(&script[1]));
    assert_eq!(mid.nomos.standing, vec![standing(1, 0, 1, 1, 0, 1, 0)]);
    let ix = mid.norm_index();
    assert!(!ix.is_empty());
    assert_eq!(ix.flags(1), NormIndex::ACCUSED | NormIndex::HARMABLE);
    assert_eq!((ix.flags(0), ix.flags(2)), (0, 0));
    mid.apply(intents(&script[2]));
    assert_eq!(mid.nomos.standing, vec![standing(1, 0, 1, 1, 0, 1, 1)]);
    mid.check_invariants().unwrap();

    // Not recording violations changes the events only.
    let (quiet, qr) = run(ncfg(), Some(rule));
    assert_eq!(quiet.ledger, w.ledger);
    assert_eq!(quiet.nomos, w.nomos);
    assert_eq!(body(&quiet), body(&w));
    for (a, b) in qr.iter().zip(&r) {
        assert_eq!(a.violations, b.violations);
        assert_eq!(a.sanctions, b.sanctions);
        assert_eq!(
            a.events,
            b.events
                .iter()
                .copied()
                .filter(|e| !matches!(e, Event::Violated { .. }))
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(quiet.dynamics_hash(), w.dynamics_hash());

    // Physics is untouched: the same intents in a world that tracks but
    // has no rule, and in one that tracks nothing, move the same food and
    // health, and the rule-free worlds remember the harms as harms.
    let (facts, fr) = run(ncfg(), None);
    let (off, offr) = run(cfg(), None);
    for other in [&facts, &off] {
        assert_eq!(body(other), body(&w));
        assert_eq!(other.resources, w.resources);
        assert_eq!(physical(&other.ledger), physical(&w.ledger));
        assert_eq!(other.ledger.violations, [0; 3]);
        assert_eq!(other.ledger.sanctions, [0; 3]);
        assert!(other.nomos.standing.is_empty() && other.nomos.rules.is_empty());
    }
    for (a, b) in fr.iter().zip(&offr) {
        assert_eq!(a.events, b.events);
    }
    for (other, reports) in [(&facts, &fr), (&off, &offr)] {
        for (a, b) in reports.iter().zip(&r) {
            assert_eq!(a.events, physical_events(b));
            assert_eq!(a.violations, [0; 3]);
            assert_eq!(a.sanctions, [0; 3]);
        }
        for (x, y) in other.agents.iter().zip(&w.agents) {
            let expected: Vec<Interaction> = y
                .recent
                .iter()
                .map(|e| Interaction {
                    kind: as_harm(e.kind),
                    ..*e
                })
                .filter(|e| other.config.nomos || e.kind != InteractionKind::EncroachedBy)
                .collect();
            assert_eq!(x.recent, expected);
        }
    }
    assert_eq!(facts.ledger.encroachments, w.ledger.encroachments);
    assert_eq!(off.ledger.encroachments, 0);
}

#[test]
fn a_harm_rule_accuses_attackers_and_except_sanction_spares_the_sanctioners() {
    use Action::Rest;
    // V (1) and W (2) are adjacent to S (0); V and W are two apart.
    let script: Vec<Vec<Action>> = vec![
        vec![harm(1), Rest, Rest], // 0: S harms V, unprovoked
        vec![Rest, harm(0), Rest], // 1: V answers the accused S
        vec![Rest, Rest, harm(0)], // 2: so does W
        vec![Rest, Rest, Rest],    // 3
        vec![Rest, Rest, Rest],    // 4: S's accusation (0 + 4) ends
    ];
    let run = |n: Norm| -> (World, Vec<TickReport>) {
        let mut w = empty_world(rcfg());
        add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
        add_agent(&mut w, Pos::new(3, 2), 50_000, 0);
        add_agent(&mut w, Pos::new(2, 3), 50_000, 0);
        enact(&mut w, n);
        let reports = script.iter().map(|s| w.apply(intents(s))).collect();
        (w, reports)
    };
    let harmed = |tick: u64, by: u64, target: u64| Event::Harmed {
        tick,
        by: AgentId(by),
        target: AgentId(target),
        damage: 20,
    };
    let v = |tick: u64, by: u64, target: u64| {
        violated(
            tick,
            by,
            0,
            NormKind::Harm,
            target,
            Grounds::Harm { damage: 20 },
        )
    };

    // Sanctions exempt: the answers are sanctions and nothing else.
    let (w, r) = run(norm(NormKind::Harm, 4, Sanction::HARM, true));
    assert_eq!(r[0].events, vec![harmed(0, 0, 1), v(0, 0, 1)]);
    assert_eq!(
        r[1].events,
        vec![harmed(1, 1, 0), sanctioned(1, 1, 0, 0, Via::Harm)]
    );
    assert_eq!(
        r[2].events,
        vec![harmed(2, 2, 0), sanctioned(2, 2, 0, 0, Via::Harm)]
    );
    assert_eq!(w.ledger.violations, [0, 0, 1]);
    assert_eq!(w.ledger.sanctions, [0, 0, 2]);
    assert_eq!(r[4].episodes_closed, by_kind(HARM, 1));
    assert_eq!(w.ledger.episodes_sanctioned, [0, 0, 1]);
    assert!(w.nomos.standing.is_empty());
    assert_eq!(
        w.agents[0].health,
        100 - 2 * 20 + 4,
        "two harms, four rests"
    );
    assert_eq!(w.agents[1].health, 100 - 20 + 4, "one harm, four rests");
    w.check_invariants().unwrap();

    // Sanctions not exempt: every harm is a violation, sanction or not,
    // so the sanctioners stand accused in their turn.
    let (w, r) = run(norm(NormKind::Harm, 4, Sanction::HARM, false));
    assert_eq!(r[0].events, vec![harmed(0, 0, 1), v(0, 0, 1)]);
    assert_eq!(
        r[1].events,
        vec![
            harmed(1, 1, 0),
            sanctioned(1, 1, 0, 0, Via::Harm),
            v(1, 1, 0)
        ]
    );
    assert_eq!(
        r[2].events,
        vec![
            harmed(2, 2, 0),
            sanctioned(2, 2, 0, 0, Via::Harm),
            v(2, 2, 0)
        ]
    );
    assert_eq!(w.ledger.violations, [0, 0, 3]);
    assert_eq!(w.ledger.sanctions, [0, 0, 2]);
    assert_eq!(
        w.nomos.standing,
        vec![standing(1, 0, 1, 1, 0, 1, 0), standing(2, 0, 2, 2, 0, 1, 0)],
        "S's episode closed at tick 4; V's and W's are open"
    );
    w.check_invariants().unwrap();

    // A rule that allows no sanction: the accused are named, but no
    // answer counts, so V's and W's harms are plain harms (and
    // violations), remembered as harms.
    let (w, r) = run(norm(NormKind::Harm, 4, Sanction::NONE, false));
    assert_eq!(r[1].events, vec![harmed(1, 1, 0), v(1, 1, 0)]);
    assert_eq!(w.ledger.sanctions, [0; 3]);
    assert_eq!(w.ledger.violations, [0, 0, 3]);
    assert_eq!(w.ledger.episodes_sanctioned, [0; 3]);
    assert_eq!(
        w.agents[0].recent,
        vec![
            note(0, 1, InteractionKind::Harmed, 20),
            note(1, 1, InteractionKind::HarmedBy, 20),
            note(2, 2, InteractionKind::HarmedBy, 20),
        ]
    );
    let mut mid = empty_world(rcfg());
    add_agent(&mut mid, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut mid, Pos::new(3, 2), 50_000, 0);
    enact(&mut mid, norm(NormKind::Harm, 4, Sanction::NONE, false));
    mid.apply(intents(&[harm(1), Rest]));
    let ix = mid.norm_index();
    assert!(ix.accused(0) && !ix.harmable(0) && !ix.rejectable(0));
}

#[test]
fn a_broken_promise_accuses_the_short_side_and_a_rejection_can_be_its_sanction() {
    use Action::Rest;
    // A (0) and B (1) adjacent, five food each.
    let script: Vec<(Option<Intervention>, Vec<Action>)> = vec![
        (None, vec![propose(1, 2, 3, 4), Rest]), // 0: due at 4
        (None, vec![Rest, accept(0, 0)]),        // 1
        (None, vec![transfer(1, 2), Rest]),      // 2: A delivers
        (None, vec![Rest, Rest]),                // 3
        (None, vec![Rest, Rest]),                // 4: B is short by 3
        (None, vec![reject(1), Rest]),           // 5: A rejects the accused B
        (None, vec![propose(1, 1, 1, 2), Rest]), // 6: due at 8
        (None, vec![Rest, accept(0, 6)]),        // 7
        (None, vec![Rest, Rest]),                // 8: both short
        (None, vec![propose(1, 1, 0, 2), Rest]), // 9: due at 11
        (None, vec![Rest, accept(0, 9)]),        // 10
        (
            // 11: A has left the world: a broken promise, no violator.
            Some(Intervention::RemoveAgent { id: AgentId(0) }),
            vec![Rest],
        ),
    ];
    let run = |rule: Option<Norm>| -> (World, Vec<TickReport>) {
        let mut w = empty_world(rcfg());
        add_agent(&mut w, Pos::new(2, 2), 50_000, 5);
        add_agent(&mut w, Pos::new(3, 2), 50_000, 5);
        if let Some(n) = rule {
            enact(&mut w, n);
        }
        let mut reports = Vec::new();
        for (iv, actions) in &script {
            if let Some(iv) = iv {
                w.intervene(*iv).unwrap();
            }
            reports.push(w.apply(intents(actions)));
        }
        (w, reports)
    };
    let (w, r) = run(Some(norm(
        NormKind::BrokenPromise,
        5,
        Sanction::REJECT,
        false,
    )));
    assert!(r.iter().all(|t| t.failures.iter().sum::<u32>() == 0));
    assert_eq!(
        r[4].events,
        vec![violated(
            4,
            1,
            0,
            NormKind::BrokenPromise,
            0,
            Grounds::BrokenPromise {
                offer_tick: 0,
                short: 3
            }
        )]
    );
    assert_eq!(r[4].promises_settled.len(), 1);
    assert_eq!(
        r[5].events,
        vec![
            Event::Rejected {
                tick: 5,
                from: AgentId(0),
                to: AgentId(1)
            },
            sanctioned(5, 0, 1, 0, Via::Reject),
        ]
    );
    // Both short: the proposer first, then the acceptor, whose episode is
    // refreshed rather than reopened.
    assert_eq!(
        r[8].events,
        vec![
            violated(
                8,
                0,
                0,
                NormKind::BrokenPromise,
                1,
                Grounds::BrokenPromise {
                    offer_tick: 6,
                    short: 1
                }
            ),
            violated(
                8,
                1,
                0,
                NormKind::BrokenPromise,
                0,
                Grounds::BrokenPromise {
                    offer_tick: 6,
                    short: 1
                }
            ),
        ]
    );
    let mut after8 = empty_world(rcfg());
    add_agent(&mut after8, Pos::new(2, 2), 50_000, 5);
    add_agent(&mut after8, Pos::new(3, 2), 50_000, 5);
    enact(
        &mut after8,
        norm(NormKind::BrokenPromise, 5, Sanction::REJECT, false),
    );
    for (_, actions) in &script[..9] {
        after8.apply(intents(actions));
    }
    assert_eq!(
        after8.nomos.standing,
        vec![standing(0, 0, 8, 8, 1, 1, 0), standing(1, 0, 4, 8, 0, 2, 1)]
    );
    assert_eq!(
        after8.agents[0].recent.iter().find(|e| e.tick == 5),
        Some(&note(5, 1, InteractionKind::Sanctioned, 0))
    );
    assert_eq!(
        after8.agents[1].recent.iter().find(|e| e.tick == 5),
        Some(&note(5, 0, InteractionKind::SanctionedBy, 0)),
        "the sanction stands in for the refusal"
    );
    after8.check_invariants().unwrap();
    // Tick 11: A's removal closed A's episode; its unpaid promise is
    // broken but accuses nobody.
    assert_eq!(r[11].events, vec![]);
    assert_eq!(r[11].promises_settled.len(), 1);
    assert_eq!((w.ledger.promises_due, w.ledger.promises_broken), (3, 3));
    assert_eq!(w.ledger.violations, [0, 3, 0]);
    assert_eq!(w.ledger.sanctions, [0, 1, 0]);
    assert_eq!(w.ledger.episodes_closed, [0, 1, 0], "A's, by removal");
    assert_eq!(w.nomos.standing, vec![standing(1, 0, 4, 8, 0, 2, 1)]);
    w.check_invariants().unwrap();

    // Without the rule: the same deals, the same shortfalls, no accusation
    // — and the rejection is remembered as one.
    let (plain, pr) = run(None);
    assert_eq!(body(&plain), body(&w));
    assert_eq!(physical(&plain.ledger), physical(&w.ledger));
    assert_eq!(
        (plain.ledger.promises_due, plain.ledger.promises_broken),
        (3, 3)
    );
    assert_eq!(plain.ledger.violations, [0; 3]);
    for (a, b) in pr.iter().zip(&r) {
        assert_eq!(a.events, physical_events(b));
        assert_eq!(a.promises_settled, b.promises_settled);
    }
    let b = plain.agents.iter().find(|a| a.id == AgentId(1)).unwrap();
    assert_eq!(
        b.recent.iter().find(|e| e.tick == 5),
        Some(&note(5, 0, InteractionKind::Rejected, 0))
    );
}

#[test]
fn a_sanction_answers_the_oldest_episode_whose_rule_allows_it() {
    use Action::{Gather, Move, Rest};
    // O (0) founder on the food cell, V (1) one cell north of it, S (2)
    // adjacent to the food cell.
    let mut w = empty_world(rcfg());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(2, 1), 50_000, 0);
    add_agent(&mut w, Pos::new(3, 2), 50_000, 0);
    set_food(&mut w, Pos::new(2, 2), 40);
    let encroach = enact(&mut w, norm(NormKind::Encroach, 10, Sanction::HARM, false));
    let harm_rule = enact(&mut w, norm(NormKind::Harm, 10, Sanction::HARM, false));
    assert_eq!((encroach.id, harm_rule.id), (0, 1));

    w.apply(intents(&[Rest, Move(Dir::South), Rest])); // 0: V arrives
    w.apply(intents(&[Rest, Gather, Rest])); // 1: V encroaches on O
    w.apply(intents(&[Rest, harm(0), Rest])); // 2: V harms O
    assert_eq!(
        w.nomos.standing,
        vec![standing(1, 0, 1, 1, 0, 1, 0), standing(1, 1, 2, 2, 0, 1, 0)]
    );
    // 3: S harms V, who stands accused under both rules; the older
    // episode (rule 0, since 1) takes the sanction. S's own harm is a
    // violation of rule 1 (no exemption).
    let r = w.apply(intents(&[Rest, Rest, harm(1)]));
    assert_eq!(
        r.events,
        vec![
            Event::Harmed {
                tick: 3,
                by: AgentId(2),
                target: AgentId(1),
                damage: 20
            },
            sanctioned(3, 2, 1, 0, Via::Harm),
            violated(3, 2, 1, NormKind::Harm, 1, Grounds::Harm { damage: 20 }),
        ]
    );
    assert_eq!(
        w.nomos.standing,
        vec![
            standing(1, 0, 1, 1, 0, 1, 1),
            standing(1, 1, 2, 2, 0, 1, 0),
            standing(2, 1, 3, 3, 1, 1, 0),
        ]
    );
    assert_eq!(w.ledger.sanctions, [1, 0, 0]);

    // Repeal the encroachment rule: its episode closes, sanctioned.
    w.intervene(Intervention::Repeal { rule: 0 }).unwrap();
    assert_eq!(w.ledger.episodes_closed, [1, 0, 0]);
    assert_eq!(w.ledger.episodes_sanctioned, [1, 0, 0]);
    assert_eq!(w.nomos.standing.len(), 2);
    // 4: the next harm answers the remaining episode, under rule 1.
    let r = w.apply(intents(&[Rest, Rest, harm(1)]));
    assert_eq!(r.events[1], sanctioned(4, 2, 1, 1, Via::Harm));
    assert_eq!(w.ledger.sanctions, [1, 0, 1]);

    // A new encroachment rule that only lets rejections answer (rule 2):
    // a harm skips the older-but-ineligible episode, a rejection finds it.
    enact(
        &mut w,
        norm(NormKind::Encroach, 10, Sanction::REJECT, false),
    );
    w.apply(intents(&[Rest, Gather, Rest])); // 5: V encroaches again
    assert_eq!(w.nomos.standing_of(AgentId(1)).len(), 2);
    let r = w.apply(intents(&[Rest, Rest, harm(1)])); // 6
    assert_eq!(r.events[1], sanctioned(6, 2, 1, 1, Via::Harm));
    let r = w.apply(intents(&[Rest, Rest, reject(1)])); // 7
    assert_eq!(
        r.events,
        vec![
            Event::Rejected {
                tick: 7,
                from: AgentId(2),
                to: AgentId(1)
            },
            sanctioned(7, 2, 1, 2, Via::Reject),
        ]
    );
    assert_eq!(w.ledger.sanctions, [2, 0, 2]);
    let ix = w.norm_index();
    assert_eq!(
        ix.flags(1),
        NormIndex::ACCUSED | NormIndex::HARMABLE | NormIndex::REJECTABLE
    );
    assert_eq!(ix.flags(2), NormIndex::ACCUSED | NormIndex::HARMABLE);
    assert_eq!(ix.flags(0), 0);
    w.check_invariants().unwrap();
}

#[test]
fn episodes_close_at_their_deadline_or_at_death_and_the_counters_give_s_and_s_prime() {
    use Action::{Gather, Move, Rest};
    // O (0) founder on the food cell, V (1) north of it, S (2) east of
    // it, U (3) south of it.
    let mut w = empty_world(rcfg());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(2, 1), 50_000, 0);
    add_agent(&mut w, Pos::new(3, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(2, 3), 50_000, 0);
    set_food(&mut w, Pos::new(2, 2), 40);
    enact(&mut w, norm(NormKind::Encroach, 2, Sanction::HARM, false));

    w.apply(intents(&[Rest, Move(Dir::South), Rest, Rest])); // 0
    let r = w.apply(intents(&[Rest, Gather, Rest, Rest])); // 1: episode opens
    assert_eq!(r.violations, by_kind(ENCROACH, 1));
    assert_eq!(w.nomos.standing, vec![standing(1, 0, 1, 1, 0, 1, 0)]);
    // 2: a second violation refreshes the episode (deadline 2 + 2 = 4),
    // and S's harm — judged against S(2), where V already stood accused —
    // is its first sanction. Harm first, then the sanction, then the
    // violation (ADR-0014 D9).
    let r = w.apply(intents(&[Rest, Gather, harm(1), Rest]));
    assert_eq!(
        r.events
            .iter()
            .map(std::mem::discriminant)
            .collect::<Vec<_>>(),
        vec![
            std::mem::discriminant(&Event::Harmed {
                tick: 0,
                by: AgentId(0),
                target: AgentId(0),
                damage: 0
            }),
            std::mem::discriminant(&sanctioned(0, 0, 0, 0, Via::Harm)),
            std::mem::discriminant(&violated(
                0,
                0,
                0,
                NormKind::Encroach,
                0,
                Grounds::Harm { damage: 0 }
            )),
        ]
    );
    assert_eq!(w.nomos.standing, vec![standing(1, 0, 1, 2, 0, 2, 1)]);
    let r = w.apply(intents(&[Rest, Rest, Rest, Rest])); // 3
    assert_eq!(r.episodes_closed, [0; 3]);
    assert!(w.norm_index().harmable(1), "still accused at tick 4");
    let r = w.apply(intents(&[Rest, Rest, Rest, Rest])); // 4: closes
    assert_eq!(r.episodes_closed, by_kind(ENCROACH, 1));
    assert_eq!(r.episodes_sanctioned, by_kind(ENCROACH, 1));
    assert!(w.nomos.standing.is_empty());
    assert!(w.norm_index().is_empty() || !w.norm_index().accused(1));

    // 5: a fresh episode; 6: V, worn down, dies of the sanction that
    // answers it — the episode closes with the death, counted as
    // sanctioned, and the death follows the sanction in the record.
    w.apply(intents(&[Rest, Gather, Rest, Rest]));
    assert_eq!(w.nomos.standing, vec![standing(1, 0, 5, 5, 0, 1, 0)]);
    w.agents[1].health = 20;
    // V eats rather than rests, so nothing heals it before phase 6.
    let r = w.apply(intents(&[Rest, Action::Consume(1), harm(1), Rest]));
    assert_eq!(r.deaths_by_cause[DeathCause::Violence.index()], 1);
    assert_eq!(
        r.events
            .iter()
            .map(std::mem::discriminant)
            .collect::<Vec<_>>(),
        vec![
            std::mem::discriminant(&Event::Harmed {
                tick: 0,
                by: AgentId(0),
                target: AgentId(0),
                damage: 0
            }),
            std::mem::discriminant(&sanctioned(0, 0, 0, 0, Via::Harm)),
            std::mem::discriminant(&Event::Died {
                tick: 0,
                agent: AgentId(0),
                cause: DeathCause::Violence,
                pos: Pos::new(0, 0),
                age: 0,
                energy_written_off: 0,
                food_dropped: 0
            }),
        ]
    );
    assert_eq!(r.episodes_closed, by_kind(ENCROACH, 1));
    assert_eq!(r.episodes_sanctioned, by_kind(ENCROACH, 1));
    assert!(w.nomos.standing.is_empty());
    assert_eq!(w.agents.len(), 3);
    w.check_invariants().unwrap();

    // 7–10: U encroaches and nobody answers: an episode closed unsanctioned.
    w.apply(intents(&[Rest, Rest, Move(Dir::North)])); // 7: U arrives at 8
    w.apply(intents(&[Rest, Rest, Gather])); // 8
    w.apply(intents(&[Rest, Rest, Rest])); // 9
    let r = w.apply(intents(&[Rest, Rest, Rest])); // 10: closes (8 + 2)
    assert_eq!(r.episodes_closed, by_kind(ENCROACH, 1));
    assert_eq!(r.episodes_sanctioned, [0; 3]);

    // H1's readings from the ledger alone (ADR-0014 D4).
    let l = &w.ledger;
    assert_eq!(l.violations, [4, 0, 0]);
    assert_eq!(l.sanctions, [2, 0, 0]);
    assert_eq!(l.episodes_closed, [3, 0, 0]);
    assert_eq!(l.episodes_sanctioned, [2, 0, 0]);
    let s = l.episodes_sanctioned[ENCROACH] as f64 / l.episodes_closed[ENCROACH] as f64;
    let s_prime = l.sanctions[ENCROACH] as f64 / l.violations[ENCROACH] as f64;
    assert!((s - 2.0 / 3.0).abs() < 1e-9);
    assert!((s_prime - 0.5).abs() < 1e-9);
    assert_eq!(l.harms, 2);
    assert_eq!(l.encroachments, 4);
    w.check_invariants().unwrap();
}

#[test]
fn decrees_enact_replace_repeal_and_recharter_and_need_the_switch() {
    use Action::{Gather, Move, Rest};
    use InterventionError::Invalid;
    let encroach = norm(NormKind::Encroach, 32, Sanction::HARM, false);
    let harm_rule = norm(NormKind::Harm, 8, Sanction::ALL, true);
    let charter = Charter {
        quorum_permille: 0,
        threshold_permille: 1000,
        vote_ticks: 1,
        amendable: false,
    };

    // No switch, no institutions: every decree is invalid and leaves no
    // trace, not even a counted intervention.
    let mut off = empty_world(cfg());
    add_agent(&mut off, Pos::new(2, 2), 50_000, 0);
    for iv in [
        Intervention::Enact { norm: encroach },
        Intervention::Repeal { rule: 0 },
        Intervention::Charter { charter },
    ] {
        assert!(matches!(off.intervene(iv), Err(Invalid(_))), "{iv:?}");
    }
    assert_eq!(off.ledger.interventions, 0);
    assert!(off.nomos.is_empty());
    off.check_invariants().unwrap();

    // The config's charter is the assembly's first.
    let custom = WorldConfig {
        charter: Charter {
            quorum_permille: 250,
            ..Charter::default()
        },
        ..ncfg()
    };
    let w = empty_world(custom.clone());
    assert_eq!(w.nomos.charter.quorum_permille, 250);
    assert!(!w.nomos.is_empty());
    w.check_invariants().unwrap();
    let w = empty_world(WorldConfig {
        nomos: false,
        ..custom
    });
    assert_eq!(
        w.nomos,
        simulation_core::Nomos::default(),
        "off, the charter is not carried"
    );
    assert!(WorldConfig {
        charter: Charter {
            threshold_permille: 1001,
            ..Charter::default()
        },
        ..ncfg()
    }
    .validate()
    .is_err());

    // On: a decree is one `Intervened` record; ids are never reused; an
    // enactment of a kind in force replaces it (and closes its episodes).
    let mut w = empty_world(rcfg());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(2, 1), 50_000, 0);
    set_food(&mut w, Pos::new(2, 2), 40);
    let e = w.intervene(Intervention::Enact { norm: encroach }).unwrap();
    assert_eq!(
        e,
        Event::Intervened {
            tick: 0,
            intervention: Intervention::Enact { norm: encroach }
        }
    );
    assert_eq!(
        serde_json::to_string(&e).unwrap(),
        r#"{"kind":"intervened","tick":0,"intervention":{"op":"enact","norm":{"kind":"encroach","standing_ticks":32,"sanction":1,"except_sanction":false}}}"#
    );
    w.apply(intents(&[Rest, Move(Dir::South)]));
    w.apply(intents(&[Rest, Gather]));
    assert_eq!(w.nomos.standing.len(), 1);
    w.intervene(Intervention::Enact { norm: harm_rule })
        .unwrap();
    assert_eq!(
        w.nomos.rules,
        vec![
            Rule {
                id: 0,
                norm: encroach,
                enacted: 0
            },
            Rule {
                id: 1,
                norm: harm_rule,
                enacted: 2
            }
        ]
    );
    let amended = norm(NormKind::Encroach, 16, Sanction::REJECT, false);
    w.intervene(Intervention::Enact { norm: amended }).unwrap();
    assert_eq!(
        w.nomos
            .rules
            .iter()
            .map(|r| (r.id, r.norm))
            .collect::<Vec<_>>(),
        vec![(1, harm_rule), (2, amended)],
        "replacement = repeal of 0 + enactment of 2"
    );
    assert_eq!(w.nomos.next_rule_id, 3);
    assert!(w.nomos.standing.is_empty(), "the old rule's episode closed");
    assert_eq!(w.ledger.episodes_closed, [1, 0, 0]);
    assert_eq!(w.ledger.episodes_sanctioned, [0; 3]);
    assert_eq!(
        w.intervene(Intervention::Repeal { rule: 0 }),
        Err(Invalid("no rule with that id is in force"))
    );
    let e = w.intervene(Intervention::Repeal { rule: 1 }).unwrap();
    assert_eq!(
        serde_json::to_string(&e).unwrap(),
        r#"{"kind":"intervened","tick":2,"intervention":{"op":"repeal","rule":1}}"#
    );
    assert_eq!(
        w.nomos.rules.iter().map(|r| r.id).collect::<Vec<_>>(),
        vec![2]
    );
    assert!(matches!(
        w.intervene(Intervention::Enact {
            norm: Norm {
                sanction: Sanction(4),
                ..encroach
            }
        }),
        Err(Invalid(_))
    ));
    let e = w.intervene(Intervention::Charter { charter }).unwrap();
    assert_eq!(w.nomos.charter, charter);
    assert_eq!(
        serde_json::to_string(&e).unwrap(),
        r#"{"kind":"intervened","tick":2,"intervention":{"op":"charter","charter":{"quorum_permille":0,"threshold_permille":1000,"vote_ticks":1,"amendable":false}}}"#
    );
    assert!(matches!(
        w.intervene(Intervention::Charter {
            charter: Charter {
                quorum_permille: 1001,
                ..charter
            }
        }),
        Err(Invalid(_))
    ));
    assert_eq!(w.ledger.interventions, 5);
    // The same JSON reads back as the same decree.
    for iv in [
        Intervention::Enact { norm: amended },
        Intervention::Repeal { rule: 2 },
        Intervention::Charter { charter },
    ] {
        let json = serde_json::to_string(&iv).unwrap();
        assert_eq!(serde_json::from_str::<Intervention>(&json).unwrap(), iv);
    }
    w.check_invariants().unwrap();
}

#[test]
fn snapshots_carry_the_institutions_and_inconsistent_states_are_rejected() {
    use Action::{Gather, Move, Rest};
    let mut w = empty_world(rcfg());
    add_agent(&mut w, Pos::new(2, 2), 50_000, 0);
    add_agent(&mut w, Pos::new(2, 1), 50_000, 0);
    add_agent(&mut w, Pos::new(3, 2), 50_000, 0);
    set_food(&mut w, Pos::new(2, 2), 40);
    enact(&mut w, norm(NormKind::Encroach, 10, Sanction::HARM, false));
    enact(&mut w, norm(NormKind::Harm, 10, Sanction::HARM, true));
    w.apply(intents(&[Rest, Move(Dir::South), Rest]));
    w.apply(intents(&[Rest, Gather, Rest]));
    w.apply(intents(&[Rest, Rest, harm(1)]));
    assert_eq!(w.nomos.standing.len(), 1);
    assert_eq!(w.nomos.standing[0].sanctions, 1);
    w.check_invariants().unwrap();

    let bin = w.to_bytes();
    let json = w.to_snapshot();
    assert_eq!(World::from_bytes(&bin).unwrap(), w);
    assert_eq!(World::from_snapshot(&json).unwrap(), w);
    let v: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(v["format"], SNAPSHOT_FORMAT);
    assert_eq!(SNAPSHOT_FORMAT, 13);
    assert_eq!(v["world"]["nomos"]["rules"].as_array().unwrap().len(), 2);
    assert_eq!(v["world"]["nomos"]["rules"][1]["norm"]["kind"], "harm");
    assert_eq!(v["world"]["nomos"]["standing"][0]["agent"], 1);
    assert_eq!(v["world"]["nomos"]["next_rule_id"], 2);
    assert!(v["world"]["nomos"]["motion"].is_null());
    assert_eq!(v["world"]["nomos"]["charter"]["quorum_permille"], 100);
    assert_eq!(v["world"]["config"]["record_violations"], true);
    assert_eq!(v["world"]["ledger"]["sanctions"][0], 1);
    assert_eq!(
        v["world"]["agents"][2]["recent"][0]["kind"], "sanctioned",
        "memory kinds render in snake case"
    );
    let mut old = v.clone();
    old["format"] = serde_json::json!(12);
    assert_eq!(
        World::from_snapshot(&serde_json::to_vec(&old).unwrap()).err(),
        Some(SnapshotError::Format(12))
    );

    // Rules, episodes and the charter are state: the hash sees them.
    let h = w.dynamics_hash();
    let mut t = w.clone();
    t.nomos.rules.pop();
    assert_ne!(t.dynamics_hash(), h);
    let mut t = w.clone();
    t.nomos.standing[0].sanctions = 0;
    assert_ne!(t.dynamics_hash(), h);
    let mut t = w.clone();
    t.nomos.charter.amendable = false;
    assert_ne!(t.dynamics_hash(), h);
    let mut t = w.clone();
    t.ledger.violations[0] += 1;
    assert_eq!(t.dynamics_hash(), h, "ledger counters are bookkeeping");
    assert_ne!(t.state_hash(), w.state_hash());

    // What the invariants refuse.
    let refuse = |label: &str, edit: &dyn Fn(&mut World)| {
        let mut t = w.clone();
        edit(&mut t);
        assert!(t.check_invariants().is_err(), "{label}");
        assert!(
            matches!(
                World::from_snapshot(&t.to_snapshot()),
                Err(SnapshotError::Invalid(_))
            ),
            "{label}"
        );
    };
    refuse("accused but dead", &|t| {
        t.nomos.standing[0].agent = AgentId(9)
    });
    refuse("accused under no rule", &|t| t.nomos.standing[0].rule = 7);
    refuse("two rules of one kind", &|t| {
        let mut r = t.nomos.rules[0];
        r.id = 5;
        t.nomos.rules.push(r);
        t.nomos.next_rule_id = 6;
    });
    refuse("rule id not below next", &|t| t.nomos.next_rule_id = 1);
    refuse("rule ids descending", &|t| t.nomos.rules.swap(0, 1));
    refuse("episode out of order", &|t| {
        let mut s = t.nomos.standing[0];
        s.agent = AgentId(0);
        t.nomos.standing.push(s);
    });
    refuse("episode expired", &|t| {
        t.nomos.standing[0].last = 0;
        t.nomos.standing[0].since = 0;
        t.tick = 20;
    });
    refuse("episode from the future", &|t| {
        t.nomos.standing[0].last = t.tick
    });
    refuse("episode without a violation", &|t| {
        t.nomos.standing[0].violations = 0
    });
    refuse("charter out of range", &|t| {
        t.nomos.charter.quorum_permille = 1001
    });
    refuse("rule enacted in the future", &|t| {
        t.nomos.rules[0].enacted = 99
    });
    refuse("unknown sanction bits", &|t| {
        t.nomos.rules[0].norm.sanction = Sanction(8)
    });
    refuse("institutions with the switch off", &|t| {
        t.config.nomos = false
    });
    refuse("a motion voted twice", &|t| {
        t.nomos.motion = Some(Motion {
            id: 0,
            by: AgentId(0),
            opened: 1,
            kind: MotionKind::Repeal { rule: 0 },
            ayes: vec![AgentId(2)],
            nays: vec![AgentId(2)],
        });
        t.nomos.next_motion_id = 1;
    });
    refuse("a motion with a dead voter", &|t| {
        t.nomos.motion = Some(Motion {
            id: 0,
            by: AgentId(0),
            opened: 1,
            kind: MotionKind::Enact {
                norm: norm(NormKind::Harm, 1, Sanction::NONE, false),
            },
            ayes: vec![AgentId(2), AgentId(9)],
            nays: vec![],
        });
        t.nomos.next_motion_id = 1;
    });
    // A well-formed motion is accepted and hashed.
    let mut t = w.clone();
    t.nomos.motion = Some(Motion {
        id: 0,
        by: AgentId(0),
        opened: 1,
        kind: MotionKind::Charter {
            charter: Charter::default(),
        },
        ayes: vec![AgentId(0), AgentId(2)],
        nays: vec![AgentId(1)],
    });
    t.nomos.next_motion_id = 1;
    t.check_invariants().unwrap();
    assert_ne!(t.dynamics_hash(), h);
    assert_eq!(World::from_bytes(&t.to_bytes()).unwrap(), t);
    let ix = t.norm_index();
    assert!(ix.voted(0) && ix.voted(1) && ix.voted(2));
}

#[test]
fn rules_change_no_decision_of_forager_or_learner_worlds_only_their_readings() {
    // Blind to standing (ADR-0014 D6, 4-3): a seeing learner's Harm aims at
    // an accused neighbour first, so under rules its targets differ
    // (`tests/nomos_policy.rs`); with `see_standing` off the policies read
    // nothing of the rules and the claim below holds as in 4-2. Foragers
    // never harm, so for them it holds either way.
    for kind in [PolicyKind::Forager, PolicyKind::Learner] {
        let base = WorldConfig {
            seed: 7,
            width: 48,
            height: 48,
            initial_agents: 150,
            patch_count: 6,
            nomos: true,
            perception: simulation_core::Perception {
                see_standing: false,
                ..Default::default()
            },
            ..WorldConfig::default()
        };
        let mut plain = World::generate(base.clone(), |_| kind).unwrap();
        let mut ruled = World::generate(base, |_| kind).unwrap();
        ruled
            .intervene(Intervention::Enact {
                norm: norm(NormKind::Encroach, 32, Sanction::HARM, false),
            })
            .unwrap();
        ruled
            .intervene(Intervention::Enact {
                norm: norm(NormKind::Harm, 16, Sanction::ALL, true),
            })
            .unwrap();
        for _ in 0..300 {
            let a = plain.step();
            let b = ruled.step();
            assert_eq!(a.actions, b.actions, "{kind:?} tick {}", a.tick);
            assert_eq!(a.failures, b.failures);
            assert_eq!(a.encroachments, b.encroachments);
            assert_eq!(a.harms, b.harms);
            assert_eq!(physical_events(&a), physical_events(&b));
            assert_eq!(a.violations, [0; 3]);
            assert_eq!(
                b.violations[ENCROACH],
                b.encroachments.iter().sum::<u32>(),
                "every encroachment is a violation while the rule stands"
            );
            // Every harm is either a violation of the harm rule or (exempt)
            // a sanction under some rule.
            assert!(b.violations[HARM] + b.sanctions.iter().sum::<u32>() >= b.harms);
        }
        assert_eq!(body(&plain), body(&ruled));
        assert_eq!(plain.resources, ruled.resources);
        assert_eq!(physical(&plain.ledger), physical(&ruled.ledger));
        assert_eq!(plain.ledger.encroachments, ruled.ledger.encroachments);
        assert!(ruled.ledger.violations[ENCROACH] > 0);
        assert!(
            !ruled.nomos.standing.is_empty(),
            "{kind:?}: someone stands accused"
        );
        assert!(ruled.ledger.episodes_closed[ENCROACH] > 0);
        assert_ne!(plain.dynamics_hash(), ruled.dynamics_hash());
        plain.check_invariants().unwrap();
        ruled.check_invariants().unwrap();
        // Nobody ever answers in a forager world; learners harm their
        // neighbours (3-7), so with everyone accused some harms are read
        // as sanctions.
        if kind == PolicyKind::Forager {
            assert_eq!(ruled.ledger.sanctions, [0; 3]);
        } else {
            assert!(ruled.ledger.sanctions.iter().sum::<u64>() > 0);
            assert!(ruled.ledger.episodes_sanctioned.iter().sum::<u64>() > 0);
        }
    }
}

/// Where each event must sit within a tick (ADR-0014 D9): physical social
/// events, sanctions, violations, the assembly (motions, votes, the
/// closing and its effects), actions, deaths, births.
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

fn norm_strategy(kind: NormKind) -> impl Strategy<Value = Norm> {
    (0u16..=12, 0u8..=3, any::<bool>()).prop_map(move |(standing_ticks, bits, except)| Norm {
        kind,
        standing_ticks,
        sanction: Sanction(bits),
        except_sanction: except,
    })
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 16, .. ProptestConfig::default() })]

    /// Institution worlds are as deterministic as the rest: serial and
    /// parallel decisions agree down to the report, a twin replays to the
    /// same hash, the invariants hold every tick, the per-tick view agrees
    /// with the state, the events keep their order, the ledger's counters
    /// agree with the reports, and snapshots round-trip.
    #[test]
    fn institution_worlds_are_deterministic(
        seed in any::<u64>(),
        encroach in prop::option::of(norm_strategy(NormKind::Encroach)),
        promise in prop::option::of(norm_strategy(NormKind::BrokenPromise)),
        harm_norm in prop::option::of(norm_strategy(NormKind::Harm)),
        record in any::<bool>(),
        slots in prop_oneof![Just(0u8), Just(8)],
        ticks in 1u64..=60,
    ) {
        let cfg = WorldConfig {
            seed,
            width: 24,
            height: 24,
            initial_agents: 120,
            patch_count: 4,
            birth_threshold: Some(60_000),
            child_energy: 30_000,
            memory_slots: slots,
            record_violations: record,
            nomos: true,
            ..WorldConfig::default()
        };
        let mix = |i: u32| match i % 3 {
            0 => PolicyKind::Forager,
            1 => PolicyKind::Random,
            _ => PolicyKind::Learner,
        };
        let mut a = World::generate(cfg.clone(), mix).unwrap();
        for n in [encroach, promise, harm_norm].into_iter().flatten() {
            a.intervene(Intervention::Enact { norm: n }).unwrap();
        }
        let mut b = a.clone();
        let mut twin = a.clone();
        let mut sums = ([0u64; 3], [0u64; 3], [0u64; 3], [0u64; 3]);
        for _ in 0..ticks {
            // The view at the tick's start agrees with the state.
            let ix = a.norm_index();
            // The rules in force at the tick's start are what the tick
            // judges by. Since 4-5 the random walkers move and vote, so an
            // assembly can enact a norm the test never decreed; read the
            // index instead of the test's own arguments.
            let in_force = [
                ix.rule(NormKind::Encroach).is_some(),
                ix.rule(NormKind::BrokenPromise).is_some(),
                ix.rule(NormKind::Harm).is_some(),
            ];
            for (i, ag) in a.agents.iter().enumerate() {
                let eps = a.nomos.standing_of(ag.id);
                prop_assert_eq!(ix.accused(i), !eps.is_empty());
                let allows = |via: Via| eps.iter().any(|s| a.nomos.rule(s.rule).unwrap().norm.sanction.allows(via));
                prop_assert_eq!(ix.harmable(i), allows(Via::Harm));
                prop_assert_eq!(ix.rejectable(i), allows(Via::Reject));
            }
            let ra = a.step();
            #[cfg(feature = "parallel")]
            let rb = b.step_parallel();
            #[cfg(not(feature = "parallel"))]
            let rb = b.step();
            prop_assert_eq!(&ra, &rb);
            twin.step();
            prop_assert!(a.check_invariants().is_ok(), "{:?}", a.check_invariants());
            for k in 0..3 {
                sums.0[k] += ra.violations[k] as u64;
                sums.1[k] += ra.sanctions[k] as u64;
                sums.2[k] += ra.episodes_closed[k] as u64;
                sums.3[k] += ra.episodes_sanctioned[k] as u64;
                prop_assert!(ra.episodes_sanctioned[k] <= ra.episodes_closed[k]);
            }
            prop_assert!(ra.events.windows(2).all(|w| rank(&w[0]) <= rank(&w[1])), "{:?}", ra.events);
            let recorded = ra.events.iter().filter(|e| matches!(e, Event::Violated { .. })).count() as u32;
            prop_assert_eq!(recorded, if record { ra.violations.iter().sum() } else { 0 });
            prop_assert_eq!(
                ra.events.iter().filter(|e| matches!(e, Event::Sanctioned { .. })).count() as u32,
                ra.sanctions.iter().sum::<u32>()
            );
            prop_assert!(ra.sanctions.iter().sum::<u32>() <= ra.harms + ra.rejections);
            if in_force[0] {
                prop_assert_eq!(ra.violations[ENCROACH], ra.encroachments.iter().sum::<u32>());
            } else {
                prop_assert_eq!(ra.violations[ENCROACH], 0);
            }
            if !in_force[1] {
                prop_assert_eq!(ra.violations[PROMISE], 0);
            }
            if !in_force[2] {
                prop_assert_eq!(ra.violations[HARM], 0);
            }
        }
        prop_assert_eq!(a.state_hash(), b.state_hash());
        prop_assert_eq!(a.state_hash(), twin.state_hash());
        prop_assert_eq!(&a.ledger.violations, &sums.0);
        prop_assert_eq!(&a.ledger.sanctions, &sums.1);
        prop_assert_eq!(&a.ledger.episodes_closed, &sums.2);
        prop_assert_eq!(&a.ledger.episodes_sanctioned, &sums.3);
        for s in &a.nomos.standing {
            prop_assert!(a.agents.binary_search_by_key(&s.agent, |x| x.id).is_ok());
        }
        prop_assert_eq!(World::from_bytes(&a.to_bytes()).unwrap(), a.clone());
        prop_assert_eq!(World::from_snapshot(&a.to_snapshot()).unwrap(), a);
    }
}
