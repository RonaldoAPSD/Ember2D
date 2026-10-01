// tests/rpg_demo.rs — Step 9-8 (docs/ember2d-master-plan.md §5.8): the RPG
// demo (`demos/rpg/`), driven headlessly through `Simulation` with real
// key presses: the title menu, walking the town, the elder's branching
// conversation, a battle fought to the end, and a save made from the
// pause menu that loads back mid-town.

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::save::SaveState;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::{BTreeMap, BTreeSet};

const RPG: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/rpg");

fn level(name: &str) -> LevelData {
    LevelData::load(&format!("{RPG}/{name}")).unwrap_or_else(|e| panic!("load {name}: {e}"))
}

struct Game {
    sim: Simulation,
    world: World,
    persistent: BTreeMap<String, rhai::Dynamic>,
    logs: Vec<LogEntry>,
    pending: Option<LevelData>,
}

impl Game {
    fn start(data: LevelData, persistent: BTreeMap<String, rhai::Dynamic>) -> Self {
        let mut sim = Simulation::new(data);
        sim.set_level_source(Box::new(FsLevelSource));
        sim.set_world_cell_scale((2.0, 1.0));
        let mut world = World::new();
        let mut persistent = persistent;
        let logs = sim.on_start(&mut world, 80, 24, &mut persistent);
        Game { sim, world, persistent, logs, pending: None }
    }

    /// One step with `keys` held, and pressed if `press`.
    fn step_keys(&mut self, keys: &[&str], press: bool) {
        let held: BTreeSet<String> = keys.iter().map(|s| s.to_string()).collect();
        let pressed = if press { held.clone() } else { BTreeSet::new() };
        let input = InputSnapshot { held, pressed };
        let mut out = self.sim.step(
            &mut self.world,
            StepInput {
                input: &input,
                mouse: MouseSnapshot::default(),
                gamepad: &GamepadSnapshot::default(),
                external_commands: &[],
                animating: &[],
                camera_origin: Vec2::ZERO,
                sim_dt: 1.0 / 60.0,
                elapsed: 0.0,
                viewport_w: 80,
                viewport_h: 24,
            },
            &mut self.persistent,
        );
        self.logs.append(&mut out.logs);
        if out.pending_level.is_some() {
            self.pending = out.pending_level;
        }
    }

    /// A key tapped: pressed for one step, then a few idle steps.
    fn tap(&mut self, key: &str) {
        self.step_keys(&[key], true);
        for _ in 0..3 {
            self.step_keys(&[], false);
        }
    }

    /// An arrow held long enough to walk `tiles` tiles.
    fn walk(&mut self, key: &str, tiles: usize) {
        // The first step is immediate, then one every 0.14 s (9 steps).
        for _ in 0..(tiles * 9).saturating_sub(4) {
            self.step_keys(&[key], false);
        }
        for _ in 0..10 {
            self.step_keys(&[], false);
        }
    }

    fn player_at(&self) -> (f32, f32) {
        let p = self.world.find_by_tag("player").expect("player");
        let t = self.world.get_global_position(p);
        (t.x, t.y)
    }

    fn int(&self, key: &str) -> i64 {
        self.persistent.get(key).and_then(|v| v.as_int().ok()).unwrap_or(i64::MIN)
    }

    fn no_problems(&self) {
        let bad: Vec<_> = self
            .logs
            .iter()
            .filter(|l| l.level != LogLevel::Info)
            .map(|l| l.text.clone())
            .collect();
        assert!(bad.is_empty(), "{bad:#?}");
    }
}

#[test]
fn every_rpg_level_starts_with_its_sprites_and_scripts_and_runs_quietly() {
    for name in ["title.level", "town.level", "inn.level", "field.level"] {
        let mut g = Game::start(level(name), BTreeMap::new());
        for _ in 0..30 {
            g.step_keys(&[], false);
        }
        g.no_problems();
        assert!(!g.world.tilemaps.is_empty(), "{name}: the scenery is a baked tilemap");
    }
}

#[test]
fn a_new_game_walks_to_the_elder_and_takes_the_quest() {
    let mut title = Game::start(level("title.level"), BTreeMap::new());
    title.step_keys(&[], false);
    title.tap("enter"); // New Game
    let town = title.pending.take().expect("New Game loads the town");
    assert_eq!(title.int("gold"), 20);
    let mut g = Game::start(town, title.persistent.clone());
    assert_eq!(g.player_at(), (24.0, 17.0));

    // Up the street to the plaza, then stand below the elder (27, 11).
    g.walk("right", 3);
    g.walk("up", 5);
    assert_eq!(g.player_at(), (27.0, 12.0));
    g.tap("space");
    for _ in 0..3 {
        g.tap("enter"); // read the elder's plea
    }
    // The branching choice: "Yes, we'll go" is the first item.
    assert_eq!(g.persistent.get("quest").map(|q| q.to_string()), Some("accepted".into()));
    assert_eq!(g.int("potions"), 4, "the elder's two potions");
    // The third Enter above closed the elder's thanks; the conversation is
    // over (one more Enter would start it again — Enter is also "talk").
    for _ in 0..5 {
        g.step_keys(&[], false);
    }
    assert!(!g.sim.globals().contains_key("talking"), "the conversation ended");
    g.no_problems();
}

