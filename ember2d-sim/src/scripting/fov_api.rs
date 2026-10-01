// scripting/fov_api.rs — field of view for scripts: `compute_fov`,
// `is_in_fov`, `is_explored`, `fov_reset`, `set_fov_visibility`.
//
// Step 9.5-2 (docs/ember2d-master-plan.md §5.8.5). The algorithm and the
// map it fills are `crate::fov`; this is the script surface. Writes queue
// a `FovOp` and travel back in `ScriptUpdateResult::fov_ops`, applied by
// `Simulation` (simulation/tiles.rs) AFTER the same pass's tile ops — so a
// script that carves a floor and computes the view in one `on_start` gets
// a view of the floor it just carved. Reads see the view as it was at the
// start of the pass, like every other read.
//
// Fog of war is opt-in: nothing happens until a script calls
// `compute_fov`. Until then (and after `fov_reset`) `is_in_fov` and
// `is_explored` answer `true` everywhere — no fog means everything counts
// as seen, so a monster script asking "can I see the player?" works the
// same in a level without fog.

use crate::fov::FovVisibility;

use super::api::ScriptCtx;

/// One field-of-view request from a script.
#[derive(Debug, Clone, PartialEq)]
pub enum FovOp {
    /// Recompute what's visible from (x, y) out to radius `r`.
    Compute(i64, i64, i64),
    /// Forget everything explored and turn fog of war off.
    Reset,
    Visibility(i64, FovVisibility),
    /// A request refused at call time; the reason is logged when applied.
    Bad(String),
}

fn cell(v: f64) -> i64 {
    if v.is_finite() {
        v.floor().clamp(i64::MIN as f64, i64::MAX as f64) as i64
    } else {
        i64::MIN
    }
}

impl ScriptCtx {
    /// Everything visible from (x, y) out to `radius` cells, walls (solid
    /// tilemap cells) blocking sight. Turns fog of war on; call it again
    /// whenever the viewer moves.
    pub fn compute_fov(&mut self, x: i64, y: i64, radius: i64) {
        self.inner.borrow_mut().fov_ops.push(FovOp::Compute(x, y, radius));
    }
    fn compute_fov_f(&mut self, x: f64, y: f64, radius: f64) {
        self.compute_fov(cell(x), cell(y), cell(radius));
    }
    // `compute_fov(ctx.get_x(id), ctx.get_y(id), 8)`: a position from
    // `get_x` (float) with a radius literal (int) — Rhai needs the exact
    // overload (see `tile_set_layer_fi` in tiles.rs).
    fn compute_fov_fi(&mut self, x: f64, y: f64, radius: i64) {
        self.compute_fov(cell(x), cell(y), radius);
    }

    /// Is (x, y) in view? `true` everywhere while fog of war is off.
    pub fn is_in_fov(&mut self, x: i64, y: i64) -> bool {
        let s = self.inner.borrow();
        match (&s.fov, i32::try_from(x), i32::try_from(y)) {
            (None, _, _) => true,
            (Some(f), Ok(x), Ok(y)) => f.is_visible(x, y),
            _ => false,
        }
    }
    fn is_in_fov_f(&mut self, x: f64, y: f64) -> bool {
        self.is_in_fov(cell(x), cell(y))
    }

    /// Has (x, y) ever been in view? `true` everywhere while fog of war is
    /// off.
    pub fn is_explored(&mut self, x: i64, y: i64) -> bool {
        let s = self.inner.borrow();
        match (&s.fov, i32::try_from(x), i32::try_from(y)) {
            (None, _, _) => true,
            (Some(f), Ok(x), Ok(y)) => f.is_explored(x, y),
            _ => false,
        }
    }
    fn is_explored_f(&mut self, x: f64, y: f64) -> bool {
        self.is_explored(cell(x), cell(y))
    }

    /// Forget what's been explored and turn fog of war off (until the next
    /// `compute_fov`).
    pub fn fov_reset(&mut self) {
        self.inner.borrow_mut().fov_ops.push(FovOp::Reset);
    }

    /// How `id` is drawn while it's out of view: `"hide"` (not at all),
    /// `"remember"` (dimmed, once its cell has been seen) or `"always"`.
    /// Without one: the player always, an actor hidden, anything else
    /// remembered.
    pub fn set_fov_visibility(&mut self, id: i64, mode: String) {
        let op = match FovVisibility::parse(&mode) {
            Some(v) => FovOp::Visibility(id, v),
            None => FovOp::Bad(format!(
                "set_fov_visibility: '{mode}' — use \"hide\", \"remember\" or \"always\""
            )),
        };
        self.inner.borrow_mut().fov_ops.push(op);
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("compute_fov", ScriptCtx::compute_fov);
    engine.register_fn("compute_fov", ScriptCtx::compute_fov_f);
    engine.register_fn("compute_fov", ScriptCtx::compute_fov_fi);
    engine.register_fn("is_in_fov", ScriptCtx::is_in_fov);
    engine.register_fn("is_in_fov", ScriptCtx::is_in_fov_f);
    engine.register_fn("is_explored", ScriptCtx::is_explored);
    engine.register_fn("is_explored", ScriptCtx::is_explored_f);
    engine.register_fn("fov_reset", ScriptCtx::fov_reset);
    engine.register_fn("set_fov_visibility", ScriptCtx::set_fov_visibility);
}
