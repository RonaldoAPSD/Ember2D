// tests/spawn_points.rs — Step 9-4 (docs/ember2d-master-plan.md §5.8):
// positional continuity and structured state. A level transition can name
// the spawn point the next level is entered at — `ctx.load_level(path,
// name)` from a script, or an exit tile whose target is `"path#name"` — and
// a party roster / inventory kept as nested maps and arrays in `persistent`
// survives a save file.

mod common;

use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::level_source::LevelSource;
use ember2d_sim::save::SaveState;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput, StepOutcome};
use std::collections::BTreeMap;

/// An in-memory `LevelSource` that knows one level, "town.level".
struct Town(LevelData);

impl LevelSource for Town {
    fn exists(&self, path: &str) -> bool {
        path == "town.level"
    }
    fn read_to_string(&self, _path: &str) -> Result<String, String> {
        Err("no file contents".to_string())
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        if path == "town.level" {
            Ok(self.0.clone())
        } else {
            Err(format!("no such level '{path}'"))
        }
    }
}

/// The town: the player normally starts at (2, 2); the inn's door is at
/// (15, 7).
fn town() -> LevelData {
    let mut data = LevelData::empty(20, 10);
    data.set_player_spawn((2.0, 2.0));
    data.add_spawn("inn_door", (15.0, 7.0));
    data
}

/// A level whose player script runs `body` every update.
fn scripted_level(tag: &str, body: &str) -> LevelData {
    let path = common::test_temp_dir().join(format!("spawn_points_{tag}.rhai"));
    std::fs::write(&path, format!("fn on_update(id, ctx) {{ {body} }}")).unwrap();
    let mut data = LevelData::empty(20, 10);
    data.player.script = Some(path.to_string_lossy().into_owned());
    data
}

fn step(sim: &mut Simulation, world: &mut World, persistent: &mut BTreeMap<String, rhai::Dynamic>) -> StepOutcome {
    sim.step(
        world,
        StepInput {
            input: &InputSnapshot::default(),
            mouse: MouseSnapshot::default(),
            gamepad: &GamepadSnapshot::default(),
            external_commands: &[],
            animating: &[],
            camera_origin: Vec2::ZERO,
            sim_dt: 1.0 / 60.0,
            elapsed: 0.0,
            viewport_w: 20,
            viewport_h: 10,
        },
        persistent,
    )
}

/// Starts `level` and returns where its player is, plus its start logs.
fn enter(level: LevelData) -> (Vec2, Vec<LogEntry>) {
    let mut sim = Simulation::new(level);
    let mut world = World::new();
    let logs = sim.on_start(&mut world, 20, 10, &mut BTreeMap::new());
    let player = world.find_by_tag("player").expect("player spawned");
    (world.get_global_position(player), logs)
}

/// Runs `level`'s script for one step and returns the level it asked for.
fn transition_from(level: LevelData) -> LevelData {
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(Town(town())));
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 20, 10, &mut persistent);
    let out = step(&mut sim, &mut world, &mut persistent);
    out.pending_level.expect("the transition loaded the town")
}

