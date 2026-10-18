//! Lifecycle events (ADR-0008).
//!
//! Every record carries its own tick and is a plain tagged enum, so a record
//! can be appended verbatim to the Phase 2 event ledger (one JSON object per
//! line) without translation. Phase 2 will add `Acted { agent, intent,
//! outcome }` next to these; nothing here should need to change for that.

use crate::action::{Action, Outcome};
use crate::agent::AgentId;
use crate::intervention::Intervention;
use crate::memory::Message;
use crate::nomos::{Charter, Grounds, MotionKind, Norm, NormKind, Via};
use crate::space::Pos;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeathCause {
    /// Health reached zero while energy was exhausted.
    Starvation,
    /// `age >= max_age`.
    OldAge,
    /// Health reached zero in a tick in which the agent was harmed
    /// (ADR-0013 D3); takes precedence over starvation.
    Violence,
}

impl DeathCause {
    pub const COUNT: usize = 3;

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Born {
        tick: u64,
        child: AgentId,
        parent: AgentId,
        pos: Pos,
        /// Energy moved from the parent; the child starts with exactly this.
        energy: i64,
    },
    Died {
        tick: u64,
        agent: AgentId,
        cause: DeathCause,
        pos: Pos,
        age: u64,
        /// Energy the body still held; removed from the world via the ledger.
        energy_written_off: i64,
        /// Carried food returned to the cell the agent died on.
        food_dropped: u32,
    },
    /// An external change applied by `World::intervene` before `tick` ran.
    Intervened {
        tick: u64,
        intervention: Intervention,
    },
    /// Food handed from one agent to an adjacent one (`Action::Transfer`).
    Transferred {
        tick: u64,
        from: AgentId,
        to: AgentId,
        amount: u32,
    },
    /// Health taken from an adjacent agent (`Action::Harm`).
    Harmed {
        tick: u64,
        by: AgentId,
        target: AgentId,
        damage: i64,
    },
    /// What an agent tried and what came of it. Recorded for every agent of
    /// every tick when `record_actions` is on (ADR-0013 D3): the raw material
    /// for defining rule violations in Phase 4 (R5).
    Acted {
        tick: u64,
        agent: AgentId,
        action: Action,
        outcome: Outcome,
    },
    /// A `Say` was made (`Action::Signal(Message::Say)`). Recorded when
    /// `record_signals`. Negotiation messages have their own, always-recorded
    /// events below; an undelivered message is recorded nowhere.
    Signaled {
        tick: u64,
        agent: AgentId,
        message: Message,
    },
    /// `from`'s proposal reached `to` (ADR-0013 D5, 3-4): "I give you `give`
    /// now; give me `want` back by tick `tick + by`". Always recorded — it is
    /// the first half of the material from which Phase 4 judges fulfilment.
    Proposed {
        tick: u64,
        from: AgentId,
        to: AgentId,
        give: u8,
        want: u8,
        by: u16,
    },
    /// `from` accepted the proposal `to` made at `offer_tick`. The terms are
    /// repeated so the record stands alone: `to` owes `from` `give` now, and
    /// `from` owes `to` `want` by `offer_tick + by`. Whether either
    /// `Transferred` follows is not the kernel's business. Always recorded.
    Accepted {
        tick: u64,
        from: AgentId,
        to: AgentId,
        offer_tick: u64,
        give: u8,
        want: u8,
        by: u16,
    },
    /// `from` declined `to`'s proposal. Always recorded.
    Rejected {
        tick: u64,
        from: AgentId,
        to: AgentId,
    },
    /// A harm or rejection (`via`) by `by` reached `target` while `target`
    /// stood accused under `rule` (ADR-0014 D4): the kernel's reading of
    /// an ordinary `Harmed` / `Rejected` of the same tick as a sanction.
    /// Attributed to the oldest open episode whose rule allows `via`.
    /// Always recorded (bounded by harms + rejections); the physical event
    /// it reads is recorded too, just before it.
    Sanctioned {
        tick: u64,
        by: AgentId,
        target: AgentId,
        rule: u16,
        via: Via,
    },
    /// `agent` violated `rule` (a norm of kind `norm`) against `against`
    /// on the evidence `grounds` (ADR-0014 D4). Recorded only with
    /// `record_violations` (off by default: 4-1 measured ~21 per tick in
    /// a crowded world); the ledger's `violations` counter and the open
    /// episodes in `World.nomos.standing` are kept either way.
    Violated {
        tick: u64,
        agent: AgentId,
        rule: u16,
        norm: NormKind,
        against: AgentId,
        grounds: Grounds,
    },
    /// `by` moved motion `motion` before the assembly, asking for `asks`
    /// (ADR-0014 D7, 4-5): the one floor is held from this tick's phase
    /// 3b until the motion closes. Always recorded (one per motion).
    Motioned {
        tick: u64,
        motion: u16,
        by: AgentId,
        asks: MotionKind,
    },
    /// `by` voted `aye` (for) or not (against) on motion `motion`. Recorded
    /// only with `record_votes` (off by default); the tally is counted and
    /// hashed either way.
    Voted {
        tick: u64,
        motion: u16,
        by: AgentId,
        aye: bool,
    },
    /// Motion `motion` closed in this tick's phase 3c with `ayes` for and
    /// `nays` against among `alive` living agents, and `carried` or not
    /// (quorum, threshold and at least one aye, ADR-0014 D7). A carried
    /// motion's effect follows as `Enacted` / `Repealed` / `Chartered` —
    /// or nothing, when it could no longer take effect. Always recorded.
    MotionClosed {
        tick: u64,
        motion: u16,
        ayes: u32,
        nays: u32,
        alive: u32,
        carried: bool,
    },
    /// `norm` came into force as rule `rule` by motion `motion`, from the
    /// next tick. A rule of the same kind it replaced is recorded as
    /// `Repealed` just before. Always recorded. (A decree's enactment is
    /// its `Intervened` record, ADR-0014 D8.)
    Enacted {
        tick: u64,
        rule: u16,
        norm: Norm,
        motion: u16,
    },
    /// Rule `rule`, which said `norm`, left force by motion `motion`: a
    /// repeal, or the first half of a replacement. Always recorded.
    Repealed {
        tick: u64,
        rule: u16,
        norm: Norm,
        motion: u16,
    },
    /// The charter was replaced by motion `motion`; the next motion is
    /// moved and closed under `charter`. Always recorded.
    Chartered {
        tick: u64,
        charter: Charter,
        motion: u16,
    },
}

impl Event {
    pub fn tick(&self) -> u64 {
        match self {
            Event::Born { tick, .. }
            | Event::Died { tick, .. }
            | Event::Intervened { tick, .. }
            | Event::Transferred { tick, .. }
            | Event::Harmed { tick, .. }
            | Event::Acted { tick, .. }
            | Event::Signaled { tick, .. }
            | Event::Proposed { tick, .. }
            | Event::Accepted { tick, .. }
            | Event::Rejected { tick, .. }
            | Event::Sanctioned { tick, .. }
            | Event::Violated { tick, .. }
            | Event::Motioned { tick, .. }
            | Event::Voted { tick, .. }
            | Event::MotionClosed { tick, .. }
            | Event::Enacted { tick, .. }
            | Event::Repealed { tick, .. }
            | Event::Chartered { tick, .. } => *tick,
        }
    }
}
