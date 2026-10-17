//! The kernel's promise audit (ADR-0014 D3, task 4-1) against the reader
//! of 3-9b: a deal the kernel settles as kept or broken must be the deal
//! `thread::deals` judges as fulfilled or broken from the same ledger. The
//! one reading the two do not share: transfers made between the offer and
//! the acceptance count for `thread.rs` (which sees the whole ledger) and
//! not for the kernel (which keeps no transfer history and opens the deal
//! at the acceptance); such deals are set aside here and counted.

use cz_web::thread::{deals, Deal, Status};
use simulation_core::{
    Action, Agent, AgentId, Event, EventLog, Intent, Memory, Message, OpenDeal, PolicyKind, Pos,
    World, WorldConfig,
};

fn add_agent(w: &mut World, pos: Pos, inventory: u32) {
    let id = AgentId(w.next_agent_id);
    w.next_agent_id += 1;
    w.agents.push(Agent {
        id,
        pos,
        energy: 50_000,
        health: w.config.max_health,
        inventory,
        age: 0,
        policy: PolicyKind::Forager,
        params: w.config.initial_params.clone(),
        memory: Memory::default(),
        last_signal: None,
        recent: Vec::new(),
        last_step: None,
        arrived: None,
    });
    w.ledger.energy_initial += 50_000;
    w.ledger.food_initial += inventory as u64;
}

fn intents(actions: &[Action]) -> Vec<Intent> {
    actions
        .iter()
        .map(|&action| Intent {
            action,
            memory: Memory::default(),
            state: None,
        })
        .collect()
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

fn transfer(to: u64, n: u32) -> Action {
    Action::Transfer { to: AgentId(to), n }
}

/// Whether the ledger holds a transfer between the deal's parties in
/// `offer_tick..accepted_tick` — the window only `thread.rs` sees.
fn transferred_before_acceptance(log: &EventLog, d: &Deal) -> bool {
    log.range(d.offer_tick..d.accepted_tick)
        .iter()
        .any(|e| match e {
            Event::Transferred { from, to, .. } => {
                (*from == d.proposer && *to == d.acceptor)
                    || (*from == d.acceptor && *to == d.proposer)
            }
            _ => false,
        })
}

/// The kernel's settlement of the deal `thread.rs` judged.
fn kernel_view<'a>(settled: &'a [OpenDeal], d: &Deal) -> Option<&'a OpenDeal> {
    settled.iter().find(|s| {
        s.proposer == d.proposer
            && s.acceptor == d.acceptor
            && s.offer_tick == d.offer_tick
            && s.accepted_tick == d.accepted_tick
    })
}

fn agree(d: &Deal, k: &OpenDeal) {
    assert_eq!(d.deadline, k.deadline);
    assert_eq!((d.give, d.want), (k.give, k.want));
    assert_eq!((d.given, d.repaid), (k.given, k.repaid), "{d:?} vs {k:?}");
    assert_eq!(d.status == Status::Fulfilled, k.kept(), "{d:?} vs {k:?}");
    assert_eq!(
        d.broken_by.contains(&d.proposer),
        !k.kept() && k.proposer_short(),
        "{d:?} vs {k:?}"
    );
    assert_eq!(
        d.broken_by.contains(&d.acceptor),
        !k.kept() && k.acceptor_short(),
        "{d:?} vs {k:?}"
    );
}

