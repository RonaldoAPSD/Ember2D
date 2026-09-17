// audio.rs — Thin wrapper around the kira audio library.
//
// Scripts call ctx.play_sound("hit.ogg") / ctx.play_music("theme.ogg").
// Those calls queue paths in ScriptEngine; PlayState drains the queues each
// frame and forwards them here. AudioEngine is intentionally simple:
//
//   play_sound  — load + fire once (sound effects; volume + stereo pan)
//   play_music  — load + loop, stopping any previous music track
//   stop_music  — stop current music immediately
//
// AudioEngine holds an Option<AudioManager> so the engine continues to run
// even if the audio device can't be opened (headless CI, no speakers, etc.).
// In that case every method is a silent no-op.
//
// Step 7.5-11 (docs/ember2d-master-plan.md §5.6, R30): this used to live on
// `PlayState`, so every level transition (`ember2d-app/src/app.rs`'s
// `Transition::ToPlay` handling: `engine.pop_state()` then a brand-new
// `PlayState`) dropped the whole `AudioManager` — closing the real device
// stream — and opened a fresh one for the next level, killing any playing
// music and paying a device re-init cost on every single floor change, not
// just ones that actually changed track. `AudioEngine` now lives on
// `Engine` instead (one device stream for the app's entire lifetime),
// threaded down to whichever `GameState` needs it via
// `UpdateContext::audio` — see that field's own doc comment. `play_music`
// below is now a no-op when asked to (re)start the track already playing,
// specifically so a script's `on_start` calling `ctx.play_music(same_path)`
// on every level transition (the common, natural way to author "make sure
// this track is playing," `demos/roguelike/scripts/player.rhai`'s own
// pattern) no longer restarts the SAME track from the beginning just
// because a new level loaded — before this fix that would have been the
// only way a still-alive `AudioEngine` observably differed from the old
// one-per-level behavior.

use std::collections::HashMap;

// 7B-1 (docs/ember2d-master-plan.md §5.2): kira 0.9 -> 0.12 flattened
// `manager`/`tween` into top-level re-exports (`kira::AudioManager`,
// `kira::Tween`, etc. instead of `kira::manager::AudioManager` — both
// modules are `mod` now, not `pub mod`) and replaced the amplitude-based
// `Volume` enum with `Decibels`, a plain `f32` newtype — see
// `amplitude_to_decibels` below for why `play_sound`'s `volume: f64`
// parameter (still an amplitude ratio, matching the scripting API's own
// documented `0.0..=1.0+` range in `docs/ember2d-scripting-api.md`) needs
// converting rather than a straight rename.
use kira::{
    sound::static_sound::{StaticSoundData, StaticSoundHandle},
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Panning, Tween, Value,
};

pub struct AudioEngine {
    manager: Option<AudioManager<DefaultBackend>>,
    music_handle: Option<StaticSoundHandle>,
    /// The path `play_music` most recently started, if any — what makes
    /// `play_music` idempotent by path (see this module's header comment).
    /// Cleared by `stop_music`, so a script that calls `ctx.stop_music()`
    /// then `ctx.play_music(same_path)` still restarts it, correctly.
    current_music_path: Option<String>,
    /// Step 7.5-11 (R30): `play_sound`/`play_music` used to call
    /// `StaticSoundData::from_file` — a real disk read plus audio decode —
    /// on every single call, including a sound effect triggered every
    /// step (footsteps, repeated hits). `StaticSoundData` is cheap to
    /// clone (its own doc comment: "the audio data is shared among all
    /// clones," an `Arc<[Frame]>` underneath), so decoding once per path
    /// and cloning the cached result for every subsequent play is strictly
    /// cheaper with no behavior change — each clone still gets its own
    /// `settings` (volume/panning) set independently before playing.
    sound_cache: HashMap<String, StaticSoundData>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(m) => Some(m),
            Err(e) => {
                eprintln!("[audio] init failed: {}", e);
                None
            }
        };
        AudioEngine {
            manager,
            music_handle: None,
            current_music_path: None,
            sound_cache: HashMap::new(),
        }
    }

    /// Decodes `path` on first request, caching the result; every later
    /// request for the same path clones the cached `StaticSoundData`
    /// instead of touching the filesystem again. `None` on a decode
    /// failure (already logged here, once per failing call — a
    /// consistently-missing file logs every time it's requested, same as
    /// before this cache existed).
    fn load_cached(&mut self, path: &str) -> Option<StaticSoundData> {
        if let Some(data) = self.sound_cache.get(path) {
            return Some(data.clone());
        }
        match StaticSoundData::from_file(path) {
            Ok(data) => {
                self.sound_cache.insert(path.to_string(), data.clone());
                Some(data)
            }
            Err(e) => {
                eprintln!("[audio] load '{}': {}", path, e);
                None
            }
        }
    }

    /// Play a sound file once with a specific volume (0.0 to 1.0+) and
    /// stereo pan (-1.0 hard left, 0.0 center, 1.0 hard right — clamped).
    /// Silently ignored on error or with no audio device.
    pub fn play_sound(&mut self, path: &str, volume: f64, pan: f64) {
        if self.manager.is_none() {
            return;
        }
        let Some(mut data) = self.load_cached(path) else { return };
        data.settings.volume = Value::Fixed(amplitude_to_decibels(volume));
        data.settings.panning = Value::Fixed(Panning(pan.clamp(-1.0, 1.0) as f32));
        if let Some(ref mut mgr) = self.manager {
            let _ = mgr.play(data);
        }
    }

    /// Start looping music from `path`. A no-op if `path` is already the
    /// current track (see this module's header comment for why); otherwise
    /// stops whatever was playing first. `current_music_path` records the
    /// track this was ASKED to play as soon as that's established — before
    /// checking whether a device exists or `path` actually decodes — so a
    /// script repeatedly asking for a path that fails to load (a typo, a
    /// missing file) logs that failure once, not on every single call, and
    /// so the idempotence check above is exercisable with no real audio
    /// device at all (see `audio.rs`'s own test module).
    pub fn play_music(&mut self, path: &str) {
        if self.current_music_path.as_deref() == Some(path) {
            return;
        }
        self.stop_music();
        self.current_music_path = Some(path.to_string());
        if self.manager.is_none() {
            return;
        }
        let Some(data) = self.load_cached(path) else { return };
        let looping = data.loop_region(..);
        if let Some(ref mut mgr) = self.manager {
            match mgr.play(looping) {
                Ok(handle) => self.music_handle = Some(handle),
                Err(e) => eprintln!("[audio] play_music '{}': {}", path, e),
            }
        }
    }

    /// Stop the current music track immediately.
    pub fn stop_music(&mut self) {
        if let Some(ref mut handle) = self.music_handle {
            let _ = handle.stop(Tween::default());
        }
        self.music_handle = None;
        self.current_music_path = None;
    }
}

