// play.rs — Play mode: run a level built from a LevelData file.
//
// Phase 5.5 (docs/ember2d-phase5.5-plan.md Part 2): the actual simulation —
// scripts, the turn scheduler, World mutation, the whole
// on_input/on_update/on_turn/on_collide orchestration — moved into
// `ember2d_sim::simulation::Simulation`, which `PlayState` now owns and
// delegates to on every `update`/`late_update`/`on_start` call. What's left
// here is genuinely presentation: the camera (follow/lerp/clamp), particles,
// camera shake, the FPS counter and F3 debug overlay, audio playback, and
// drawing. See `simulation.rs`'s own header comment (`ember2d-sim`) for the
// full reasoning — why this split, and why `sim.rs`/`GameState`/
// `UpdateContext`/the editor are all untouched by it.

mod animation;
mod render;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use animation::{PlayingAnimation, RenderOverrides};
use crate::camera::Camera;
use ember2d_sim::components::{AnimationClip, ClipFrames, SpriteSource};
use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
pub use render::{DrawCommand, DrawList, Space};
use render::{camera_shake_jitter, draw_debug_overlay, draw_hud_queue, draw_recent_log, in_viewport, sprite_size};
use ember2d_sim::event::EventBus;
use crate::input::Key;
use ember2d_sim::level::LevelData;
use ember2d_sim::math::Vec2;
use crate::renderer::color::Color;
use crate::audio::AudioEngine;
use ember2d_sim::scripting::LogEntry;
use ember2d_sim::simulation::{Simulation, StepInput};
use ember2d_sim::world::{EntityId, World};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

// `resolve_exit_path` moved into `ember2d-sim`'s `simulation` module (Phase
// 5.5, docs/ember2d-phase5.5-plan.md Part 2) — every real caller
// (`Simulation::do_on_start`, its exit-tile resolution) is sim-side now.
// Re-exported here so `ember2d-editor/src/editor/impl_state.rs`'s existing
// `use ember2d::play::resolve_exit_path` import stays valid unchanged.
pub use ember2d_sim::simulation::resolve_exit_path;

// Draw-list types (Space, DrawCommand, DrawList) and rendering support
// (in_viewport, sprite_size) live in play/render.rs — see that file's
// header comment for why they were split out.

// ── Particles ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Particle {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub glyph: char,
    pub fg: Color,
    pub life: f32,
}

// ── PauseMenuState ────────────────────────────────────────────────────────────

pub struct PauseMenuState {
    options: Vec<String>,
    selected: usize,
    pending_transition: Option<Transition>,
}

impl PauseMenuState {
    pub fn new() -> Self {
        Self {
            options: vec!["Resume".to_string(), "Back to Editor".to_string(), "Quit Game".to_string()],
            selected: 0,
            pending_transition: None,
        }
    }
}

impl GameState for PauseMenuState {
    fn update(&mut self, ctx: UpdateContext) {
        if ctx.input.just_pressed(Key::Up)    { self.selected = self.selected.saturating_sub(1); }
        if ctx.input.just_pressed(Key::Down)  { if self.selected + 1 < self.options.len() { self.selected += 1; } }

        if ctx.input.just_pressed(Key::Enter) {
            match self.selected {
                0 => self.pending_transition = Some(Transition::Pop),
                1 => self.pending_transition = Some(Transition::ToEditor),
                2 => self.pending_transition = Some(Transition::Quit),
                _ => {}
            }
        }

        if ctx.input.just_pressed(Key::Escape) {
            self.pending_transition = Some(Transition::Pop);
        }
    }

    fn render(&mut self, ctx: RenderContext) {
        let sw = ctx.renderer.width;
        let sh = ctx.renderer.height;
        let w = 30;
        let h = 8;
        let x = (sw - w) / 2;
        let y = (sh - h) / 2;

        crate::ui::Panel::new(x, y, w, h)
            .with_title(" PAUSED ")
            .with_colors(Color::White, Color::DarkBlue)
            .draw(ctx.renderer);

        for (i, opt) in self.options.iter().enumerate() {
            let fg = if i == self.selected { Color::Yellow } else { Color::Grey };
            let bg = if i == self.selected { Color::DarkGrey } else { Color::DarkBlue };
            let prefix = if i == self.selected { "> " } else { "  " };
            ctx.renderer.draw_str(x + 2, y + 2 + i, &format!("{}{}", prefix, opt), fg, bg);
        }
    }

