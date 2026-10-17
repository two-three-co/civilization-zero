//! Institutions as a reader sees them (ADR-0014 D12, task 4-7): the
//! charter, the rules in force and who stands accused come from the
//! state; decrees, violations and sanctions come from the ledger.
//!
//! Like [`crate::thread`], nothing here judges anything the kernel did
//! not. A sanction *is* the kernel's `Sanctioned` event; a violation is its
//! `Violated` event (when `record_violations` is on) or, as the second path
//! of ADR-0014 D12, an episode open in `World.nomos.standing` right now;
//! the rules and the charter are `World.nomos` itself. What this module
//! adds is bookkeeping the ledger leaves implicit:
//!
//! - **the id a decree's rule received**: `Intervened { Enact }` carries
//!   the norm but not the id the kernel handed out (ADR-0014 D8). Ids are
//!   monotonic and never reused, so replaying the decrees from the
//!   branch's origin — the rules in force there plus one id per enactment
//!   since — recovers every id, and the rule an enactment replaced;
//! - **the episode a sanction answered**: the kernel attributes a harm or
//!   rejection to the oldest episode of the target that was open at the
//!   tick's start (D4). While that episode is still open it is the
//!   `Standing` of the same agent and rule with `since` before the
//!   sanction's tick; once it has closed it can be rebuilt from the
//!   recorded violations (the episode is the run of violations each within
//!   `standing_ticks` of the last), or not at all when violations were
//!   not recorded — then the sanction stands alone.
//!
//! A branch's ledger begins at its origin (a fork's at the fork tick,
//! ADR-0012), so a walk needs what the ledger does not say: the rules in
//! force and the episodes open at the origin ([`Origin`]). Every function
//! reads "as of `now`" — the tick of the view — so a reader that has sought
//! back sees the institution as it stood then. Motions arrive with task
//! 4-5; the shapes here leave room for them (`By`, `Institution::motion`)
//! without pretending they exist.

use serde::Serialize;
use simulation_core::{
    AgentId, Charter, Event, Grounds, Intervention, MotionKind, Nomos, Norm, NormKind, Rule,
    Standing, Via, World,
};
use std::collections::BTreeMap;

/// The institution of a world at one tick (`WasmWorld::institution_json`,
/// `Frame.institution`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Institution {
    /// `config.nomos`: whether this world has institutions at all. Off,
    /// everything below is empty and every decree is rejected.
    pub nomos: bool,
    /// `config.record_violations`: whether `violations` reads the ledger
    /// (true) or the open episodes (false).
    pub record_violations: bool,
    pub charter: Charter,
    /// Rules in force, ascending by id (at most one per kind).
    pub rules: Vec<Rule>,
    /// The open motion, if any (none before task 4-5).
    pub motion: Option<OpenMotion>,
    /// Episodes of accusation open at this tick.
    pub accused: AccusedCount,
}

/// The open motion, summarised: who moved what, when, and how the votes
/// stand (the voter lists stay in the state).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct OpenMotion {
    pub id: u16,
    pub by: AgentId,
    pub opened: u64,
    pub kind: MotionKind,
    pub ayes: u32,
    pub nays: u32,
}

/// How many stand accused: open episodes, distinct agents, and episodes by
/// the kind of the rule they are open under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AccusedCount {
    pub episodes: u32,
    pub agents: u32,
    pub by_kind: [u32; NormKind::COUNT],
}

/// The institution of `w` as it stands at `w.tick`.
pub fn institution(w: &World) -> Institution {
    let n = &w.nomos;
    let mut by_kind = [0u32; NormKind::COUNT];
    let mut agents = 0u32;
    let mut previous: Option<AgentId> = None;
    for s in &n.standing {
        if let Some(r) = n.rule(s.rule) {
            by_kind[r.norm.kind.index()] += 1;
        }
        if previous != Some(s.agent) {
            agents += 1;
            previous = Some(s.agent);
        }
    }
    Institution {
        nomos: w.config.nomos,
        record_violations: w.config.record_violations,
        charter: n.charter,
        rules: n.rules.clone(),
        motion: n.motion.as_ref().map(|m| OpenMotion {
            id: m.id,
            by: m.by,
            opened: m.opened,
            kind: m.kind,
            ayes: m.ayes.len() as u32,
            nays: m.nays.len() as u32,
        }),
        accused: AccusedCount {
            episodes: n.standing.len() as u32,
            agents,
            by_kind,
        },
    }
}

