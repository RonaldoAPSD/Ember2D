// renderer/color.rs — re-exports the crate-root `color` module.
//
// `Color` moved to `crate::color` in Step 5a (docs/ember2d-phase5-plan.md) so
// it's reachable without depending on the renderer at all — game/level data
// (a tile's authored fg/bg, a serialized save value) has nothing to do with
// GPU rendering. This file stays as a real module so every existing
// `renderer::color::Color` path keeps compiling unchanged; the renderer
// legitimately uses `Color` for every draw call, so this re-export is
// honest, not a shim scheduled for deletion.

pub use ember2d_sim::color::{Color, DEFAULT_FG, DEFAULT_BG};