    fn take_transition(&mut self) -> Option<Transition> { self.pending_transition.take() }
}

// ── PlayState ─────────────────────────────────────────────────────────────────

// ShakeState lives in scripting/types.rs (ember2d-sim) — it's a
// scripting-facing value (ctx.shake_camera queues one, StepOutcome carries
// it out), not something PlayState itself defines the shape of.
pub use ember2d_sim::scripting::ShakeState;

pub struct PlayState {
    fps: f32,
    /// Toggled by F3. Engine chrome (level name, exact position, backend,
    /// FPS) is a debug tool switched on during play, not permanent
    /// always-on UI — see docs/HANDOFF.md's Step 4g.
    show_debug: bool,
    /// Owns the level, scripting, and turn scheduler — everything a
    /// deterministic step actually needs (Phase 5.5,
    /// docs/ember2d-phase5.5-plan.md Part 2). Everything else on this
    /// struct is presentation.
    sim: Simulation,
    pending_transition: Option<Transition>,
    audio: AudioEngine,
    script_log: Vec<LogEntry>,
    pub camera_override: Option<Vec2>,
    pub shake_state: Option<ShakeState>,
    pub shake_timer: f32,
    /// Owns world<->screen conversion for this level (Step 2e). Its
    /// viewport/origin fields are refreshed every `update()` call — see
    /// `script_camera_origin` for the one place that reads it back out.
    pub camera: Camera,
    pub particles: Vec<Particle>,
    /// In-flight visual playback for the animation queue (Phase 5.5 Part 3,
    /// docs/ember2d-phase5.5-plan.md) — see `apply_outcome` for how a
    /// script's `ctx.animate_move`/etc. requests land here, `update` for how
    /// the sim is gated on this per actor (D20 fix, not globally — see that
    /// defect's note in docs/ember2d-refactor-plan.md §3), and
    /// `render`/`animation.rs` for how it's drawn without ever touching
    /// `World`. Multiple entities' animations coexist here freely and drain
    /// independently — `apply_outcome` just pushes, never assumes empty.
    animations: Vec<PlayingAnimation>,
    /// Presses/clicks/gamepad-button-presses claimed by `ember2d::sim::step`
    /// (untouched, generic per-frame pump)'s unconditional `consume_step()`
    /// calls during a frame where the scheduler's front actor still had an
    /// animation of its own in flight and `update` never called
    /// `Simulation::step` at all — found live (D19,
    /// docs/ember2d-refactor-plan.md §3) as "player movement feels bad while
    /// an enemy is animating": `consume_step()` claims and clears a buffered
    /// press whether or not anything downstream reads it, so a tap made
    /// mid-animation was silently discarded rather than merely delayed —
    /// see `update`'s own comment on the fix. Only `pressed` needs carrying
    /// forward, never `held` (that's live physical state, always correct
    /// fresh on whichever frame finally reads it). D20's per-actor gate
    /// (below) made these frames far rarer than they used to be, but not
    /// impossible — a player holding a direction key fast enough to catch
    /// up to an enemy still finishing its own previous animation still hits
    /// this path, so the buffer stays exactly as necessary as it always was.
    buffered_pressed: BTreeSet<String>,
    buffered_mouse_pressed: (bool, bool),
    buffered_gamepad_pressed: HashSet<(usize, String)>,
    /// Drives particle velocity/life at spawn time (defect D3) — the one
    /// draw site here at a deterministic once-per-sim-step cadence
    /// (`apply_outcome`). Seeded once from the level seed, never
    /// reallocated from OS entropy. R15 (7A-5): shake jitter used to draw
    /// from this same stream inside `render` (once per real frame, making
    /// particle spawns frame-rate dependent) — shake now uses `render_rng`.
    rng: SmallRng,
    /// R15 (7A-5): shake jitter's own stream, independent of `rng` so
    /// calling `render` a different number of times never changes `rng`.
    render_rng: SmallRng,
    /// Source-texture pixels per world unit, for a `Sprite` whose `size` is
    /// `None` (Step 3b). Defaults to `ProjectData::pixels_per_unit`'s own
    /// default (8.0); `set_pixels_per_unit` lets a caller that actually has
    /// the project's settings (`app.rs`) override it after construction.
    pixels_per_unit: f32,
}