/// One open episode of accusation, as the inspector and the second path of
/// `violations` show it: the kernel's `Standing` with the rule's kind and
/// the last tick the agent stands accused (`last + standing_ticks`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Accused {
    pub agent: AgentId,
    pub rule: u16,
    pub norm: NormKind,
    /// Tick of the first violation of the episode.
    pub since: u64,
    /// Tick of the latest violation.
    pub last: u64,
    /// The last tick the agent stands accused for this episode.
    pub until: u64,
    /// The other party of the latest violation.
    pub against: AgentId,
    pub violations: u16,
    pub sanctions: u16,
}

/// `s` with its rule's kind and deadline; `None` for an episode under no
/// rule in force, which the kernel's invariants rule out.
pub fn accused(s: &Standing, nomos: &Nomos) -> Option<Accused> {
    let rule = nomos.rule(s.rule)?;
    Some(Accused {
        agent: s.agent,
        rule: s.rule,
        norm: rule.norm.kind,
        since: s.since,
        last: s.last,
        until: s.until(rule.norm.standing_ticks),
        against: s.against,
        violations: s.violations,
        sanctions: s.sanctions,
    })
}

/// The open episodes of `agent`, ascending by rule id.
pub fn standing_of(nomos: &Nomos, agent: AgentId) -> Vec<Accused> {
    nomos
        .standing_of(agent)
        .iter()
        .filter_map(|s| accused(s, nomos))
        .collect()
}

/// Whether `agent` has voted on the open motion (false while no motion is
/// open, i.e. before task 4-5).
pub fn voted(nomos: &Nomos, agent: AgentId) -> bool {
    nomos.motion.as_ref().is_some_and(|m| {
        m.ayes.binary_search(&agent).is_ok() || m.nays.binary_search(&agent).is_ok()
    })
}

/// What a branch's ledger does not say: the institution at the branch's
/// origin. A fresh world has nothing in force and no episode open; a world
/// built from a snapshot (a fork, ADR-0012) inherits both from it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Origin {
    /// Rules in force at the origin, ascending by id.
    pub rules: Vec<Rule>,
    /// The id the next enactment after the origin receives.
    pub next_rule_id: u16,
    /// Episodes open at the origin, ascending by (agent, rule).
    pub standing: Vec<Standing>,
}

impl Origin {
    pub fn of(nomos: &Nomos) -> Self {
        Self {
            rules: nomos.rules.clone(),
            next_rule_id: nomos.next_rule_id,
            standing: nomos.standing.clone(),
        }
    }
}

/// Who put a rule or charter in place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum By {
    /// An intervention (`Enact` / `Repeal` / `Charter`), ADR-0014 D8.
    Decree,
}

/// One thing that happened to the institution (`timeline`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    /// `norm` came into force as rule `rule`; `replaced` is the rule of the
    /// same kind it took out of force, if there was one (D5: enacting a
    /// kind in force is repeal + enactment).
    Enacted {
        tick: u64,
        rule: u16,
        norm: Norm,
        replaced: Option<u16>,
        by: By,
    },
    /// Rule `rule` went out of force; `norm` is what it said, when the walk
    /// knows the rule (it always does for a kernel-written ledger).
    Repealed {
        tick: u64,
        rule: u16,
        norm: Option<Norm>,
        by: By,
    },
    /// The charter was replaced.
    Chartered { tick: u64, charter: Charter, by: By },
}

