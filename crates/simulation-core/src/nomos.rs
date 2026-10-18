//! Nomos Engine: the social facts and institutions of a world with
//! `config.nomos` on (ADR-0014 D2–D5; tasks 4-1 and 4-2).
//!
//! Everything in this module is a fact the ledger and the state already
//! determine, read off at a fixed place in Apply with integer arithmetic
//! and no randomness. Nothing here changes physics: an encroachment costs
//! nothing, a broken promise fails nothing, a sanction is an ordinary harm
//! or rejection that happens to answer a standing accusation (ADR-0014
//! D1). With `nomos` off none of it runs, `World.nomos` stays at its
//! default and adds no byte to `dynamics_hash`.
//!
//! **Social facts (4-1).**
//! - *First occupancy* is arrival order: of the agents standing on a cell
//!   at the tick's start, the one with the smallest `Agent.arrived` (`None`,
//!   the founding cohort, is the oldest; ties go to the smaller id) is the
//!   cell's first occupant.
//! - *Encroachment*: agent A gathered at least one unit on cell c in tick
//!   t, and the first occupant B of c at S(t) is another agent with
//!   `arrived(B) < arrived(A)` strictly — arrivals in the same tick are
//!   not encroachments, nor is gathering where nobody arrived earlier.
//!   Judged right after the gather phase, on the cells that saw a gather.
//! - *Deals*: an `Accepted` opens an [`OpenDeal`]; every `Transferred`
//!   between the pair from the acceptance tick through the deadline
//!   (`offer_tick + by`) adds to `given` / `repaid`; at the deadline tick,
//!   after that tick's transfers (Apply phase 3c), the deal is settled:
//!   kept when `given >= give && repaid >= want`, else broken by whoever
//!   fell short. This is cz-web's `thread.rs` reading (ADR-0013 3-9b),
//!   except that transfers made between the offer and the acceptance are
//!   not seen here: the kernel keeps no transfer history, so a deal's
//!   window starts when it exists.
//!
//! **Institutions (4-2).** The Institution DSL is two `Copy` records
//! (ADR-0014 D5): a [`Norm`] names one of three facts a [`NormKind`] turns
//! into a *violation*, how long a violator *stands accused*
//! (`standing_ticks`) and which answers to an accused agent count as a
//! *sanction* ([`Sanction`]: harm, rejection, both or neither); a
//! [`Charter`] holds the quorum, threshold, voting period and amendability
//! of the assembly (used by motions, task 4-5). A [`Rule`] is a norm in
//! force, with an id that is never reused; at most one rule per kind is in
//! force, so judgement is unique, and enacting a kind already in force
//! *replaces* its rule. A [`Standing`] is one *episode* of accusation per
//! (agent, rule): opened by the first violation, refreshed by every later
//! one, closed when `standing_ticks` have passed since the last violation
//! or the agent dies (then counted in `Ledger.episodes_closed` and, if any
//! sanction answered it, `episodes_sanctioned`). The per-tick view of all
//! this is the derived [`NormIndex`], built at the tick's start right after
//! the `SpatialIndex` and never stored (ADR-0014 D2).
//!
//! Rules are changed by interventions (`Enact` / `Repeal` / `Charter`,
//! ADR-0014 D8) and, since 4-5, by the assembly (ADR-0014 D7): the world
//! has one floor, [`Nomos::motion`]; a `Message::Motion` opens it when it
//! is free and the [`Charter`] admits the motion ([`Nomos::admit`]), votes
//! are counted on it ([`Nomos::vote`], one per living agent), and
//! `vote_ticks` after it was moved the kernel closes it
//! ([`Nomos::close_motion`]): quorum (votes as a share of the living) and
//! threshold (ayes as a share of the votes) met, with at least one aye,
//! and the motion carries — a norm is enacted (replacing the rule of its
//! kind, if any), a rule repealed or the charter replaced, from the next
//! tick. The judging itself — which event is a violation of which rule,
//! which harm or rejection is a sanction of which episode — lives in
//! `World::apply_with` at fixed phases (ADR-0014 D4, D9), and so does the
//! closing of motions (phase 3c).

use crate::action::Failure;
use crate::agent::{Agent, AgentId};
use serde::{Deserialize, Serialize};

pub mod index;

pub use index::NormIndex;

/// Whether gathering by an agent that arrived at `mine` encroaches on a
/// first occupant that arrived at `first`: strictly earlier, with `None`
/// (founding) earlier than any tick.
pub fn encroaches(first: Option<u64>, mine: Option<u64>) -> bool {
    first < mine
}

/// The first occupant among `indices` (indices into `agents`, ascending):
/// the smallest `arrived`, ties to the smallest id, which is the first in
/// `indices` order. `None` for an empty cell.
pub fn first_occupant(agents: &[Agent], indices: &[u32]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for &i in indices {
        let i = i as usize;
        if best.is_none_or(|b| agents[i].arrived < agents[b].arrived) {
            best = Some(i);
        }
    }
    best
}