impl Default for AudioEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// `play_sound`'s `volume` is an amplitude ratio (`0.0..=1.0+`, matching
/// `docs/ember2d-scripting-api.md`'s documented range for `ctx.play_sound`)
/// — kira 0.12's `Decibels` is a logarithmic scale, so this converts rather
/// than wrapping the value directly. `0.0` (and anything at or below it)
/// maps to `Decibels::SILENCE` rather than `-infinity` from `log10(0.0)`,
/// matching kira's own `Decibels::SILENCE` "sounds silent below this"
/// convention. Presentation-only (audio has no bearing on replay
/// determinism), so the `log10` here isn't the transcendental-math
/// restriction that applies inside `ember2d-sim` (CLAUDE.md's Determinism
/// section).
fn amplitude_to_decibels(amplitude: f64) -> Decibels {
    if amplitude <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * (amplitude as f32).log10())
    }
}

// R30 regression coverage (Step 7.5-11, docs/ember2d-master-plan.md §5.6):
// `play_music`'s idempotence-by-path is what makes `AudioEngine` surviving
// a level transition actually observable — without it, an `on_start` that
// unconditionally calls `ctx.play_music(same_path)` on every level (the
// natural way to author "make sure this track is playing") would restart
// the track from the beginning every transition regardless of whether the
// device itself survived. These tests never touch a real audio device
// (nonexistent "*.ogg" paths, and `current_music_path` is tracked before
// `self.manager` is ever consulted — see `play_music`'s own doc comment)
// so they're deterministic whether or not this machine has one.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_music_with_the_same_path_twice_is_a_no_op() {
        let mut audio = AudioEngine::new();
        audio.play_music("a.ogg");
        assert_eq!(audio.current_music_path.as_deref(), Some("a.ogg"));
        audio.play_music("a.ogg");
        assert_eq!(
            audio.current_music_path.as_deref(),
            Some("a.ogg"),
            "asking for the already-playing track again must be a no-op, not a restart"
        );
    }

    #[test]
    fn play_music_with_a_different_path_switches_tracks() {
        let mut audio = AudioEngine::new();
        audio.play_music("a.ogg");
        audio.play_music("b.ogg");
        assert_eq!(
            audio.current_music_path.as_deref(),
            Some("b.ogg"),
            "a genuinely different path must actually switch, not be swallowed by the no-op guard"
        );
    }

    #[test]
    fn stop_music_clears_the_current_track_so_the_same_path_restarts_it() {
        let mut audio = AudioEngine::new();
        audio.play_music("a.ogg");
        audio.stop_music();
        assert_eq!(
            audio.current_music_path, None,
            "stop_music must clear the tracked path, not just the handle"
        );
        audio.play_music("a.ogg");
        assert_eq!(
            audio.current_music_path.as_deref(),
            Some("a.ogg"),
            "the same path must restart cleanly after an explicit stop_music"
        );
    }
}
