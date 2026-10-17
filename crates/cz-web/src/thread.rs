//! Negotiation threads (ADR-0013 3-9b): what the ledger says happened
//! between two agents, and how each accepted deal stands.
//!
//! A *thread* is the ledger's events that name both agents — proposals,
//! acceptances, rejections, transfers and harms — in ledger order. Each
//! `Accepted` event is read as a *deal* (it repeats the terms, so it stands
//! alone) and judged from the transfers between the pair that fall inside
//! the deal's window. This is the reading ADR-0013 D5 defers to Phase 4,
//! applied here for display only:
//!
//! - the window is the offer's tick through its deadline,
//!   `offer_tick..=offer_tick + by` (the proposer's "now" starts at the
//!   offer itself: an honest proposer may hand over the `give` before the
//!   answer arrives);
//! - `given` is the food the proposer transferred to the acceptor in the
//!   window, `repaid` the food the acceptor transferred back in it;
//! - as of tick `now` (events of ticks below `now` are known): *fulfilled*
//!   once `given >= give` and `repaid >= want`; otherwise *broken* once the
//!   deadline has passed (`now > deadline`), naming who fell short;
//!   otherwise *open*.
//!
//! The kernel records no link between a transfer and a promise, so the sums
//! are per deal over its window — two overlapping deals between the same
//! pair both count the same transfer, exactly as D5's definition reads —
//! and the `deal` / `role` tags on entries are this reader's attribution
//! for display, not a kernel fact. Nothing here feeds back into the world:
//! like `social()`, it is a view of the ledger.

use serde::Serialize;
use simulation_core::{AgentId, Event};

/// The ledger between two agents over `from..now`, read at tick `now`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Thread {
    pub a: AgentId,
    pub b: AgentId,
    /// First tick covered.
    pub from: u64,
    /// The tick the thread is read at (the view's): events of earlier ticks
    /// are known, and open deadlines are judged against it.
    pub now: u64,
    /// Every event naming both agents, ledger order.
    pub entries: Vec<Entry>,
    /// One per `Accepted` entry, in ledger order; `Entry`'s `deal` indexes it.
    pub deals: Vec<Deal>,
}

/// One ledger event between the two agents, with its place in the deals.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// `from` offered `to`: "I give `give` now; return `want` by `deadline`".
    Proposed {
        tick: u64,
        from: AgentId,
        to: AgentId,
        give: u8,
        want: u8,
        by: u16,
        deadline: u64,
        answer: Answer,
        /// The deal it became, when accepted.
        deal: Option<usize>,
    },
    /// `from` accepted `to`'s offer of `offer_tick`; `deal` is its judgement.
    Accepted {
        tick: u64,
        from: AgentId,
        to: AgentId,
        offer_tick: u64,
        give: u8,
        want: u8,
        by: u16,
        deadline: u64,
        deal: usize,
    },
    /// `from` declined `to`'s offer. The kernel checks nothing here, so the
    /// offer named is the latest earlier one from `to` in the thread, if any.
    Rejected {
        tick: u64,
        from: AgentId,
        to: AgentId,
        offer_tick: Option<u64>,
    },
    /// Food moved; `deal` / `role` name the promise it went toward (the
    /// oldest one still owed on that side), none when nothing was owed.
    Transferred {
        tick: u64,
        from: AgentId,
        to: AgentId,
        amount: u32,
        deal: Option<usize>,
        role: Option<Role>,
    },
    Harmed {
        tick: u64,
        by: AgentId,
        target: AgentId,
        damage: i64,
    },
}

/// What became of a proposal, as of `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    Accepted,
    Rejected,
    /// No answer yet and the deadline has not passed.
    Pending,
    /// No answer and the deadline has passed (the kernel would still deliver
    /// a late acceptance; nothing expires in the world).
    Lapsed,
}

/// Which side of a deal a transfer serves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Proposer to acceptor: the `give`.
    Give,
    /// Acceptor to proposer: the `want`.
    Repay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Not yet delivered in full, deadline not yet passed.
    Open,
    /// Both sides delivered within the window.
    Fulfilled,
    /// The deadline passed with at least one side short.
    Broken,
}

