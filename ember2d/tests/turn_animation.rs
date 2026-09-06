// tests/turn_animation.rs — Phase 5.5 Part 3 (docs/ember2d-phase5.5-plan.md):
// proves the animation queue actually blocks turn resolution, not just that
// the plumbing compiles. Playback is presentation, owned entirely by
// `ember2d::play::PlayState` — `Simulation`/`TurnHarness` never touch it at
// all — so this test drives a real `PlayState` by hand, the same pattern
// `ember2d/src/play/tests.rs` already uses for engine-level behavior no
// harness reaches (see e.g. that file's `collide_player_with_exit`).

use std::collections::{BTreeMap, HashMap};
use ember2d::prelude::*;

const FRAME_DT: f32 = 1.0 / 60.0;

fn step(play: &mut PlayState, world: &mut World, persistent: &mut BTreeMap<String, rhai::Dynamic>) {
    let mut input = InputManager::new();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let mut events = EventBus::new();
    let prev_positions: HashMap<EntityId, Vec2> = HashMap::new();
    let mut quit = false;
    let mut turn_triggered = false;
    play.update(UpdateContext {
        world, input: &mut input, mouse: &mouse, gamepad: &gamepad, events: &mut events,
        prev_positions: &prev_positions, delta_time: FRAME_DT, frame_delta_time: FRAME_DT,
        elapsed: 0.0, quit: &mut quit, turn_triggered: &mut turn_triggered,
        viewport_width: 20, viewport_height: 10, persistent,
    });
}

fn turns(persistent: &BTreeMap<String, rhai::Dynamic>) -> i64 {
    persistent.get("turns").and_then(|d| d.as_int().ok()).unwrap_or(0)
}

/// Unlike `step` above, takes a *shared* `InputManager` across calls rather
/// than a fresh one each time — needed to reproduce D19
/// (docs/ember2d-refactor-plan.md §3), which is specifically about a press
/// surviving across several calls to `PlayState::update` while the
/// animation queue drains. Mirrors `ember2d::sim::step`'s real per-frame
/// sequence closely enough to reproduce the bug: `consume_step()` before
/// `update`, `decay` after — `PlayState`'s animation gate lives entirely
/// inside `update`, so a driver that skips either step wouldn't exercise
/// the actual failure mode (a real frame claims-then-doesn't-necessarily-read
/// a buffered press).
fn play_frame(play: &mut PlayState, world: &mut World, input: &mut InputManager, persistent: &mut BTreeMap<String, rhai::Dynamic>) {
    input.consume_step();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let mut events = EventBus::new();
    let prev_positions: HashMap<EntityId, Vec2> = HashMap::new();
    let mut quit = false;
    let mut turn_triggered = false;
    play.update(UpdateContext {
        world, input, mouse: &mouse, gamepad: &gamepad, events: &mut events,
        prev_positions: &prev_positions, delta_time: FRAME_DT, frame_delta_time: FRAME_DT,
        elapsed: 0.0, quit: &mut quit, turn_triggered: &mut turn_triggered,
        viewport_width: 20, viewport_height: 10, persistent,
    });
    input.decay(FRAME_DT);
}