/// The three facts a rule can turn into a violation (ADR-0014 D4). Each is
/// judged from the ledger's events and the 4-1 state alone, at a fixed
/// phase of Apply, with no randomness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormKind {
    /// A gather on a cell whose first occupant arrived strictly earlier
    /// than the gatherer (4-1's encroachment). Violator: the gatherer;
    /// against: the first occupant. Judged right after phase 3.
    Encroach,
    /// A deal settled short at its deadline (4-1's audit). Violator: each
    /// side that fell short; against: the other party. Judged in phase 3c.
    BrokenPromise,
    /// A `Harmed`. Violator: the attacker; against: the target. Judged in
    /// phase 3b — unless the harm is a sanction and the rule's
    /// `except_sanction` is set.
    Harm,
}

impl NormKind {
    pub const ALL: [NormKind; 3] = [NormKind::Encroach, NormKind::BrokenPromise, NormKind::Harm];

    /// Number of kinds: the length of the per-kind counters of `Ledger`
    /// and `TickReport`.
    pub const COUNT: usize = Self::ALL.len();

    /// The kind's position in `ALL` (its discriminant).
    pub fn index(self) -> usize {
        self as usize
    }

    /// Name as the CLI and JSON spell it.
    pub fn name(self) -> &'static str {
        match self {
            NormKind::Encroach => "encroach",
            NormKind::BrokenPromise => "broken_promise",
            NormKind::Harm => "harm",
        }
    }

    pub fn parse(name: &str) -> Option<NormKind> {
        match name {
            "encroach" | "encroachment" => Some(NormKind::Encroach),
            "broken_promise" | "broken-promise" | "promise" => Some(NormKind::BrokenPromise),
            "harm" => Some(NormKind::Harm),
            _ => None,
        }
    }
}

/// How a sanction reaches an accused agent: the physical action it is read
/// from (ADR-0014 D4). A harm is a harm and a rejection a rejection
/// whatever the reading; nothing physical differs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// `Event::Harmed { by: sanctioner, target: accused }`.
    Harm,
    /// `Event::Rejected { from: sanctioner, to: accused }`.
    Reject,
}

/// The set of answers a rule lets count as sanctions: a bit set over
/// [`Via`] (`HARM` = 1, `REJECT` = 2). Empty means the accused stand
/// accused but nobody's answer is a sanction (the "no sanction" null model
/// of ADR-0014 D11). Serialised as the plain integer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sanction(pub u8);

impl Sanction {
    pub const NONE: Sanction = Sanction(0);
    pub const HARM: Sanction = Sanction(1);
    pub const REJECT: Sanction = Sanction(2);
    pub const ALL: Sanction = Sanction(3);

    /// Only the two known bits may be set.
    pub fn is_valid(self) -> bool {
        self.0 <= Self::ALL.0
    }

    pub fn allows(self, via: Via) -> bool {
        let bit = match via {
            Via::Harm => Self::HARM.0,
            Via::Reject => Self::REJECT.0,
        };
        self.0 & bit != 0
    }

    pub fn union(self, other: Sanction) -> Sanction {
        Sanction(self.0 | other.0)
    }

    /// Name as the CLI spells it: `none`, `harm`, `reject` or `harm+reject`.
    pub fn name(self) -> &'static str {
        match self.0 {
            0 => "none",
            1 => "harm",
            2 => "reject",
            _ => "harm+reject",
        }
    }

    /// Inverse of `name`; also accepts `both` / `all` and `+`-joined parts.
    pub fn parse(s: &str) -> Option<Sanction> {
        match s {
            "none" | "" => return Some(Self::NONE),
            "both" | "all" => return Some(Self::ALL),
            _ => {}
        }
        let mut out = Self::NONE;
        for part in s.split('+') {
            out = out.union(match part {
                "harm" => Self::HARM,
                "reject" => Self::REJECT,
                _ => return None,
            });
        }
        Some(out)
    }
}

/// A norm: the smallest unit of the Institution DSL (ADR-0014 D5). `Copy`,
/// 8 bytes. It can say which of three facts is a violation, for how many
/// ticks a violator stands accused, which answers count as sanctions, and
/// (for `Harm`) whether a sanction is itself exempt from the rule. It
/// cannot name an agent, a cell or an amount, oblige anyone to act, change
/// a cost or a damage, or depend on a condition — by design.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Norm {
    pub kind: NormKind,
    /// A violator stands accused from the tick after each violation
    /// through `last violation + standing_ticks` (inclusive). 0 means the
    /// violation is counted but nobody is ever accused.
    pub standing_ticks: u16,
    /// Which answers to an accused agent count as sanctions.
    pub sanction: Sanction,
    /// `Harm` rules only: a harm that is a sanction is not a violation.
    /// Ignored by the other kinds.
    pub except_sanction: bool,
}

impl Norm {
    pub fn is_valid(&self) -> bool {
        self.sanction.is_valid()
    }

    /// Stable encoding for `dynamics_hash`.
    pub fn hash_code(self) -> u64 {
        self.kind as u64
            | (self.standing_ticks as u64) << 8
            | (self.sanction.0 as u64) << 24
            | (self.except_sanction as u64) << 32
    }
}

/// A norm in force (ADR-0014 D5). Ids are handed out once and never
/// reused, so a rule id in the ledger always names one enactment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rule {
    pub id: u16,
    pub norm: Norm,
    /// Tick the rule came into force (the intervention's tick, or the
    /// motion's closing tick once motions exist).
    pub enacted: u64,
}