/// A different constant offset from the level's seed for `PlayState::rng`
/// than the one `Simulation`'s `ScriptEngine` seeds from directly, so
/// script randomness and particle/shake randomness are two independent
/// deterministic streams instead of mirroring each other's sequence.
const PLAYSTATE_RNG_SEED_OFFSET: u64 = 0x9E3779B97F4A7C15; // splitmix64's golden-ratio constant

/// R15 (7A-5): `render_rng`'s offset — a third independent stream, same
/// reasoning as `PLAYSTATE_RNG_SEED_OFFSET`.
const RENDER_RNG_SEED_OFFSET: u64 = 0x2545F4914F6CDD1D;

impl PlayState {
    fn new_with_sim(sim: Simulation, seed: u64) -> Self {
        PlayState {
            fps:                0.0,
            show_debug:         false,
            sim,
            pending_transition: None,
            audio:              AudioEngine::new(),
            script_log:         Vec::new(),
            camera_override:    None,
            shake_state:        None,
            shake_timer:        0.0,
            camera:             Camera::new(0.0, 0.0), // real dimensions set every update()
            particles:          Vec::new(),
            animations:         Vec::new(),
            buffered_pressed:   BTreeSet::new(),
            buffered_mouse_pressed: (false, false),
            buffered_gamepad_pressed: HashSet::new(),
            rng:                SmallRng::seed_from_u64(seed.wrapping_add(PLAYSTATE_RNG_SEED_OFFSET)),
            render_rng:         SmallRng::seed_from_u64(seed ^ RENDER_RNG_SEED_OFFSET),
            pixels_per_unit:    crate::project::default_pixels_per_unit(),
        }
    }

    pub fn from_level(data: LevelData, _persistent: BTreeMap<String, rhai::Dynamic>) -> Self {
        let seed = data.seed;
        Self::new_with_sim(Simulation::new(data), seed)
    }

    /// `globals`/`clips` come from the loaded `SaveState` — defect D17 fix
    /// (Step 5c, docs/ember2d-phase5-plan.md); `Simulation::from_save`
    /// handles restoring them and skipping a re-run of `on_start`. See that
    /// constructor's own doc comment for why re-running `on_start` isn't
    /// the fix — `turn_number`/`scheduler` (R7, 7A-3) get the same treatment.
    pub fn from_save(level_data: LevelData, _persistent: BTreeMap<String, rhai::Dynamic>, globals: BTreeMap<String, rhai::Dynamic>, clips: BTreeMap<String, AnimationClip>, turn_number: u64, scheduler: Vec<(EntityId, u64)>) -> Self {
        let seed = level_data.seed;
        Self::new_with_sim(Simulation::from_save(level_data, globals, clips, turn_number, scheduler), seed)
    }

    /// Override the default `pixels_per_unit` with the owning project's
    /// actual setting. Optional — callers that don't have a `ProjectData`
    /// handy (tests, anything constructing a level standalone) just keep
    /// the default.
    pub fn set_pixels_per_unit(&mut self, value: f32) {
        self.pixels_per_unit = value;
    }

    /// Forwarding accessors onto the owned `Simulation` — kept public
    /// because `tests/save_load_globals.rs` (an external integration test
    /// using only `ember2d::prelude`) reads a script-set global directly to
    /// verify D17's save/load round trip. Nothing inside this crate needs
    /// these anymore; the field itself lives on `Simulation` now.
    pub fn globals(&self) -> &BTreeMap<String, rhai::Dynamic> { self.sim.globals() }
    pub fn clips(&self) -> &BTreeMap<String, AnimationClip> { self.sim.clips() }

    pub fn take_log(&mut self) -> Vec<LogEntry> { std::mem::take(&mut self.script_log) }

