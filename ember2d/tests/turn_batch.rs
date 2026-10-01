// tests/turn_batch.rs — Step 9.5-3 (docs/ember2d-master-plan.md §5.8.5):
// `make_actor` (a script-spawned entity joins the turn order) and the
// project's `ai_turns_per_step` (every AI actor answers the player's move
// in the same step, instead of one per step).

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::scripting::LogLevel;
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::{BTreeMap, BTreeSet};

/// The player spawns five monsters in `on_start`, makes each an actor, and
/// waits a turn on Space. Every monster counts its own turns in a global.
const PLAYER: &str = r#"
fn on_start(id, ctx) {
    for i in 0..5 {
        let m = ctx.spawn_entity("m", 2.0 + i, 5.0, "monster");
        ctx.set_script(m, "mon.rhai");
        ctx.make_actor(m, 100);
    }
}
fn on_input(id, ctx) { if ctx.just_pressed("space") { ctx.submit(id, "wait", []); } }
fn on_turn(id, ctx) { if ctx.command_action() == "wait" { ctx.act(100.0); } }
"#;
const MONSTER: &str = r#"fn on_turn(id, ctx) { let _n = ctx.add_global("acted", 1); }"#;

struct H {
    sim: Simulation,
    world: World,
    persistent: BTreeMap<String, rhai::Dynamic>,
    log_problems: usize,
}

fn project(tag: &str, ai_turns: u32) -> H {
    let dir = common::test_temp_dir().join(format!("turn_batch_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("player.rhai"), PLAYER).unwrap();
    std::fs::write(dir.join("mon.rhai"), MONSTER).unwrap();
    let mut data = LevelData::empty(20, 10);
    data.tiles.clear();
    data.path = dir.join("level.level").to_string_lossy().into_owned();
    data.player.script = Some("player.rhai".to_string());
    let mut sim = Simulation::new(data);
    sim.set_level_source(Box::new(FsLevelSource));
    sim.set_ai_turns_per_step(ai_turns);
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let logs = sim.on_start(&mut world, 40, 20, &mut persistent);
    let log_problems = logs.iter().filter(|l| l.level != LogLevel::Info).count();
    H { sim, world, persistent, log_problems }
}

impl H {
    /// One step, Space pressed or not; whether it resolved any turn.
    fn step(&mut self, space: bool) -> bool {
        let keys: BTreeSet<String> =
            if space { ["space".to_string()].into() } else { BTreeSet::new() };
        let input = InputSnapshot { held: keys.clone(), pressed: keys };
        let out = self.sim.step(
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
                viewport_w: 40,
                viewport_h: 20,
            },
            &mut self.persistent,
        );
        self.log_problems += out.logs.iter().filter(|l| l.level != LogLevel::Info).count();
        out.turn_triggered
    }
    fn acted(&self) -> i64 {
        self.sim
            .globals()
            .get("acted")
            .map(|v| v.as_float().map(|f| f as i64).unwrap_or(0))
            .unwrap_or(0)
    }
    /// Steps until the player is due again (no turn resolves without input).
    fn settle(&mut self) -> usize {
        let mut n = 0;
        while self.step(false) {
            n += 1;
            assert!(n < 100, "never returned to the player");
        }
        n
    }
}

#[test]
fn spawned_monsters_made_actors_take_turns() {
    let mut h = project("one", 1);
    // The monsters were made actors in `on_start`, due at once: their
    // first turns come before the player is asked for anything — and
    // before their scripts attach (`set_script` is deferred a step), so
    // those first turns do nothing. Harmless; count from here.
    h.step(false);
    h.settle();
    let first = h.acted();
    assert!(h.world.actors.len() == 6, "the player and five monsters");
    // One player turn: with one AI turn per step, the five answers take
    // five more steps.
    h.step(true);
    let follow_ups = h.settle();
    assert_eq!(h.acted() - first, 5, "every monster answered the move");
    assert_eq!(follow_ups, 5, "one monster per step");
    assert_eq!(h.log_problems, 0);
}

#[test]
fn with_ai_turns_per_step_every_monster_answers_in_the_same_step() {
    let mut h = project("batch", 256);
    h.step(false);
    h.settle();
    let first = h.acted();
    h.step(true);
    assert_eq!(h.acted() - first, 5, "the player's step resolved all five answers too");
    assert_eq!(h.settle(), 0, "nothing left for later steps");
    // And it stops at the player: one keypress, one round.
    h.step(true);
    assert_eq!(h.acted() - first, 10);
    assert_eq!(h.log_problems, 0);
}

#[test]
fn make_actor_ignores_missing_entities_and_existing_actors() {
    let mut h = project("ignore", 1);
    h.step(false);
    let before = h.world.actors.len();
    let player = h.world.find_by_tag("player").unwrap();
    // Run a one-off script through the player's own entity: making the
    // player (already a Local actor) or a nonexistent id an actor does
    // nothing.
    let dir = common::test_temp_dir().join("turn_batch_ignore");
    std::fs::write(
        dir.join("again.rhai"),
        "fn on_update(id, ctx) { ctx.make_actor(id, 50); ctx.make_actor(99999, 50.0); }",
    )
    .unwrap();
    h.world.scripts.insert(
        player,
        ember2d_sim::components::Script::new(dir.join("again.rhai").to_string_lossy().into_owned()),
    );
    h.step(false);
    h.step(false);
    assert_eq!(h.world.actors.len(), before);
    assert!(matches!(
        h.world.actors[&player].controller,
        ember2d_sim::components::Controller::Local(_)
    ));
}