#[test]
fn load_level_with_a_spawn_name_enters_at_that_spawn() {
    let next = transition_from(scripted_level("named", r#"ctx.load_level("town.level", "inn_door");"#));
    assert_eq!(next.entry_spawn.as_deref(), Some("inn_door"));
    let (at, logs) = enter(next);
    assert_eq!(at, Vec2::new(15.0, 7.0));
    assert!(logs.iter().all(|l| l.level != LogLevel::Warning), "{logs:?}");
}

#[test]
fn load_level_without_a_spawn_name_enters_at_the_player_spawn() {
    let next = transition_from(scripted_level("plain", r#"ctx.load_level("town.level");"#));
    assert_eq!(next.entry_spawn, None);
    assert_eq!(enter(next).0, Vec2::new(2.0, 2.0));
}

#[test]
fn an_unknown_spawn_name_warns_and_falls_back_to_the_player_spawn() {
    let next = transition_from(scripted_level("missing", r#"ctx.load_level("town.level", "cellar");"#));
    let (at, logs) = enter(next);
    assert_eq!(at, Vec2::new(2.0, 2.0));
    assert!(
        logs.iter().any(|l| l.level == LogLevel::Warning && l.text.contains("cellar")),
        "{logs:?}"
    );
}

#[test]
fn an_exit_tile_can_name_the_spawn_it_leads_to() {
    // The player starts standing on an exit to "town.level#inn_door".
    let mut data = LevelData::empty(10, 10);
    let mut exit = TileRecord::new(5, 5, 1, '>', Color::Cyan, Color::Reset, false, true, "door");
    exit.next_level = Some("town.level#inn_door".to_string());
    data.tiles.push(exit);
    data.set_player_spawn((5.0, 5.0));

    let mut sim = Simulation::new(data);
    sim.set_level_source(Box::new(Town(town())));
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 10, 10, &mut persistent);
    let mut events = EventBus::new();
    world.detect_collisions(&mut events);
    let prev = world.snapshot_positions();
    let out =
        sim.late_step(&mut world, &events, &prev, Vec2::ZERO, 1.0 / 60.0, 0.0, 10, 10, &mut persistent);
    let next = out.pending_level.expect("the exit resolved \"town.level\" without its #suffix");
    assert_eq!(enter(next).0, Vec2::new(15.0, 7.0));
}

#[test]
fn get_spawn_point_sees_every_spawn_including_the_players() {
    let mut level = scripted_level(
        "lookup",
        r#"let d = ctx.get_spawn_point("inn_door"); let p = ctx.get_spawn_point("player");
           ctx.set_global("dx", d[0]); ctx.set_global("px", p[0]);"#,
    );
    level.set_player_spawn((3.0, 4.0));
    level.add_spawn("inn_door", (15.0, 7.0));
    let mut sim = Simulation::new(level);
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 20, 10, &mut persistent);
    step(&mut sim, &mut world, &mut persistent);
    let g = |k: &str| sim.globals().get(k).and_then(|d| d.as_float().ok());
    assert_eq!((g("dx"), g("px")), (Some(15.0), Some(3.0)));
}

#[test]
fn a_level_saved_with_spawns_round_trips_through_its_file() {
    let path = common::test_temp_dir().join("spawn_points_round_trip.level");
    let path = path.to_string_lossy().into_owned();
    town().save(&path).unwrap();
    let back = LevelData::load(&path).unwrap();
    assert_eq!(back.spawns, town().spawns);
    assert_eq!(back.version, ember2d_sim::level::LEVEL_FORMAT_VERSION);
}

#[test]
fn a_nested_party_roster_in_persistent_survives_a_save_file() {
    // A roster (array of maps, one holding an array) and an inventory (map
    // of ints) — the RPG demo's structured state, without a new type.
    let script = r#"
        if ctx.has_persistent("party") { return; }
        let party = [
            #{ name: "Ash", level: 5, hp: 21.5, moves: ["Tackle", "Ember"] },
            #{ name: "Misty", level: 4, hp: 18.0, moves: [] },
        ];
        ctx.set_persistent("party", party);
        ctx.set_persistent("bag", #{ potion: 3, rope: 1 });
    "#;
    let mut sim = Simulation::new(scripted_level("roster", script));
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 20, 10, &mut persistent);
    step(&mut sim, &mut world, &mut persistent);
    assert!(persistent.contains_key("party"), "the script stored its roster");

    let save = SaveState::new(world, persistent.clone(), BTreeMap::new(), BTreeMap::new(), "x.level".into(), 0, vec![]);
    let file = common::test_temp_dir().join("spawn_points_roster.sav").to_string_lossy().into_owned();
    save.save_to_file(&file).unwrap();
    let back = SaveState::load_from_file(&file).unwrap().persistent;

    let party = back["party"].clone().into_array().expect("still an array");
    assert_eq!(party.len(), 2);
    let ash = party[0].clone().cast::<rhai::Map>();
    assert_eq!(ash["name"].clone().into_string().unwrap(), "Ash");
    assert_eq!(ash["level"].as_int().unwrap(), 5);
    assert_eq!(ash["hp"].as_float().unwrap(), 21.5);
    let moves = ash["moves"].clone().into_array().unwrap();
    assert_eq!(moves.iter().map(|m| m.to_string()).collect::<Vec<_>>(), ["Tackle", "Ember"]);
    assert!(party[1].clone().cast::<rhai::Map>()["moves"].clone().into_array().unwrap().is_empty());
    let bag = back["bag"].clone().cast::<rhai::Map>();
    assert_eq!((bag["potion"].as_int().unwrap(), bag["rope"].as_int().unwrap()), (3, 1));
    // And it reads back exactly as written, byte for byte.
    assert_eq!(format!("{:?}", back), format!("{:?}", persistent));
}
