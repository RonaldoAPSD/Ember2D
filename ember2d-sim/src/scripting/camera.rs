// scripting/camera.rs — what a script can tell the camera: what to follow,
// how far to zoom, where it may go, how fast it catches up.
//
// ── WHY (Step 9-2, docs/ember2d-master-plan.md §5.8) ─────────────────────────
//
// Before 9-2 the camera followed the level's camera-follow actor and nothing
// else; `set_camera(x, y)` could pin it to a point but nothing could unpin
// it, zoom it, follow a different entity, or keep it inside part of a map —
// so a cutscene (pan to the door, hold, pan back) wasn't writable at all.
//
// `CameraSettings` is the camera as scripts have set it. It's simulation
// state (deterministic, what `get_camera_zoom` reads back, owned by
// `Simulation`), but nothing in the simulation acts on it: play mode reads
// it every frame and does the following and smoothing itself, on real frame
// time — the lerp's `exp()` never feeds back into the sim. `set_camera`
// (the pre-9-2 function) is now just `set_camera_target(x, y)`.

use crate::math::{Rect, Vec2};
use crate::world::EntityId;

use super::api::ScriptCtx;
use super::engine::ScriptEngine;

/// What the camera centres on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraTarget {
    /// The level's camera-follow actor (the player, usually).
    Default,
    /// An entity — falls back to `Default` if it's despawned.
    Entity(EntityId),
    /// A fixed world point.
    Point(Vec2),
}

/// Zoom range `set_camera_zoom` clamps to.
pub const MIN_ZOOM: f32 = 0.25;
pub const MAX_ZOOM: f32 = 8.0;
/// The follow speed a camera starts with (the pre-9-2 constant).
pub const DEFAULT_CAMERA_SPEED: f32 = 5.0;

/// The camera as scripts have set it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraSettings {
    pub target: CameraTarget,
    /// Screen cells per world unit (1.0 = one cell per tile).
    pub zoom: f32,
    /// The world rect the view must stay inside; `None` = the level's own.
    pub bounds: Option<Rect>,
    /// How quickly the camera catches up with its target, per second;
    /// `0` jumps straight there.
    pub speed: f32,
}

impl Default for CameraSettings {
    fn default() -> Self {
        CameraSettings {
            target: CameraTarget::Default,
            zoom: 1.0,
            bounds: None,
            speed: DEFAULT_CAMERA_SPEED,
        }
    }
}

/// One pass's camera requests — each field last-wins within the pass.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CameraWrites {
    pub target: Option<CameraTarget>,
    pub zoom: Option<f32>,
    /// `Some(None)` clears the bounds.
    pub bounds: Option<Option<Rect>>,
    pub speed: Option<f32>,
}

impl CameraWrites {
    pub fn is_empty(&self) -> bool {
        *self == CameraWrites::default()
    }

    /// Folds these writes into `settings`.
    pub fn apply_to(&self, settings: &mut CameraSettings) {
        if let Some(t) = self.target {
            settings.target = t;
        }
        if let Some(z) = self.zoom {
            settings.zoom = z;
        }
        if let Some(b) = self.bounds {
            settings.bounds = b;
        }
        if let Some(s) = self.speed {
            settings.speed = s;
        }
    }
}

impl ScriptEngine {
    /// What `get_camera_zoom` and friends read — set by `Simulation`
    /// whenever its `CameraSettings` change, copied into each pass.
    pub fn set_camera_view(&mut self, settings: CameraSettings) {
        self.camera_view = settings;
    }

    /// Step 9-5: see `Simulation::set_world_cell_scale`.
    pub fn set_world_cell_scale(&mut self, scale: (f32, f32)) {
        self.world_cell_scale = scale;
    }

    /// What every pass copies from the engine into its `ScriptState`
    /// before running scripts: the scene stack (9-1), the camera (9-2),
    /// the world cell scale (9-5) and the widget model (9-3). One helper
    /// since 9-5 — the same three lines used to be repeated in all six
    /// passes, and a fourth would have made it twenty-four.
    pub(super) fn fill_pass_context(&self, state: &mut super::state::ScriptState) {
        state.scene = self.scene_ctx();
        state.camera_view = self.camera_view;
        state.cell_scale = self.world_cell_scale;
        state.ui = self.ui_ctx();
    }
}