/// How the assembly decides (ADR-0014 D5, D7). Read by motions (4-5); in
/// 4-2 only carried, set from `WorldConfig.charter` at generation and
/// changed by `Intervention::Charter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Charter {
    /// Votes needed, as a share of the living at closing, for a motion to
    /// count at all.
    pub quorum_permille: u16,
    /// Ayes needed, as a share of the votes cast, for a motion to carry.
    pub threshold_permille: u16,
    /// Ticks a motion stays open.
    pub vote_ticks: u16,
    /// Whether rules in force and the charter itself may be changed by
    /// motion (interventions are never bound by this).
    pub amendable: bool,
}

impl Default for Charter {
    fn default() -> Self {
        Self {
            quorum_permille: 100,
            threshold_permille: 667,
            vote_ticks: 32,
            amendable: true,
        }
    }
}

impl Charter {
    pub const PERMILLE_MAX: u16 = 1000;

    pub fn is_valid(&self) -> bool {
        self.quorum_permille <= Self::PERMILLE_MAX && self.threshold_permille <= Self::PERMILLE_MAX
    }

    /// Stable encoding for `dynamics_hash`.
    pub fn hash_code(self) -> u64 {
        self.quorum_permille as u64
            | (self.threshold_permille as u64) << 16
            | (self.vote_ticks as u64) << 32
            | (self.amendable as u64) << 48
    }
}

/// One episode of accusation (ADR-0014 D4): agent `agent` stands accused
/// under rule `rule` from the tick after `since` through `last +
/// standing_ticks`. There is one per (agent, rule), so the number of open
/// episodes is bounded by `alive × rules`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Standing {
    pub agent: AgentId,
    pub rule: u16,
    /// Tick of the first violation of this episode.
    pub since: u64,
    /// Tick of the latest violation; the deadline counts from here.
    pub last: u64,
    /// The other party of the latest violation.
    pub against: AgentId,
    /// Violations folded into this episode (saturating).
    pub violations: u16,
    /// Sanctions that answered this episode so far (saturating).
    pub sanctions: u16,
}

impl Standing {
    /// The last tick the agent stands accused for this episode.
    pub fn until(&self, standing_ticks: u16) -> u64 {
        self.last + standing_ticks as u64
    }
}

/// What a motion asks for (ADR-0014 D7): the payload of
/// `Message::Motion`, held on the floor while the assembly votes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionKind {
    /// Put `norm` in force as a new rule; a rule of the same kind in force
    /// is replaced (repealed first, ADR-0014 D5).
    Enact { norm: Norm },
    /// Take the rule with this id out of force.
    Repeal { rule: u16 },
    /// Replace the assembly's charter (for the motions after this one).
    Charter { charter: Charter },
}

impl MotionKind {
    /// Stable encoding for `dynamics_hash` and `Message::value`: a tag in
    /// the top byte over the payload's own code (norms use 33 bits,
    /// charters 49).
    pub fn hash_code(self) -> u64 {
        match self {
            MotionKind::Enact { norm } => (1 << 56) ^ norm.hash_code(),
            MotionKind::Repeal { rule } => (2 << 56) ^ rule as u64,
            MotionKind::Charter { charter } => (3 << 56) ^ charter.hash_code(),
        }
    }

    /// Whether carrying this motion would change what is already in force
    /// — a repeal, a new charter, or the replacement of a rule of a kind
    /// in force — which is what `Charter::amendable == false` forbids
    /// (ADR-0014 D7). Enacting a kind nobody has ruled on yet is not an
    /// amendment.
    pub fn amends(self, nomos: &Nomos) -> bool {
        match self {
            MotionKind::Enact { norm } => nomos.rule_of(norm.kind).is_some(),
            MotionKind::Repeal { .. } | MotionKind::Charter { .. } => true,
        }
    }
}

/// The one open motion of the assembly (ADR-0014 D7). `ayes` and `nays`
/// are ascending ids of living agents without duplicates across the two
/// (a voter that dies is struck from its side, so the lists stay bounded
/// by the living and a closing never counts the dead).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Motion {
    pub id: u16,
    pub by: AgentId,
    /// Tick the motion was moved; it closes in phase 3c of the first tick
    /// `>= opened + charter.vote_ticks` (the charter in force then).
    pub opened: u64,
    pub kind: MotionKind,
    pub ayes: Vec<AgentId>,
    pub nays: Vec<AgentId>,
}

/// The fact a `Violated` event rests on (ADR-0014 completion condition
/// 1b): enough to show "who, which rule, against whom, when" with its
/// evidence, without the rest of the ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "fact", rename_all = "snake_case")]
pub enum Grounds {
    /// Gathered `amount` units on `cell`, where `against` had arrived first.
    Encroachment {
        cell: crate::space::Pos,
        amount: u32,
    },
    /// Fell `short` units short on the deal `against` and the violator
    /// struck when the proposal of `offer_tick` was accepted.
    BrokenPromise { offer_tick: u64, short: u32 },
    /// Took `damage` health from `against`.
    Harm { damage: i64 },
}

impl Grounds {
    pub fn kind(self) -> NormKind {
        match self {
            Grounds::Encroachment { .. } => NormKind::Encroach,
            Grounds::BrokenPromise { .. } => NormKind::BrokenPromise,
            Grounds::Harm { .. } => NormKind::Harm,
        }
    }
}