/// An accepted proposal, judged from the transfers that followed it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Deal {
    pub proposer: AgentId,
    pub acceptor: AgentId,
    pub offer_tick: u64,
    pub accepted_tick: u64,
    /// `offer_tick + by`: the last tick a transfer counts.
    pub deadline: u64,
    pub give: u8,
    pub want: u8,
    /// Food the proposer handed the acceptor within the window.
    pub given: u32,
    /// Food the acceptor handed back within the window.
    pub repaid: u32,
    pub status: Status,
    /// Tick at which both sides had delivered (never before the acceptance).
    pub settled_tick: Option<u64>,
    /// Who fell short when broken: the proposer, the acceptor, or both.
    pub broken_by: Vec<AgentId>,
}

#[derive(Clone, Copy)]
struct Transfer {
    tick: u64,
    from: AgentId,
    to: AgentId,
    amount: u32,
}

fn transfers_of<'a>(events: impl IntoIterator<Item = &'a Event>) -> Vec<Transfer> {
    events
        .into_iter()
        .filter_map(|e| match *e {
            Event::Transferred {
                tick,
                from,
                to,
                amount,
            } => Some(Transfer {
                tick,
                from,
                to,
                amount,
            }),
            _ => None,
        })
        .collect()
}

/// Whether `e` names exactly the pair `a`, `b` (either way round).
fn between(e: &Event, a: AgentId, b: AgentId) -> bool {
    let pair = |x: AgentId, y: AgentId| (x == a && y == b) || (x == b && y == a);
    match e {
        Event::Transferred { from, to, .. }
        | Event::Proposed { from, to, .. }
        | Event::Accepted { from, to, .. }
        | Event::Rejected { from, to, .. } => pair(*from, *to),
        Event::Harmed { by, target, .. } => pair(*by, *target),
        _ => false,
    }
}

/// Reads an `Accepted` event as a deal and judges it against `transfers`
/// (tick-ascending; other pairs are ignored) as of `now`.
fn judge(accepted: &Event, transfers: &[Transfer], now: u64) -> Option<Deal> {
    let Event::Accepted {
        tick,
        from,
        to,
        offer_tick,
        give,
        want,
        by,
    } = *accepted
    else {
        return None;
    };
    let (acceptor, proposer) = (from, to);
    let deadline = offer_tick + by as u64;
    let done = |given: u32, repaid: u32| given >= give as u32 && repaid >= want as u32;
    let (mut given, mut repaid) = (0u32, 0u32);
    let mut settled = done(0, 0).then_some(tick);
    let start = transfers.partition_point(|t| t.tick < offer_tick);
    for t in &transfers[start..] {
        if t.tick > deadline {
            break;
        }
        if t.from == proposer && t.to == acceptor {
            given += t.amount;
        } else if t.from == acceptor && t.to == proposer {
            repaid += t.amount;
        } else {
            continue;
        }
        if settled.is_none() && done(given, repaid) {
            settled = Some(t.tick.max(tick));
        }
    }
    let status = if settled.is_some() {
        Status::Fulfilled
    } else if now > deadline {
        Status::Broken
    } else {
        Status::Open
    };
    let mut broken_by = Vec::new();
    if status == Status::Broken {
        if given < give as u32 {
            broken_by.push(proposer);
        }
        if repaid < want as u32 {
            broken_by.push(acceptor);
        }
    }
    Some(Deal {
        proposer,
        acceptor,
        offer_tick,
        accepted_tick: tick,
        deadline,
        give,
        want,
        given,
        repaid,
        status,
        settled_tick: settled,
        broken_by,
    })
}

/// Every deal among `events` (any pair; the events of one tick range, in
/// ledger order), judged as of `now`, in ledger order of the acceptances.
pub fn deals(events: &[Event], now: u64) -> Vec<Deal> {
    let transfers = transfers_of(events);
    events
        .iter()
        .filter_map(|e| judge(e, &transfers, now))
        .collect()
}

