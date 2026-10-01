// tests/roguelike_level_integrity.rs — data-level invariants that must
// hold for every generated roguelike level (Step 4j, docs/HANDOFF.md /
// the Phase 4 plan file's "Tests to write now" list). These check
// LevelData directly, no PlayState/TurnHarness needed — pure data checks
// on what examples/gen_roguelike.rs produces.

use ember2d::prelude::*;
use ember2d_sim::level::LEVEL_FORMAT_VERSION;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

// `CARGO_MANIFEST_DIR`-relative, not CWD-relative — see
// tests/replay.rs's own comment on this (Step 5i's workspace split,
// docs/ember2d-phase5-plan.md).
const LEVELS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/floor1.level"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/floor2.level"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/floor3.level"),
    concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/victory.level"),
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
        assert_ne!(
            data.seed, 0,
            "{}: seed must be pinned by the generator, not the pre-seed-field default",
            path
        );
    }
}

#[test]
fn every_level_is_the_current_format_version_with_collision_layers() {
    // R8 (7A-4, docs/ember2d-master-plan.md): the shipped levels lagged
    // LEVEL_FORMAT_VERSION for a while (v2 on disk, v3 in code) with
    // nothing catching the drift — pins that a regeneration keeps them
    // matched going forward.
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        assert_eq!(
            data.version, LEVEL_FORMAT_VERSION,
            "{}: shipped level must be regenerated to the current format version",
            path
        );
        assert!(!data.collision_layers.is_empty(), "{}: collision_layers must not be empty", path);
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
        let original: Vec<(u8, i32, i32)> =
            data.tiles.iter().map(|t| (t.layer, t.y, t.x)).collect();
        let mut sorted = original.clone();
        sorted.sort();
        assert_eq!(original, sorted, "{}: tiles must already be sorted by (layer, y, x)", path);
    }
}

#[test]
fn every_script_and_next_level_path_a_level_references_exists_on_disk() {
    // Unlike every other check in this file, this one resolves paths
    // (`tile.script`/`next_level`) *stored inside* the level data itself,
    // authored repo-root-relative (e.g. "demos/roguelike/scripts/enemy.rhai")
    // — not just the `LEVELS` constants above, which are already
    // `CARGO_MANIFEST_DIR`-absolute. `cargo test` runs this binary with
    // CWD set to this package's own directory (`ember2d/`), one level
    // below the repo root (Step 5i's workspace split,
    // docs/ember2d-phase5-plan.md) — see tests/common/mod.rs's
    // `ensure_workspace_root_cwd` for the same fix applied to every other
    // test file via `TurnHarness::load`; this file has no `TurnHarness`
    // to funnel through, so it sets its own CWD directly.
    let _ = std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    // Step 9-6 (docs/ember2d-master-plan.md §5.8): paths are project-
    // relative now, so "exists" means "resolves the way the engine
    // resolves it" — beside the level, up to the project root, then CWD.
    let exists = |p: &str, level: &str| {
        let full = ember2d::play::resolve_exit_path(p, level, &|q| Path::new(q).exists());
        Path::new(&full).exists()
    };
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        if let Some(ref script) = data.player.script {
            assert!(
                exists(script, path),
                "{}: player script '{}' does not exist",
                path,
                script
            );
        }
        // Step 8-1: `all_tiles()`, not `tiles` — on a v4 level `tiles` is
        // only the interactive ones; the walls/floors are in `tilemap`.
        for tile in &data.all_tiles() {
            if let Some(ref script) = tile.script {
                assert!(
                    exists(script, path),
                    "{}: tile ({},{}) script '{}' does not exist",
                    path,
                    tile.x,
                    tile.y,
                    script
                );
            }
            if let Some(ref next) = tile.next_level {
                assert!(
                    exists(next, path),
                    "{}: tile ({},{}) next_level '{}' does not exist",
                    path,
                    tile.x,
                    tile.y,
                    next
                );
            }
        }
    }
}

#[test]
fn every_levels_spawn_point_is_not_inside_a_solid_tile() {
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let (sx, sy) = (data.player_spawn().0.round() as i32, data.player_spawn().1.round() as i32);
        // Step 8-1: every tile, baked tilemap cells included (see above).
        let blocked = data.all_tiles().iter().any(|t| t.x == sx && t.y == sy && t.solid);
        assert!(!blocked, "{}: spawn point ({},{}) must not be inside a solid tile", path, sx, sy);
    }
}

