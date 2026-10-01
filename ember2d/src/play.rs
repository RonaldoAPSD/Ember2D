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
// Step 9-2: the per-frame camera follow — see that file's header.
mod camera_ctl;
// Step 9-3: drawing script menus and dialogue — see that file's header.
mod ui_draw;
// Step 9-1: apply_outcome/flush_audio — see that file's header.
mod outcome;
mod render;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::camera::Camera;
use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::input::Key;
use crate::renderer::color::Color;
use animation::{PlayingAnimation, RenderOverrides};
use ember2d_sim::components::{AnimationClip, SpriteSource};
use ember2d_sim::event::EventBus;
use ember2d_sim::level::LevelData;
use ember2d_sim::math::{Rect, Vec2};
use ember2d_sim::scripting::LogEntry;
use ember2d_sim::simulation::{Simulation, StepInput};
use ember2d_sim::world::{EntityId, World};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use render::{
    camera_shake_jitter, clip_frame, draw_debug_overlay, draw_hud_queue, draw_recent_log,
    in_viewport, sprite_size, ClipFrame,
};
pub use render::{DrawCommand, DrawList, Space};
// Step 9-1: what `SaveState::scenes` holds, for `set_saved_scenes` callers.
pub use ember2d_sim::simulation::scenes::SceneFrame;
// Step 9-3: what `SaveState::ui` holds, for `set_saved_ui` callers.
pub use ember2d_sim::ui::UiModel;

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

