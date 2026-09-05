// lib.rs — ember2d-editor: the level/script editor.
//
// Split out of the single `ember2d` crate in Phase 5 Step 5i
// (docs/ember2d-phase5-plan.md §5.5) — depends on both `ember2d-sim`
// (World, level data, components — what a level actually contains) and
// `ember2d` (renderer, input, `PlayState` for F5 preview). See
// `ember2d-sim`'s own `lib.rs` for the full module-to-crate reasoning.

pub mod editor;

/// Re-exports for the one thing outside this crate that needs it —
/// `ember2d-app`'s `app.rs`, the only place that orchestrates both this
/// crate's `EditorState` and `ember2d`'s `PlayState`/`Engine` (see
/// `ember2d`'s own `lib.rs` for why that orchestration couldn't stay in
/// `ember2d` itself without a dependency cycle).
pub mod prelude {
    pub use crate::editor::EditorState;
    pub use crate::editor::start_screen::StartScreen;
}