/// An accepted proposal the kernel is auditing (ADR-0014 D3): the terms
/// `Event::Accepted` repeated, plus what has been delivered so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpenDeal {
    pub proposer: AgentId,
    pub acceptor: AgentId,
    pub offer_tick: u64,
    /// Tick the acceptance was delivered; transfers count from this tick on.
    pub accepted_tick: u64,
    /// `offer_tick + by`: the last tick a transfer counts, and the tick the
    /// deal is settled in (or the acceptance tick if that is later).
    pub deadline: u64,
    /// Owed by the proposer to the acceptor.
    pub give: u8,
    /// Owed by the acceptor to the proposer.
    pub want: u8,
    /// Food the proposer handed the acceptor within the window.
    pub given: u32,
    /// Food the acceptor handed the proposer within the window.
    pub repaid: u32,
}

impl OpenDeal {
    /// Both sides delivered.
    pub fn kept(&self) -> bool {
        self.given >= self.give as u32 && self.repaid >= self.want as u32
    }

    /// The proposer still owes part of `give`.
    pub fn proposer_short(&self) -> bool {
        self.given < self.give as u32
    }

    /// The acceptor still owes part of `want`.
    pub fn acceptor_short(&self) -> bool {
        self.repaid < self.want as u32
    }

    /// Books a transfer of `amount` from `from` to `to` at `tick` against
    /// this deal if it is between the parties and inside the window.
    fn credit(&mut self, tick: u64, from: AgentId, to: AgentId, amount: u32) {
        if tick < self.accepted_tick || tick > self.deadline {
            return;
        }
        if from == self.proposer && to == self.acceptor {
            self.given += amount;
        } else if from == self.acceptor && to == self.proposer {
            self.repaid += amount;
        }
    }
}

/// An episode that just closed: what the ledger counts (`episodes_closed`
/// and, when `sanctioned`, `episodes_sanctioned`, both by kind).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Closed {
    pub kind: NormKind,
    pub sanctioned: bool,
}

/// What a carried motion did when it closed (ADR-0014 D7), for the ledger:
/// `Enacted` / `Repealed` / `Chartered` events follow a `MotionClosed`
/// with `carried: true`; `Nothing` when the motion carried but could no
/// longer take effect (its rule left force during the vote, the charter
/// stopped amending, the rule ids ran out) — or when it was defeated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Nothing,
    /// `rule` is in force from the next tick; `replaced` is the rule of the
    /// same kind it took out of force, with that rule's closed episodes.
    Enacted {
        rule: Rule,
        replaced: Option<(Rule, Vec<Closed>)>,
    },
    /// `rule` left force; its episodes closed.
    Repealed {
        rule: Rule,
        closed: Vec<Closed>,
    },
    Chartered(Charter),
}

/// A motion that just closed (`Nomos::close_motion`): the motion as it
/// stood, the tally, the living at closing, the verdict and its effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closing {
    pub motion: Motion,
    pub ayes: u32,
    pub nays: u32,
    /// Living agents at closing: the quorum's denominator.
    pub alive: u32,
    pub carried: bool,
    pub effect: Effect,
}

/// The institutional state of a world (ADR-0014 D2). Part of the
/// snapshot; hashed only when `config.nomos` is on. Everything in it is
/// changed only by `World` (Apply's fixed phases and interventions).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Nomos {
    /// How the assembly decides; `WorldConfig.charter` at generation.
    pub charter: Charter,
    /// Rules in force, ascending by id; at most one per `NormKind`.
    pub rules: Vec<Rule>,
    /// Next rule id to hand out. Monotonic; never reused.
    pub next_rule_id: u16,
    /// The one floor of the assembly (ADR-0014 D7): the open motion, if any.
    pub motion: Option<Motion>,
    /// Next motion id to hand out. Monotonic; never reused.
    pub next_motion_id: u16,
    /// Open episodes, ascending by (agent, rule); every agent alive, every
    /// rule in force.
    pub standing: Vec<Standing>,
    /// Accepted deals not yet settled, in acceptance order.
    pub deals: Vec<OpenDeal>,
}

impl Nomos {
    /// Nothing tracked and nothing in force: the state a world with
    /// `nomos` off must be in.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The rule with this id, if in force.
    pub fn rule(&self, id: u16) -> Option<&Rule> {
        self.rules.iter().find(|r| r.id == id)
    }

    /// The rule in force for a kind, if any.
    pub fn rule_of(&self, kind: NormKind) -> Option<&Rule> {
        self.rules.iter().find(|r| r.norm.kind == kind)
    }

    /// Puts `norm` in force at `tick` as a new rule and returns it, with
    /// the rule of the same kind it replaced (and that rule's closed
    /// episodes), if there was one (ADR-0014 D5: replacement = repeal +
    /// enactment). Ids are never reused; the caller must have checked
    /// `next_rule_id < u16::MAX`.
    pub fn enact(&mut self, norm: Norm, tick: u64) -> (Rule, Option<(Rule, Vec<Closed>)>) {
        let replaced = self
            .rule_of(norm.kind)
            .map(|r| r.id)
            .and_then(|id| self.repeal(id));
        let rule = Rule {
            id: self.next_rule_id,
            norm,
            enacted: tick,
        };
        self.next_rule_id += 1;
        self.rules.push(rule);
        (rule, replaced)
    }

