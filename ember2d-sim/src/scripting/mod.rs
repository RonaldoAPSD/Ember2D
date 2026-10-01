// scripting/mod.rs — Scripting module re-exports.

mod api;
mod api_animation;
mod api_ext;
mod api_spatial;
mod apply;
mod collisions;
mod engine;
mod lifecycle;
mod registry;
// Step 9-1: the scene stack's scripting half — see that file's header.
mod scene;
// Step 9-2: the script-drivable camera — see that file's header.
mod camera;
// Step 9-3: menus and dialogue for scripts — see that file's header.
mod widgets;
mod sprite;
mod tiles;
mod state;
mod types;

pub use api::*;
pub use engine::*;
pub use types::*;
pub use widgets::DIALOGUE_MARGIN_COLS;
pub use sprite::SpriteOp;
pub use tiles::TileOp;
pub use camera::{CameraSettings, CameraTarget, CameraWrites, MAX_ZOOM, MIN_ZOOM};
pub use scene::{FlowRequest, SceneInfo, SceneOp, BUILTIN_PAUSE_KEY, BUILTIN_PAUSE_SOURCE};
// `WorldSnapshot` itself stays otherwise internal (`pub(super)` within this
// module) — this one re-export is just so `play.rs` can build one once per
// step and share it across `on_input`/`on_update`/`on_turn` (Step 5f's
// performance fix; see that type's own doc comment in scripting/state.rs).
pub use state::WorldSnapshot;
// `PassArgs` (Step 7.5-10, docs/ember2d-master-plan.md §5.6) is what every
// `run_*` method on `ScriptEngine` (re-exported via `engine::*` above) takes
// in place of its old 11-17 positional arguments — callers outside this
// module (`simulation.rs`, `simulation/step.rs`, `simulation/spawn.rs`)
// need to be able to construct one.
pub use state::PassArgs;
