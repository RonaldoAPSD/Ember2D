// fov.rs — field of view and fog of war: which cells a viewer can see from
// where it stands, and which cells it has ever seen.
//
// Step 9.5-2 (docs/ember2d-master-plan.md §5.8.5), for the roguelike. A
// script calls `ctx.compute_fov(x, y, radius)` after the player moves; play
// mode then draws only what has been seen — what's in view as normal, what
// was seen before dimmed (remembered), the rest not at all — and hides
// monsters out of view (`World::fov_visibility`).
//
// ── THE ALGORITHM ────────────────────────────────────────────────────────────
//
// Symmetric shadowcasting (Albert Ford, 2021): each of the four quadrants
// around the viewer is scanned row by row outward, carrying the slopes of
// the still-unblocked wedge; a wall narrows the wedge or splits it into a
// recursion. Two properties a roguelike needs, which the cheaper ray-casting
// methods don't have:
//   - symmetric — if A can see B, B can see A, so a monster that sees you
//     is one you can see;
//   - no gaps — a wall's whole face is visible from the room it bounds,
//     and a pillar casts a clean shadow behind it.
//
// Determinism (CLAUDE.md): every slope is an exact fraction of two integers
// (`Slope`), compared by cross-multiplying — no floats, so two machines can't
// disagree about a cell on a shadow's edge. Opaque means "a solid tilemap
// cell", the same solidity `is_solid_at` reads; entities don't block sight
// (a monster doesn't hide the one behind it).

use serde::{Deserialize, Serialize};

use crate::components::Tilemap;

/// The largest radius `compute_fov` accepts. Far past any screen; a bound
/// so a script's `compute_fov(x, y, 1e9)` costs a full map, never more.
pub const MAX_FOV_RADIUS: i32 = 256;

/// What a viewer saw: `visible` (now) and `explored` (ever), one bit per
/// cell of the grid it was computed over — the level's tilemap, at the
/// time of the first `compute_fov`. Part of `World`, so a save keeps
/// what's been explored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FovMap {
    pub origin: (i32, i32),
    pub width: u32,
    pub height: u32,
    visible: Vec<u64>,
    explored: Vec<u64>,
}

/// How an entity is drawn while FOV is on and its cell isn't in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FovVisibility {
    /// Not drawn (a monster you can't see).
    Hide,
    /// Drawn dimmed once its cell has been seen (an item, the stairs).
    Remember,
    /// Always drawn normally (the player, a quest marker).
    Always,
}

impl FovVisibility {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hide" => Some(FovVisibility::Hide),
            "remember" => Some(FovVisibility::Remember),
            "always" => Some(FovVisibility::Always),
            _ => None,
        }
    }
}

impl FovMap {
    pub fn new(origin: (i32, i32), width: u32, height: u32) -> Self {
        let words = (width as usize * height as usize).div_ceil(64);
        FovMap { origin, width, height, visible: vec![0; words], explored: vec![0; words] }
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let lx = x as i64 - self.origin.0 as i64;
        let ly = y as i64 - self.origin.1 as i64;
        if lx < 0 || ly < 0 || lx >= self.width as i64 || ly >= self.height as i64 {
            return None;
        }
        Some((ly * self.width as i64 + lx) as usize)
    }

    fn bit(bits: &[u64], i: usize) -> bool {
        bits.get(i / 64).is_some_and(|w| w >> (i % 64) & 1 == 1)
    }

    /// Is (x, y) in view? `false` outside the grid.
    pub fn is_visible(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some_and(|i| Self::bit(&self.visible, i))
    }

    /// Has (x, y) ever been in view? `false` outside the grid.
    pub fn is_explored(&self, x: i32, y: i32) -> bool {
        self.index(x, y).is_some_and(|i| Self::bit(&self.explored, i))
    }

    fn mark(&mut self, x: i32, y: i32) {
        if let Some(i) = self.index(x, y) {
            self.visible[i / 64] |= 1 << (i % 64);
            self.explored[i / 64] |= 1 << (i % 64);
        }
    }

