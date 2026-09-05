// components/animator.rs — Per-entity animation playback state (Phase 3,
// Step 3c of docs/ember2d-refactor-plan.md).
//
// A Sprite says WHAT to show (`SpriteSource::Clip { name }` points at a
// registered `AnimationClip`); an `Animator` says WHERE PLAYBACK CURRENTLY
// IS in that clip. Splitting these out of `Sprite` itself (where the old
// `frames`/`frame_rate`/`frame_timer` lived) means static tiles — most of a
// tilemap — carry zero animation state instead of three fields they never
// use.
//
// `AnimationClip`/`ClipFrames` describe the shared, named *definition*
// (script-registered via `ctx.register_clip`, held in
// `PlayState::clips: BTreeMap<String, AnimationClip>`) — NOT stored per
// entity and NOT part of `World`'s serialized state, unlike `Sprite`.
//
// **Corrected in Step 5c** (docs/ember2d-phase5-plan.md, defect D17): this
// used to say clips "don't need to survive save/load: a script re-registers
// them every on_start" — true only as long as `on_start` actually re-runs on
// load, which it deliberately does NOT (re-running it would also re-run
// every script's per-entity init, resetting things like `enemy_rat.rhai`'s
// unconditional `hp_<id>` write back to full health). So `SaveState` now
// carries `clips` explicitly, alongside `globals`, and restores them
// directly instead of relying on `on_start` to rebuild them.

use crate::math::Rect;
use serde::{Serialize, Deserialize};

/// Per-entity playback state for a `SpriteSource::Clip`-sourced sprite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animator {
    /// Name of the `AnimationClip` (`PlayState::clips`) this plays. Looked
    /// up by name each time playback advances rather than resolved once,
    /// since a script can register a new clip under an existing name at
    /// any time (e.g. hot-reloading animation data).
    pub clip: String,
    pub frame: usize,
    pub elapsed: f32,
    pub playing: bool,
    pub speed: f32,
    /// Set by `play_clip_once` (script API) to force this particular
    /// playback to stop at its last frame even if the registered clip
    /// itself loops — `play_clip` leaves this false and simply defers to
    /// the clip's own `looping` flag. Lets one clip serve both a looping
    /// idle animation and a one-shot use (e.g. an attack swing) without
    /// registering it twice.
    #[serde(default)]
    pub oneshot: bool,
    /// True for exactly the one tick a non-looping run reaches its last
    /// frame — what `clip_finished(id)` reads. Not meaningful to persist
    /// (a save always resumes mid-tick, never "the instant after"), so
    /// this is skipped on both sides of (de)serialization rather than
    /// carrying `#[serde(default)]`'s implication that a stored value ever
    /// exists.
    #[serde(skip)]
    pub just_finished: bool,
}

impl Animator {
    /// Start playing `clip` from frame 0, looping, at normal speed.
    pub fn new(clip: impl Into<String>) -> Self {
        Animator { clip: clip.into(), frame: 0, elapsed: 0.0, playing: true, speed: 1.0, oneshot: false, just_finished: false }
    }

    /// Advance playback by `delta_time`, using `clip`'s fps and (unless
    /// `oneshot` overrides it) looping flag. Sets `just_finished` for
    /// exactly the tick a non-looping run reaches its last frame, and
    /// clears it otherwise — callers tick every `Animator` once per frame,
    /// so this doubles as the "just" edge `clip_finished(id)` reads.
    pub fn advance(&mut self, clip: &AnimationClip, delta_time: f32) {
        self.just_finished = false;
        if !self.playing || clip.fps <= 0.0 { return; }
        let frame_count = clip.frame_count();
        if frame_count == 0 { return; }
        let looping = clip.looping && !self.oneshot;
        self.elapsed += delta_time * self.speed;
        let frame_duration = 1.0 / clip.fps;
        while self.elapsed >= frame_duration {
            self.elapsed -= frame_duration;
            self.frame += 1;
            if self.frame >= frame_count {
                if looping {
                    self.frame = 0;
                } else {
                    self.frame = frame_count - 1;
                    self.playing = false;
                    self.just_finished = true;
                    break;
                }
            }
        }
    }
}