#[test]
fn a_battle_is_fought_to_victory_and_pays_gold() {
    // The field, with a player script that starts a slime fight at once.
    let dir = common::test_temp_dir().join("rpg_battle");
    std::fs::create_dir_all(&dir).unwrap();
    let script = dir.join("fight.rhai");
    std::fs::write(
        &script,
        r#"fn on_update(id, ctx) {
            if !ctx.has_global("go") {
                ctx.set_global("go", true);
                ctx.push_scene("battle", #{ data: #{ enemy: "slime", boss: false } });
            }
        }"#,
    )
    .unwrap();
    let mut data = level("field.level");
    data.player.script = Some(script.to_string_lossy().into_owned());
    let persistent = party_persistent(5, 24);
    let mut g = Game::start(data, persistent);
    g.step_keys(&[], false);
    assert_eq!(g.sim.scene_names(), vec!["battle"], "{:#?}", g.logs);
    fight_until_over(&mut g);
    assert_eq!(g.int("gold"), 24, "a slime pays 4 gold");
    assert!(g.world.find_by_tag("battle").is_none(), "the battle's sprites are gone");
    g.no_problems();
}

/// The persistent state a new game starts with, but with Ash's attack and
/// health set by the test.
fn party_persistent(ash_atk: i64, ash_hp: i64) -> BTreeMap<String, rhai::Dynamic> {
    let mut persistent = BTreeMap::new();
    let party: rhai::Array = vec![
        rhai::Dynamic::from(rhai::Map::from([
            ("name".into(), rhai::Dynamic::from("Ash".to_string())),
            ("hp".into(), rhai::Dynamic::from(ash_hp)),
            ("max".into(), rhai::Dynamic::from(ash_hp)),
            ("atk".into(), rhai::Dynamic::from(ash_atk)),
            ("def".into(), rhai::Dynamic::from(1_i64)),
            ("mag".into(), rhai::Dynamic::from(0_i64)),
        ])),
        rhai::Dynamic::from(rhai::Map::from([
            ("name".into(), rhai::Dynamic::from("Lyra".to_string())),
            ("hp".into(), rhai::Dynamic::from(16_i64)),
            ("max".into(), rhai::Dynamic::from(16_i64)),
            ("atk".into(), rhai::Dynamic::from(2_i64)),
            ("def".into(), rhai::Dynamic::from(0_i64)),
            ("mag".into(), rhai::Dynamic::from(6_i64)),
        ])),
    ];
    persistent.insert("party".into(), rhai::Dynamic::from(party));
    persistent.insert("gold".into(), rhai::Dynamic::from(20_i64));
    persistent.insert("potions".into(), rhai::Dynamic::from(2_i64));
    persistent.insert("quest".into(), rhai::Dynamic::from("accepted".to_string()));
    persistent.insert("boss_done".into(), rhai::Dynamic::from(false));
    persistent
}

/// Enter picks each menu's first option (Attack) and turns every page.
fn fight_until_over(g: &mut Game) {
    for _ in 0..300 {
        if g.sim.scene_names().is_empty() {
            break;
        }
        g.tap("enter");
    }
    assert!(g.sim.scene_names().is_empty(), "the battle ended");
}

#[test]
fn beating_the_cyclops_clears_the_field_for_good() {
    // Walk up to the cyclops on the field and talk to it.
    let mut g = Game::start(level("field.level"), party_persistent(40, 99));
    g.step_keys(&[], false);
    let boss = g.world.find_by_tag("boss").expect("the cyclops is there");
    let p = g.world.find_by_tag("player").unwrap();
    g.world.transforms.get_mut(&p).unwrap().position = Vec2::new(46.0, 12.0);
    g.step_keys(&[], false);
    g.walk("right", 1); // blocked by the cyclops, but now facing it
    g.tap("space");
    g.tap("enter"); // its roar
    assert_eq!(g.sim.scene_names(), vec!["battle"], "{:#?}", g.logs);
    fight_until_over(&mut g);
    assert_eq!(g.persistent.get("boss_done").and_then(|v| v.as_bool().ok()), Some(true));
    assert!(!g.world.transforms.contains_key(&boss), "the cyclops is gone");
    assert_eq!(g.int("gold"), 80, "it carried 60 gold");

    // Entering the field again: it stays gone.
    let again = Game::start(level("field.level"), g.persistent.clone());
    assert!(
        again.world.find_by_tag("boss").is_none() || {
            let mut a = again;
            a.step_keys(&[], false);
            a.world.find_by_tag("boss").is_none()
        }
    );
    g.no_problems();
}

#[test]
fn a_save_from_the_pause_menu_loads_back_mid_town() {
    let dir = common::test_temp_dir().join("rpg_save");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // `save_game("emberfall.sav")` writes beside the working directory.
    std::env::set_current_dir(&dir).unwrap();

    let mut g = Game::start(level("town.level"), BTreeMap::new());
    g.step_keys(&[], false);
    g.walk("left", 2);
    let at = g.player_at();
    let mut log = Vec::new();
    g.sim.request_pause(&mut g.world, &mut log);
    g.step_keys(&[], false);
    g.step_keys(&[], false);
    g.tap("down");
    g.tap("down"); // Party, Items, [Save]
    g.tap("enter");
    assert!(g.sim.scene_names().is_empty(), "Save closes the menu");
    let save = SaveState::load_from_file("emberfall.sav").expect("the save was written");
    assert!(save.scenes.is_empty(), "the pause menu isn't in the save");
    assert_eq!(save.persistent.get("gold").and_then(|v| v.as_int().ok()), Some(20));

    let mut world = save.world;
    let mut persistent = save.persistent;
    let mut back = Simulation::from_save(
        level("town.level"),
        save.globals,
        save.clips,
        save.turn_number,
        save.scheduler,
    );
    back.set_level_source(Box::new(FsLevelSource));
    back.on_start(&mut world, 80, 24, &mut persistent);
    let p = world.find_by_tag("player").unwrap();
    let pos = world.get_global_position(p);
    assert_eq!((pos.x, pos.y), at, "the player is where the save was made");
    g.no_problems();
}