    /// Everything currently in view: from (ox, oy), up to `radius` cells
    /// away (a disc, not a square), walls blocking sight. Replaces the
    /// visible set; adds to the explored one.
    pub fn compute(&mut self, map: Option<&Tilemap>, ox: i32, oy: i32, radius: i32) {
        self.visible.iter_mut().for_each(|w| *w = 0);
        let radius = radius.clamp(0, MAX_FOV_RADIUS);
        self.mark(ox, oy);
        let opaque = |x: i32, y: i32| match map {
            Some(m) => m.index(x, y).is_none() || m.solid_at(x, y, 0),
            None => false,
        };
        for quadrant in 0..4 {
            let q = Quadrant { dir: quadrant, ox, oy };
            self.scan(&q, &opaque, radius, Row { depth: 1, start: Slope(-1, 1), end: Slope(1, 1) });
        }
    }

    fn scan(&mut self, q: &Quadrant, opaque: &dyn Fn(i32, i32) -> bool, radius: i32, mut row: Row) {
        if row.depth > radius {
            return;
        }
        let r2 = radius as i64 * radius as i64 + radius as i64; // a rounder disc
        let mut prev: Option<bool> = None; // was the previous tile a wall?
        for col in row.min_col()..=row.max_col() {
            let (x, y) = q.transform(row.depth, col);
            let wall = opaque(x, y);
            let in_disc = (row.depth as i64).pow(2) + (col as i64).pow(2) <= r2;
            if in_disc && (wall || row.is_symmetric(col)) {
                self.mark(x, y);
            }
            if prev == Some(true) && !wall {
                row.start = Slope::of(row.depth, col);
            }
            if prev == Some(false) && wall {
                let mut next = row.next();
                next.end = Slope::of(row.depth, col);
                self.scan(q, opaque, radius, next);
            }
            prev = Some(wall);
        }
        if prev == Some(false) {
            self.scan(q, opaque, radius, row.next());
        }
    }
}

/// An exact slope `num / den` (`den > 0`).
#[derive(Debug, Clone, Copy)]
struct Slope(i64, i64);

impl Slope {
    /// The slope through the near corner of a tile: `(2col - 1) / 2depth`.
    fn of(depth: i32, col: i32) -> Self {
        Slope(2 * col as i64 - 1, 2 * depth as i64)
    }
}

struct Row {
    depth: i32,
    start: Slope,
    end: Slope,
}

impl Row {
    /// `round_ties_up(depth * start)` = floor(depth*num/den + 1/2).
    fn min_col(&self) -> i32 {
        let (n, d) = (self.depth as i64 * self.start.0, self.start.1);
        (2 * n + d).div_euclid(2 * d) as i32
    }
    /// `round_ties_down(depth * end)` = ceil(depth*num/den - 1/2).
    fn max_col(&self) -> i32 {
        let (n, d) = (self.depth as i64 * self.end.0, self.end.1);
        -((-(2 * n - d)).div_euclid(2 * d)) as i32
    }
    /// Is the tile's centre inside the wedge? (`col >= depth*start` and
    /// `col <= depth*end`, cross-multiplied.)
    fn is_symmetric(&self, col: i32) -> bool {
        let c = col as i64;
        let dep = self.depth as i64;
        c * self.start.1 >= dep * self.start.0 && c * self.end.1 <= dep * self.end.0
    }
    fn next(&self) -> Row {
        Row { depth: self.depth + 1, start: self.start, end: self.end }
    }
}

/// One of the four quadrants around the viewer: maps a (row depth, column)
/// in the quadrant's own frame to a world cell.
struct Quadrant {
    dir: u8,
    ox: i32,
    oy: i32,
}

impl Quadrant {
    fn transform(&self, depth: i32, col: i32) -> (i32, i32) {
        match self.dir {
            0 => (self.ox + col, self.oy - depth), // north
            1 => (self.ox + depth, self.oy + col), // east
            2 => (self.ox + col, self.oy + depth), // south
            _ => (self.ox - depth, self.oy + col), // west
        }
    }
}

#[cfg(test)]
#[path = "fov_tests.rs"]
mod tests;
