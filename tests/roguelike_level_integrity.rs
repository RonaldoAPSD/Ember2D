// tests/roguelike_level_integrity.rs — data-level invariants that must
// hold for every generated roguelike level (Step 4j, docs/HANDOFF.md /
// the Phase 4 plan file's "Tests to write now" list). These check
// LevelData directly, no PlayState/TurnHarness needed — pure data checks
// on what examples/gen_roguelike.rs produces.

use ember2d::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

const LEVELS: &[&str] = &[
    "roguelike/floor1.level",
    "roguelike/floor2.level",
    "roguelike/floor3.level",
    "roguelike/victory.level",
];

#[test]
fn every_level_has_a_pinned_nonzero_seed() {
    // LevelGrid::new (the editor's own constructor) picks a fresh seed from
    // OS entropy — a generator that forgot to override it would produce a
    // level whose seed is whatever rand::random() happened to return at
    // generation time, not necessarily zero. What actually matters is that
    // every regeneration produces the SAME seed (see the next test); "not
    // the sentinel a pre-Phase-3 level would load as" is what's checked
    // here — see LevelData::seed's own doc comment.
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        assert_ne!(data.seed, 0, "{}: seed must be pinned by the generator, not the pre-seed-field default", path);
    }
}

#[test]
fn every_levels_tiles_are_sorted_by_layer_then_y_then_x() {
    // examples/gen_roguelike.rs::build_level sorts explicitly because
    // LevelData.tiles is a Vec but the generator assembles it by iterating
    // a 2D array plus a features Vec — two runs would otherwise produce
    // different orders, and therefore different entity-id assignment in
    // play/spawn.rs::do_on_start (ids are handed out in tile order).
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let original: Vec<(u8, i32, i32)> = data.tiles.iter().map(|t| (t.layer, t.y, t.x)).collect();
        let mut sorted = original.clone();
        sorted.sort();
        assert_eq!(original, sorted, "{}: tiles must already be sorted by (layer, y, x)", path);
    }
}

#[test]
fn every_script_and_next_level_path_a_level_references_exists_on_disk() {
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        if let Some(ref script) = data.player.script {
            assert!(Path::new(script).exists(), "{}: player script '{}' does not exist", path, script);
        }
        for tile in &data.tiles {
            if let Some(ref script) = tile.script {
                assert!(Path::new(script).exists(), "{}: tile ({},{}) script '{}' does not exist", path, tile.x, tile.y, script);
            }
            if let Some(ref next) = tile.next_level {
                assert!(Path::new(next).exists(), "{}: tile ({},{}) next_level '{}' does not exist", path, tile.x, tile.y, next);
            }
        }
    }
}

#[test]
fn every_levels_spawn_point_is_not_inside_a_solid_tile() {
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let (sx, sy) = (data.spawn_point.0.round() as i32, data.spawn_point.1.round() as i32);
        let blocked = data.tiles.iter().any(|t| t.x == sx && t.y == sy && t.solid);
        assert!(!blocked, "{}: spawn point ({},{}) must not be inside a solid tile", path, sx, sy);
    }
}

#[test]
fn no_cell_in_any_level_has_more_than_one_collider_bearing_tile() {
    // A tile only gets a real Collider in play/spawn.rs's do_on_start if
    // solid || trigger. At most one such tile may occupy a given (x,y)
    // across all layers, or get_entity_at/is_solid_at stop being
    // deterministic — a documented "Engine fact" this refactor's scripts
    // (enemy_rat.rhai, player.rhai's bump-to-attack) depend on.
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let mut seen: HashMap<(i32, i32), u32> = HashMap::new();
        for t in &data.tiles {
            if t.solid || t.trigger {
                *seen.entry((t.x, t.y)).or_insert(0) += 1;
            }
        }
        for (&(x, y), &count) in &seen {
            assert!(count <= 1, "{}: cell ({},{}) has {} collider-bearing tiles, must have at most 1", path, x, y, count);
        }
    }
}

#[test]
fn every_level_is_fully_walkable_from_spawn_to_the_stairs_and_every_enemy() {
    // Flood-fill from spawn through non-solid TERRAIN; every stairs/enemy/
    // boss tile must be reachable, or the level is broken by construction
    // (a missing corridor tile, a monster sealed behind a wall, etc).
    //
    // Enemies/bosses are deliberately excluded from the blocking set even
    // though rat()/boss() mark them solid=true (that's a gameplay fact —
    // the player can't walk through a live one, must attack instead — see
    // player.rhai's bump-to-attack path). For a STATIC architecture check
    // they're killable/movable obstacles, not walls: a rat sitting on its
    // own only entrance would make this flood-fill correctly call that
    // rat itself unreachable, which is a false positive, not a level bug.
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));

        let mut solid: HashSet<(i32, i32)> = HashSet::new();
        for t in &data.tiles {
            if t.solid && t.tag != "enemy" && t.tag != "boss" { solid.insert((t.x, t.y)); }
        }

        let start = (data.spawn_point.0.round() as i32, data.spawn_point.1.round() as i32);
        let (w, h) = (data.width as i32, data.height as i32);
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        let mut queue = VecDeque::new();
        visited.insert(start);
        queue.push_back(start);
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if nx < 0 || ny < 0 || nx >= w || ny >= h { continue; }
                if solid.contains(&(nx, ny)) { continue; }
                if visited.insert((nx, ny)) { queue.push_back((nx, ny)); }
            }
        }

        for t in &data.tiles {
            if t.tag == "stairs" || t.tag == "enemy" || t.tag == "boss" {
                assert!(visited.contains(&(t.x, t.y)), "{}: {} at ({},{}) is not reachable from spawn {:?}", path, t.tag, t.x, t.y, start);
            }
        }
    }
}
