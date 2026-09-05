// command.rs — actor-addressed commands and input snapshots (Step 5e,
// docs/ember2d-phase5-plan.md).
//
// A `Command` is the input boundary the whole Phase 5 design turns on
// (plan §1.1): the sim routes it to its actor, but the *script* decides
// what the named action means — "move"/"attack"/"quaff"/whatever a project
// invents. Rust never learns the vocabulary. That's deliberate: the
// alternative (a typed `CommandKind` enum the engine applies itself) would
// move movement and combat rules back into Rust, undoing what Phase 4 just
// finished, and every new action a project wants would need a Rust change.
// It's also what makes a recorded/transmitted command stream (the replay
// test, Step 5h; lockstep netcode, Phase 9b) fully determine the next
// state without the engine needing to know what a "move" is.
//
// `InputSnapshot` is the plain-data half of the same boundary, on the
// input-reading side: `InputManager`/`Key` are winit-flavored and
// engine-side, but a script's `on_input` only ever needs string key names
// ("w", "space") — see `InputManager::snapshot` (input.rs) for where the
// winit-to-string conversion actually happens; this type itself is just
// two sets of strings. Neither type here depends on winit, specifically so
// both can move into the eventual `ember2d-sim` crate (plan §5.5, step 5i)
// without dragging a window dependency with them.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::world::EntityId;

/// One actor's requested action for a step. `action` is a script-chosen
/// name the engine never interprets; `params` are whatever numbers that
/// action needs (e.g. `[dx, dy]` for a "move"). See this module's header
/// comment for why the shape is this loose.
#[derive(Debug, Clone)]
pub struct Command {
    pub actor: EntityId,
    pub action: String,
    pub params: Vec<f64>,
}

/// Which key names are held/just-pressed this step, with no winit or `Key`
/// type in it — see this module's header comment. `just_pressed` here
/// means whatever `InputManager::just_pressed` already means: buffered
/// until consumed, true for exactly the one simulation step that claims a
/// physical press (see `input::INPUT_BUFFER_WINDOW`).
#[derive(Debug, Clone, Default)]
pub struct InputSnapshot {
    pub held: BTreeSet<String>,
    pub pressed: BTreeSet<String>,
}

impl InputSnapshot {
    pub fn is_held(&self, key: &str) -> bool { self.held.contains(key) }
    pub fn just_pressed(&self, key: &str) -> bool { self.pressed.contains(key) }
}

/// The mouse's cell position and left/right button state this step, with no
/// winit type in it — same boundary `InputSnapshot` draws for the keyboard
/// (see this module's header comment), just discovered late: `MouseState`
/// (mouse.rs) itself pulls in `winit::event::MouseButton` for
/// `MouseButton::from_winit`, so `scripting/state.rs` referencing
/// `&MouseState` directly (as it did before Step 5i) would have dragged
/// winit into `ember2d-sim`'s dependency tree. `MouseState::snapshot`
/// (mouse.rs) is where the conversion happens.
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseSnapshot {
    /// (column, row) under the cursor, in character cells.
    pub cell: (f32, f32),
    /// (left, right) — held.
    pub held: (bool, bool),
    /// (left, right) — just pressed this step.
    pub pressed: (bool, bool),
}

/// Every gamepad's held/just-pressed buttons and axis values this step,
/// with no gilrs type in it — same reasoning as `MouseSnapshot` above:
/// `GamepadState` (gamepad.rs) owns a live `gilrs::Gilrs` handle directly,
/// so `scripting` referencing it by reference would have dragged gilrs into
/// `ember2d-sim` too. `GamepadState::snapshot` (gamepad.rs) builds this.
/// `HashSet`/`HashMap`, not `BTree*` — every read here is `.contains(&id)`/
/// `.get(&id)` against a known `(gamepad_id, name)` key, never iterated as
/// a whole (see `scripting/state.rs`'s own header comment on this
/// distinction), so there's no order to make deterministic.
#[derive(Debug, Clone, Default)]
pub struct GamepadSnapshot {
    pub held: HashSet<(usize, String)>,
    pub pressed: HashSet<(usize, String)>,
    pub axes: HashMap<(usize, String), f32>,
}
