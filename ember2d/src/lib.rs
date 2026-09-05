// lib.rs — ember2d: the engine crate — rendering, windowing, input devices,
// audio, and play-mode orchestration, all built on top of `ember2d-sim`.
//
// Split out of a single `ember2d` crate in Phase 5 Step 5i
// (docs/ember2d-phase5-plan.md §5.5) — see `ember2d-sim`'s own `lib.rs` for
// the full reasoning (the module-to-crate table, the two prep gaps that
// step found, and why `sim.rs` lives here rather than in `ember2d-sim`
// despite its name).
//
// `editor` and `app` (top-level Editor <-> Play orchestration) are NOT
// here — `editor` is its own `ember2d-editor` crate (depends on this one),
// and `app` moved into the thin `ember2d-app` bin crate alongside
// `main.rs`, since it's the one place that needs BOTH this crate's
// `PlayState`/`Engine` and `ember2d-editor`'s `EditorState` — this crate
// can't depend on `ember2d-editor` (that's the dependency direction
// reversed) or the workspace would cycle.

pub mod audio;
pub mod camera;
pub mod engine;
pub mod gamepad;
pub mod input;
pub mod mouse;
pub mod play;
pub mod project;
pub mod renderer;
pub mod sim;
pub mod ui;

/// The ember2d prelude: import everything a game needs in one shot.
///
/// Add this to the top of your game file:
/// ```rust
/// use ember2d::prelude::*;
/// ```
///
/// Re-exports `ember2d_sim` types alongside this crate's own, so a caller
/// (a game, the `ember2d-app` bin crate, an integration test) never needs
/// to know or care that `World`/`LevelData`/etc. physically live in a
/// different crate — exactly the same one-import experience the
/// pre-Step-5i single crate gave. Does **not** re-export `EditorState`/
/// `run_editor_app`/`run_play_app`/`StartScreen` — those moved out of this
/// crate entirely (see this file's header comment); reach them via
/// `ember2d_editor` and the `ember2d-app` bin crate directly.
pub mod prelude {
    // ── Math ──────────────────────────────────────────────────────────────
    // Fundamental types: 2D vectors and rectangles. (ember2d-sim)
    pub use ember2d_sim::math::{IVec2, Rect, Vec2};

    // ── Camera ────────────────────────────────────────────────────────────
    // World-space <-> screen-space conversion (Phase 2).
    pub use crate::camera::Camera;

    // ── Renderer ──────────────────────────────────────────────────────────
    // The terminal renderer and its color palette.
    pub use crate::renderer::{Color, Renderer, Texture, TextureId, RenderBackend, WgpuBackend, AssetManager};

    // ── Components ────────────────────────────────────────────────────────
    // The core components every entity can have, plus Phase 3's animation
    // playback (Animator) and shared clip-definition (AnimationClip,
    // ClipFrames) types. (ember2d-sim)
    pub use ember2d_sim::components::{Actor, Animator, AnimationClip, ClipFrames, Collider, Controller, Sprite, SpriteSource, Tag, Transform};

    // ── World ─────────────────────────────────────────────────────────────
    // The entity database and the EntityId type alias. (ember2d-sim)
    pub use ember2d_sim::world::{EntityId, World};

    // ── Input ─────────────────────────────────────────────────────────────
    // The input manager and Key.
    // Game code uses Key::W, Key::Up, Key::Escape, etc.
    pub use crate::input::{InputManager, Key};
    pub use crate::gamepad::{GamepadState, GamepadButton, GamepadAxis};

    // ── Events ────────────────────────────────────────────────────────────
    // The event bus and all event types. (ember2d-sim)
    pub use ember2d_sim::event::{EventBus, GameEvent};

    // ── Engine ────────────────────────────────────────────────────────────
    // The engine itself and the trait + context types for game code.
    pub use crate::engine::{Engine, GameState, RenderContext, UpdateContext};

    // ── Mouse ─────────────────────────────────────────────────────────────
    // Mouse position + button state (available in UpdateContext and RenderContext).
    pub use crate::mouse::MouseState;

    // ── Level ─────────────────────────────────────────────────────────────
    // Serializable level format for saving/loading levels. (ember2d-sim)
    pub use ember2d_sim::level::{ActorRecord, LevelData, TileRecord};
    pub use ember2d_sim::save::SaveState;

    // ── Project ───────────────────────────────────────────────────────────
    pub use crate::project::{StartResult, StartTemplate};
    pub use crate::project::{ProjectData, VisualStyle, GameplayLoop};

    // ── Play mode ─────────────────────────────────────────────────────────
    // Runs a level loaded from a LevelData.
    pub use crate::play::PlayState;

    // ── Engine transition ─────────────────────────────────────────────────
    pub use crate::engine::Transition;
}
