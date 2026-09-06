// play/animation.rs — presentation-side playback of the sim's animation
// queue (Phase 5.5 Part 3, docs/ember2d-phase5.5-plan.md).
//
// `ember2d_sim::scripting::AnimationEvent` is what a script queues
// (`ctx.animate_move`/`animate_flash`/`animate_shake`) — a one-shot request
// with no notion of "how far along it is." `PlayingAnimation` is that
// request plus the real-time progress `PlayState` tracks turning it into
// something drawable: `PlayState::update` advances `elapsed` by
// `frame_delta_time` every frame the queue is non-empty (deliberately real
// wall-clock, not the fixed sim step — see that call site's own comment),
// and `PlayState::render` reads it back out through `RenderOverrides`
// without ever touching `World` — grid state already resolved the instant
// the script called `ctx.set_position`; this is purely what's shown while
// real time passes before the sim is allowed to step again.

use std::collections::HashMap;
use ember2d_sim::math::Vec2;
use ember2d_sim::scripting::AnimationEvent;
use ember2d_sim::world::EntityId;
use crate::renderer::color::Color;

/// Per-entity jitter magnitude for `AnimationEvent::Shake` — unlike camera
/// shake (`ctx.shake_camera`), a script never supplies an intensity here;
/// one fixed value keeps the API surface small (`duration` only) since
/// nothing in the roguelike or any other content needs a stronger/weaker
/// per-entity shake yet. Revisit if a real use case wants control over it.
const ENTITY_SHAKE_INTENSITY: f32 = 0.15;

pub(super) enum PlayingKind {
    Move { from: Vec2, to: Vec2 },
    Flash { color: Color },
    Shake,
}

pub(super) struct PlayingAnimation {
    pub entity: EntityId,
    elapsed: f32,
    duration: f32,
    kind: PlayingKind,
}

impl PlayingAnimation {
    pub fn from_event(ev: AnimationEvent) -> Self {
        match ev {
            AnimationEvent::Move { entity, from, to, duration } => PlayingAnimation { entity, elapsed: 0.0, duration, kind: PlayingKind::Move { from, to } },
            AnimationEvent::Flash { entity, color, duration } => PlayingAnimation { entity, elapsed: 0.0, duration, kind: PlayingKind::Flash { color } },
            AnimationEvent::Shake { entity, duration } => PlayingAnimation { entity, elapsed: 0.0, duration, kind: PlayingKind::Shake },
        }
    }

    /// Advance by one real frame; returns whether this animation is still
    /// playing (false once it's finished and should be dropped). Called
    /// from `PlayState::update`'s `self.animations.retain_mut(...)`.
    pub fn advance(&mut self, frame_delta_time: f32) -> bool {
        self.elapsed += frame_delta_time;
        self.elapsed < self.duration
    }

    /// 0..1 progress through playback, clamped — 1.0 for a non-positive
    /// duration so a script that passes `0.0` applies instantly rather than
    /// dividing by zero or lingering forever.
    fn progress(&self) -> f32 {
        if self.duration <= 0.0 { 1.0 } else { (self.elapsed / self.duration).clamp(0.0, 1.0) }
    }
}

/// Render-time overrides for whatever's currently in `PlayState::animations`
/// — built once per `render()` call so the draw loop can look up a
/// position/tint/shake override per command without touching `World` at
/// all (see this module's header comment for why that matters).
#[derive(Default)]
pub(super) struct RenderOverrides {
    positions: HashMap<EntityId, Vec2>,
    tints: HashMap<EntityId, Color>,
    /// Entity -> fade-out scale (1.0 at the start of the shake, 0.0 once
    /// it's finished) — same decay shape `PlayState`'s own camera shake
    /// already uses (`shake_timer / shake.duration`).
    shakes: HashMap<EntityId, f32>,
}

impl RenderOverrides {
    pub fn build(playing: &[PlayingAnimation]) -> Self {
        let mut overrides = RenderOverrides::default();
        for anim in playing {
            let t = anim.progress();
            match &anim.kind {
                PlayingKind::Move { from, to } => {
                    overrides.positions.insert(anim.entity, Vec2::new(
                        from.x + (to.x - from.x) * t,
                        from.y + (to.y - from.y) * t,
                    ));
                }
                PlayingKind::Flash { color } => { overrides.tints.insert(anim.entity, *color); }
                PlayingKind::Shake => { overrides.shakes.insert(anim.entity, 1.0 - t); }
            }
        }
        overrides
    }

    pub fn position(&self, id: EntityId) -> Option<Vec2> { self.positions.get(&id).copied() }
    pub fn tint(&self, id: EntityId) -> Option<Color> { self.tints.get(&id).copied() }
    /// `Some(scale)` (1.0 fading to 0.0) if `id` is mid-shake — multiply by
    /// `ENTITY_SHAKE_INTENSITY` for the actual jitter magnitude to apply.
    pub fn shake_scale(&self, id: EntityId) -> Option<f32> { self.shakes.get(&id).copied() }
}

pub(super) const SHAKE_INTENSITY: f32 = ENTITY_SHAKE_INTENSITY;
