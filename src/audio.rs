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

use kira::{
    manager::{backend::DefaultBackend, AudioManager, AudioManagerSettings},
    sound::static_sound::{StaticSoundData, StaticSoundHandle},
    tween::Tween,
};

pub struct AudioEngine {
    manager:      Option<AudioManager<DefaultBackend>>,
    music_handle: Option<StaticSoundHandle>,
}

impl AudioEngine {
    pub fn new() -> Self {
        let manager = match AudioManager::<DefaultBackend>::new(AudioManagerSettings::default()) {
            Ok(m)  => Some(m),
            Err(e) => { eprintln!("[audio] init failed: {}", e); None }
        };
        AudioEngine { manager, music_handle: None }
    }

    /// Play a sound file once (fire-and-forget). Silently ignored on error.
    pub fn play_sound(&mut self, path: &str) {
        let Some(ref mut mgr) = self.manager else { return };
        match StaticSoundData::from_file(path) {
            Ok(data) => { let _ = mgr.play(data); }
            Err(e)   => eprintln!("[audio] play_sound '{}': {}", path, e),
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
                    Ok(handle) => { self.music_handle = Some(handle); }
                    Err(e)     => eprintln!("[audio] play_music '{}': {}", path, e),
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
