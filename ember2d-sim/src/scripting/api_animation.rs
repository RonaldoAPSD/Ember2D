// scripting/api_animation.rs — ScriptCtx's animation-queue methods (Phase
// 5.5 Part 3, docs/ember2d-phase5.5-plan.md).
//
// Split into its own file rather than added to api.rs directly: api.rs was
// already at the project's 600-line hard limit (CLAUDE.md) before this
// phase touched it — a pre-existing violation, not something to expand
// while adding unrelated functionality. A second `impl ScriptCtx` block in
// a sibling file is exactly the pattern `engine.rs`/`engine_tests.rs`
// already established for the same reason.
//
// Grid/game state has already resolved by the time any of these are
// called — a script calls `ctx.set_position`/despawn/etc. for the real
// consequence same as always, then queues one of these purely to tell the
// presentation layer (`ember2d::play::PlayState`) what to show while real
// time passes. None of this is read back by the sim; see
// `AnimationEvent`'s own doc comment (scripting/types.rs).

use super::api::ScriptCtx;
use super::types::AnimationEvent;

impl ScriptCtx {
    /// `from` is read from this entity's position *right now* (before any
    /// `set_position` this same call makes takes effect — deferred writes,
    /// same rule as every other `get_*` in api.rs), not passed by the
    /// caller, so a script can't accidentally desync the animation's start
    /// point from where the entity actually was.
    pub fn animate_move(&mut self, id: i64, to_x: f64, to_y: f64, duration: f64) {
        let mut state = self.inner.borrow_mut();
        let (from_x, from_y) = state.positions.get(&id).copied().unwrap_or((0.0, 0.0));
        state.pending_animations.push(AnimationEvent::Move {
            entity: id as crate::world::EntityId,
            from: crate::math::Vec2::new(from_x, from_y),
            to: crate::math::Vec2::new(to_x as f32, to_y as f32),
            duration: duration as f32,
        });
    }

    pub fn animate_flash(&mut self, id: i64, color: String, duration: f64) {
        self.inner.borrow_mut().pending_animations.push(AnimationEvent::Flash {
            entity: id as crate::world::EntityId,
            color: super::types::parse_color(&color),
            duration: duration as f32,
        });
    }

    pub fn animate_shake(&mut self, id: i64, duration: f64) {
        self.inner.borrow_mut().pending_animations.push(AnimationEvent::Shake {
            entity: id as crate::world::EntityId,
            duration: duration as f32,
        });
    }

    /// Always `false` under the current design: the scheduler blocks ALL
    /// stepping while any animation is draining
    /// (docs/ember2d-phase5.5-plan.md Part 3 — "the scheduler waits for the
    /// queue to drain before resolving the next turn"), so no script can
    /// ever run *while* an animation is still in flight — by the time any
    /// script executes again, everything queued before it has already
    /// finished playing. Registered now, honestly returning a real (if
    /// currently constant) answer rather than erroring, so a future
    /// per-entity (rather than whole-queue) animation gate can make this
    /// meaningful without a scripting-API change.
    pub fn is_animating(&mut self, _id: i64) -> bool { false }
}