/// The thread between `a` and `b` in `events` (the ledger's `from..now`,
/// in ledger order), read at tick `now`.
pub fn thread(events: &[Event], a: AgentId, b: AgentId, from: u64, now: u64) -> Thread {
    let pair: Vec<&Event> = events.iter().filter(|e| between(e, a, b)).collect();
    let transfers = transfers_of(pair.iter().copied());
    let deals: Vec<Deal> = pair
        .iter()
        .filter_map(|e| judge(e, &transfers, now))
        .collect();
    // The offer a rejection declines: the latest earlier one from the
    // rejected party (the kernel records no `offer_tick` for a rejection).
    let declined = |at: u64, rejecter: AgentId, proposer: AgentId| -> Option<u64> {
        pair.iter().rev().find_map(|e| match e {
            Event::Proposed { tick, from, to, .. }
                if *tick < at && *from == proposer && *to == rejecter =>
            {
                Some(*tick)
            }
            _ => None,
        })
    };

    let mut entries = Vec::with_capacity(pair.len());
    let (mut accepted_seen, mut transfers_seen) = (0usize, 0usize);
    for e in &pair {
        let entry = match **e {
            Event::Proposed {
                tick,
                from,
                to,
                give,
                want,
                by,
            } => {
                let deadline = tick + by as u64;
                let deal = deals
                    .iter()
                    .position(|d| d.proposer == from && d.acceptor == to && d.offer_tick == tick);
                let rejected = pair.iter().any(|r| {
                    matches!(**r, Event::Rejected { tick: at, from: rf, to: rt }
                        if rf == to && rt == from && declined(at, rf, rt) == Some(tick))
                });
                let answer = if deal.is_some() {
                    Answer::Accepted
                } else if rejected {
                    Answer::Rejected
                } else if now > deadline {
                    Answer::Lapsed
                } else {
                    Answer::Pending
                };
                Entry::Proposed {
                    tick,
                    from,
                    to,
                    give,
                    want,
                    by,
                    deadline,
                    answer,
                    deal,
                }
            }
            Event::Accepted {
                tick,
                from,
                to,
                offer_tick,
                give,
                want,
                by,
            } => {
                let deal = accepted_seen;
                accepted_seen += 1;
                Entry::Accepted {
                    tick,
                    from,
                    to,
                    offer_tick,
                    give,
                    want,
                    by,
                    deadline: offer_tick + by as u64,
                    deal,
                }
            }
            Event::Rejected { tick, from, to } => Entry::Rejected {
                tick,
                from,
                to,
                offer_tick: declined(tick, from, to),
            },
            Event::Transferred {
                tick,
                from,
                to,
                amount,
            } => {
                // Shown against the oldest deal whose window holds it and
                // which still owed on that side; a transfer no promise
                // called for is shown against none.
                let role_in = |d: &Deal| {
                    if d.proposer == from && d.acceptor == to {
                        Some(Role::Give)
                    } else if d.acceptor == from && d.proposer == to {
                        Some(Role::Repay)
                    } else {
                        None
                    }
                };
                let earlier = &transfers[..transfers_seen];
                transfers_seen += 1;
                let candidates: Vec<(usize, Role)> = deals
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.offer_tick <= tick && tick <= d.deadline)
                    .filter_map(|(i, d)| role_in(d).map(|r| (i, r)))
                    .collect();
                let still_owed = |&(i, role): &(usize, Role)| {
                    let d = &deals[i];
                    let target = match role {
                        Role::Give => d.give,
                        Role::Repay => d.want,
                    } as u32;
                    let delivered: u32 = earlier
                        .iter()
                        .filter(|t| t.tick >= d.offer_tick && t.from == from && t.to == to)
                        .map(|t| t.amount)
                        .sum();
                    delivered < target
                };
                let pick = candidates.iter().find(|c| still_owed(c)).copied();
                Entry::Transferred {
                    tick,
                    from,
                    to,
                    amount,
                    deal: pick.map(|(i, _)| i),
                    role: pick.map(|(_, r)| r),
                }
            }
            Event::Harmed {
                tick,
                by,
                target,
                damage,
            } => Entry::Harmed {
                tick,
                by,
                target,
                damage,
            },
            _ => continue,
        };
        entries.push(entry);
    }
    Thread {
        a,
        b,
        from,
        now,
        entries,
        deals,
    }
}
