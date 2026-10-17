//! Per-tick view of who stands accused (ADR-0014 D2): derived from
//! `World.nomos` at the tick's start, right after the `SpatialIndex`, and
//! never stored or hashed.
//!
//! The index holds one flag byte per agent index of `World.agents`:
//! `ACCUSED` (some episode is open), `HARMABLE` / `REJECTABLE` (open under
//! a rule whose sanction set allows that answer) and `VOTED` (has voted on
//! the open motion; motions arrive with task 4-5), plus the rule in force
//! per kind and the kind of the open motion. Decide reads it through
//! `Observation.norms` (4-3: the norm features of the learning policies
//! and the Harm actuator) and Apply reads it to tell a sanction from an
//! ordinary harm or rejection: what counts is who stood accused at S(t),
//! not what this tick's violations add. With no rule in force and no
//! motion open the index is empty, holds no per-agent bytes, and every
//! query answers "no" in constant time.
//!
//! Beside the flags it keeps, per cell of the grid, how many agents carry
//! `HARMABLE` (`harmable_on`), so that a policy can tell whether an
//! accused agent stands within reach by reading five counts instead of
//! walking five cells (4-9: `Observation::violator_adjacent`, behind the
//! reflex feature `VIOLATOR_ADJACENT` and the learner bit of that name).
//! The counts are allocated the first time an agent turns out harmable:
//! a world with no rule, or none whose sanction set allows a harm, or no
//! open episode under such a rule, pays nothing for them.
//!
//! Construction is one merge walk over `nomos.standing` (ascending by
//! agent id) and `agents` (ascending by id): O(agents + episodes), no hash
//! map, no binary search per episode, plus the zeroing of the counts when
//! they are needed (one `u16` per cell).

use super::{MotionKind, Nomos, NormKind, Rule, Sanction, Via};
use crate::agent::Agent;
use crate::space::{Bounds, Pos};

pub struct NormIndex {
    /// One flag byte per agent index; empty when nothing is in force.
    flags: Vec<u8>,
    /// How many agents of each cell (index = `Bounds::index`) carry
    /// `HARMABLE`, saturating at `u16::MAX`; empty until the first one.
    harmable: Vec<u16>,
    /// The grid `harmable` is laid out over.
    bounds: Bounds,
    /// The rule in force per kind (index = `NormKind::index`).
    rules: [Option<Rule>; NormKind::COUNT],
    /// What the open motion asks for, if one is open.
    motion: Option<MotionKind>,
}

impl NormIndex {
    /// Some episode of the agent is open.
    pub const ACCUSED: u8 = 1 << 0;
    /// Accused under a rule that lets a harm count as a sanction.
    pub const HARMABLE: u8 = 1 << 1;
    /// Accused under a rule that lets a rejection count as a sanction.
    pub const REJECTABLE: u8 = 1 << 2;
    /// Has voted on the open motion.
    pub const VOTED: u8 = 1 << 3;

    /// The view of a world with nothing in force.
    pub fn empty() -> Self {
        Self {
            flags: Vec::new(),
            harmable: Vec::new(),
            bounds: Bounds {
                width: 0,
                height: 0,
            },
            rules: [None; NormKind::COUNT],
            motion: None,
        }
    }

