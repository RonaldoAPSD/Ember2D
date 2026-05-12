// lib.rs — The ember2d engine library root.
//
// This file is the "front door" of the library crate.
//
// It does two things:
//   1. Declares every sub-module with `pub mod` so Rust knows to compile them.
//      - `pub mod math` → looks for src/math.rs
//      - `pub mod renderer` → looks for src/renderer/mod.rs
//      - etc.
//
//   2. Provides a `prelude` module with all the commonly-used types,
//      so game code can import everything with one line:
//      `use ember2d::prelude::*;`
//
// WHY A PRELUDE?
//   It's a Rust convention: a `prelude` module re-exports the most essential
//   types so users don't have to write a dozen `use` statements.
//   The Rust standard library has one too: `std::prelude::v1`.
//
// LIBRARY vs BINARY:
//   This file compiles into the LIBRARY crate (a `.rlib`).
//   src/main.rs compiles into the BINARY crate (the demo game executable).
//   The binary links against this library — that's how `use ember2d::prelude::*`
//   works inside main.rs even though they're in the same Cargo package.

pub mod app;
pub mod audio;
pub mod components;
pub mod editor;
pub mod engine;
pub mod event;
pub mod gamepad;
pub mod input;
pub mod level;
pub mod math;
pub mod mouse;
pub mod play;
pub mod project;
pub mod renderer;
pub mod save;
pub mod scripting;
pub mod ui;
pub mod world;

/// The ember2d prelude: import everything a game needs in one shot.
///
/// Add this to the top of your game file:
/// ```rust
/// use ember2d::prelude::*;
/// ```
pub mod prelude {
    // ── Math ──────────────────────────────────────────────────────────────
    // Fundamental types: 2D vectors and rectangles.
    pub use crate::math::{IVec2, Rect, Vec2};

    // ── Renderer ──────────────────────────────────────────────────────────
    // The terminal renderer and its color palette.
    pub use crate::renderer::{Color, Renderer, Texture, RenderBackend, WgpuBackend, AssetManager};

    // ── Components ────────────────────────────────────────────────────────
    // The four core components every entity can have.
    pub use crate::components::{Collider, Sprite, Tag, Transform};

    // ── World ─────────────────────────────────────────────────────────────
    // The entity database and the EntityId type alias.
    pub use crate::world::{EntityId, World};

    // ── Input ─────────────────────────────────────────────────────────────
    // The input manager and Key.
    // Game code uses Key::W, Key::Up, Key::Escape, etc.
    pub use crate::input::{InputManager, Key};
    pub use crate::gamepad::{GamepadState, GamepadButton, GamepadAxis};

    // ── Events ────────────────────────────────────────────────────────────
    // The event bus and all event types.
    pub use crate::event::{EventBus, GameEvent};

    // ── Engine ────────────────────────────────────────────────────────────
    // The engine itself and the trait + context types for game code.
    pub use crate::engine::{Engine, GameState, RenderContext, UpdateContext};

    // ── Mouse ─────────────────────────────────────────────────────────────
    // Mouse position + button state (available in UpdateContext and RenderContext).
    pub use crate::mouse::MouseState;

    // ── Level ─────────────────────────────────────────────────────────────
    // Serializable level format for saving/loading levels.
    pub use crate::level::{LevelData, TileRecord};
    pub use crate::save::SaveState;

    // ── Editor ────────────────────────────────────────────────────────────
    pub use crate::editor::EditorState;
    pub use crate::editor::start_screen::StartScreen;
    pub use crate::project::{StartResult, StartTemplate};
    pub use crate::project::{ProjectData, VisualStyle, GameplayLoop};

    // ── Play mode ─────────────────────────────────────────────────────────
    // Runs a level loaded from a LevelData.
    pub use crate::play::PlayState;

    // ── App orchestration ─────────────────────────────────────────────────
    // Top-level loops that manage Editor ↔ Play transitions.
    pub use crate::app::{run_editor_app, run_play_app};

    // ── Engine transition ─────────────────────────────────────────────────
    pub use crate::engine::Transition;
}