    /// Folds a `Simulation` step's outcome into this state's own
    /// presentation fields — shared between `update` and `late_update`
    /// since both call into `Simulation` and get one of these back.
    /// `turn_triggered` is handled by the caller directly (it's the one
    /// field `update` needs to hand back through `UpdateContext`, not
    /// something presentation reacts to).
    fn apply_outcome(&mut self, outcome: ember2d_sim::simulation::StepOutcome) {
        // Sticky until a script sets a new one — matches
        // `docs/ember2d-scripting-api.md`'s "Camera" section ("setting the
        // camera overrides follow until cleared"): a step with nothing new
        // to say just leaves this alone.
        if outcome.camera_override.is_some() { self.camera_override = outcome.camera_override; }
        if let Some(shake) = outcome.shake_state {
            self.shake_state = Some(shake);
            self.shake_timer = shake.duration;
        }
        for req in outcome.particles {
            let vx = self.rng.gen_range(-5.0..5.0);
            let vy = self.rng.gen_range(-5.0..5.0);
            let life = self.rng.gen_range(0.2..0.8);
            self.particles.push(Particle { x: req.x, y: req.y, vx, vy, glyph: req.glyph, fg: req.fg, life });
        }
        if let Some(next) = outcome.pending_level { self.pending_transition = Some(Transition::ToPlay(next)); }
        if let Some(state) = outcome.pending_load { self.pending_transition = Some(Transition::LoadGame(state)); }
        for ev in outcome.animations { self.animations.push(PlayingAnimation::from_event(ev)); }
        self.script_log.extend(outcome.logs);
    }

    fn flush_audio(&mut self) {
        let reqs = self.sim.take_audio_requests();
        for path in reqs.sounds { self.audio.play_sound(&path, 1.0); }
        let cam_pos = self.camera.position;
        let max_dist = 20.0f32;
        for (path, x, y) in reqs.spatial_sounds {
            let dx = x - cam_pos.x;
            let dy = y - cam_pos.y;
            let dist = (dx*dx + dy*dy).sqrt();
            let volume = (1.0 - (dist / max_dist)).clamp(0.0, 1.0);
            if volume > 0.01 { self.audio.play_sound(&path, volume as f64); }
        }
        if reqs.stop_music { self.audio.stop_music(); }
        if let Some(path) = reqs.music { self.audio.play_music(&path); }
    }

    /// World position scripts see as the camera's origin — `get_camera_x/y`
    /// and (added to the mouse's screen cell) `get_mouse_world_x/y` both key
    /// off this. Rounded to whole cells, matching the precision scripts have
    /// always seen; only the render path benefits from `self.camera`'s true
    /// float precision. A dedicated method so `update`/`late_update` don't
    /// hand-duplicate this formula.
    fn script_camera_origin(&self) -> Vec2 {
        let tl = self.camera.top_left();
        Vec2::new(tl.x.round(), tl.y.round())
    }

}

impl GameState for PlayState {
    fn on_start(&mut self, world: &mut World, _events: &mut EventBus, viewport_width: usize, viewport_height: usize, persistent: &mut BTreeMap<String, rhai::Dynamic>) {
        let logs = self.sim.on_start(world, viewport_width, viewport_height, persistent);
        self.script_log.extend(logs);
    }