    /// Builds the view of `agents` (ascending by id, standing on the grid
    /// `bounds`) under `nomos`.
    pub fn build(agents: &[Agent], nomos: &Nomos, bounds: Bounds) -> Self {
        let mut rules = [None; NormKind::COUNT];
        for r in &nomos.rules {
            rules[r.norm.kind.index()] = Some(*r);
        }
        let motion = nomos.motion.as_ref().map(|m| m.kind);
        if nomos.rules.is_empty() && motion.is_none() {
            return Self {
                flags: Vec::new(),
                harmable: Vec::new(),
                bounds,
                rules,
                motion,
            };
        }
        let mut flags = vec![0u8; agents.len()];
        let mut harmable: Vec<u16> = Vec::new();
        // Merge walk: both sequences ascend by agent id.
        let mut i = 0usize;
        for s in &nomos.standing {
            while i < agents.len() && agents[i].id < s.agent {
                i += 1;
            }
            if i >= agents.len() {
                break;
            }
            if agents[i].id != s.agent {
                continue; // not alive: cannot happen under the invariants
            }
            let sanction = nomos
                .rule(s.rule)
                .map_or(Sanction::NONE, |r| r.norm.sanction);
            let bits = Self::ACCUSED
                | if sanction.allows(Via::Harm) {
                    Self::HARMABLE
                } else {
                    0
                }
                | if sanction.allows(Via::Reject) {
                    Self::REJECTABLE
                } else {
                    0
                };
            // An agent counts once in its cell, however many of its
            // episodes allow a harm.
            if bits & Self::HARMABLE != 0 && flags[i] & Self::HARMABLE == 0 {
                if harmable.is_empty() {
                    harmable = vec![0u16; bounds.cell_count()];
                }
                let c = &mut harmable[bounds.index(agents[i].pos)];
                *c = c.saturating_add(1);
            }
            flags[i] |= bits;
        }
        if let Some(m) = &nomos.motion {
            for voters in [&m.ayes, &m.nays] {
                let mut i = 0usize;
                for v in voters {
                    while i < agents.len() && agents[i].id < *v {
                        i += 1;
                    }
                    if i >= agents.len() {
                        break;
                    }
                    if agents[i].id == *v {
                        flags[i] |= Self::VOTED;
                    }
                }
            }
        }
        Self {
            flags,
            harmable,
            bounds,
            rules,
            motion,
        }
    }