/// A shared, named animation definition. `Serialize`/`Deserialize` (Step 5c,
/// docs/ember2d-phase5-plan.md) so `SaveState` can carry `PlayState::clips`
/// through a mid-run save/load — see this module's header comment for why
/// that's necessary now (defect D17). Possible now that `ClipFrames::Rects`
/// holds a plain `String` path rather than a `TextureId` (Step 5a); a
/// `TextureId` is deliberately not serializable (see its own doc comment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnimationClip {
    pub frames: ClipFrames,
    pub fps: f32,
    pub looping: bool,
}

/// The old per-Sprite `frames: Vec<char>` migrates into `Glyphs` here.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClipFrames {
    /// Cycles through glyphs — a torch's "*+#" flicker, a spinning coin.
    Glyphs { frames: Vec<char> },

    /// Cycles through pixel-space sub-rects of one texture (a sprite
    /// sheet). Defined for shape-completeness with `SpriteSource::Texture`'s
    /// own `src: Option<Rect>` — nothing constructs this yet. No demo
    /// content needs sheet animation, and there's no way to author texture
    /// rects until Phase 8's asset tooling exists; `register_clip`'s script
    /// API only builds `Glyphs` clips for now.
    ///
    /// `texture` is a path, not a `TextureId` (Step 5a,
    /// docs/ember2d-phase5-plan.md) — matching `SpriteSource::Texture`'s own
    /// path-not-handle convention (see that type's doc comment): ids are
    /// runtime-assigned and don't survive save/load, and a sim-side type
    /// can't depend on the renderer's handle type at all once the Phase 5
    /// workspace split (§5.5) lands.
    Rects { texture: String, frames: Vec<Rect> },
}

impl AnimationClip {
    pub fn frame_count(&self) -> usize {
        match &self.frames {
            ClipFrames::Glyphs { frames } => frames.len(),
            ClipFrames::Rects { frames, .. } => frames.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph_clip(frames: &str, fps: f32, looping: bool) -> AnimationClip {
        AnimationClip { frames: ClipFrames::Glyphs { frames: frames.chars().collect() }, fps, looping }
    }

    #[test]
    fn advance_wraps_to_frame_zero_when_the_clip_loops() {
        let clip = glyph_clip("abc", 10.0, true); // 0.1s per frame
        let mut a = Animator::new("flicker");
        a.advance(&clip, 0.35); // 3 whole frames: 0->1->2->0
        assert_eq!(a.frame, 0);
        assert!(a.playing, "a looping clip must keep playing after wrapping");
        assert!(!a.just_finished);
    }

    #[test]
    fn advance_stops_on_the_last_frame_when_the_clip_does_not_loop() {
        let clip = glyph_clip("abc", 10.0, false);
        let mut a = Animator::new("swing");
        a.advance(&clip, 0.35); // would wrap past 'c' if looping
        assert_eq!(a.frame, 2, "a non-looping clip must clamp to its last frame, not wrap");
        assert!(!a.playing, "a finished non-looping clip must stop playing");
        assert!(a.just_finished, "the exact tick playback reaches the end must report just_finished");
    }

    #[test]
    fn just_finished_is_only_true_for_the_one_tick_playback_ends() {
        let clip = glyph_clip("ab", 10.0, false);
        let mut a = Animator::new("swing");
        a.advance(&clip, 0.25); // overflows past frame 1, clamping and finishing
        assert!(a.just_finished);
        a.advance(&clip, 0.1); // already stopped; nothing changes this tick
        assert!(!a.just_finished, "just_finished must not linger into the following tick");
    }

    #[test]
    fn oneshot_overrides_a_looping_clips_own_looping_flag() {
        let clip = glyph_clip("ab", 10.0, true); // registered as looping
        let mut a = Animator::new("swing");
        a.oneshot = true; // what play_clip_once sets
        a.advance(&clip, 0.25); // 2.5 frames -> would land back on frame 0 if looping
        assert_eq!(a.frame, 1, "oneshot must clamp to the last frame despite clip.looping");
        assert!(!a.playing);
        assert!(a.just_finished);
    }

    #[test]
    fn advance_is_a_no_op_when_not_playing_or_the_clip_has_no_frames() {
        let clip = glyph_clip("abc", 10.0, true);
        let mut stopped = Animator::new("idle");
        stopped.playing = false;
        stopped.advance(&clip, 1.0);
        assert_eq!(stopped.frame, 0, "a stopped Animator must not advance");

        let empty_clip = glyph_clip("", 10.0, true);
        let mut a = Animator::new("empty");
        a.advance(&empty_clip, 1.0);
        assert_eq!(a.frame, 0, "a clip with zero frames must not advance or panic");
    }
}