    fn update(&mut self, ctx: UpdateContext) {
        // R16 (7A-5): `ctx.elapsed` deliberately not bound — see sim_elapsed below.
        let UpdateContext { world, input, mouse, delta_time, frame_delta_time, viewport_width, viewport_height, turn_triggered, persistent, .. } = ctx;

        // FPS counter and shake-timer decay are presentation only (the F3
        // debug overlay, the render-time shake jitter) — never read back by
        // scripts — so they use frame_delta_time (real wall-clock), not
        // delta_time (the fixed sim step). See UpdateContext::frame_delta_time's
        // own doc comment.
        if frame_delta_time > 0.0 { self.fps = self.fps * 0.9 + (1.0 / frame_delta_time) * 0.1; }
        if self.shake_timer > 0.0 {
            self.shake_timer -= frame_delta_time;
            if self.shake_timer <= 0.0 { self.shake_state = None; }
        }

        if input.just_pressed(Key::Escape) {
            self.pending_transition = Some(Transition::Push(Box::new(PauseMenuState::new())));
            return;
        }

        if input.just_pressed(Key::F3) { self.show_debug = !self.show_debug; }

        let mut target_cam = self.sim.camera_entity().map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
        if let Some(over) = self.camera_override { target_cam = over; }

        // Step 4g: the world gets the full viewport now — the two
        // hardcoded HUD bars that used to reserve row 0 and the last row
        // are gone; engine chrome is a toggleable F3 overlay drawn on top
        // instead of reserving space.
        let level = self.sim.level();
        let game_h = (viewport_height as i32).max(1) as f32;
        let half_w = viewport_width as f32 / 2.0;
        let half_h = game_h / 2.0;

        let min_x = half_w;
        let max_x = (level.width as f32 - half_w).max(min_x);
        let min_y = half_h;
        let max_y = (level.height as f32 - half_h).max(min_y);

        target_cam.x = target_cam.x.clamp(min_x, max_x);
        target_cam.y = target_cam.y.clamp(min_y, max_y);

        if self.camera.position == Vec2::ZERO { self.camera.position = target_cam; }
        else {
            // Presentation, not simulation — frame_delta_time (real
            // wall-clock), not delta_time, so the camera stays visually
            // smooth regardless of the sim's own clock. `exp()` is a named
            // cross-platform determinism hazard (refactor plan §5.2 H2) —
            // one more reason camera position must never be read back into
            // anything a script or the sim depends on.
            let lerp_speed = 5.0;
            self.camera.position = self.camera.position + (target_cam - self.camera.position) * (1.0 - (-lerp_speed * frame_delta_time).exp());
        }
        self.camera.viewport_width = viewport_width as f32;
        self.camera.viewport_height = game_h;
        self.camera.viewport_origin = Vec2::ZERO;
        self.camera.zoom = 1.0; // Phase 2 doesn't add a scripted zoom control yet

        // Phase 5.5 Part 3 (docs/ember2d-phase5.5-plan.md) gated the whole
        // scheduler on the WHOLE animation queue being empty — no actor's
        // turn could resolve while ANY entity's animation was still
        // draining, even one that had nothing to do with whoever was about
        // to act next. Defect D20 (docs/ember2d-refactor-plan.md §3): with
        // several actors acting in one round (floor2's 3 rats), that meant
        // paying every one of their animation durations back to back —
        // 3×0.08s = up to 240ms of the player's input going nowhere, every
        // single round, reported live as "movement feels bad when taking
        // turns with the enemy." Fixed by gating PER ACTOR instead: only the
        // specific actor about to act next (`current_actor()`) needs its OWN
        // prior animation to have finished — a different actor's turn
        // resolves immediately regardless of what's still playing, so their
        // animations overlap in real time instead of stacking. This still
        // can't let the same actor receive a second `animate_move` before
        // its first one finishes (the one thing the old global gate
        // actually had to prevent, per the Phase 5.5 Part 3 comment this
        // replaces) — `current_actor()` only changes to a NEW actor once the
        // current one's turn has fully resolved, so checking "is the actor
        // *now* at the front still animating" is exactly "has THIS actor's
        // own most recent animation finished," never anyone else's. The
        // player's own movement is deliberately never animated (see
        // `enemy_rat.rhai`'s header comment), so the player is never gated
        // by this at all — turn resolution resumes the instant it's
        // genuinely the player's turn again, not after a fixed animation
        // tax paid on enemies' behalf.
        //
        // Draining on `frame_delta_time` (real wall-clock), not `delta_time`
        // (the fixed sim step), is what makes an animation last a fixed
        // real-world duration regardless of the sim's own cadence — same
        // category as the camera lerp/particle motion above. Camera/
        // particles/audio keep flowing either way ("letting render frames
        // continue" per the plan) — only stepping itself is gated, and now
        // only for the one actor it actually needs to wait on.
        self.animations.retain_mut(|a| a.advance(frame_delta_time));
        let front_is_animating = self.sim.current_actor()
            .map(|id| self.animations.iter().any(|a| a.entity == id))
            .unwrap_or(false);

        if front_is_animating {
            // Defect D19 (docs/ember2d-refactor-plan.md §3): `ember2d::sim::step`
            // already called `input`/`mouse`/`gamepad`'s `consume_step()`
            // *before* this method even ran, unconditionally — that's what
            // actually claims a buffered press and clears it from the
            // buffer, whether or not we go on to read it. Left alone, a key
            // tapped while the front actor's animation is playing would be
            // claimed here and then never looked at, vanishing instead of
            // merely waiting — the buffer's whole "survives a frame that ran
            // zero steps" guarantee (§4.1) didn't anticipate a step that
            // runs but chooses not to consume input. Folding this frame's
            // just-pressed sets into our own buffer, merged into the real
            // step below once this actor's animation drains, restores that
            // guarantee. D20's per-actor gate made this path far rarer than
            // it used to be (see this struct's `buffered_pressed` field doc
            // comment), but not impossible.
            self.buffered_pressed.extend(input.snapshot().pressed);
            let ms = mouse.snapshot();
            self.buffered_mouse_pressed.0 |= ms.pressed.0;
            self.buffered_mouse_pressed.1 |= ms.pressed.1;
            self.buffered_gamepad_pressed.extend(ctx.gamepad.snapshot().pressed);
        } else {
            let camera_origin = self.script_camera_origin();
            let mut input_snapshot = input.snapshot();
            input_snapshot.pressed.extend(std::mem::take(&mut self.buffered_pressed));

            let mut mouse_snapshot = mouse.snapshot();
            mouse_snapshot.pressed.0 |= self.buffered_mouse_pressed.0;
            mouse_snapshot.pressed.1 |= self.buffered_mouse_pressed.1;
            self.buffered_mouse_pressed = (false, false);

            let mut gamepad_snapshot = ctx.gamepad.snapshot();
            gamepad_snapshot.pressed.extend(std::mem::take(&mut self.buffered_gamepad_pressed));

            // No externally-supplied commands from real play — that's the
            // seam `tests/external_commands.rs` exercises directly against
            // `Simulation`, not something `PlayState` itself ever needs to feed.
            // R16 (7A-5): step_count is read before this call.
            let sim_elapsed = self.sim.step_count() as f32 * delta_time;
            let outcome = self.sim.step(world, StepInput {
                input: &input_snapshot,
                mouse: mouse_snapshot,
                gamepad: &gamepad_snapshot,
                external_commands: &[],
                camera_origin,
                sim_dt: delta_time,
                elapsed: sim_elapsed,
                viewport_w: viewport_width,
                viewport_h: viewport_height,
            }, persistent);

            *turn_triggered = outcome.turn_triggered;
            self.apply_outcome(outcome);
        }

        // Particles are cosmetic and never read back by scripts (same
        // category as the camera lerp/shake above), so they move at real
        // wall-clock speed rather than the fixed sim step.
        self.particles.retain_mut(|p| {
            p.x += p.vx * frame_delta_time; p.y += p.vy * frame_delta_time; p.life -= frame_delta_time; p.life > 0.0
        });
        self.flush_audio();
    }