#[test]
fn scripted_deals_are_settled_by_the_kernel_as_thread_rs_judges_them() {
    use Action::Rest;
    // The script of `tests/nomos_facts.rs`: kept, broken by the acceptor,
    // a same-tick transfer, a late acceptance, an empty deal, both short,
    // two overlapping deals — plus one deal whose give precedes the
    // acceptance, the case the two readings differ on.
    let script: Vec<[Action; 2]> = vec![
        [propose(1, 2, 3, 4), Rest],
        [Rest, accept(0, 0)],
        [transfer(1, 2), Rest],
        [Rest, transfer(0, 3)],
        [Rest, Rest],
        [propose(1, 1, 2, 3), Rest],
        [Rest, accept(0, 5)],
        [transfer(1, 1), Rest],
        [Rest, Rest],
        [propose(1, 1, 1, 2), Rest],
        [transfer(1, 1), accept(0, 9)],
        [Rest, transfer(0, 1)],
        [propose(1, 0, 1, 1), Rest],
        [Rest, Rest],
        [Rest, accept(0, 12)],
        [propose(1, 0, 0, 1), Rest],
        [Rest, accept(0, 15)],
        [propose(1, 2, 2, 2), Rest],
        [Rest, accept(0, 17)],
        [Rest, Rest],
        [propose(1, 1, 1, 4), Rest],
        [Rest, accept(0, 20)],
        [propose(1, 1, 1, 3), Rest],
        [Rest, accept(0, 22)],
        [transfer(1, 1), transfer(0, 1)],
        [Rest, Rest],
        // 26-29: A hands over the give in the offer's tick, B accepts a
        // tick later and repays: fulfilled for thread.rs, broken by the
        // proposer for the kernel.
        [propose(1, 1, 1, 3), Rest],
        [transfer(1, 1), Rest],
        [Rest, accept(0, 26)],
        [Rest, transfer(0, 1)],
        [Rest, Rest],
    ];
    let mut w = World::new(WorldConfig {
        width: 8,
        height: 8,
        initial_agents: 0,
        patch_count: 0,
        regrow_interval: 1_000_000,
        birth_threshold: None,
        max_age: None,
        nomos: true,
        ..WorldConfig::default()
    })
    .unwrap();
    add_agent(&mut w, Pos::new(2, 2), 5);
    add_agent(&mut w, Pos::new(3, 2), 5);
    let mut log = EventLog::new();
    let mut settled: Vec<OpenDeal> = Vec::new();
    for s in &script {
        let r = w.apply(intents(s));
        assert_eq!(r.failures.iter().sum::<u32>(), 0);
        log.record(&r).unwrap();
        settled.extend(r.promises_settled);
    }
    let now = w.tick;
    let judged = deals(log.events(), now);
    assert_eq!(judged.len(), 9);
    assert_eq!(settled.len(), 9);
    let (mut compared, mut set_aside) = (0, 0);
    for d in &judged {
        assert!(d.deadline < now);
        let k = kernel_view(&settled, d).expect("settled by the kernel");
        if transferred_before_acceptance(&log, d) {
            set_aside += 1;
            assert_eq!(d.offer_tick, 26);
            assert_eq!(d.status, Status::Fulfilled, "thread.rs saw the early give");
            assert!(!k.kept() && k.proposer_short() && !k.acceptor_short());
            assert_eq!((k.given, k.repaid), (0, 1));
            continue;
        }
        agree(d, k);
        compared += 1;
    }
    assert_eq!((compared, set_aside), (8, 1));
    assert_eq!(
        judged.iter().filter(|d| d.status == Status::Broken).count(),
        3
    );
    assert_eq!(w.ledger.promises_broken, 4, "the early give is the fourth");
}

#[test]
fn random_worlds_settle_their_deals_as_thread_rs_reads_them() {
    // Crowded, with random walkers: proposals, acceptances and transfers
    // every tick (the social surface's test world of 3-9a).
    let mut w = World::generate(
        WorldConfig {
            seed: 23,
            width: 32,
            height: 32,
            initial_agents: 200,
            patch_count: 3,
            nomos: true,
            ..WorldConfig::default()
        },
        |i| {
            if i % 2 == 0 {
                PolicyKind::Forager
            } else {
                PolicyKind::Random
            }
        },
    )
    .unwrap();
    let mut log = EventLog::new();
    let mut settled: Vec<OpenDeal> = Vec::new();
    for _ in 0..300 {
        let r = w.step();
        log.record(&r).unwrap();
        settled.extend(r.promises_settled);
    }
    let now = w.tick;
    let judged = deals(log.events(), now);
    let due: Vec<&Deal> = judged.iter().filter(|d| d.deadline < now).collect();
    assert!(due.len() > 20, "{} deals came due", due.len());
    assert_eq!(
        due.len(),
        settled.len(),
        "every deal past its deadline was settled, once"
    );
    assert_eq!(w.ledger.promises_due, settled.len() as u64);
    let (mut compared, mut set_aside, mut broken) = (0, 0, 0);
    for d in &due {
        let k = kernel_view(&settled, d).expect("settled by the kernel");
        if transferred_before_acceptance(&log, d) {
            set_aside += 1;
            continue;
        }
        agree(d, k);
        compared += 1;
        broken += u32::from(!k.kept());
    }
    println!(
        "{} deals compared ({} broken), {} set aside for a transfer before the acceptance, {} still open",
        compared,
        broken,
        set_aside,
        w.nomos.deals.len()
    );
    assert!(compared > 20);
    // What is still open for the kernel is open or already fulfilled for
    // thread.rs, never broken: no deadline has passed.
    for d in judged.iter().filter(|d| d.deadline >= now) {
        assert_ne!(d.status, Status::Broken);
        assert!(w.nomos.deals.iter().any(|o| o.offer_tick == d.offer_tick
            && o.proposer == d.proposer
            && o.acceptor == d.acceptor
            && o.accepted_tick == d.accepted_tick));
    }
    assert_eq!(
        judged.iter().filter(|d| d.deadline >= now).count(),
        w.nomos.deals.len()
    );
}
