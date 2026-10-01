// play/outcome.rs — folding a `Simulation` step's outcome into PlayState's
// presentation fields, and playing the audio it queued.
//
// Moved out of play.rs unchanged at Step 9-1 (docs/ember2d-master-plan.md
// §5.8), which play.rs's own scene/flow additions would otherwise have
// pushed past CLAUDE.md's 750-line limit — plus the one 9-1 addition here,
// turning a script's `FlowRequest` into a `Transition`.

use rand::Rng;

use super::{Particle, PlayState, PlayingAnimation};
use crate::audio::AudioEngine;
use crate::engine::Transition;
use ember2d_sim::scripting::FlowRequest;

impl PlayState {
    /// Folds a `Simulation` step's outcome into this state's own
    /// presentation fields — shared between `update` and `late_update`
    /// since both call into `Simulation` and get one of these back.
    /// `turn_triggered` is handled by the caller directly (it's the one
    /// field `update` needs to hand back through `UpdateContext`, not
    /// something presentation reacts to).
    pub(super) fn apply_outcome(&mut self, outcome: ember2d_sim::simulation::StepOutcome) {
        // Sticky until a script sets a new one — matches
        // `docs/ember2d-scripting-api.md`'s "Camera" section ("setting the
        // camera overrides follow until cleared"): a step with nothing new
        // to say just leaves this alone.
        if outcome.camera_override.is_some() {
            self.camera_override = outcome.camera_override;
        }
        if let Some(shake) = outcome.shake_state {
            self.shake_state = Some(shake);
            self.shake_timer = shake.duration;
        }
        for req in outcome.particles {
            let vx = self.rng.gen_range(-5.0..5.0);
            let vy = self.rng.gen_range(-5.0..5.0);
            let life = self.rng.gen_range(0.2..0.8);
            self.particles.push(Particle {
                x: req.x,
                y: req.y,
                vx,
                vy,
                glyph: req.glyph,
                fg: req.fg,
                life,
            });
        }
        if let Some(next) = outcome.pending_level {
            self.pending_transition = Some(Transition::ToPlay(next));
        }
        if let Some(state) = outcome.pending_load {
            self.pending_transition = Some(Transition::LoadGame(state));
        }
        // Step 9-1 (docs/ember2d-master-plan.md §5.8): a script's
        // `quit_game`/`return_to_editor` — what the Rust pause menu's own
        // Quit/Back to Editor rows used to do directly.
        match outcome.flow {
            Some(FlowRequest::Quit) => self.pending_transition = Some(Transition::Quit),
            Some(FlowRequest::ToEditor) => self.pending_transition = Some(Transition::ToEditor),
            None => {}
        }
        for ev in outcome.animations {
            self.animations.push(PlayingAnimation::from_event(ev));
        }
        self.script_log.extend(outcome.logs);
    }

    /// `audio` is `Engine`'s own long-lived `AudioEngine` (Step 7.5-11,
    /// docs/ember2d-master-plan.md §5.6, R30), passed in via
    /// `UpdateContext::audio` rather than owned here — see that field's own
    /// doc comment for why.
    pub(super) fn flush_audio(&mut self, audio: &mut AudioEngine) {
        let reqs = self.sim.take_audio_requests();
        for path in reqs.sounds {
            audio.play_sound(&path, 1.0, 0.0);
        }
        // Step 7.5-11: stereo pan from the camera's horizontal offset, on
        // top of the pre-existing distance falloff — both presentation-only
        // (a script's `play_sound_at` call carries a world position, never
        // read back), sharing the same `max_dist` so a sound at the edge of
        // audible range also reaches full hard-left/hard-right.
        let cam_pos = self.camera.position;
        let max_dist = 20.0f32;
        for (path, x, y) in reqs.spatial_sounds {
            let dx = x - cam_pos.x;
            let dy = y - cam_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            let volume = (1.0 - (dist / max_dist)).clamp(0.0, 1.0);
            if volume > 0.01 {
                let pan = (dx / max_dist).clamp(-1.0, 1.0);
                audio.play_sound(&path, volume as f64, pan as f64);
            }
        }
        if reqs.stop_music {
            audio.stop_music();
        }
        if let Some(path) = reqs.music {
            audio.play_music(&path);
        }
    }
}
