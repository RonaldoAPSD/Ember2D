// audio.rs — Thin wrapper around the kira audio library.
//
// Scripts call ctx.play_sound("hit.ogg") / ctx.play_music("theme.ogg").
// Those calls queue paths in ScriptEngine; PlayState drains the queues each
// frame and forwards them here. AudioEngine is intentionally simple:
//
//   play_sound  — load + fire once (sound effects)
//   play_music  — load + loop, stopping any previous music track
//   stop_music  — stop current music immediately
//
// AudioEngine holds an Option<AudioManager> so the engine continues to run
// even if the audio device can't be opened (headless CI, no speakers, etc.).
// In that case every method is a silent no-op.

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
    AudioManager, AudioManagerSettings, Decibels, DefaultBackend, Tween, Value,
};

pub struct AudioEngine {
    manager: Option<AudioManager<DefaultBackend>>,
    music_handle: Option<StaticSoundHandle>,
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
        AudioEngine { manager, music_handle: None }
    }

    /// Play a sound file once with a specific volume (0.0 to 1.0+). Silently ignored on error.
    pub fn play_sound(&mut self, path: &str, volume: f64) {
        let Some(ref mut mgr) = self.manager else { return };
        match StaticSoundData::from_file(path) {
            Ok(mut data) => {
                data.settings.volume = Value::Fixed(amplitude_to_decibels(volume));
                let _ = mgr.play(data);
            }
            Err(e) => eprintln!("[audio] play_sound '{}': {}", path, e),
        }
    }

    /// Start looping music from `path`. Stops any currently playing music first.
    pub fn play_music(&mut self, path: &str) {
        self.stop_music();
        let Some(ref mut mgr) = self.manager else { return };
        match StaticSoundData::from_file(path) {
            Ok(data) => {
                let looping = data.loop_region(..);
                match mgr.play(looping) {
                    Ok(handle) => {
                        self.music_handle = Some(handle);
                    }
                    Err(e) => eprintln!("[audio] play_music '{}': {}", path, e),
                }
            }
            Err(e) => eprintln!("[audio] load_music '{}': {}", path, e),
        }
    }

    /// Stop the current music track immediately.
    pub fn stop_music(&mut self) {
        if let Some(ref mut handle) = self.music_handle {
            let _ = handle.stop(Tween::default());
        }
        self.music_handle = None;
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
