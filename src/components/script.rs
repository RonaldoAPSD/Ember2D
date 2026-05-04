// components/script.rs — The Script component: attaches a .rhai file to an entity.
//
// ── WHAT IS A SCRIPT COMPONENT? ──────────────────────────────────────────────
//
// A Script component is the simplest possible component — it's just a file path.
// When PlayState runs the level, it looks for entities that have this component
// and calls the `on_update(entity_id, ctx)` function in the .rhai file every frame.
//
// ── WORKFLOW ──────────────────────────────────────────────────────────────────
//
//   1. In the level file (.level RON), add `script: Some("goblin.rhai")` to a tile.
//   2. PlayState.on_start() reads it and calls ScriptEngine.compile("goblin.rhai").
//   3. Every frame, ScriptEngine.run_scripts() calls on_update() in the script.
//   4. The script uses the ctx object to read positions and set velocities.
//
// ── EXAMPLE .rhai SCRIPT ─────────────────────────────────────────────────────
//
//   // goblin.rhai — chases the player
//   fn on_update(entity_id, ctx) {
//       let player = ctx.find_by_tag("player");
//       if player >= 0 {
//           if ctx.get_x(player) > ctx.get_x(entity_id) {
//               ctx.set_velocity(entity_id, 3.0, 0.0);
//           } else {
//               ctx.set_velocity(entity_id, -3.0, 0.0);
//           }
//       }
//   }

/// Marks an entity as having a script that runs every frame.
///
/// The `path` field is a relative or absolute path to a `.rhai` script file.
/// The ScriptEngine in `src/scripting.rs` compiles the file once on level load
/// and calls its `on_update(entity_id, ctx)` function every game frame.
pub struct Script {
    /// Path to the `.rhai` file on disk.
    /// Relative paths are resolved from the current working directory (where
    /// you ran `cargo run`), which is typically the project root.
    pub path: String,
}

impl Script {
    /// Create a Script component from any string-like path.
    pub fn new(path: impl Into<String>) -> Self {
        Script { path: path.into() }
    }
}
