// tests/external_commands.rs — proves seam 2 (docs/ember2d-phase5.5-plan.md
// Part 2, docs/ember2d-refactor-plan.md §5.4) actually works:
// `Simulation::step`'s `StepInput::external_commands` lets something OTHER
// than a script's own `on_input`/`ctx.submit()` drive an actor's turn.
//
// `tests/replay.rs` migrating to the rewritten `TurnHarness` (Step 2e)
// doesn't exercise this at all — every call there passes
// `external_commands: &[]`, same as `TurnHarness::frame`/`turn` always do.
// This is the one test that actually calls it with something non-empty,
// which is the only way to demonstrate the seam is real rather than just
// plumbing that compiles.

mod common;
use common::TurnHarness;
use ember2d::prelude::*;
use ember2d_sim::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::simulation::StepInput;

// `CARGO_MANIFEST_DIR`-relative, not CWD-relative — see tests/replay.rs's
// own comment on this (Step 5i's workspace split moved this crate below
// `roguelike/`, and `cargo test` runs each integration test binary with
// CWD set to the package's own directory).
const FLOOR1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../roguelike/floor1.level");

#[test]
fn an_externally_supplied_command_resolves_a_turn_with_no_key_ever_pressed() {
    let mut h = TurnHarness::load(FLOOR1);
    let player = h.player_id();
    let before = h.player_pos();

    // No key pressed at all — an empty InputSnapshot, exactly what a
    // netcode peer or a headless test with no real input device would send.
    // player.rhai's on_input never runs anything for this actor since
    // nothing was pressed; the "move" command below is injected straight
    // past on_input, at the same point ctx.submit() would have landed it
    // (see Simulation::step's own doc comment on the merge point).
    let empty_input = InputSnapshot::default();
    let mouse = MouseSnapshot::default();
    let gamepad = GamepadSnapshot::default();
    let external = [Command { actor: player, action: "move".to_string(), params: vec![0.0, -1.0] }];

    let outcome = h.sim.step(&mut h.world, StepInput {
        input: &empty_input,
        mouse,
        gamepad: &gamepad,
        external_commands: &external,
        camera_origin: Vec2::ZERO,
        sim_dt: 1.0 / 60.0,
        elapsed: h.elapsed,
        viewport_w: h.viewport_width,
        viewport_h: h.viewport_height,
    }, &mut h.persistent);

    assert!(outcome.turn_triggered, "an externally-supplied command must resolve a turn exactly like a real keypress would");
    assert_eq!(
        h.player_pos(), Vec2::new(before.x, before.y - 1.0),
        "the injected \"move\" command's [dx, dy] = [0.0, -1.0] must move the player exactly like on_input's own \"w\" -> [0.0, -1.0] translation would (see tests/roguelike_floor1.rs's own \"w\" test)"
    );
}

#[test]
fn an_external_command_for_an_actor_not_awaiting_input_is_silently_ignored_this_step() {
    // The scheduler still gates whose turn it is — external_commands isn't
    // a bypass of turn order, only of on_input's key-to-command
    // translation for whichever actor the scheduler already picked. A
    // command addressed to some other (nonexistent, here) actor id must
    // not resolve anyone's turn.
    let mut h = TurnHarness::load(FLOOR1);
    let before = h.player_pos();
    let bogus_actor = 999_999;

    let empty_input = InputSnapshot::default();
    let mouse = MouseSnapshot::default();
    let gamepad = GamepadSnapshot::default();
    let external = [Command { actor: bogus_actor, action: "move".to_string(), params: vec![0.0, -1.0] }];

    let outcome = h.sim.step(&mut h.world, StepInput {
        input: &empty_input,
        mouse,
        gamepad: &gamepad,
        external_commands: &external,
        camera_origin: Vec2::ZERO,
        sim_dt: 1.0 / 60.0,
        elapsed: h.elapsed,
        viewport_w: h.viewport_width,
        viewport_h: h.viewport_height,
    }, &mut h.persistent);

    assert!(!outcome.turn_triggered, "a command addressed to an actor whose turn it isn't must not resolve any turn");
    assert_eq!(h.player_pos(), before, "the player must not move when the injected command targets a different actor id");
}