    fn late_update(&mut self, ctx: UpdateContext) {
        // R16 (7A-5): step_count is unchanged since update() ran this step.
        let UpdateContext { world, events, prev_positions, delta_time, viewport_width, viewport_height, persistent, .. } = ctx;
        let sim_elapsed = self.sim.step_count() as f32 * delta_time;

        // self.camera was already refreshed this step by the preceding
        // update() call (see engine.rs's per-step order: update, physics,
        // collisions, late_update) — no need to recompute it here, just
        // read the same origin update() already used.
        let camera_origin = self.script_camera_origin();
        let outcome = self.sim.late_step(world, &*events, prev_positions, camera_origin, delta_time, sim_elapsed, viewport_width, viewport_height, persistent);
        self.apply_outcome(outcome);
        self.flush_audio();
    }

    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { world, renderer, assets, .. } = ctx;
        renderer.draw_rect_filled(0, 0, renderer.width, renderer.height, ' ', Color::Reset, Color::Reset);

        // self.camera's viewport/origin were already refreshed this frame by
        // update() (see script_camera_origin's doc comment). Shake jitters a
        // *copy* — self.camera.position must stay the stable, unshaken value
        // flush_audio's distance falloff (and script_camera_origin) read.
        let mut render_camera = self.camera;
        let jitter = camera_shake_jitter(&mut self.render_rng, self.shake_state, self.shake_timer);
        render_camera.position.x += jitter.x;
        render_camera.position.y += jitter.y;

