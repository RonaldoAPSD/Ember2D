// scripting/mod.rs — Scripting module re-exports.

mod types;
mod api;
mod state;
mod engine;

pub use types::*;
pub use api::*;
pub use engine::*;
// `WorldSnapshot` itself stays otherwise internal (`pub(super)` within this
// module) — this one re-export is just so `play.rs` can build one once per
// step and share it across `on_input`/`on_update`/`on_turn` (Step 5f's
// performance fix; see that type's own doc comment in scripting/state.rs).
pub use state::WorldSnapshot;
