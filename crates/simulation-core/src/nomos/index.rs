//! Per-tick view of who stands accused (ADR-0014 D2): derived from
//! `World.nomos` at the tick's start, right after the `SpatialIndex`, and
//! never stored or hashed.
//!
//! The index holds one flag byte per agent index of `World.agents`:
//! `ACCUSED` (some episode is open), `HARMABLE` / `REJECTABLE` (open under
//! a rule whose sanction set allows that answer) and `VOTED` (has voted on
//! the open motion; motions arrive with task 4-5). Decide reads it (from
//! 4-3 on, through `Observation`) and Apply reads it to tell a sanction
//! from an ordinary harm or rejection: what counts is who stood accused at
//! S(t), not what this tick's violations add. With no rule in force and no
//! motion open the index is empty, holds no per-agent bytes, and every
//! query answers "no" in constant time.
//!
//! Construction is one merge walk over `nomos.standing` (ascending by
//! agent id) and `agents` (ascending by id): O(agents + episodes), no hash
//! map, no binary search per episode.

use super::{Nomos, NormKind, Rule, Sanction, Via};
use crate::agent::Agent;

pub struct NormIndex {
    /// One flag byte per agent index; empty when nothing is in force.
    flags: Vec<u8>,
    /// The rule in force per kind (index = `NormKind::index`).
    rules: [Option<Rule>; NormKind::COUNT],
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
            rules: [None; NormKind::COUNT],
        }
    }

    /// Builds the view of `agents` (ascending by id) under `nomos`.
    pub fn build(agents: &[Agent], nomos: &Nomos) -> Self {
        let mut rules = [None; NormKind::COUNT];
        for r in &nomos.rules {
            rules[r.norm.kind.index()] = Some(*r);
        }
        if nomos.rules.is_empty() && nomos.motion.is_none() {
            return Self {
                flags: Vec::new(),
                rules,
            };
        }
        let mut flags = vec![0u8; agents.len()];
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
            flags[i] |= Self::ACCUSED
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
        Self { flags, rules }
    }

    /// Nothing in force: no rule, no open motion.
    pub fn is_empty(&self) -> bool {
        self.flags.is_empty()
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

    fn agent(id: u64) -> Agent {
        Agent {
            id: AgentId(id),
            pos: Pos::new(0, 0),
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
        let ix = NormIndex::build(&agents, &n);
        assert!(!ix.is_empty() && ix.any_rule());
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
    }

    #[test]
    fn nothing_in_force_means_an_empty_index() {
        let agents: Vec<Agent> = (0..5).map(agent).collect();
        let n = Nomos::default();
        let ix = NormIndex::build(&agents, &n);
        assert!(ix.is_empty() && !ix.any_rule());
        for i in 0..5 {
            assert_eq!(ix.flags(i), 0);
            assert!(!ix.accused(i) && !ix.harmable(i) && !ix.rejectable(i) && !ix.voted(i));
        }
        assert!(ix.rule(NormKind::Harm).is_none());
        let e = NormIndex::empty();
        assert!(e.is_empty());
        assert!(!e.accused(0));
    }
}