#[test]
fn no_cell_in_any_level_has_more_than_one_collider_bearing_tile() {
    // A tile only gets a real Collider in play/spawn.rs's do_on_start if
    // solid || trigger. At most one such tile may occupy a given (x,y)
    // across all layers, or get_entity_at/is_solid_at stop being
    // deterministic — a documented "Engine fact" this refactor's scripts
    // (enemy.rhai, player.rhai's bump-to-attack) depend on.
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let mut seen: HashMap<(i32, i32), u32> = HashMap::new();
        // Step 8-1: every tile, baked tilemap cells included.
        for t in &data.all_tiles() {
            if t.solid || t.trigger {
                *seen.entry((t.x, t.y)).or_insert(0) += 1;
            }
        }
        for (&(x, y), &count) in &seen {
            assert!(
                count <= 1,
                "{}: cell ({},{}) has {} collider-bearing tiles, must have at most 1",
                path,
                x,
                y,
                count
            );
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

        // Step 8-1: `all_tiles()` — on the baked (v4) levels every wall is
        // a tilemap cell, not an entry in `tiles`; flood-filling over
        // `tiles` alone would see no walls at all and pass vacuously.
        let all = data.all_tiles();
        let mut solid: HashSet<(i32, i32)> = HashSet::new();
        for t in &all {
            if t.solid && t.tag != "enemy" && t.tag != "boss" {
                solid.insert((t.x, t.y));
            }
        }

        let start = (data.player_spawn().0.round() as i32, data.player_spawn().1.round() as i32);
        let (w, h) = (data.width as i32, data.height as i32);
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        let mut queue = VecDeque::new();
        visited.insert(start);
        queue.push_back(start);
        while let Some((x, y)) = queue.pop_front() {
            for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if nx < 0 || ny < 0 || nx >= w || ny >= h {
                    continue;
                }
                if solid.contains(&(nx, ny)) {
                    continue;
                }
                if visited.insert((nx, ny)) {
                    queue.push_back((nx, ny));
                }
            }
        }

        assert!(solid.len() > 50, "{}: the flood fill must actually see the walls", path);
        for t in &all {
            if t.tag == "stairs" || t.tag == "enemy" || t.tag == "boss" {
                assert!(
                    visited.contains(&(t.x, t.y)),
                    "{}: {} at ({},{}) is not reachable from spawn {:?}",
                    path,
                    t.tag,
                    t.x,
                    t.y,
                    start
                );
            }
        }
    }
}

#[test]
fn every_shipped_level_is_baked_with_no_static_tile_left_as_an_entity() {
    // Step 8-1 (docs/ember2d-master-plan.md §5.7): the generators bake
    // before saving (`LevelData::bake_tilemap`), so a shipped level's
    // `tiles` must hold only interactive tiles and its `tilemap` every
    // static one — a static tile left in `tiles` would still collapse at
    // load (`split_static`), but would mean the file wasn't regenerated
    // through the current generator. And the baked grid must agree cell
    // for cell with the tile list it came from: every solid static tile is
    // a solid cell, nothing else is.
    for path in LEVELS {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        assert!(
            data.tiles.iter().all(|t| !t.is_static()),
            "{}: a static tile was left in `tiles` — regenerate the level",
            path
        );
        let mut map =
            data.tilemap.clone().unwrap_or_else(|| panic!("{}: must have a baked tilemap", path));
        map.refresh(&ember2d_sim::layers::LayerRegistry::new(&data.collision_layers));
        let solid_statics: HashSet<(i32, i32)> = data
            .all_tiles()
            .iter()
            .filter(|t| t.is_static() && t.solid)
            .map(|t| (t.x, t.y))
            .collect();
        for y in -1..=data.height as i32 {
            for x in -1..=data.width as i32 {
                assert_eq!(
                    map.solid_at(x, y, 0),
                    solid_statics.contains(&(x, y)),
                    "{}: cell ({},{}) solidity differs between the tilemap and its tiles",
                    path,
                    x,
                    y
                );
            }
        }
    }
}