    /// Nothing in force: no rule, no open motion.
    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
    }

    /// Number of agents the index was built over (0 when nothing is in
    /// force, whatever the population).
    pub fn len(&self) -> usize {
        self.flags.len()
    }

    /// What the open motion asks for, if one is open.
    pub fn motion(&self) -> Option<MotionKind> {
        self.motion
    }

    /// Some rule is in force.
    pub fn any_rule(&self) -> bool {
        self.rules.iter().any(Option::is_some)
    }

    /// The rule in force for `kind`.
    pub fn rule(&self, kind: NormKind) -> Option<&Rule> {
        self.rules[kind.index()].as_ref()
    }

    /// The flag byte of agent index `i` (0 when nothing is in force).
    pub fn flags(&self, i: usize) -> u8 {
        self.flags.get(i).copied().unwrap_or(0)
    }

    pub fn accused(&self, i: usize) -> bool {
        self.flags(i) & Self::ACCUSED != 0
    }

    pub fn harmable(&self, i: usize) -> bool {
        self.flags(i) & Self::HARMABLE != 0
    }

    /// How many agents standing on `p` are `harmable` (0 with nothing in
    /// force, with no harmable agent anywhere, or off the grid the index
    /// was built over); `u16::MAX` stands for that many or more.
    pub fn harmable_on(&self, p: Pos) -> u16 {
        if self.harmable.is_empty() {
            return 0;
        }
        self.harmable
            .get(self.bounds.index(p))
            .copied()
            .unwrap_or(0)
    }

    pub fn rejectable(&self, i: usize) -> bool {
        self.flags(i) & Self::REJECTABLE != 0
    }

    pub fn voted(&self, i: usize) -> bool {
        self.flags(i) & Self::VOTED != 0
    }

    /// Whether `via` answers an accused agent `i` as a sanction.
    pub fn sanctionable(&self, i: usize, via: Via) -> bool {
        match via {
            Via::Harm => self.harmable(i),
            Via::Reject => self.rejectable(i),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentId, Memory};
    use crate::nomos::{Motion, MotionKind, Norm, Standing};
    use crate::policy::{PolicyKind, PolicyParams};
    use crate::space::Pos;

    const GRID: Bounds = Bounds {
        width: 3,
        height: 2,
    };

    fn agent(id: u64) -> Agent {
        agent_at(id, Pos::new(0, 0))
    }

    fn agent_at(id: u64, pos: Pos) -> Agent {
        Agent {
            id: AgentId(id),
            pos,
            energy: 1,
            health: 1,
            inventory: 0,
            age: 0,
            policy: PolicyKind::Forager,
            params: PolicyParams::default(),
            memory: Memory::default(),
            last_signal: None,
            recent: Vec::new(),
            last_step: None,
            arrived: None,
        }
    }

    fn standing(agent: u64, rule: u16) -> Standing {
        Standing {
            agent: AgentId(agent),
            rule,
            since: 1,
            last: 1,
            against: AgentId(0),
            violations: 1,
            sanctions: 0,
        }
    }

    #[test]
    fn flags_follow_the_rules_sanction_sets_and_the_motions_voters() {
        let agents: Vec<Agent> = [1u64, 4, 6, 9, 12].into_iter().map(agent).collect();
        let mut n = Nomos::default();
        n.enact(
            Norm {
                kind: NormKind::Encroach,
                standing_ticks: 8,
                sanction: Sanction::HARM,
                except_sanction: false,
            },
            0,
        );
        n.enact(
            Norm {
                kind: NormKind::Harm,
                standing_ticks: 8,
                sanction: Sanction::REJECT,
                except_sanction: true,
            },
            0,
        );
        n.enact(
            Norm {
                kind: NormKind::BrokenPromise,
                standing_ticks: 8,
                sanction: Sanction::NONE,
                except_sanction: false,
            },
            0,
        );
        // 4: rule 0 (harm); 6: rules 1 and 2 (reject); 9: rule 2 only
        // (accused, no sanction); 7: not alive, skipped; 20: past the end.
        n.standing = vec![
            standing(4, 0),
            standing(6, 1),
            standing(6, 2),
            standing(7, 0),
            standing(9, 2),
            standing(20, 0),
        ];
        n.motion = Some(Motion {
            id: 0,
            by: AgentId(1),
            opened: 3,
            kind: MotionKind::Repeal { rule: 0 },
            ayes: vec![AgentId(1), AgentId(12)],
            nays: vec![AgentId(9), AgentId(30)],
        });
        let ix = NormIndex::build(&agents, &n, GRID);
        assert!(!ix.is_empty() && ix.any_rule());
        assert_eq!(ix.len(), agents.len());
        assert_eq!(ix.motion(), Some(MotionKind::Repeal { rule: 0 }));
        assert_eq!(ix.flags(0), NormIndex::VOTED);
        assert_eq!(ix.flags(1), NormIndex::ACCUSED | NormIndex::HARMABLE);
        assert_eq!(ix.flags(2), NormIndex::ACCUSED | NormIndex::REJECTABLE);
        assert_eq!(ix.flags(3), NormIndex::ACCUSED | NormIndex::VOTED);
        assert_eq!(ix.flags(4), NormIndex::VOTED);
        assert_eq!(ix.flags(5), 0, "out of range reads as nothing");
        assert!(ix.harmable(1) && !ix.rejectable(1) && ix.accused(1));
        assert!(ix.sanctionable(2, Via::Reject) && !ix.sanctionable(2, Via::Harm));
        assert!(ix.accused(3) && !ix.harmable(3) && !ix.rejectable(3) && ix.voted(3));
        assert_eq!(ix.rule(NormKind::Encroach).map(|r| r.id), Some(0));
        assert_eq!(
            ix.rule(NormKind::Harm).map(|r| r.norm.except_sanction),
            Some(true)
        );
        // Everyone stands on (0, 0): the one harmable agent (4) is counted
        // there and nowhere else.
        assert_eq!(ix.harmable_on(Pos::new(0, 0)), 1);
        assert_eq!(ix.harmable_on(Pos::new(1, 0)), 0);
        assert_eq!(ix.harmable_on(Pos::new(2, 1)), 0);
    }

    #[test]
    fn harmable_counts_follow_the_cells_and_count_an_agent_once() {
        let agents = vec![
            agent_at(1, Pos::new(0, 0)),
            agent_at(4, Pos::new(0, 0)),
            agent_at(6, Pos::new(1, 0)),
            agent_at(9, Pos::new(2, 1)),
            agent_at(12, Pos::new(2, 1)),
        ];
        let mut n = Nomos::default();
        n.enact(
            Norm {
                kind: NormKind::Encroach,
                standing_ticks: 8,
                sanction: Sanction::HARM,
                except_sanction: false,
            },
            0,
        );
        n.enact(
            Norm {
                kind: NormKind::Harm,
                standing_ticks: 8,
                sanction: Sanction::ALL,
                except_sanction: true,
            },
            0,
        );
        n.enact(
            Norm {
                kind: NormKind::BrokenPromise,
                standing_ticks: 8,
                sanction: Sanction::REJECT,
                except_sanction: false,
            },
            0,
        );
        // 4: two harm-answerable episodes, counted once; 6: harmable; 9:
        // accused under the reject-only rule, not harmable; 12: nothing.
        n.standing = vec![
            standing(4, 0),
            standing(4, 1),
            standing(6, 1),
            standing(9, 2),
        ];
        let ix = NormIndex::build(&agents, &n, GRID);
        assert_eq!(ix.harmable_on(Pos::new(0, 0)), 1, "4 once, 1 is clean");
        assert_eq!(ix.harmable_on(Pos::new(1, 0)), 1);
        assert_eq!(ix.harmable_on(Pos::new(2, 0)), 0);
        assert_eq!(ix.harmable_on(Pos::new(0, 1)), 0);
        assert_eq!(ix.harmable_on(Pos::new(1, 1)), 0);
        assert_eq!(ix.harmable_on(Pos::new(2, 1)), 0, "9 is only rejectable");
        assert!(ix.harmable(1) && ix.harmable(2) && !ix.harmable(3));
        // Two more harmable agents on (2, 1): the count follows.
        n.standing.push(standing(9, 0));
        n.standing.push(standing(12, 1));
        let ix = NormIndex::build(&agents, &n, GRID);
        assert_eq!(ix.harmable_on(Pos::new(2, 1)), 2);
        assert_eq!(ix.harmable_on(Pos::new(0, 0)), 1);
        // Rules in force but no harm-answerable episode: every count 0.
        n.standing = vec![standing(9, 2)];
        let ix = NormIndex::build(&agents, &n, GRID);
        assert!(!ix.is_empty() && ix.accused(3));
        for y in 0..GRID.height {
            for x in 0..GRID.width {
                assert_eq!(ix.harmable_on(Pos::new(x, y)), 0);
            }
        }
    }

    #[test]
    fn nothing_in_force_means_an_empty_index() {
        let agents: Vec<Agent> = (0..5).map(agent).collect();
        let n = Nomos::default();
        let ix = NormIndex::build(&agents, &n, GRID);
        assert!(ix.is_empty() && !ix.any_rule());
        assert_eq!(ix.len(), 0);
        assert_eq!(ix.motion(), None);
        for i in 0..5 {
            assert_eq!(ix.flags(i), 0);
            assert!(!ix.accused(i) && !ix.harmable(i) && !ix.rejectable(i) && !ix.voted(i));
        }
        assert_eq!(ix.harmable_on(Pos::new(0, 0)), 0);
        assert!(ix.rule(NormKind::Harm).is_none());
        let e = NormIndex::empty();
        assert!(e.is_empty());
        assert!(!e.accused(0));
        assert_eq!(e.harmable_on(Pos::new(0, 0)), 0);
    }
}