#[test]
fn a_key_pressed_while_an_animation_plays_is_not_lost() {
    // D19 (docs/ember2d-refactor-plan.md §3): a movement key tapped while an
    // enemy's (or here, the player's own) animation is still draining used
    // to vanish rather than being honored once the queue emptied, because
    // `ember2d::sim::step`'s unconditional `consume_step()` claims a
    // buffered press every real frame regardless of whether `PlayState`
    // goes on to read it. The player moves itself here (no separate enemy
    // needed) — the failure mode is identical either way, since it lives in
    // `PlayState::update`'s own animation-gate branch, not in anything
    // enemy-specific.
    let mut script_path = std::env::temp_dir();
    script_path.push("ember2d_test_d19_buffered_press.rhai");
    std::fs::write(&script_path, r#"
        fn on_input(id, ctx) {
            if ctx.just_pressed("d") { ctx.submit(id, "move", [1.0, 0.0]); }
        }
        fn on_turn(id, ctx) {
            if ctx.command_action() == "move" {
                let dx = ctx.command_param(0);
                ctx.set_position(id, ctx.get_x(id) + dx, ctx.get_y(id));
                ctx.animate_move(id, ctx.get_x(id), ctx.get_y(id), 5.0 / 60.0);
                ctx.act(100.0);
            }
        }
    "#).expect("write temp script");

    let mut data = LevelData::empty(10, 10);
    data.player.script = Some(script_path.to_string_lossy().to_string());

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);
    let player_id = world.find_by_tag("player").expect("player should have spawned");
    let start = world.get_global_position(player_id);

    let mut input = InputManager::new();

    // Frame 1: press "d" — resolves the first move and queues a 5-frame
    // animation.
    input.handle_pressed(Key::D);
    play_frame(&mut play, &mut world, &mut input, &mut persistent);
    input.handle_released(Key::D);
    assert_eq!(world.get_global_position(player_id), Vec2::new(start.x + 1.0, start.y), "the first press must move the player one cell");

    // Frame 2: the FIRST blocked frame (animation elapsed 0 < 5/60, still
    // draining) — tap "d" again right here. Before the D19 fix, this press
    // would be claimed and discarded by this frame's own `consume_step()`
    // without ever being read, since `update` takes the animation-blocked
    // branch and never used to look at input there at all.
    input.handle_pressed(Key::D);
    play_frame(&mut play, &mut world, &mut input, &mut persistent);
    input.handle_released(Key::D);

    // Frames 3-7: drain the remaining animation with no further input.
    for _ in 0..5 { play_frame(&mut play, &mut world, &mut input, &mut persistent); }

    // The frame-2 press must have survived and been honored once stepping
    // resumed — the player should now be two cells over, not stuck at one
    // (which is what D19 looked like: the second tap silently vanished).
    assert_eq!(
        world.get_global_position(player_id), Vec2::new(start.x + 2.0, start.y),
        "a key pressed while the animation queue was draining must not be silently dropped (D19)"
    );

    let _ = std::fs::remove_file(&script_path);
}

#[test]
fn the_scheduler_waits_for_the_animation_queue_to_drain_before_the_next_turn() {
    // The player is the only actor and its own `on_input` submits
    // unconditionally, so its turn resolves every step nothing else is
    // blocking — the simplest possible setup for observing whether the
    // *animation* queue, not the scheduler itself, is what withholds the
    // next turn. Every turn queues a same-position "move" animation lasting
    // 5 real frames (5/60s at FRAME_DT) — long enough to observe several
    // blocked steps without being so long the test is slow, short enough
    // that the assertions below aren't sensitive to float-rounding right at
    // the boundary (they check well inside/outside the 5-frame window, not
    // exactly on it).
    let mut script_path = std::env::temp_dir();
    script_path.push("ember2d_test_turn_animation.rhai");
    std::fs::write(&script_path, r#"
        fn on_input(id, ctx) {
            ctx.submit(id, "tick", []);
        }
        fn on_turn(id, ctx) {
            let n = ctx.get_persistent("turns");
            let n = if n == () { 0 } else { n };
            ctx.set_persistent("turns", n + 1);
            ctx.animate_move(id, ctx.get_x(id), ctx.get_y(id), 5.0 / 60.0);
            ctx.act(100.0);
        }
    "#).expect("write temp script");

    let mut data = LevelData::empty(10, 10);
    data.player.script = Some(script_path.to_string_lossy().to_string());

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    step(&mut play, &mut world, &mut persistent);
    assert_eq!(turns(&persistent), 1, "the first step must resolve exactly one turn and queue its animation");

    // Comfortably inside the 5-frame window — no further turn may resolve
    // while that animation is still draining.
    step(&mut play, &mut world, &mut persistent);
    step(&mut play, &mut world, &mut persistent);
    assert_eq!(turns(&persistent), 1, "no further turn may resolve while the animation queue is still draining");

    // Comfortably past the 5-frame window (6 more steps, 8 total since the
    // turn that queued it) — stepping must have resumed, and exactly once,
    // not more (a bug that ignored the gate entirely would show turns > 2
    // here just as readily as a bug that never resumed would show turns
    // stuck at 1).
    for _ in 0..6 { step(&mut play, &mut world, &mut persistent); }
    assert_eq!(turns(&persistent), 2, "the scheduler must resolve exactly one more turn once the animation queue drains");

    let _ = std::fs::remove_file(&script_path);
}