        // Phase 5.5 Part 3: built once from whatever's currently playing so
        // the loop below can look up a position/tint/shake override per
        // command without ever touching `World` — grid state has already
        // resolved (see play/animation.rs's own header comment).
        let overrides = RenderOverrides::build(&self.animations);

        let draw_list = DrawList::from_world(world);
        for cmd in draw_list.commands {
            let mut world_pos = overrides.position(cmd.id).unwrap_or(cmd.world_pos);
            let tint = overrides.tint(cmd.id).unwrap_or(cmd.tint);
            if let Some(scale) = overrides.shake_scale(cmd.id) {
                let intensity = animation::SHAKE_INTENSITY * scale;
                // R15 (7A-5): render_rng, not rng.
                world_pos.x += self.render_rng.gen_range(-intensity..=intensity);
                world_pos.y += self.render_rng.gen_range(-intensity..=intensity);
            }

            let screen = render_camera.world_to_screen(world_pos);
            let (col, row) = (screen.x.round() as i32, screen.y.round() as i32);
            // Defect D13: the texture branch used to draw and `continue`
            // before this bounds check ran, so textured sprites bypassed
            // viewport culling entirely (glyph sprites were always culled
            // correctly). Both paths now share one check up front.
            if !in_viewport(col, row, renderer.width, renderer.height) { continue; }

            match cmd.source {
                SpriteSource::Glyph { ch, bg } => {
                    renderer.draw_char_world(&render_camera, world_pos, *ch, tint, *bg);
                }
                SpriteSource::Texture { path, src } => {
                    let id = assets.load(path);
                    if let Some(t) = assets.get(id) {
                        let size = sprite_size(cmd.size, t.width, t.height, self.pixels_per_unit);
                        renderer.draw_texture_world(&render_camera, world_pos, t, size, 0.0, tint, *src);
                    }
                }
                SpriteSource::Clip { name } => {
                    // ClipFrames::Rects isn't resolved here yet (Step 3c's
                    // scope is glyph clips only).
                    if let Some(ClipFrames::Glyphs { frames }) = self.sim.clips().get(name).map(|c| &c.frames) {
                        if !frames.is_empty() {
                            let frame = world.animators.get(&cmd.id).map(|a| a.frame).unwrap_or(0) % frames.len();
                            renderer.draw_char_world(&render_camera, world_pos, frames[frame], tint, Color::Reset);
                        }
                    }
                }
            }
        }

        for p in &self.particles {
            let world_pos = Vec2::new(p.x, p.y);
            let screen = render_camera.world_to_screen(world_pos);
            let (col, row) = (screen.x.round() as i32, screen.y.round() as i32);
            if in_viewport(col, row, renderer.width, renderer.height) {
                renderer.draw_char_world(&render_camera, world_pos, p.glyph, p.fg, Color::Reset);
            }
        }

        // Step 4g: engine chrome is an F3-toggled debug overlay, not
        // permanent always-on bars — the world gets the full viewport, and
        // gameplay HUD (health, gold, controls hint, etc.) is drawn by
        // scripts via ctx.draw_hud (the loop below), not hardcoded here.
        if self.show_debug {
            let pos = self.sim.camera_entity().map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
            draw_debug_overlay(renderer, &self.sim.level().name, pos, self.fps);
        }

        // Render last 3 log messages at the bottom of the (now full-height)
        // viewport — used to sit just above the bottom bar; there's no bar
        // to sit above anymore.
        draw_recent_log(renderer, &self.script_log, 3);

        draw_hud_queue(renderer, self.sim.pending_hud_draws().iter());
        // Not cleared here anymore (Step 4g) — see
        // ScriptEngine::run_scripts's own clear for why: clearing on every
        // render, regardless of whether a script actually ran that frame,
        // made a script's drawn HUD vanish the instant the game paused.
    }

    fn take_transition(&mut self) -> Option<Transition> { self.pending_transition.take() }
}

// Tests split into play/tests.rs — see that file's header comment — once
// this file approached the project's 600-line hard limit (CLAUDE.md).
#[cfg(test)]
mod tests;
