// simulation/tilemap_spawn_tests.rs — Step 8-1 (docs/ember2d-master-
// plan.md §5.7): `do_on_start`'s tilemap spawning, R93 (exits keyed by the
// exit entity's real id on `World.exits`), and a tilemap's save/load round
// trip, all through the real `Simulation::on_start` entry point. A sibling
// of spawn_tests.rs for the same 750-line reason every `*_tests.rs` here is.

use super::*;
use crate::color::Color;
use crate::event::EventBus;
use crate::level::{LevelData, TileRecord};
use crate::level_source::LevelSource;
use crate::save::SaveState;
use std::collections::BTreeMap as Map;

/// Knows no files at all; `load_level` fails with a recognizable message —
/// enough to prove an exit lookup HIT (the failure is logged as "Exit
/// failed") without needing a second real level.
struct NoFiles;

impl LevelSource for NoFiles {
    fn exists(&self, _path: &str) -> bool {
        false
    }
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        Err(format!("not found: {path}"))
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        Err(format!("NoFiles: {path}"))
    }
}

/// A row of five walls, then (last in tile order) a stairs tile the player
/// spawns on top of. Pre-8-1 the stairs was entity 6 (tile index + 1);
/// with the walls collapsed into a tilemap it isn't — the R93 trap.
fn walls_then_stairs() -> LevelData {
    let mut level = LevelData::empty(10, 10);
    for x in 0..5 {
        level.tiles.push(TileRecord::new(
            x,
            0,
            1,
            '#',
            Color::Grey,
            Color::Reset,
            true,
            false,
            "wall",
        ));
    }
    let mut stairs =
        TileRecord::new(4, 4, 1, '>', Color::Yellow, Color::Reset, false, true, "stairs");
    stairs.next_level = Some("next.level".to_string());
    level.tiles.push(stairs);
    level.set_player_spawn((4.0, 4.0));
    level
}

fn start(level: LevelData) -> (Simulation, World) {
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(NoFiles));
    let mut world = World::new();
    let mut persistent = Map::new();
    sim.on_start(&mut world, 10, 10, &mut persistent);
    (sim, world)
}

/// One detect-collisions → `late_step` pass; returns its log text.
fn late_step_logs(sim: &mut Simulation, world: &mut World) -> String {
    let mut events = EventBus::new();
    world.detect_collisions(&mut events);
    let prev = world.snapshot_positions();
    let mut persistent = Map::new();
    let out =
        sim.late_step(world, &events, &prev, Vec2::ZERO, 1.0 / 60.0, 0.0, 10, 10, &mut persistent);
    out.logs.iter().map(|l| l.text.clone()).collect::<Vec<_>>().join("\n")
}

#[test]
fn the_tilemap_entity_spawns_first_and_holds_every_static_tile() {
    let (_, world) = start(walls_then_stairs());
    assert_eq!(world.tilemaps.len(), 1);
    let (&map_id, map) = world.tilemaps.iter().next().unwrap();
    assert_eq!(map_id, 1, "the tilemap is spawned before any entity tile");
    assert_eq!(map.tile_count(), 5, "all five walls, and only them");
    assert!(
        world.transforms.contains_key(&map_id),
        "entity_exists/get_x need it to have a position"
    );
    assert_eq!(world.find_by_tag("wall"), None, "no wall is its own entity any more");
}

#[test]
fn r93_an_exit_after_collapsed_walls_is_keyed_by_its_real_entity_id() {
    let (mut sim, mut world) = start(walls_then_stairs());
    let stairs = world.find_by_tag("stairs").expect("the stairs stay an entity");
    assert_ne!(stairs, 6, "the old tile-index+1 assumption would point at the wrong id here");
    assert_eq!(world.exits.get(&stairs).map(String::as_str), Some("next.level"));
    assert_eq!(world.exits.len(), 1);

    let logs = late_step_logs(&mut sim, &mut world);
    assert!(
        logs.contains("Exit failed"),
        "standing on the stairs must reach the exit lookup (the level load itself fails here by design), got: {logs}"
    );
}

#[test]
fn r93_a_pre_8_1_save_without_exits_gets_them_rebuilt_from_tile_order() {
    // Build the pre-8-1 world shape: every tile an entity, ids in tile
    // order. `camera_follow` forces each wall to stay an entity (see
    // `TileRecord::is_static`) without changing anything else here.
    let mut level = walls_then_stairs();
    for t in &mut level.tiles {
        if t.is_static() {
            t.camera_follow = true;
        }
    }
    let (_, mut world) = start(level.clone());
    assert!(world.tilemaps.is_empty());
    let expected = world.exits.clone();
    assert_eq!(expected.keys().copied().collect::<Vec<_>>(), vec![6], "tile index 5 + 1");

    // A pre-8-1 save never had `World.exits` at all.
    world.exits.clear();
    let mut sim = Simulation::from_save(level, Map::new(), Map::new(), 0, Vec::new());
    sim.set_level_source(Box::new(NoFiles));
    let mut persistent = Map::new();
    sim.on_start(&mut world, 10, 10, &mut persistent);
    assert_eq!(world.exits, expected, "the legacy rebuild must recover the exact same exit ids");
    assert!(late_step_logs(&mut sim, &mut world).contains("Exit failed"));
}

#[test]
fn a_saved_tilemap_is_solid_again_after_a_ron_round_trip_and_load() {
    let level = walls_then_stairs();
    let (_, world) = start(level.clone());
    let ron =
        SaveState::new(world, Map::new(), Map::new(), Map::new(), String::new(), 0, Vec::new())
            .to_ron()
            .expect("serialize");
    let state = SaveState::from_ron(&ron).expect("deserialize");
    let mut world = state.world;
    let raw = world.tilemaps.values().next().expect("the tilemap must be part of the save").clone();
    assert!(!raw.solid_at(0, 0, 0), "straight out of RON the caches are empty (#[serde(skip)])");

    let mut sim = Simulation::from_save(level, Map::new(), Map::new(), 0, Vec::new());
    sim.set_level_source(Box::new(NoFiles));
    let mut persistent = Map::new();
    sim.on_start(&mut world, 10, 10, &mut persistent);
    let map = world.tilemaps.values().next().unwrap();
    assert!(
        (0..5).all(|x| map.solid_at(x, 0, 0)),
        "loading must refresh the caches — every wall solid again"
    );
    assert_eq!(world.exits.len(), 1, "exits travel inside the save now");
}
