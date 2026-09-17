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
    /// `i64` overload (7.5-1, docs/ember2d-master-plan.md §5.6, R31) — see
    /// `api.rs`'s `draw_hud_f` for the full reasoning every coordinate/
    /// size/layer-order function in this crate gets one of these.
    pub fn animate_move_i(&mut self, id: i64, to_x: i64, to_y: i64, duration: i64) {
        self.animate_move(id, to_x as f64, to_y as f64, duration as f64)
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

    /// Step 7.5-7 (docs/ember2d-master-plan.md §5.6): whether `id` has an
    /// in-flight `PlayingAnimation` in `ember2d::play::PlayState`'s own
    /// queue this real step — see `ScriptState::animating`'s own doc
    /// comment (state.rs) for exactly which passes populate it.
    ///
    /// This used to be permanently `false`: `PlayState::update` skips
    /// calling `Simulation::step` at all while the SCHEDULER'S FRONT actor
    /// is still animating (docs/ember2d-phase5.5-plan.md Part 3), so no
    /// script could ever observe that ONE entity's animation in progress —
    /// by the time any script ran again, it had already finished. That
    /// gate is (and stays) per-actor, though, not whole-queue: a step can
    /// still run normally while a DIFFERENT entity's animation lingers in
    /// the queue (an enemy whose `animate_move` outlives its own turn,
    /// say), and `is_animating` can genuinely answer `true` for THAT
    /// entity from any other script that asks — the case this always
    /// honestly could have covered, once the real membership data existed.
    pub fn is_animating(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().animating.contains(&id)
    }
}
