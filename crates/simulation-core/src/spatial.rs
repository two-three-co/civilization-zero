//! Uniform-grid spatial index over the agents of one tick (ADR-0013 D2).
//!
//! Built at the start of a tick from `World.agents` (which is in id order)
//! by a stable counting sort, so the agents of a cell stay in id order and
//! every query is deterministic and independent of thread count. It is
//! derived state: never serialised, never hashed, rebuilt per tick in O(n +
//! cells).

use crate::agent::Agent;
use crate::space::{Bounds, Pos};

pub struct SpatialIndex {
    bounds: Bounds,
    /// `ends[c]` = one past the last entry of cell `c` in `order`; cell `c`
    /// starts at `ends[c - 1]` (or 0).
    ends: Vec<u32>,
    /// Indices into the agents slice, grouped by cell, ascending within a cell.
    order: Vec<u32>,
}

impl SpatialIndex {
    pub fn build(bounds: Bounds, agents: &[Agent]) -> Self {
        let cells = bounds.cell_count();
        let mut ends = vec![0u32; cells];
        for a in agents {
            ends[bounds.index(a.pos)] += 1;
        }
        // Exclusive prefix sums: ends[c] becomes the start of cell c …
        let mut acc = 0u32;
        for e in ends.iter_mut() {
            let count = *e;
            *e = acc;
            acc += count;
        }
        // … and the fill pass advances it to the end of cell c. Iterating
        // agents in slice order keeps each cell's entries ascending.
        let mut order = vec![0u32; agents.len()];
        for (i, a) in agents.iter().enumerate() {
            let c = bounds.index(a.pos);
            order[ends[c] as usize] = i as u32;
            ends[c] += 1;
        }
        Self {
            bounds,
            ends,
            order,
        }
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Indices (into the agents slice the index was built from) of the
    /// agents standing on `p`, ascending.
    pub fn cell(&self, p: Pos) -> &[u32] {
        let c = self.bounds.index(p);
        let start = if c == 0 { 0 } else { self.ends[c - 1] as usize };
        &self.order[start..self.ends[c] as usize]
    }

    /// How many agents stand within Manhattan `radius` of `center`, and how
    /// many of those within distance 1 (both include whoever stands on
    /// `center` itself). The cells of one diamond row are contiguous, so
    /// each row costs one difference of the index's prefix sums: no walk
    /// over the agents, no allocation (the reflex policy's crowd features).
    pub fn crowd(&self, center: Pos, radius: u16) -> (u32, u32) {
        let (w, h) = (self.bounds.width as i32, self.bounds.height as i32);
        let r = radius as i32;
        let (cx, cy) = (center.x as i32, center.y as i32);
        let (mut near, mut adjacent) = (0u32, 0u32);
        for dy in -r..=r {
            let y = cy + dy;
            if y < 0 || y >= h {
                continue;
            }
            let row = y as usize * w as usize;
            let span = r - dy.abs();
            let (x0, x1) = ((cx - span).max(0), (cx + span).min(w - 1));
            if x0 <= x1 {
                near += self.count_cells(row + x0 as usize, row + x1 as usize);
            }
            // Of this row, the cells within distance 1 that the radius reaches.
            let s = (1 - dy.abs()).min(span);
            if s >= 0 {
                let (a0, a1) = ((cx - s).max(0), (cx + s).min(w - 1));
                if a0 <= a1 {
                    adjacent += self.count_cells(row + a0 as usize, row + a1 as usize);
                }
            }
        }
        (near, adjacent)
    }

    /// Agents in cells `c0..=c1` (contiguous cell indices).
    fn count_cells(&self, c0: usize, c1: usize) -> u32 {
        let before = if c0 == 0 { 0 } else { self.ends[c0 - 1] };
        self.ends[c1] - before
    }

    /// Everyone within Manhattan `radius` of `center` (including `center`'s
    /// own cell), as `(distance, agent index)` sorted by distance then index.
    /// Because the agents slice is in id order, index order is id order.
    pub fn within(&self, center: Pos, radius: u16) -> Vec<(u32, usize)> {
        let mut out = Vec::new();
        for p in self.bounds.diamond(center, radius) {
            let d = center.manhattan(p);
            out.extend(self.cell(p).iter().map(|&i| (d, i as usize)));
        }
        out.sort_unstable();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{AgentId, Memory};
    use crate::policy::{PolicyKind, PolicyParams};

    fn agent(id: u64, x: u16, y: u16) -> Agent {
        Agent {
            id: AgentId(id),
            pos: Pos::new(x, y),
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
        }
    }

    #[test]
    fn cells_hold_their_agents_in_id_order() {
        let b = Bounds {
            width: 4,
            height: 3,
        };
        let agents = [
            agent(0, 1, 1),
            agent(1, 3, 2),
            agent(2, 1, 1),
            agent(3, 0, 0),
            agent(4, 1, 1),
        ];
        let ix = SpatialIndex::build(b, &agents);
        assert_eq!(ix.len(), 5);
        assert_eq!(ix.cell(Pos::new(1, 1)), &[0, 2, 4]);
        assert_eq!(ix.cell(Pos::new(0, 0)), &[3]);
        assert_eq!(ix.cell(Pos::new(3, 2)), &[1]);
        assert_eq!(ix.cell(Pos::new(2, 2)), &[] as &[u32]);
        assert_eq!(
            ix.within(Pos::new(1, 1), 1),
            vec![(0, 0), (0, 2), (0, 4)],
            "radius 1 reaches no other occupied cell"
        );
        assert_eq!(
            ix.within(Pos::new(1, 1), 2),
            vec![(0, 0), (0, 2), (0, 4), (2, 3)]
        );
        assert_eq!(
            ix.within(Pos::new(3, 2), 3),
            vec![(0, 1), (3, 0), (3, 2), (3, 4)]
        );
    }

    #[test]
    fn empty_world_has_empty_cells() {
        let b = Bounds {
            width: 2,
            height: 2,
        };
        let ix = SpatialIndex::build(b, &[]);
        assert!(ix.is_empty());
        assert!(ix.cell(Pos::new(1, 1)).is_empty());
        assert!(ix.within(Pos::new(0, 0), 5).is_empty());
    }

    #[test]
    fn crowd_counts_agree_with_within_everywhere() {
        let bounds = Bounds {
            width: 6,
            height: 5,
        };
        let agents: Vec<Agent> = [
            (0, 0, 0),
            (1, 0, 0),
            (2, 1, 0),
            (3, 2, 2),
            (4, 2, 2),
            (5, 2, 3),
            (6, 5, 4),
            (7, 3, 1),
            (8, 5, 0),
        ]
        .iter()
        .map(|&(id, x, y)| agent(id, x, y))
        .collect();
        let index = SpatialIndex::build(bounds, &agents);
        for y in 0..bounds.height {
            for x in 0..bounds.width {
                for r in 0..=4u16 {
                    let c = Pos::new(x, y);
                    let w = index.within(c, r);
                    let expected = (
                        w.len() as u32,
                        w.iter().filter(|(d, _)| *d <= 1).count() as u32,
                    );
                    assert_eq!(index.crowd(c, r), expected, "centre {c:?} radius {r}");
                }
            }
        }
    }
}