impl Entry {
    pub fn tick(&self) -> u64 {
        match self {
            Entry::Enacted { tick, .. }
            | Entry::Repealed { tick, .. }
            | Entry::Chartered { tick, .. } => *tick,
        }
    }
}

/// The institution's history over ticks `from..now`, ledger order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Timeline {
    pub from: u64,
    pub now: u64,
    pub entries: Vec<Entry>,
}

/// The rules a ledger walk knows: those in force as the walk proceeds,
/// and every rule it has ever seen (kept after repeal, so a rule id in a
/// later event can still be named).
#[derive(Clone, Debug, PartialEq, Eq)]
struct RuleBook {
    in_force: Vec<Rule>,
    known: Vec<Rule>,
    next_id: u16,
}

impl RuleBook {
    fn at(origin: &Origin) -> Self {
        Self {
            in_force: origin.rules.clone(),
            known: origin.rules.clone(),
            next_id: origin.next_rule_id,
        }
    }

    /// Any rule the walk has seen, in force or not.
    fn known(&self, id: u16) -> Option<&Rule> {
        self.known.iter().rev().find(|r| r.id == id)
    }

    /// Applies one ledger event, mirroring `Nomos::enact` / `repeal` and
    /// `World::intervene`'s charter change, and says what it did.
    fn apply(&mut self, e: &Event) -> Option<Entry> {
        let Event::Intervened { tick, intervention } = e else {
            return None;
        };
        let tick = *tick;
        match *intervention {
            Intervention::Enact { norm } => {
                let replaced = self
                    .in_force
                    .iter()
                    .position(|r| r.norm.kind == norm.kind)
                    .map(|i| self.in_force.remove(i).id);
                let rule = Rule {
                    id: self.next_id,
                    norm,
                    enacted: tick,
                };
                self.next_id = self.next_id.saturating_add(1);
                self.in_force.push(rule);
                self.known.push(rule);
                Some(Entry::Enacted {
                    tick,
                    rule: rule.id,
                    norm,
                    replaced,
                    by: By::Decree,
                })
            }
            Intervention::Repeal { rule } => {
                let norm = self
                    .in_force
                    .iter()
                    .position(|r| r.id == rule)
                    .map(|i| self.in_force.remove(i).norm);
                Some(Entry::Repealed {
                    tick,
                    rule,
                    norm,
                    by: By::Decree,
                })
            }
            Intervention::Charter { charter } => Some(Entry::Chartered {
                tick,
                charter,
                by: By::Decree,
            }),
            _ => None,
        }
    }
}

/// The decrees among `events` — a branch's ledger from its origin, ledger
/// order — replayed over `origin`, as entries with `from <= tick < now`.
/// Events at `now` or later are not read.
pub fn timeline(events: &[Event], origin: &Origin, from: u64, now: u64) -> Timeline {
    let mut book = RuleBook::at(origin);
    let entries = events
        .iter()
        .take_while(|e| e.tick() < now)
        .filter_map(|e| book.apply(e))
        .filter(|e| e.tick() >= from)
        .collect();
    Timeline { from, now, entries }
}

/// Where a list of violations or an episode was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// `Violated` events (the world records violations).
    Ledger,
    /// The episodes open in `World.nomos.standing` at the view's tick.
    Standing,
}

/// One violation (`violations`): a recorded one, or an episode open now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Violation {
    /// The kernel's `Violated` event: who, which rule, against whom, when,
    /// on what evidence.
    Violated {
        tick: u64,
        agent: AgentId,
        rule: u16,
        norm: NormKind,
        against: AgentId,
        grounds: Grounds,
    },
    /// An episode open at the view's tick (the second path of D12).
    Accused(Accused),
}

/// Violations over ticks `from..now` (`source: ledger`), or every episode
/// open at `now`, latest violation first (`source: standing`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Violations {
    pub source: Source,
    pub from: u64,
    pub now: u64,
    pub entries: Vec<Violation>,
}