    /// Takes the rule with this id out of force and closes every episode
    /// under it (counted as closed, sanctioned or not). `None` if no such
    /// rule is in force.
    pub fn repeal(&mut self, id: u16) -> Option<(Rule, Vec<Closed>)> {
        let at = self.rules.iter().position(|r| r.id == id)?;
        let rule = self.rules.remove(at);
        let closed = self.close_where(Some(rule.norm.kind), |s| s.rule == id);
        Some((rule, closed))
    }

    /// Open episodes of `agent`, ascending by rule id (at most one per rule).
    pub fn standing_of(&self, agent: AgentId) -> &[Standing] {
        let start = self.standing.partition_point(|s| s.agent < agent);
        let end = start + self.standing[start..].partition_point(|s| s.agent == agent);
        &self.standing[start..end]
    }

    /// Records a violation of `rule` by `agent` against `against` at
    /// `tick`: opens the (agent, rule) episode or refreshes it (`last`,
    /// `against`, `violations`). Keeps `standing` sorted by (agent, rule).
    pub fn accuse(&mut self, agent: AgentId, rule: u16, tick: u64, against: AgentId) {
        match self
            .standing
            .binary_search_by_key(&(agent, rule), |s| (s.agent, s.rule))
        {
            Ok(i) => {
                let s = &mut self.standing[i];
                s.last = tick;
                s.against = against;
                s.violations = s.violations.saturating_add(1);
            }
            Err(i) => self.standing.insert(
                i,
                Standing {
                    agent,
                    rule,
                    since: tick,
                    last: tick,
                    against,
                    violations: 1,
                    sanctions: 0,
                },
            ),
        }
    }

    /// Attributes an answer `via` to `target` at `tick` to the oldest
    /// episode `target` stood accused in at the tick's start — one opened
    /// before `tick` under a rule whose sanction set allows `via` (ties by
    /// `since` go to the smaller rule id). Counts it on that episode and
    /// returns the episode's rule and kind, or `None` when the answer is
    /// an ordinary harm or rejection.
    pub fn sanction(&mut self, target: AgentId, tick: u64, via: Via) -> Option<(u16, NormKind)> {
        let start = self.standing.partition_point(|s| s.agent < target);
        let mut best: Option<(usize, NormKind)> = None;
        for i in start..self.standing.len() {
            let s = self.standing[i];
            if s.agent != target {
                break;
            }
            if s.since >= tick {
                continue;
            }
            let Some(rule) = self.rule(s.rule) else {
                continue;
            };
            if !rule.norm.sanction.allows(via) {
                continue;
            }
            // `standing` is ascending by rule id within an agent, so the
            // first entry with the smallest `since` wins ties.
            if best.is_none_or(|(b, _)| s.since < self.standing[b].since) {
                best = Some((i, rule.norm.kind));
            }
        }
        let (i, kind) = best?;
        let s = &mut self.standing[i];
        s.sanctions = s.sanctions.saturating_add(1);
        Some((s.rule, kind))
    }

    /// Closes every episode whose accusation does not reach the next tick:
    /// `last + standing_ticks <= tick`. Called at the end of phase 3c.
    pub fn expire(&mut self, tick: u64) -> Vec<Closed> {
        let rules = self.rules.clone();
        self.close_where(None, |s| {
            rules
                .iter()
                .find(|r| r.id == s.rule)
                .is_none_or(|r| s.until(r.norm.standing_ticks) <= tick)
        })
    }

    /// Closes every episode of an agent that left the world (death, or
    /// `RemoveAgent`).
    pub fn drop_agent(&mut self, agent: AgentId) -> Vec<Closed> {
        self.close_where(None, |s| s.agent == agent)
    }

    /// Closes every episode whose agent is not in `alive` (ascending by id).
    pub fn drop_dead(&mut self, alive: &[Agent]) -> Vec<Closed> {
        self.close_where(None, |s| {
            alive.binary_search_by_key(&s.agent, |a| a.id).is_err()
        })
    }

    /// Removes every episode `pred` selects and reports each with its
    /// rule's kind — looked up in `rules`, or `repealed` for a rule that
    /// has just been taken out of force (an episode under a rule that is
    /// neither cannot exist under the invariants and is dropped uncounted).
    fn close_where(
        &mut self,
        repealed: Option<NormKind>,
        mut pred: impl FnMut(&Standing) -> bool,
    ) -> Vec<Closed> {
        let mut closed = Vec::new();
        let rules = &self.rules;
        self.standing.retain(|s| {
            if !pred(s) {
                return true;
            }
            let kind = rules
                .iter()
                .find(|r| r.id == s.rule)
                .map(|r| r.norm.kind)
                .or(repealed);
            if let Some(kind) = kind {
                closed.push(Closed {
                    kind,
                    sanctioned: s.sanctions > 0,
                });
            }
            false
        });
        closed
    }

    /// Books one transfer against every open deal whose window holds it.
    /// Overlapping deals between the same pair both count it, as
    /// `thread.rs` does (ADR-0013 D5 addendum).
    pub fn credit_transfer(&mut self, tick: u64, from: AgentId, to: AgentId, amount: u32) {
        for d in &mut self.deals {
            d.credit(tick, from, to, amount);
        }
    }