/// A finite float, or `None` (so a NaN from a script is ignored, never stored).
fn finite(v: f64) -> Option<f32> {
    let f = v as f32;
    f.is_finite().then_some(f)
}

impl ScriptCtx {
    /// Follow entity `id` (back to the default follow if it disappears).
    pub fn set_camera_target_entity(&mut self, id: i64) {
        if id >= 0 {
            self.inner.borrow_mut().pending_camera.target =
                Some(CameraTarget::Entity(id as EntityId));
        }
    }
    /// Centre on world point `(x, y)`.
    pub fn set_camera_target_point(&mut self, x: f64, y: f64) {
        if let (Some(x), Some(y)) = (finite(x), finite(y)) {
            self.inner.borrow_mut().pending_camera.target =
                Some(CameraTarget::Point(Vec2::new(x, y)));
        }
    }
    pub fn set_camera_target_point_i(&mut self, x: i64, y: i64) {
        self.set_camera_target_point(x as f64, y as f64)
    }
    /// Back to following the level's camera-follow actor.
    pub fn clear_camera_target(&mut self) {
        self.inner.borrow_mut().pending_camera.target = Some(CameraTarget::Default);
    }
    /// Screen cells per world unit, clamped to 0.25..8.
    pub fn set_camera_zoom(&mut self, zoom: f64) {
        if let Some(z) = finite(zoom) {
            self.inner.borrow_mut().pending_camera.zoom = Some(z.clamp(MIN_ZOOM, MAX_ZOOM));
        }
    }
    pub fn set_camera_zoom_i(&mut self, zoom: i64) {
        self.set_camera_zoom(zoom as f64)
    }
    /// The zoom as of the start of this pass.
    pub fn get_camera_zoom(&mut self) -> f64 {
        self.inner.borrow().camera_view.zoom as f64
    }
    /// Keep the view inside world rect `(x, y, w, h)`.
    pub fn set_camera_bounds(&mut self, x: f64, y: f64, w: f64, h: f64) {
        if let (Some(x), Some(y), Some(w), Some(h)) = (finite(x), finite(y), finite(w), finite(h)) {
            if w > 0.0 && h > 0.0 {
                self.inner.borrow_mut().pending_camera.bounds = Some(Some(Rect::new(x, y, w, h)));
            }
        }
    }
    pub fn set_camera_bounds_i(&mut self, x: i64, y: i64, w: i64, h: i64) {
        self.set_camera_bounds(x as f64, y as f64, w as f64, h as f64)
    }
    /// Back to the level's own bounds.
    pub fn clear_camera_bounds(&mut self) {
        self.inner.borrow_mut().pending_camera.bounds = Some(None);
    }
    /// Follow speed per second (`0` = jump straight to the target).
    pub fn set_camera_speed(&mut self, speed: f64) {
        if let Some(s) = finite(speed) {
            self.inner.borrow_mut().pending_camera.speed = Some(s.max(0.0));
        }
    }
    pub fn set_camera_speed_i(&mut self, speed: i64) {
        self.set_camera_speed(speed as f64)
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("set_camera_target", ScriptCtx::set_camera_target_entity);
    engine.register_fn("set_camera_target", ScriptCtx::set_camera_target_point);
    engine.register_fn("set_camera_target", ScriptCtx::set_camera_target_point_i);
    engine.register_fn("clear_camera_target", ScriptCtx::clear_camera_target);
    engine.register_fn("set_camera_zoom", ScriptCtx::set_camera_zoom);
    engine.register_fn("set_camera_zoom", ScriptCtx::set_camera_zoom_i);
    engine.register_fn("get_camera_zoom", ScriptCtx::get_camera_zoom);
    engine.register_fn("set_camera_bounds", ScriptCtx::set_camera_bounds);
    engine.register_fn("set_camera_bounds", ScriptCtx::set_camera_bounds_i);
    engine.register_fn("clear_camera_bounds", ScriptCtx::clear_camera_bounds);
    engine.register_fn("set_camera_speed", ScriptCtx::set_camera_speed);
    engine.register_fn("set_camera_speed", ScriptCtx::set_camera_speed_i);
}