/// The violations of ticks `from..now` when the world records them
/// (`events` are the ledger's events of exactly those ticks); otherwise
/// the episodes open in `nomos` at `now`, newest `last` first (ties: the
/// older `since` first, then agent, then rule). `from` only bounds the
/// first reading: an agent accused since before `from` still stands
/// accused.
pub fn violations(
    events: &[Event],
    nomos: &Nomos,
    recorded: bool,
    from: u64,
    now: u64,
) -> Violations {
    if recorded {
        let entries = events
            .iter()
            .filter(|e| (from..now).contains(&e.tick()))
            .filter_map(|e| match *e {
                Event::Violated {
                    tick,
                    agent,
                    rule,
                    norm,
                    against,
                    grounds,
                } => Some(Violation::Violated {
                    tick,
                    agent,
                    rule,
                    norm,
                    against,
                    grounds,
                }),
                _ => None,
            })
            .collect();
        return Violations {
            source: Source::Ledger,
            from,
            now,
            entries,
        };
    }
    let mut open: Vec<Accused> = nomos
        .standing
        .iter()
        .filter_map(|s| accused(s, nomos))
        .collect();
    open.sort_by(|a, b| {
        b.last
            .cmp(&a.last)
            .then(a.since.cmp(&b.since))
            .then(a.agent.cmp(&b.agent))
            .then(a.rule.cmp(&b.rule))
    });
    Violations {
        source: Source::Standing,
        from,
        now,
        entries: open.into_iter().map(Violation::Accused).collect(),
    }
}

/// The episode a sanction answered: the `Standing` still open at the view
/// (`source: standing`, `open: true`), or its reconstruction from the
/// recorded violations once it has closed (`source: ledger`, `open:
/// false`). Counts are as of `now`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Episode {
    pub since: u64,
    pub last: u64,
    pub against: AgentId,
    pub violations: u16,
    pub sanctions: u16,
    pub open: bool,
    pub source: Source,
}

/// One sanction (`sanctions`): the kernel's `Sanctioned` event with the
/// rule's kind (when the walk knows the rule) and the episode it answered
/// (when that can be known, see [`Episode`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Sanction {
    pub tick: u64,
    pub by: AgentId,
    pub target: AgentId,
    pub rule: u16,
    pub norm: Option<NormKind>,
    pub via: Via,
    pub episode: Option<Episode>,
}

/// Sanctions over ticks `from..now`, ledger order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Sanctions {
    pub from: u64,
    pub now: u64,
    pub entries: Vec<Sanction>,
}

/// An episode as the recorded violations rebuild it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rebuilt {
    since: u64,
    last: u64,
    against: AgentId,
    violations: u16,
    sanctions: u16,
}

impl Rebuilt {
    fn open(s: &Standing) -> Self {
        Self {
            since: s.since,
            last: s.last,
            against: s.against,
            violations: s.violations,
            sanctions: s.sanctions,
        }
    }
}

/// The episodes of one (agent, rule) as the ledger from the origin
/// rebuilds them, and the episode index each sanction of the pair fell in
/// (by the sanction's tick; several sanctions in one tick share it).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Episodes {
    episodes: Vec<Rebuilt>,
    sanctioned_at: Vec<(u64, usize)>,
}

impl Episodes {
    /// Folds one violation of the pair at `tick` against `against`: it
    /// refreshes the open episode when it falls within `standing_ticks` of
    /// the episode's last violation, else it opens a new one (D4).
    fn violated(&mut self, tick: u64, against: AgentId, standing_ticks: u16) {
        match self.episodes.last_mut() {
            Some(e) if tick <= e.last + standing_ticks as u64 => {
                e.last = tick;
                e.against = against;
                e.violations = e.violations.saturating_add(1);
            }
            _ => self.episodes.push(Rebuilt {
                since: tick,
                last: tick,
                against,
                violations: 1,
                sanctions: 0,
            }),
        }
    }