    /// Removes and returns, in acceptance order, every deal whose deadline
    /// is `tick` or earlier (a late acceptance settles in its own tick).
    pub fn settle(&mut self, tick: u64) -> Vec<OpenDeal> {
        let mut due = Vec::new();
        self.deals.retain(|d| {
            if d.deadline <= tick {
                due.push(*d);
                false
            } else {
                true
            }
        });
        due
    }

    // --- the assembly (ADR-0014 D7, 4-5) --------------------------------

    /// Whether a motion asking for `kind` may be moved now: the floor must
    /// be free (`FloorTaken`, also when the motion ids are exhausted), the
    /// charter must allow an amendment if the motion is one
    /// (`NotAmendable`), and the content must be able to become a rule or
    /// charter (`NoSuchRule`: a repeal of an id not in force, a norm with
    /// unknown sanction bits or no rule id left, a charter out of range).
    /// Procedural impossibilities, all of them — never a judgement.
    pub fn admit(&self, kind: MotionKind) -> Result<(), Failure> {
        if self.motion.is_some() || self.next_motion_id == u16::MAX {
            return Err(Failure::FloorTaken);
        }
        if !self.charter.amendable && kind.amends(self) {
            return Err(Failure::NotAmendable);
        }
        let admissible = match kind {
            MotionKind::Enact { norm } => norm.is_valid() && self.next_rule_id < u16::MAX,
            MotionKind::Repeal { rule } => self.rule(rule).is_some(),
            MotionKind::Charter { charter } => charter.is_valid(),
        };
        if admissible {
            Ok(())
        } else {
            Err(Failure::NoSuchRule)
        }
    }

    /// Opens the floor with a motion `admit` accepted, moved by `by` at
    /// `tick`, and returns its id (handed out once, never reused).
    pub fn open_motion(&mut self, kind: MotionKind, by: AgentId, tick: u64) -> u16 {
        debug_assert!(self.admit(kind).is_ok(), "admit first");
        let id = self.next_motion_id;
        self.next_motion_id += 1;
        self.motion = Some(Motion {
            id,
            by,
            opened: tick,
            kind,
            ayes: Vec::new(),
            nays: Vec::new(),
        });
        id
    }

    /// Counts `agent`'s vote on the open motion and returns the motion's
    /// id: `NoMotion` without one, `AlreadyVoted` for a second vote (on
    /// either side). The sides stay ascending by id.
    pub fn vote(&mut self, agent: AgentId, aye: bool) -> Result<u16, Failure> {
        let m = self.motion.as_mut().ok_or(Failure::NoMotion)?;
        let (mine, theirs) = if aye {
            (&mut m.ayes, &m.nays)
        } else {
            (&mut m.nays, &m.ayes)
        };
        if theirs.binary_search(&agent).is_ok() {
            return Err(Failure::AlreadyVoted);
        }
        match mine.binary_search(&agent) {
            Ok(_) => Err(Failure::AlreadyVoted),
            Err(at) => {
                mine.insert(at, agent);
                Ok(m.id)
            }
        }
    }

    /// Whether the open motion closes in `tick`'s phase 3c: the charter's
    /// `vote_ticks` (as it stands now) have passed since it was moved. With
    /// `vote_ticks` 0 a motion closes in the tick it was moved, counting
    /// the votes of the actors after the mover.
    pub fn motion_due(&self, tick: u64) -> bool {
        self.motion
            .as_ref()
            .is_some_and(|m| m.opened + self.charter.vote_ticks as u64 <= tick)
    }

    /// Closes the open motion at `tick` before `alive` living agents
    /// (ADR-0014 D7) and puts it into effect if it carried: the votes cast
    /// reach the quorum (`votes · 1000 >= quorum_permille · alive`), the
    /// ayes reach the threshold of the votes (`ayes · 1000 >=
    /// threshold_permille · votes`), and someone voted aye (a motion nobody
    /// supported never carries, whatever the permilles). The charter read
    /// is the one in force at closing. A carried motion that can no longer
    /// take effect — its rule left force meanwhile, the charter no longer
    /// amends, no rule id is left — closes as carried with `Effect::Nothing`:
    /// the assembly's decision is recorded as it was, the effect is what the
    /// kernel could do. `None` when no motion is open. The floor is free
    /// afterwards.
    pub fn close_motion(&mut self, tick: u64, alive: usize) -> Option<Closing> {
        let motion = self.motion.take()?;
        let (ayes, nays) = (motion.ayes.len() as u64, motion.nays.len() as u64);
        let votes = ayes + nays;
        let c = self.charter;
        let carried = votes * 1000 >= c.quorum_permille as u64 * alive as u64
            && ayes * 1000 >= c.threshold_permille as u64 * votes
            && ayes > 0;
        let effect = if !carried || (!c.amendable && motion.kind.amends(self)) {
            Effect::Nothing
        } else {
            match motion.kind {
                MotionKind::Enact { norm } => {
                    if norm.is_valid() && self.next_rule_id < u16::MAX {
                        let (rule, replaced) = self.enact(norm, tick);
                        Effect::Enacted { rule, replaced }
                    } else {
                        Effect::Nothing
                    }
                }
                MotionKind::Repeal { rule } => match self.repeal(rule) {
                    Some((rule, closed)) => Effect::Repealed { rule, closed },
                    None => Effect::Nothing,
                },
                MotionKind::Charter { charter } => {
                    if charter.is_valid() {
                        self.charter = charter;
                        Effect::Chartered(charter)
                    } else {
                        Effect::Nothing
                    }
                }
            }
        };
        Some(Closing {
            motion,
            ayes: ayes as u32,
            nays: nays as u32,
            alive: alive as u32,
            carried,
            effect,
        })
    }

