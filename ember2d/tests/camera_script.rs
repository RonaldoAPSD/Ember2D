// ember2d/tests/camera_script.rs — Step 9-2 (docs/ember2d-master-plan.md
// §5.8): the script-drivable camera, driven through a real `PlayState`
// (the camera itself is presentation) plus one `Simulation`-level check of
// `get_mouse_world_x/y` under zoom.

mod common;

use ember2d::audio::AudioEngine;
use ember2d::engine::{GameState, UpdateContext};
use ember2d::gamepad::GamepadState;
use ember2d::input::InputManager;
use ember2d::mouse::MouseState;
use ember2d::play::PlayState;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::{BTreeMap, HashMap};

const VIEW: (usize, usize) = (20, 10);

/// A 60x40 level whose player runs `script`, started and ready to update.
fn play_with(tag: &str, script: &str) -> (PlayState, World) {
    let path = common::test_temp_dir().join(format!("camera_{tag}.rhai"));
    std::fs::write(&path, script).unwrap();
    let mut data = LevelData::empty(60, 40);
    data.set_player_spawn((5.0, 5.0));
    data.player.script = Some(path.to_string_lossy().into_owned());
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent = BTreeMap::new();
    play.on_start(&mut world, &mut events, VIEW.0, VIEW.1, &mut persistent);
    (play, world)
}

fn update(play: &mut PlayState, world: &mut World) {
    let mut input = InputManager::new();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let mut events = EventBus::new();
    let prev: HashMap<EntityId, Vec2> = HashMap::new();
    let (mut quit, mut turn) = (false, false);
    let mut persistent = BTreeMap::new();
    let mut audio = AudioEngine::new();
    play.update(UpdateContext {
        world,
        input: &mut input,
        mouse: &mouse,
        gamepad: &gamepad,
        events: &mut events,
        prev_positions: &prev,
        delta_time: 1.0 / 60.0,
        frame_delta_time: 1.0 / 60.0,
        elapsed: 0.0,
        quit: &mut quit,
        turn_triggered: &mut turn,
        viewport_width: VIEW.0,
        viewport_height: VIEW.1,
        persistent: &mut persistent,
        audio: &mut audio,
    });
}

fn run(play: &mut PlayState, world: &mut World, frames: usize) {
    for _ in 0..frames {
        update(play, world);
    }
}

#[test]
fn a_script_zooms_and_points_the_camera_and_reads_the_zoom_back() {
    let script = r#"
fn on_update(id, ctx) {
    ctx.set_camera_zoom(2);
    ctx.set_camera_speed(0);
    ctx.set_camera_target(30, 20);
    ctx.set_global("z", ctx.get_camera_zoom());
}
"#;
    let (mut play, mut world) = play_with("zoom", script);
    run(&mut play, &mut world, 3);
    assert_eq!(play.camera.zoom, 2.0);
    assert_eq!(play.camera.position, Vec2::new(30.0, 20.0), "speed 0 jumps straight there");
    assert_eq!(play.globals().get("z").and_then(|v| v.as_float().ok()), Some(2.0));
    // At zoom 2 a 20x10 view spans 10x5 world units around the centre.
    assert_eq!(play.camera.top_left(), Vec2::new(25.0, 17.5));
}

#[test]
fn an_entity_target_is_followed_and_dropped_when_the_entity_goes() {
    let script = r#"
fn on_update(id, ctx) {
    let n = ctx.add_global("n", 1);
    if n == 1.0 {
        let e = ctx.spawn_entity("x", 40.0, 25.0, "beacon");
        ctx.set_camera_speed(0);
        ctx.set_camera_target(e);
    }
    if n == 4.0 { ctx.despawn(ctx.find_by_tag("beacon")); }
}
"#;
    let (mut play, mut world) = play_with("entity", script);
    run(&mut play, &mut world, 3);
    assert_eq!(play.camera.position, Vec2::new(40.0, 25.0), "follows the beacon");
    run(&mut play, &mut world, 3);
    let player = world.find_by_tag("player").unwrap();
    let p = world.get_global_position(player);
    // Back on the player, clamped to the level: (5,5) can't be a 20x10
    // view's centre, so the view's top-left sits on the level's corner.
    assert_eq!(play.camera.position, Vec2::new(p.x.max(10.0), p.y.max(5.0)));
}

