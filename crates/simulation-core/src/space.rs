//! Discrete 2D space: bounded integer grid, 4-neighbourhood (ADR-0003).

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pos {
    pub x: u16,
    pub y: u16,
}

impl Pos {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    pub fn manhattan(self, other: Pos) -> u32 {
        (self.x.abs_diff(other.x) as u32) + (self.y.abs_diff(other.y) as u32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Dir {
    North,
    East,
    South,
    West,
}

impl Dir {
    pub const ALL: [Dir; 4] = [Dir::North, Dir::East, Dir::South, Dir::West];

    pub fn index(self) -> u8 {
        self as u8
    }
}

/// Grid geometry. The world boundary is a wall: moving off-grid is
/// physically impossible (not a rule violation).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bounds {
    pub width: u16,
    pub height: u16,
}

impl Bounds {
    #[inline]
    pub fn contains(&self, p: Pos) -> bool {
        p.x < self.width && p.y < self.height
    }

    #[inline]
    pub fn index(&self, p: Pos) -> usize {
        debug_assert!(self.contains(p));
        p.y as usize * self.width as usize + p.x as usize
    }

    #[inline]
    pub fn cell_count(&self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn step(&self, p: Pos, d: Dir) -> Option<Pos> {
        let (x, y) = (p.x as i32, p.y as i32);
        let (nx, ny) = match d {
            Dir::North => (x, y - 1),
            Dir::East => (x + 1, y),
            Dir::South => (x, y + 1),
            Dir::West => (x - 1, y),
        };
        if nx < 0 || ny < 0 || nx >= self.width as i32 || ny >= self.height as i32 {
            None
        } else {
            Some(Pos::new(nx as u16, ny as u16))
        }
    }

    /// Cells within Manhattan distance `r` of `c`, clipped to the grid, in a
    /// fixed row-major order (deterministic iteration).
    pub fn diamond(&self, c: Pos, r: u16) -> impl Iterator<Item = Pos> + '_ {
        let r = r as i32;
        let (cx, cy) = (c.x as i32, c.y as i32);
        let (w, h) = (self.width as i32, self.height as i32);
        (-r..=r).flat_map(move |dy| {
            let span = r - dy.abs();
            (-span..=span).filter_map(move |dx| {
                let (x, y) = (cx + dx, cy + dy);
                (x >= 0 && y >= 0 && x < w && y < h).then(|| Pos::new(x as u16, y as u16))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_respects_walls() {
        let b = Bounds {
            width: 3,
            height: 3,
        };
        assert_eq!(b.step(Pos::new(0, 0), Dir::North), None);
        assert_eq!(b.step(Pos::new(0, 0), Dir::West), None);
        assert_eq!(b.step(Pos::new(2, 2), Dir::East), None);
        assert_eq!(b.step(Pos::new(1, 1), Dir::South), Some(Pos::new(1, 2)));
    }

    #[test]
    fn diamond_size() {
        let b = Bounds {
            width: 100,
            height: 100,
        };
        // 2r^2 + 2r + 1 cells away from edges.
        assert_eq!(b.diamond(Pos::new(50, 50), 4).count(), 41);
        // Corner is clipped.
        assert_eq!(b.diamond(Pos::new(0, 0), 1).count(), 3);
    }
}