    /// Strikes the vote of an agent that left the world (death,
    /// `RemoveAgent`): the dead do not count at closing (ADR-0014 D7).
    pub fn drop_voter(&mut self, agent: AgentId) {
        if let Some(m) = self.motion.as_mut() {
            for side in [&mut m.ayes, &mut m.nays] {
                if let Ok(i) = side.binary_search(&agent) {
                    side.remove(i);
                }
            }
        }
    }

    /// Strikes the votes of every agent not in `alive` (ascending by id).
    pub fn drop_dead_voters(&mut self, alive: &[Agent]) {
        if let Some(m) = self.motion.as_mut() {
            for side in [&mut m.ayes, &mut m.nays] {
                side.retain(|v| alive.binary_search_by_key(v, |a| a.id).is_ok());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn founders_are_the_oldest_and_same_tick_arrivals_do_not_encroach() {
        assert!(encroaches(None, Some(0)));
        assert!(encroaches(Some(3), Some(4)));
        assert!(!encroaches(Some(4), Some(4)));
        assert!(!encroaches(None, None));
        assert!(!encroaches(Some(4), Some(3)));
        assert!(!encroaches(Some(1), None));
    }

    #[test]
    fn deals_credit_only_the_pair_inside_the_window_and_settle_at_the_deadline() {
        let (p, a, x) = (AgentId(1), AgentId(2), AgentId(3));
        let mut n = Nomos::default();
        n.deals.push(OpenDeal {
            proposer: p,
            acceptor: a,
            offer_tick: 5,
            accepted_tick: 6,
            deadline: 9,
            give: 2,
            want: 3,
            given: 0,
            repaid: 0,
        });
        n.credit_transfer(5, p, a, 2); // before the acceptance: unseen
        n.credit_transfer(6, p, a, 2); // the acceptance tick counts
        n.credit_transfer(7, a, p, 1);
        n.credit_transfer(7, x, p, 5); // a third party
        n.credit_transfer(10, a, p, 9); // after the deadline
        assert_eq!((n.deals[0].given, n.deals[0].repaid), (2, 1));
        assert!(n.settle(8).is_empty(), "not due yet");
        let due = n.settle(9);
        assert_eq!(due.len(), 1);
        assert!(n.deals.is_empty());
        let d = due[0];
        assert!(!d.kept() && !d.proposer_short() && d.acceptor_short());
        let kept = OpenDeal { repaid: 3, ..d };
        assert!(kept.kept() && !kept.acceptor_short());
    }

    fn norm(kind: NormKind, standing_ticks: u16, sanction: Sanction) -> Norm {
        Norm {
            kind,
            standing_ticks,
            sanction,
            except_sanction: false,
        }
    }

    #[test]
    fn sanction_bits_name_and_parse() {
        for s in [
            Sanction::NONE,
            Sanction::HARM,
            Sanction::REJECT,
            Sanction::ALL,
        ] {
            assert_eq!(Sanction::parse(s.name()), Some(s));
            assert!(s.is_valid());
        }
        assert_eq!(Sanction::parse("reject+harm"), Some(Sanction::ALL));
        assert_eq!(Sanction::parse("both"), Some(Sanction::ALL));
        assert_eq!(Sanction::parse("fine"), None);
        assert!(!Sanction(4).is_valid());
        assert!(Sanction::HARM.allows(Via::Harm) && !Sanction::HARM.allows(Via::Reject));
        assert!(Sanction::REJECT.allows(Via::Reject) && !Sanction::REJECT.allows(Via::Harm));
        assert!(!Sanction::NONE.allows(Via::Harm));
        for k in NormKind::ALL {
            assert_eq!(NormKind::parse(k.name()), Some(k));
            assert_eq!(NormKind::ALL[k.index()], k);
        }
        assert_eq!(
            NormKind::parse("broken-promise"),
            Some(NormKind::BrokenPromise)
        );
        assert_eq!(NormKind::parse("theft"), None);
    }

    #[test]
    fn enacting_a_kind_in_force_replaces_its_rule_and_closes_its_episodes() {
        let mut n = Nomos::default();
        let (r0, replaced) = n.enact(norm(NormKind::Encroach, 32, Sanction::HARM), 10);
        assert_eq!((r0.id, r0.enacted), (0, 10));
        assert!(replaced.is_none());
        let (r1, _) = n.enact(norm(NormKind::Harm, 8, Sanction::NONE), 11);
        assert_eq!(r1.id, 1);
        n.accuse(AgentId(5), 0, 12, AgentId(2));
        n.accuse(AgentId(3), 0, 12, AgentId(2));
        n.accuse(AgentId(5), 1, 13, AgentId(9));
        assert_eq!(
            n.sanction(AgentId(5), 14, Via::Harm),
            Some((0, NormKind::Encroach))
        );
        let (r2, replaced) = n.enact(norm(NormKind::Encroach, 16, Sanction::REJECT), 20);
        assert_eq!((r2.id, r2.norm.standing_ticks), (2, 16));
        let (old, closed) = replaced.unwrap();
        assert_eq!(old, r0);
        assert_eq!(
            closed,
            vec![
                Closed {
                    kind: NormKind::Encroach,
                    sanctioned: false
                },
                Closed {
                    kind: NormKind::Encroach,
                    sanctioned: true
                },
            ]
        );
        assert_eq!(n.rules.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(n.standing.len(), 1);
        assert_eq!(n.standing_of(AgentId(5)).len(), 1);
        assert!(n.standing_of(AgentId(3)).is_empty());
        assert_eq!(n.next_rule_id, 3);
        assert!(n.repeal(0).is_none());
        let (gone, closed) = n.repeal(1).unwrap();
        assert_eq!(gone.id, 1);
        assert_eq!(closed.len(), 1);
        assert!(n.standing.is_empty());
        assert_eq!(n.rule_of(NormKind::Encroach).map(|r| r.id), Some(2));
        assert!(n.rule_of(NormKind::Harm).is_none());
    }

    #[test]
    fn accusations_fold_into_one_episode_and_sanctions_go_to_the_oldest() {
        let mut n = Nomos::default();
        n.enact(norm(NormKind::Encroach, 4, Sanction::HARM), 0);
        n.enact(norm(NormKind::Harm, 4, Sanction::ALL), 0);
        n.enact(norm(NormKind::BrokenPromise, 4, Sanction::REJECT), 0);
        n.accuse(AgentId(7), 2, 5, AgentId(1)); // harm rule (id 1) is older below
        n.accuse(AgentId(7), 1, 3, AgentId(1));
        n.accuse(AgentId(7), 0, 3, AgentId(2));
        n.accuse(AgentId(7), 0, 4, AgentId(3));
        n.accuse(AgentId(2), 0, 4, AgentId(3));
        assert_eq!(
            n.standing
                .iter()
                .map(|s| (s.agent.0, s.rule))
                .collect::<Vec<_>>(),
            vec![(2, 0), (7, 0), (7, 1), (7, 2)],
            "sorted by (agent, rule)"
        );
        let s = n.standing_of(AgentId(7));
        assert_eq!(
            (s[0].since, s[0].last, s[0].violations, s[0].against),
            (3, 4, 2, AgentId(3))
        );
        // Same `since` under rules 0 and 1: the smaller rule id wins; a
        // rejection may only answer rule 1 or 2 — rule 1 is older.
        assert_eq!(
            n.sanction(AgentId(7), 6, Via::Harm),
            Some((0, NormKind::Encroach))
        );
        assert_eq!(
            n.sanction(AgentId(7), 6, Via::Reject),
            Some((1, NormKind::Harm))
        );
        assert_eq!(n.sanction(AgentId(9), 6, Via::Harm), None, "nobody accused");
        // An episode opened this very tick is not standing yet.
        n.accuse(AgentId(9), 0, 6, AgentId(1));
        assert_eq!(n.sanction(AgentId(9), 6, Via::Harm), None);
        assert_eq!(
            n.sanction(AgentId(9), 7, Via::Harm),
            Some((0, NormKind::Encroach))
        );
        assert_eq!(n.standing_of(AgentId(9))[0].sanctions, 1);
        // Accused through `last + standing_ticks`: 7's rule-0 episode
        // (last 4) closes at the end of tick 8, 2's too; 7's rule 1 (last
        // 3) at tick 7.
        assert!(n.expire(6).is_empty());
        let closed = n.expire(7);
        assert_eq!(
            closed,
            vec![Closed {
                kind: NormKind::Harm,
                sanctioned: true
            }]
        );
        let closed = n.expire(8);
        assert_eq!(closed.len(), 2);
        assert!(closed.iter().all(|c| c.kind == NormKind::Encroach));
        assert_eq!(closed.iter().filter(|c| c.sanctioned).count(), 1);
        assert_eq!(
            n.standing
                .iter()
                .map(|s| (s.agent.0, s.rule))
                .collect::<Vec<_>>(),
            vec![(7, 2), (9, 0)]
        );
        let closed = n.drop_agent(AgentId(7));
        assert_eq!(
            closed,
            vec![Closed {
                kind: NormKind::BrokenPromise,
                sanctioned: false
            }]
        );
        assert!(!n.is_empty());
    }

    #[test]
    fn a_zero_standing_period_never_accuses_and_the_charter_validates() {
        let mut n = Nomos::default();
        n.enact(norm(NormKind::Harm, 0, Sanction::HARM), 0);
        n.accuse(AgentId(1), 0, 5, AgentId(2));
        // Phase 3c of the violation's own tick closes it: nobody ever
        // sees the agent accused.
        assert_eq!(
            n.expire(5),
            vec![Closed {
                kind: NormKind::Harm,
                sanctioned: false
            }]
        );
        assert_eq!(
            n.sanction(AgentId(1), 6, Via::Harm),
            None,
            "closed before it was seen"
        );
        assert!(Charter::default().is_valid());
        assert!(!Charter {
            quorum_permille: 1001,
            ..Charter::default()
        }
        .is_valid());
        assert_eq!(Nomos::default().charter, Charter::default());
        assert!(Nomos::default().is_empty());
        let mut with_charter = Nomos::default();
        with_charter.charter.amendable = false;
        assert!(!with_charter.is_empty());
    }
}
