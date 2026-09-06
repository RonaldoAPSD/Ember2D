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
