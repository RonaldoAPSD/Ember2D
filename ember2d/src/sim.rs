// sim.rs — the shared per-step simulation sequence (Step 5d,
// docs/ember2d-phase5-plan.md).
//
// Both `engine.rs`'s `run()` (realtime's accumulator loop and turn-based's
// single-step-per-frame branch) and `tests/common/mod.rs`'s `TurnHarness`
// used to hand-duplicate the exact same sequence: consume the buffered
// input, run the top-of-stack `GameState`'s `update()`, then — always in
// realtime, only if a turn was triggered in turn-based — integrate physics,
// detect collisions, and run `late_update()`. Getting any piece of that
// sequence wrong or out of order in one of the two copies would silently
// test (or run) different behavior than the other; `docs/HANDOFF.md`
// documents exactly that hazard for `TurnHarness`. This module is now the
// one place that sequence lives — both callers just supply the values that
// differ between them.
//
// Deliberately a free function, not a method on `Engine` or `PlayState` —
// "extracting the loop body, not restructuring ownership" (Phase 5 plan,
// step 5d): `World` still lives on `Engine`, `PlayState` still owns the
// script engine. This is scaffolding toward the real `Simulation::step()`
// a later Phase 5 step builds (§5.4 seam 1 in the refactor plan), not that
// seam itself.

use std::collections::BTreeMap;

use crate::engine::{GameState, UpdateContext};
use ember2d_sim::event::EventBus;
use crate::gamepad::GamepadState;
use crate::input::InputManager;
use crate::mouse::MouseState;
use ember2d_sim::world::World;

/// What a caller needs back after one step.
pub struct StepResult {
    /// Set if `update`/`late_update` asked the engine to quit this step.
    pub should_quit: bool,
    /// Set if `update` resolved an actor's turn this step — as of Step 5f
    /// (docs/ember2d-phase5-plan.md), that means `PlayState::run_actor_turn`
    /// decided `TurnScheduler` should advance (there's no more
    /// `ctx.trigger_turn()` for a script to call directly). Meaningful in
    /// turn-based mode, where it's what gates the late phase below;
    /// realtime callers generally ignore it.
    pub turn_triggered: bool,
}

/// Run exactly one simulation step for `state`: consume buffered input,
/// call `update`, then — unless `should_quit` came back set — if
/// `gate_late_phase_on_turn` is false (realtime) or a turn was triggered
/// (turn-based), detect collisions and call `late_update`; physics only
/// integrates in realtime mode (see the D7 note below). Mirrors the two
/// branches `engine.rs::run()` used to inline directly; see this module's
/// header comment for why they're one function now.
///
/// `sim_dt` and `frame_dt` become `UpdateContext::delta_time` and
/// `::frame_delta_time` respectively — see those fields' own doc comments
/// for the boundary they maintain. `physics_dt` is what
/// `World::integrate_physics` itself is called with when it runs at all —
/// kept as its own parameter, separate from `sim_dt`, because realtime and
/// turn-based mode used to want (and turn-based, before the Step 5f fix
/// below, actually got) different values here. Defect D7
/// (docs/ember2d-refactor-plan.md §3) was turn-based mode integrating
/// physics at a hardcoded `dt = 1.0` regardless of `sim_dt` — meaningless
/// for grid movement, since nothing in the roguelike ever sets a nonzero
/// velocity. Fixed in Step 5f (docs/ember2d-phase5-plan.md, alongside the
/// turn scheduler): turn-based mode no longer calls `integrate_physics` at
/// all, not even on a step that did resolve a turn — see the `if
/// !gate_late_phase_on_turn` guard around that call below.
#[allow(clippy::too_many_arguments)]
pub fn step(
    state: &mut dyn GameState,
    world: &mut World,
    input: &mut InputManager,
    mouse: &mut MouseState,
    gamepad: &mut GamepadState,
    events: &mut EventBus,
    persistent: &mut BTreeMap<String, rhai::Dynamic>,
    sim_dt: f32,
    frame_dt: f32,
    physics_dt: f32,
    elapsed: f32,
    viewport_width: usize,
    viewport_height: usize,
    gate_late_phase_on_turn: bool,
) -> StepResult {
    events.clear();
    let prev_positions = world.snapshot_positions();

    // This step claims whatever presses are sitting in the input buffer —
    // see input::INPUT_BUFFER_WINDOW. A later step (this frame, or a future
    // one if this step never runs) sees none of them, so a light frame
    // doesn't silently drop a press.
    input.consume_step();
    mouse.consume_step();
    gamepad.consume_step();

    let mut should_quit = false;
    let mut turn_triggered = false;

    state.update(UpdateContext {
        world: &mut *world,
        input: &mut *input,
        mouse: &*mouse,
        gamepad: &*gamepad,
        events: &mut *events,
        prev_positions: &prev_positions,
        delta_time: sim_dt,
        frame_delta_time: frame_dt,
        elapsed,
        quit: &mut should_quit,
        turn_triggered: &mut turn_triggered,
        viewport_width,
        viewport_height,
        persistent: &mut *persistent,
    });

    let run_late_phase = !should_quit && (!gate_late_phase_on_turn || turn_triggered);
    if run_late_phase {
        // Defect D7 fix (Step 5f, docs/ember2d-phase5-plan.md): turn-based
        // mode never integrates physics at all now, not even on a step
        // that did resolve a turn — it used to run this with `physics_dt`
        // hardcoded to a full second's worth of velocity regardless of how
        // much sim time the turn actually represented, which was
        // meaningless for grid movement (nothing in the roguelike ever
        // sets a nonzero velocity; every actor moves via `set_position`
        // specifically so this never mattered functionally — but the call
        // itself was still dead weight asserting a physics model turn-based
        // play doesn't have). Only realtime mode integrates now.
        if !gate_late_phase_on_turn { world.integrate_physics(physics_dt); }
        world.detect_collisions(events);

        state.late_update(UpdateContext {
            world: &mut *world,
            input: &mut *input,
            mouse: &*mouse,
            gamepad: &*gamepad,
            events: &mut *events,
            prev_positions: &prev_positions,
            delta_time: sim_dt,
            frame_delta_time: frame_dt,
            elapsed,
            quit: &mut should_quit,
            turn_triggered: &mut turn_triggered,
            viewport_width,
            viewport_height,
            persistent: &mut *persistent,
        });
    }

    StepResult { should_quit, turn_triggered }
}
