// fov_tests.rs — Step 9.5-2: the shadowcasting in `fov.rs` against known
// shapes (an open room's disc, a pillar's shadow, a wall, a corridor), its
// symmetry over random maps, and how `explored` accumulates.

use super::*;
use crate::color::Color;
use crate::components::{TileDef, Tilemap};
use crate::layers::LayerRegistry;

fn wall() -> TileDef {
    TileDef {
        glyph: '#',
        fg: Color::Grey,
        bg: Color::Reset,
        solid: true,
        tag: String::new(),
        collider_layer: String::new(),
        texture: None,
        sprite: None,
        src: None,
        name: String::new(),
    }
}

/// A `w`×`h` map with a wall wherever `is_wall(x, y)`.
fn map(w: u32, h: u32, is_wall: impl Fn(i32, i32) -> bool) -> Tilemap {
    let mut m = Tilemap::new((0, 0), w, h);
    let c = m.intern(&wall()).unwrap();
    for y in 0..h as i32 {
        for x in 0..w as i32 {
            if is_wall(x, y) {
                m.set_cell(1, x, y, c);
            }
        }
    }
    m.refresh(&LayerRegistry::new(&[]));
    m
}

fn fov_from(m: &Tilemap, x: i32, y: i32, r: i32) -> FovMap {
    let mut f = FovMap::new(m.origin, m.width, m.height);
    f.compute(Some(m), x, y, r);
    f
}

#[test]
fn an_open_room_sees_a_disc_of_the_radius() {
    let m = map(11, 11, |_, _| false);
    let f = fov_from(&m, 5, 5, 3);
    for (x, y) in [(5, 5), (5, 2), (8, 5), (2, 5), (5, 8), (7, 7), (3, 3)] {
        assert!(f.is_visible(x, y), "({x},{y}) is within the radius");
    }
    // (3,2) from the centre: 9 + 4 = 13 > 3² + 3 — just outside the disc.
    for (x, y) in [(8, 7), (2, 3), (5, 1), (9, 5), (8, 8)] {
        assert!(!f.is_visible(x, y), "({x},{y}) is beyond the radius");
    }
    assert!(!f.is_visible(-1, 5) && !f.is_explored(50, 50), "outside the grid: never");
}

#[test]
fn a_pillar_casts_a_shadow_and_is_itself_seen() {
    let m = map(14, 7, |x, y| (x, y) == (6, 3));
    let f = fov_from(&m, 3, 3, 12);
    assert!(f.is_visible(6, 3), "the pillar's own face");
    for x in 7..14 {
        assert!(!f.is_visible(x, 3), "({x},3) is straight behind the pillar");
    }
    assert!(
        f.is_visible(3, 0) && f.is_visible(10, 0) && f.is_visible(10, 6),
        "off-axis cells aren't"
    );
}

#[test]
fn a_wall_hides_the_room_behind_it_but_not_its_own_face() {
    let m = map(12, 7, |x, _| x == 6);
    let f = fov_from(&m, 3, 3, 12);
    for y in 0..7 {
        assert!(f.is_visible(6, y), "the whole wall face (6,{y}) is seen — no gaps");
        assert!(!f.is_visible(8, y), "(8,{y}) is behind it");
    }
}

#[test]
fn a_corridor_is_seen_end_to_end_and_its_walls_but_nothing_past_them() {
    let m = map(14, 7, |x, y| y != 3 || x == 0 || x == 13);
    let f = fov_from(&m, 1, 3, 20);
    for x in 1..13 {
        assert!(f.is_visible(x, 3), "corridor cell ({x},3)");
    }
    assert!(f.is_visible(5, 2) && f.is_visible(5, 4), "its walls");
    assert!(!f.is_visible(5, 1) && !f.is_visible(5, 5), "nothing behind them");
    assert!(f.is_visible(13, 3), "the end wall");
}

/// A tiny deterministic generator — the test needs varied maps, not good
/// randomness, and must not depend on a crate's RNG staying the same.
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

#[test]
fn sight_is_symmetric_on_random_maps() {
    // If A can see B, B can see A — for every pair of floor cells, on maps
    // a third walls. Ford's algorithm guarantees it; this pins it.
    for seed in [1u64, 7, 42, 1234] {
        let mut rng = Lcg(seed);
        let (w, h) = (16, 12);
        let walls: Vec<bool> = (0..w * h).map(|_| rng.next().is_multiple_of(3)).collect();
        let m = map(w as u32, h as u32, |x, y| walls[(y * w + x) as usize]);
        let floors: Vec<(i32, i32)> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|&(x, y)| !walls[(y * w + x) as usize])
            .collect();
        let views: Vec<FovMap> = floors.iter().map(|&(x, y)| fov_from(&m, x, y, 8)).collect();
        for (i, a) in floors.iter().enumerate() {
            for (j, b) in floors.iter().enumerate() {
                assert_eq!(
                    views[i].is_visible(b.0, b.1),
                    views[j].is_visible(a.0, a.1),
                    "seed {seed}: {a:?} and {b:?} disagree about seeing each other"
                );
            }
        }
    }
}

#[test]
fn explored_accumulates_and_visible_is_replaced() {
    // Two rooms split by a wall with no door: look from each in turn.
    let m = map(13, 5, |x, _| x == 6);
    let mut f = FovMap::new(m.origin, m.width, m.height);
    f.compute(Some(&m), 2, 2, 10);
    assert!(f.is_visible(2, 2) && !f.is_explored(10, 2));
    f.compute(Some(&m), 10, 2, 10);
    assert!(f.is_visible(10, 2) && !f.is_visible(2, 2), "the first room is out of view now");
    assert!(f.is_explored(2, 2) && f.is_explored(10, 2), "but both have been seen");
}

#[test]
fn a_radius_of_zero_sees_only_the_viewers_cell_and_a_huge_one_is_capped() {
    let m = map(9, 9, |_, _| false);
    let f = fov_from(&m, 4, 4, 0);
    assert!(f.is_visible(4, 4) && !f.is_visible(4, 3));
    let f = fov_from(&m, 4, 4, i32::MAX);
    assert!(
        f.is_visible(0, 0) && f.is_visible(8, 8),
        "capped at MAX_FOV_RADIUS, not looping forever"
    );
}
