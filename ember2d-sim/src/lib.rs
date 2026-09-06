// lib.rs — ember2d-sim: the deterministic simulation core.
//
// Split out of the single `ember2d` crate in Phase 5 Step 5i
// (docs/ember2d-phase5-plan.md §5.5) — see that step's own section of the
// plan for the full module-to-crate table and the two gaps this step found
// beyond what the plan called "mechanical":
//   - `Color` (and everything importing it) had to be repointed from
//     `crate::renderer::color::Color` to `crate::color::Color` — Step 5a
//     moved the type itself out of `renderer` for exactly this split, but
//     most call sites never got updated to import it from the new location.
//   - `scripting/state.rs` took `&MouseState`/`&GamepadState` directly,
//     both of which own real `winit`/`gilrs` types — that would have
//     dragged a window/input-device dependency into this crate's tree, the
//     one thing this split exists to prevent. Fixed by adding
//     `MouseSnapshot`/`GamepadSnapshot` (command.rs) — the same "sim-safe
//     plain-data twin of an engine-side type" pattern `InputSnapshot`
//     already established for the keyboard in Step 5e.
//
// No renderer, no window, no input-device polling anywhere in this crate's
// dependency tree — `cargo build -p ember2d-sim` succeeding with that
// property is the whole point (docs/ember2d-refactor-plan.md §5.5): a
// netcode host, a headless dedicated server, or a desync-checking replay
// tool never needs to link a GPU or open a window.
//
// `sim.rs` (the per-step orchestration loop extracted in Step 5d) is
// DELIBERATELY NOT here, despite being named "sim" and listed under this
// crate in the plan's own sketch table. Its `step()` function's signature
// is written entirely in terms of engine-side types — `GameState` (the
// trait `engine.rs` defines, whose `render()` method needs a GPU-backed
// `RenderContext`), and real `InputManager`/`MouseState`/`GamepadState`
// (not their sim-safe snapshots) — because its job is "run one real engine
// frame," not "advance the simulation by one deterministic step." That's
// what `simulation.rs`'s `Simulation` type is, added in Phase 5.5
// (docs/ember2d-phase5.5-plan.md Part 2) to close the gap this comment used
// to describe as future work ("scaffolding toward the real seam, not that
// seam itself") — see that module's own header comment. `sim.rs` stays in
// the main `ember2d` crate: it's still the generic per-frame pump shared by
// every `GameState` (`PlayState`, but also the editor and start screen,
// which genuinely need the raw device types it threads through — moving it
// here would mean either dragging `engine`/`input`/`mouse`/`gamepad` into
// this crate too, defeating the split, or a real redesign those two states
// don't need).

pub mod math;
pub mod color;
pub mod world;
pub mod components;
pub mod level;
pub mod save;
pub mod scripting;
pub mod command;
pub mod scheduler;
pub mod graph;
pub mod event;
pub mod simulation;
pub mod layers;