    /// Folds one sanction of the pair at `tick`: it answered the latest
    /// episode opened before `tick` and not yet expired then (the kernel
    /// guarantees one exists for every `Sanctioned` it records).
    fn sanctioned(&mut self, tick: u64, standing_ticks: u16) {
        let Some(i) = self
            .episodes
            .iter()
            .rposition(|e| e.since < tick && tick <= e.last + standing_ticks as u64)
        else {
            return;
        };
        self.episodes[i].sanctions = self.episodes[i].sanctions.saturating_add(1);
        self.sanctioned_at.push((tick, i));
    }

    fn at(&self, tick: u64) -> Option<&Rebuilt> {
        self.sanctioned_at
            .iter()
            .find(|(t, _)| *t == tick)
            .map(|(_, i)| &self.episodes[*i])
    }
}

/// The sanctions of ticks `from..now` among `events` — a branch's ledger
/// from its origin, ledger order; events at `now` or later are not read —
/// each with the episode it answered as of `now`: the `Standing` in
/// `nomos` (the view's state at `now`) of the same agent and rule opened
/// before the sanction, or, when the world records violations
/// (`recorded`), the closed episode rebuilt from them (seeded with the
/// episodes open at the origin, so a fork reads right too), or none.
pub fn sanctions(
    events: &[Event],
    origin: &Origin,
    nomos: &Nomos,
    recorded: bool,
    from: u64,
    now: u64,
) -> Sanctions {
    let mut book = RuleBook::at(origin);
    let mut rebuilt: BTreeMap<(AgentId, u16), Episodes> = BTreeMap::new();
    let mut entries = Vec::new();
    for e in events.iter().take_while(|e| e.tick() < now) {
        book.apply(e);
        match *e {
            Event::Sanctioned {
                tick,
                by,
                target,
                rule,
                via,
            } => {
                if recorded {
                    if let Some(r) = book.known(rule) {
                        rebuilt
                            .entry((target, rule))
                            .or_insert_with(|| episodes_at_origin(origin, target, rule))
                            .sanctioned(tick, r.norm.standing_ticks);
                    }
                }
                if tick >= from {
                    entries.push(Sanction {
                        tick,
                        by,
                        target,
                        rule,
                        norm: book.known(rule).map(|r| r.norm.kind),
                        via,
                        episode: None,
                    });
                }
            }
            Event::Violated {
                tick,
                agent,
                rule,
                against,
                ..
            } if recorded => {
                if let Some(r) = book.known(rule) {
                    rebuilt
                        .entry((agent, rule))
                        .or_insert_with(|| episodes_at_origin(origin, agent, rule))
                        .violated(tick, against, r.norm.standing_ticks);
                }
            }
            _ => {}
        }
    }
    for s in &mut entries {
        // Still open at the view: the kernel's own record of the episode.
        let standing = nomos
            .standing_of(s.target)
            .iter()
            .find(|st| st.rule == s.rule && st.since < s.tick);
        s.episode = match standing {
            Some(st) => Some(Episode {
                since: st.since,
                last: st.last,
                against: st.against,
                violations: st.violations,
                sanctions: st.sanctions,
                open: true,
                source: Source::Standing,
            }),
            None => rebuilt
                .get(&(s.target, s.rule))
                .and_then(|eps| eps.at(s.tick))
                .map(|e| Episode {
                    since: e.since,
                    last: e.last,
                    against: e.against,
                    violations: e.violations,
                    sanctions: e.sanctions,
                    open: false,
                    source: Source::Ledger,
                }),
        };
    }
    Sanctions { from, now, entries }
}

/// The episodes of a pair before the branch's ledger begins: the one open
/// at the origin, if any.
fn episodes_at_origin(origin: &Origin, agent: AgentId, rule: u16) -> Episodes {
    let mut eps = Episodes::default();
    if let Some(s) = origin
        .standing
        .iter()
        .find(|s| s.agent == agent && s.rule == rule)
    {
        eps.episodes.push(Rebuilt::open(s));
    }
    eps
}