#[test]
fn bounds_keep_the_view_inside_and_clearing_them_restores_the_level() {
    let script = r#"
fn on_update(id, ctx) {
    let n = ctx.add_global("n", 1);
    ctx.set_camera_speed(0);
    ctx.set_camera_target(0, 0);
    if n == 1.0 { ctx.set_camera_bounds(20, 10, 30, 20); }
    if n == 3.0 { ctx.clear_camera_bounds(); }
}
"#;
    let (mut play, mut world) = play_with("bounds", script);
    run(&mut play, &mut world, 2);
    assert_eq!(play.camera.top_left(), Vec2::new(20.0, 10.0), "pinned to the bounds' corner");
    run(&mut play, &mut world, 2);
    assert_eq!(play.camera.top_left(), Vec2::new(0.0, 0.0), "the level's own corner again");
}

#[test]
fn set_camera_is_a_point_target_that_clear_camera_target_undoes() {
    let script = r#"
fn on_update(id, ctx) {
    let n = ctx.add_global("n", 1);
    ctx.set_camera_speed(0);
    if n == 1.0 { ctx.set_camera(30, 20); }
    if n == 3.0 { ctx.clear_camera_target(); }
}
"#;
    let (mut play, mut world) = play_with("legacy", script);
    run(&mut play, &mut world, 2);
    assert_eq!(play.camera.position, Vec2::new(30.0, 20.0));
    run(&mut play, &mut world, 2);
    assert_eq!(play.camera.position, Vec2::new(10.0, 5.0), "back on the player (clamped)");
}

#[test]
fn nonsense_camera_values_are_ignored_or_clamped() {
    let script = r#"
fn on_update(id, ctx) {
    ctx.set_camera_zoom(100);
    ctx.set_camera_bounds(0, 0, -5, 10);
    ctx.set_camera_speed(-3);
    ctx.set_camera_target(-1);
}
"#;
    let (mut play, mut world) = play_with("nonsense", script);
    run(&mut play, &mut world, 3);
    assert_eq!(play.camera.zoom, 8.0, "zoom clamps to 8");
    assert!(play.camera.position.x.is_finite());
}

#[test]
fn mouse_world_coordinates_account_for_the_zoom() {
    let path = common::test_temp_dir().join("camera_mouse.rhai");
    std::fs::write(
        &path,
        r#"
fn on_update(id, ctx) {
    ctx.set_camera_zoom(2);
    ctx.set_global("mx", ctx.get_mouse_world_x());
    ctx.set_global("sx", ctx.get_mouse_x());
}
"#,
    )
    .unwrap();
    let mut data = LevelData::empty(60, 40);
    data.player.script = Some(path.to_string_lossy().into_owned());
    let mut sim = Simulation::new(data);
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, VIEW.0, VIEW.1, &mut persistent);
    for _ in 0..2 {
        sim.step(
            &mut world,
            StepInput {
                input: &InputSnapshot::default(),
                mouse: MouseSnapshot { cell: (10.0, 4.0), ..Default::default() },
                gamepad: &GamepadSnapshot::default(),
                external_commands: &[],
                animating: &[],
                camera_origin: Vec2::new(3.0, 1.0),
                sim_dt: 1.0 / 60.0,
                elapsed: 0.0,
                viewport_w: VIEW.0,
                viewport_h: VIEW.1,
            },
            &mut persistent,
        );
    }
    let f = |k: &str| sim.globals().get(k).and_then(|v| v.as_float().ok()).unwrap();
    assert_eq!(f("sx"), 10.0, "get_mouse_x stays the screen cell");
    assert_eq!(f("mx"), 3.0 + 10.0 / 2.0, "world = camera origin + cell / zoom");
}