// The Esc pause menu used to be a Rust `PauseMenuState` here
// (`play/pause_menu.rs`); since Step 9-1 (docs/ember2d-master-plan.md §5.8)
// it's a scene — `ember2d-sim`'s built-in `builtin_pause.rhai`, or the
// project's own `scenes/pause.rhai` — pushed by `update` below.

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
    // `audio: AudioEngine` used to live here — Step 7.5-11 (docs/ember2d-
    // master-plan.md §5.6, R30) moved it onto `Engine` instead, so it
    // survives this exact struct being destroyed and recreated on every
    // level transition. `flush_audio` (below) now takes it as a borrowed
    // parameter, sourced from `UpdateContext::audio` — see that field's own
    // doc comment (engine.rs) and `audio.rs`'s header comment for why.
    script_log: Vec<LogEntry>,
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
    /// Step 9-3 (docs/ember2d-master-plan.md §5.8): the bundled Cascadia
    /// Mono that script menus and dialogue draw with — loaded on the first
    /// frame that has one open. `None` if it failed to parse (they then
    /// don't draw; the simulation still runs them).
    ui_font: Option<crate::renderer::TtfFont>,
    ui_font_tried: bool,
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
    fn new_with_sim(mut sim: Simulation, seed: u64) -> Self {
        // Step 7.5-9 (docs/ember2d-master-plan.md §5.6, R17 fix): every
        // real `PlayState` gets a working, disk-backed `LevelSource`
        // unconditionally — `Simulation`'s own default (`NullLevelSource`)
        // does no I/O at all, which is correct for `ember2d-sim` in
        // isolation but wrong for an actual running game, which needs
        // level transitions and node-graph tiles to really resolve.
        sim.set_level_source(Box::new(crate::level_source::FsLevelSource));
        PlayState {
            fps: 0.0,
            show_debug: false,
            sim,
            pending_transition: None,
            script_log: Vec::new(),
            shake_state: None,
            shake_timer: 0.0,
            camera: Camera::new(0.0, 0.0), // real dimensions set every update()
            particles: Vec::new(),
            animations: Vec::new(),
            buffered_pressed: BTreeSet::new(),
            buffered_mouse_pressed: (false, false),
            buffered_gamepad_pressed: HashSet::new(),
            rng: SmallRng::seed_from_u64(seed.wrapping_add(PLAYSTATE_RNG_SEED_OFFSET)),
            render_rng: SmallRng::seed_from_u64(seed ^ RENDER_RNG_SEED_OFFSET),
            pixels_per_unit: crate::project::default_pixels_per_unit(),
            ui_font: None,
            ui_font_tried: false,
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
    pub fn from_save(
        level_data: LevelData,
        _persistent: BTreeMap<String, rhai::Dynamic>,
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        turn_number: u64,
        scheduler: Vec<(EntityId, u64)>,
    ) -> Self {
        let seed = level_data.seed;
        Self::new_with_sim(
            Simulation::from_save(level_data, globals, clips, turn_number, scheduler),
            seed,
        )
    }

    /// Step 9-1: this run is an editor preview (F5), so the pause menu's
    /// Back to Editor row (`return_to_editor`) is offered and works.
    pub fn set_editor_preview(&mut self, on: bool) {
        self.sim.set_editor_preview(on);
    }

    /// Step 9-1: the scenes a loaded save had open (`SaveState::scenes`).
    pub fn set_saved_scenes(&mut self, scenes: Vec<SceneFrame>) {
        self.sim.set_saved_scenes(scenes);
    }

    /// Step 9-3: the menus/dialogue a loaded save had open (`SaveState::ui`).
    pub fn set_saved_ui(&mut self, ui: ember2d_sim::ui::UiModel) {
        self.sim.set_saved_ui(ui);
    }

    /// Step 9-3: open menus and the dialogue box (tests, debugging).
    pub fn ui(&self) -> &ember2d_sim::ui::UiModel {
        self.sim.ui()
    }

    /// Step 9-1: the scene stack's names, bottom to top (tests, debugging).
    pub fn scene_names(&self) -> Vec<String> {
        self.sim.scene_names()
    }

    /// Override the default `pixels_per_unit` with the owning project's
    /// actual setting. Optional — callers that don't have a `ProjectData`
    /// handy (tests, anything constructing a level standalone) just keep
    /// the default.
    pub fn set_pixels_per_unit(&mut self, value: f32) {
        self.pixels_per_unit = value;
    }

    /// Override the default `TurnModel` (`Alternating`) with the owning
    /// project's actual setting — Step 7.5-7 (docs/ember2d-master-plan.md
    /// §5.6). Same reasoning as `set_pixels_per_unit` immediately above:
    /// a setter, not a constructor parameter, since `PlayState::from_level`/
    /// `from_save` never see a `ProjectData`; optional for the same reason.
    pub fn set_turn_model(&mut self, model: ember2d_sim::scheduler::TurnModel) {
        self.sim.set_turn_model(model);
    }

    /// Forwarding accessors onto the owned `Simulation` — kept public
    /// because `tests/save_load_globals.rs` (an external integration test
    /// using only `ember2d::prelude`) reads a script-set global directly to
    /// verify D17's save/load round trip. Nothing inside this crate needs
    /// these anymore; the field itself lives on `Simulation` now.
    pub fn globals(&self) -> &BTreeMap<String, rhai::Dynamic> {
        self.sim.globals()
    }
    pub fn clips(&self) -> &BTreeMap<String, AnimationClip> {
        self.sim.clips()
    }

    pub fn take_log(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut self.script_log)
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
    fn on_start(
        &mut self,
        world: &mut World,
        _events: &mut EventBus,
        viewport_width: usize,
        viewport_height: usize,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
    ) {
        let logs = self.sim.on_start(world, viewport_width, viewport_height, persistent);
        self.script_log.extend(logs);
    }

    fn update(&mut self, ctx: UpdateContext) {
        // R16 (7A-5): `ctx.elapsed` deliberately not bound — see sim_elapsed below.
        let UpdateContext {
            world,
            input,
            mouse,
            delta_time,
            frame_delta_time,
            viewport_width,
            viewport_height,
            turn_triggered,
            persistent,
            audio,
            ..
        } = ctx;

        // FPS counter and shake-timer decay are presentation only (the F3
        // debug overlay, the render-time shake jitter) — never read back by
        // scripts — so they use frame_delta_time (real wall-clock), not
        // delta_time (the fixed sim step). See UpdateContext::frame_delta_time's
        // own doc comment.
        if frame_delta_time > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / frame_delta_time) * 0.1;
        }
        if self.shake_timer > 0.0 {
            self.shake_timer -= frame_delta_time;
            if self.shake_timer <= 0.0 {
                self.shake_state = None;
            }
        }

        // Step 9-1: Esc opens the pause scene when no scene is open (an
        // open scene gets Esc in its own `on_input` instead). Not a return:
        // the step below runs, starting the scene this same frame — and a
        // scene never gets `on_input` on the step it was pushed, so this
        // same Esc press can't immediately close it again.
        if input.just_pressed(Key::Escape) {
            self.sim.request_pause(world, &mut self.script_log);
        }

        if input.just_pressed(Key::F3) {
            self.show_debug = !self.show_debug;
        }

        // Step 9-2 (docs/ember2d-master-plan.md §5.8): what the camera
        // follows, its zoom, bounds and speed are script-set now — see
        // `play/camera_ctl.rs`.
        self.update_camera(world, frame_delta_time, viewport_width, viewport_height);

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
        // `enemy.rhai`'s header comment), so the player is never gated
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
        let front_is_animating = self
            .sim
            .current_actor()
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
            // Step 7.5-7 (docs/ember2d-master-plan.md §5.6): what
            // `ctx.is_animating(id)` reads — see `StepInput::animating`'s
            // own doc comment (simulation.rs) for why this crosses the
            // sim/presentation boundary as a borrowed snapshot rather than
            // `Simulation` owning the queue itself.
            let animating: Vec<EntityId> = self.animations.iter().map(|a| a.entity).collect();
            let outcome = self.sim.step(
                world,
                StepInput {
                    input: &input_snapshot,
                    mouse: mouse_snapshot,
                    gamepad: &gamepad_snapshot,
                    external_commands: &[],
                    animating: &animating,
                    camera_origin,
                    sim_dt: delta_time,
                    elapsed: sim_elapsed,
                    viewport_w: viewport_width,
                    viewport_h: viewport_height,
                },
                persistent,
            );

            *turn_triggered = outcome.turn_triggered;
            self.apply_outcome(outcome);
        }

        // Particles are cosmetic and never read back by scripts (same
        // category as the camera lerp/shake above), so they move at real
        // wall-clock speed rather than the fixed sim step.
        self.particles.retain_mut(|p| {
            p.x += p.vx * frame_delta_time;
            p.y += p.vy * frame_delta_time;
            p.life -= frame_delta_time;
            p.life > 0.0
        });
        self.flush_audio(audio);
    }

    fn late_update(&mut self, ctx: UpdateContext) {
        // R16 (7A-5): step_count is unchanged since update() ran this step.
        let UpdateContext {
            world,
            events,
            prev_positions,
            delta_time,
            viewport_width,
            viewport_height,
            persistent,
            audio,
            ..
        } = ctx;
        let sim_elapsed = self.sim.step_count() as f32 * delta_time;

        // self.camera was already refreshed this step by the preceding
        // update() call (see engine.rs's per-step order: update, physics,
        // collisions, late_update) — no need to recompute it here, just
        // read the same origin update() already used.
        let camera_origin = self.script_camera_origin();
        let outcome = self.sim.late_step(
            world,
            &*events,
            prev_positions,
            camera_origin,
            delta_time,
            sim_elapsed,
            viewport_width,
            viewport_height,
            persistent,
        );
        self.apply_outcome(outcome);
        self.flush_audio(audio);
    }

    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { world, renderer, assets, .. } = ctx;
        // 7B-3 (docs/ember2d-master-plan.md §5.2): used to blank the whole
        // viewport here with a width*height loop of individual blank-glyph
        // draw calls — purely to get DEFAULT_BG as the background, since
        // the render pass's own per-frame GPU clear was hardcoded to pure
        // black. The clear color now matches DEFAULT_BG
        // (renderer/backend.rs's `render()`), so the GPU clear alone
        // already does this — but ONLY because `Engine::run` no longer draws
        // the paused `EditorState` between that clear and this state (R51,
        // §3.2: that fill had also been burying the editor under an F5
        // preview, and removing it exposed the editor's chrome through every
        // cell play didn't draw). Don't re-add a fill here; the engine's
        // opaque-state rule (`state_stack.rs`) is the real fix.

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

        // Step 8-1: tilemap cells only inside the camera's view (one cell
        // of slack is `Tilemap::visible_cells`' own, for shake/rounding).
        let view_min = render_camera.screen_to_world(Vec2::ZERO);
        let view_max =
            render_camera.screen_to_world(Vec2::new(renderer.width as f32, renderer.height as f32));
        let view = Rect::new(view_min.x, view_min.y, view_max.x - view_min.x, view_max.y - view_min.y);
        let draw_list = DrawList::from_world_in(world, Some(view));
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
            if !in_viewport(col, row, renderer.width, renderer.height) {
                continue;
            }

            match cmd.source {
                SpriteSource::Glyph { ch, bg } => {
                    renderer.draw_char_world(&render_camera, world_pos, *ch, tint, *bg);
                }
                SpriteSource::Texture { path, src } => {
                    let id = assets.load(path);
                    if let Some(t) = assets.get(id) {
                        let size =
                            sprite_size(cmd.size, *src, t.width, t.height, self.pixels_per_unit);
                        renderer.draw_texture_world(
                            &render_camera,
                            world_pos,
                            t,
                            size,
                            0.0,
                            tint,
                            *src,
                        );
                    }
                }
                SpriteSource::Clip { name } => {
                    // Glyph clips (scripts' `register_clip`) and, since Step
                    // 8-3, sheet clips (project clips: `ClipFrames::Rects`).
                    let frame = world.animators.get(&cmd.id).map(|a| a.frame).unwrap_or(0);
                    match self.sim.clips().get(name).and_then(|c| clip_frame(c, frame)) {
                        Some(ClipFrame::Glyph(ch)) => renderer.draw_char_world(
                            &render_camera,
                            world_pos,
                            ch,
                            tint,
                            Color::Reset,
                        ),
                        Some(ClipFrame::Rect(path, rect)) => {
                            let id = assets.load(path);
                            if let Some(t) = assets.get(id) {
                                let size = sprite_size(
                                    cmd.size,
                                    Some(rect),
                                    t.width,
                                    t.height,
                                    self.pixels_per_unit,
                                );
                                let r = Some(rect);
                                renderer
                                    .draw_texture_world(&render_camera, world_pos, t, size, 0.0, tint, r);
                            }
                        }
                        None => {}
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
            let pos = self
                .sim
                .camera_entity()
                .map(|id| world.get_global_position(id))
                .unwrap_or(Vec2::ZERO);
            draw_debug_overlay(renderer, &self.sim.level().name, pos, self.fps);
        }

        // Render last 3 log messages at the bottom of the (now full-height)
        // viewport — used to sit just above the bottom bar; there's no bar
        // to sit above anymore.
        draw_recent_log(renderer, &self.script_log, 3);

        draw_hud_queue(renderer, self.sim.pending_hud_draws().iter());
        // Step 9-1: scene scripts' HUD, above the level's.
        draw_hud_queue(renderer, self.sim.scene_hud_draws().iter());

        // Step 9-3: script menus and the dialogue box, above every HUD.
        let ui = self.sim.ui();
        if ui.open_dialogue().is_some() || ui.active_menu().is_some() {
            if !self.ui_font_tried {
                self.ui_font_tried = true;
                match crate::renderer::font::bundled_ui_font() {
                    Ok(f) => self.ui_font = Some(f),
                    Err(e) => self.script_log.push(LogEntry::error(format!("UI font: {e}"))),
                }
            }
            if let Some(font) = self.ui_font.as_mut() {
                let (vw, vh) = (renderer.width, renderer.height);
                ui_draw::draw_ui(renderer, font, self.sim.ui(), vw, vh);
            }
        }
        // Not cleared here anymore (Step 4g) — see
        // ScriptEngine::run_scripts's own clear for why: clearing on every
        // render, regardless of whether a script actually ran that frame,
        // made a script's drawn HUD vanish the instant the game paused.
    }

    /// Step 9-1: while a world-pausing scene was on top at the start of
    /// this step, the engine skips physics and the late phase too.
    fn world_paused(&self) -> bool {
        self.sim.paused_this_step()
    }

    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }

    // 7C-7 (master plan §5.3, R18): reuses the existing `take_log` — see
    // that method's own doc comment for why it's a `mem::take`, not a
    // clone.
    fn take_script_log(&mut self) -> Vec<LogEntry> {
        self.take_log()
    }
}

// Tests split into play/tests.rs — see that file's header comment — once
// this file approached the project's 600-line hard limit (CLAUDE.md).
#[cfg(test)]
mod tests;
