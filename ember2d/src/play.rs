// play.rs — Play mode: run a level built from a LevelData file.

mod render;
mod spawn;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::camera::Camera;
use ember2d_sim::components::{AnimationClip, ClipFrames, Controller, SpriteSource};
use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
pub use render::{DrawCommand, DrawList, Space};
use render::{in_viewport, sprite_size};
use ember2d_sim::command::Command;
use ember2d_sim::event::{EventBus, GameEvent};
use crate::input::Key;
use ember2d_sim::level::LevelData;
use ember2d_sim::math::Vec2;
use crate::renderer::color::Color;
use crate::audio::AudioEngine;
use ember2d_sim::scripting::{LogEntry, ScriptEngine, HudDraw};
use ember2d_sim::world::{EntityId, World};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

// ── Path resolution ───────────────────────────────────────────────────────────

pub fn resolve_exit_path(next: &str, current_level_path: &str) -> String {
    if Path::new(next).is_absolute() || current_level_path.is_empty() {
        return next.to_string();
    }
    if Path::new(next).exists() {
        return next.to_string();
    }
    match Path::new(current_level_path).parent() {
        Some(dir) if dir != Path::new("") => dir.join(next).to_string_lossy().into_owned(),
        _ => next.to_string(),
    }
}

// ── Player identity (Step 5g, docs/ember2d-phase5-plan.md) ─────────────────────
//
// "The player" used to be a single `PlayState::player_id: EntityId`, stored
// once at spawn time. That assumed exactly one player, baked into both the
// stored id itself and the `other == self.player_id` shape of every check
// that used it. Player identity now comes from `Actor::controller` instead
// — any entity with a `Controller::Local(_)` actor is a player — so these
// checks generalize to "any" local player with no stored id to keep in
// sync. Authoring is still singular (one `PlayerRecord` per level; see
// play/spawn.rs), and `camera_entity` stays singular too (one viewport —
// split-screen is out of scope here), but the *code* no longer assumes
// there's only ever one local player to find.

pub(crate) fn is_local_player(world: &World, id: EntityId) -> bool {
    matches!(world.actors.get(&id).map(|a| a.controller), Some(Controller::Local(_)))
}

/// Every locally-controlled actor, in `EntityId` order (`world.actors` is a
/// `BTreeMap`, Step 5b) — used where more than one might plausibly need
/// considering (today: picking a camera target to fall back on). Most call
/// sites only need `is_local_player`'s yes/no check on a specific id.
fn local_player_ids(world: &World) -> impl Iterator<Item = EntityId> + '_ {
    world.actors.iter().filter(|(_, a)| matches!(a.controller, Controller::Local(_))).map(|(&id, _)| id)
}

// `HUD_TOP_ROWS` (Step 4g: rows reserved for HUD chrome above the playable
// viewport) was deleted in Step 5a (docs/ember2d-phase5-plan.md) — its value
// had been `0` since Step 4g removed both hardcoded HUD bars, and both call
// sites (`Camera::viewport_origin` below, `get_mouse_world_y` in
// `scripting/api.rs`) were inert. If a future HUD design needs to reserve
// rows again, `Camera::viewport_origin` is already the mechanism to write a
// nonzero value into — the constant added nothing beyond that.

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

// ShakeState moved to scripting/types.rs in Step 5a (docs/ember2d-phase5-plan.md)
// — it's a scripting-facing value (ctx.shake_camera queues one, ScriptUpdateResult
// carries it out), not something PlayState itself defines the shape of.
pub use ember2d_sim::scripting::ShakeState;

pub struct PlayState {
    fps: f32,
    /// Toggled by F3. Engine chrome (level name, exact position, backend,
    /// FPS) is a debug tool switched on during play, not permanent
    /// always-on UI — see docs/HANDOFF.md's Step 4g.
    show_debug: bool,
    level: LevelData,
    pending_transition: Option<Transition>,
    script_engine: ScriptEngine,
    audio:         AudioEngine,
    script_log: Vec<LogEntry>,
    camera_entity: Option<EntityId>,
    exit_targets: HashMap<EntityId, String>,
    /// `BTreeMap`, not `HashMap` (Step 5b, docs/ember2d-phase5-plan.md) —
    /// this round-trips into `ScriptState.globals`/`.persistent` every frame
    /// and out through `ScriptUpdateResult`, and Phase 5c/5h need its
    /// serialized form (RON, for `SaveState`, and the eventual replay test)
    /// to be byte-identical across runs with identical content — a
    /// `HashMap`'s per-process-random iteration order would make the
    /// serialized text differ even when the logical map doesn't.
    pub globals: BTreeMap<String, rhai::Dynamic>,
    /// Script-registered clip definitions (Step 3c) — round-trips through
    /// the script engine every frame the same way `globals` does; see
    /// `scripting::state::ScriptState`'s `clips` field. `BTreeMap` for the
    /// same reason as `globals` above.
    pub clips: BTreeMap<String, AnimationClip>,
    /// This step's commands, keyed by actor id — the `on_input` pass's
    /// result (Step 5e, docs/ember2d-phase5-plan.md), fed to the
    /// following `on_update` pass so `ctx.command_action()`/
    /// `command_param()` can read them. Rebuilt fresh every step; see
    /// `ScriptUpdateResult::commands`'s doc comment for why it doesn't
    /// accumulate.
    pub commands: BTreeMap<i64, Command>,
    /// Deterministic turn order (Step 5f, docs/ember2d-phase5-plan.md) —
    /// see `scheduler.rs`'s header comment for the split of responsibility
    /// between it (a dumb ordering primitive) and `run_actor_turn` below
    /// (the "is this actor local, does it have a command yet" policy).
    /// Rebuilt from scratch by `rebuild_scheduler` at level load — nothing
    /// about its internal ordering state is saved/restored across
    /// save/load, only which entities have an `Actor` component at all
    /// (which round-trips through `World` normally); a loaded game simply
    /// starts every actor at a fresh round.
    scheduler: ember2d_sim::scheduler::TurnScheduler,
    /// How many turns the local player has completed so far this level —
    /// what `ctx.get_turn_number()` reads. See `run_actor_turn`.
    turn_number: i64,
    pub camera_override: Option<Vec2>,
    pub shake_state: Option<ShakeState>,
    pub shake_timer: f32,
    /// Owns world<->screen conversion for this level (Step 2e). Its
    /// viewport/origin fields are refreshed every `update()` call — see
    /// `script_camera_origin` for the one place that reads it back out.
    pub camera: Camera,
    pub particles: Vec<Particle>,
    pub is_loading_save: bool,
    /// Drives particle velocity/life and camera shake jitter (defect D3).
    /// Seeded once from the level's stored seed and reused for its whole
    /// lifetime — never reallocated from OS entropy per call.
    rng: SmallRng,
    /// Source-texture pixels per world unit, for a `Sprite` whose `size` is
    /// `None` (Step 3b). Defaults to `ProjectData::pixels_per_unit`'s own
    /// default (8.0); `set_pixels_per_unit` lets a caller that actually has
    /// the project's settings (`app.rs`) override it after construction,
    /// since `PlayState::from_level`/`from_save` take a `LevelData`, not a
    /// `ProjectData` — keeping those constructors' signatures (and every
    /// existing call site, tests included) unchanged.
    pixels_per_unit: f32,
}

/// A different constant offset from the level's seed for `PlayState::rng`
/// than the one `ScriptEngine` seeds from directly (see `from_level`), so
/// script randomness and particle/shake randomness are two independent
/// deterministic streams instead of mirroring each other's sequence.
const PLAYSTATE_RNG_SEED_OFFSET: u64 = 0x9E3779B97F4A7C15; // splitmix64's golden-ratio constant

impl PlayState {
    pub fn from_level(data: LevelData, _persistent: BTreeMap<String, rhai::Dynamic>) -> Self {
        let seed = data.seed;
        PlayState {
            fps:                0.0,
            show_debug:         false,
            level:              data,
            pending_transition: None,
            script_engine:      ScriptEngine::new(seed),
            audio:              AudioEngine::new(),
            script_log:         Vec::new(),
            camera_entity:      None,
            exit_targets:       HashMap::new(),
            globals:            BTreeMap::new(),
            clips:              BTreeMap::new(),
            commands:           BTreeMap::new(),
            scheduler:          ember2d_sim::scheduler::TurnScheduler::new(),
            turn_number:        0,
            camera_override:    None,
            shake_state:        None,
            shake_timer:        0.0,
            camera:             Camera::new(0.0, 0.0), // real dimensions set every update()
            particles:          Vec::new(),
            is_loading_save:    false,
            rng:                SmallRng::seed_from_u64(seed.wrapping_add(PLAYSTATE_RNG_SEED_OFFSET)),
            pixels_per_unit:    crate::project::default_pixels_per_unit(),
        }
    }

    /// `globals`/`clips` come from the loaded `SaveState` and are set
    /// directly here rather than left for `on_start` to rebuild — defect
    /// D17 fix (Step 5c, docs/ember2d-phase5-plan.md). `on_start`'s
    /// `is_loading_save` branch deliberately never re-runs a script's own
    /// `on_start`, so nothing else would ever populate these otherwise (see
    /// this type's `on_start` impl, and `SaveState::globals`'s doc comment
    /// for why re-running scripts' `on_start` on load isn't the fix).
    pub fn from_save(level_data: LevelData, _persistent: BTreeMap<String, rhai::Dynamic>, globals: BTreeMap<String, rhai::Dynamic>, clips: BTreeMap<String, AnimationClip>) -> Self {
        let mut ps = Self::from_level(level_data, _persistent);
        ps.is_loading_save = true;
        ps.globals = globals;
        ps.clips = clips;
        ps
    }

    /// Override the default `pixels_per_unit` with the owning project's
    /// actual setting. Optional — callers that don't have a `ProjectData`
    /// handy (tests, anything constructing a level standalone) just keep
    /// the default.
    pub fn set_pixels_per_unit(&mut self, value: f32) {
        self.pixels_per_unit = value;
    }

    /// (Re)populate `self.scheduler` from every entity `World` currently
    /// has an `Actor` component for — called once after spawning (both
    /// `do_on_start` and the `is_loading_save` branch of `on_start` below)
    /// since neither leaves `self.scheduler` anywhere else to come from.
    /// Iterates `world.actors` (a `BTreeMap`, Step 5b) in ascending
    /// `EntityId` order for a reproducible insertion sequence — though
    /// `TurnScheduler::insert`'s own rank/id tiebreak already makes the
    /// *outcome* order-independent; this is just not leaving it to chance.
    fn rebuild_scheduler(&mut self, world: &World) {
        self.scheduler = ember2d_sim::scheduler::TurnScheduler::new();
        for (&id, actor) in &world.actors {
            self.scheduler.insert(id, actor.controller);
        }
    }

    /// Runs `actor`'s `on_turn`, applies the result, and — unless a
    /// `Local` actor's action was rejected — advances `self.scheduler` and
    /// marks this step as having consumed a turn. Called at most once per
    /// step (Step 5f, docs/ember2d-phase5-plan.md's "one actor per step"),
    /// for whichever actor `update`'s scheduler poll decided should act.
    #[allow(clippy::too_many_arguments)]
    fn run_actor_turn(
        &mut self, world: &mut World, snapshot: std::rc::Rc<ember2d_sim::scripting::WorldSnapshot>, actor: EntityId, is_local: bool, commands: BTreeMap<i64, Command>,
        delta_time: f32, elapsed: f32, persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_origin: Vec2, turn_triggered: &mut bool, viewport_width: usize, viewport_height: usize,
    ) {
        let res = self.script_engine.run_on_turn(
            world, snapshot, &mut self.script_log, actor, delta_time, elapsed,
            &self.level.extra_spawns, self.globals.clone(), self.clips.clone(),
            persistent, camera_origin, commands, self.turn_number,
            (viewport_width, viewport_height),
        );
        let act_cost = res.act_cost;
        self.apply_script_result(world, res, persistent);

        // An AI actor's turn always counts, whether or not it calls
        // ctx.act — a sleeping monster still "used" its turn doing
        // nothing, and unconditionally skipping the advance would wedge
        // the scheduler on it forever. A Local actor's turn counts only if
        // it called ctx.act — see that method's doc comment for why this
        // is what lets a rejected action (a wall bump, an empty-handed
        // quaff) cost nothing. Guarded on the scheduler's front still
        // being `actor`: `apply_script_result` above may already have
        // removed it (a self-despawn during its own on_turn — not
        // exercised by the roguelike today, but a real possibility for a
        // future script), in which case there's nothing left to advance.
        let consumed = !is_local || act_cost.is_some();
        if consumed && self.scheduler.peek() == Some(actor) {
            let controller = world.actors.get(&actor).map(|a| a.controller).unwrap_or(ember2d_sim::components::Controller::Ai);
            let cost = act_cost.unwrap_or(ember2d_sim::scheduler::ALTERNATING_COST as f64).max(1.0) as u64;
            self.scheduler.advance(actor, controller, cost);
            *turn_triggered = true;
            if is_local { self.turn_number += 1; }
        }
    }

    fn apply_script_result(&mut self, world: &mut World, res: ember2d_sim::scripting::ScriptUpdateResult, persistent: &mut BTreeMap<String, rhai::Dynamic>) {
        // Step 5f: a despawned actor must not keep cycling a dead turn
        // slot forever — see TurnScheduler::remove's own doc comment.
        for &id in &res.despawned { self.scheduler.remove(id); }
        if let Some(level_path) = res.pending_level {
            let full = resolve_exit_path(&level_path, &self.level.path);
            match LevelData::load(&full) {
                Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                Err(e)   => { self.script_log.push(LogEntry::warn(format!("load_level failed: {}", e))); }
            }
        }
        self.globals = res.globals;
        self.clips = res.clips;
        self.commands = res.commands;
        *persistent = res.persistent;

        if let Some(save_path) = res.pending_save {
            // globals/clips were just refreshed from `res` above, so this
            // captures the exact state a script saw the moment it called
            // save_game — defect D17 fix (Step 5c, docs/ember2d-phase5-plan.md).
            let state = ember2d_sim::save::SaveState::new(world.clone(), persistent.clone(), self.globals.clone(), self.clips.clone(), self.level.path.clone());
            if let Err(e) = state.save_to_file(&save_path) {
                self.script_log.push(LogEntry::error(format!("save_game failed: {}", e)));
            } else {
                self.script_log.push(LogEntry::info(format!("Game saved to {}", save_path)));
            }
        }

        if let Some(load_path) = res.pending_load {
            match ember2d_sim::save::SaveState::load_from_file(&load_path) {
                Ok(state) => {
                    self.pending_transition = Some(Transition::LoadGame(state));
                }
                Err(e) => {
                    self.script_log.push(LogEntry::error(format!("load_game failed: {}", e)));
                }
            }
        }

        if res.camera_override.is_some() { self.camera_override = res.camera_override; }
        if let Some(shake) = res.shake_state {
            self.shake_state = Some(shake);
            self.shake_timer = shake.duration;
        }

        if !res.particles.is_empty() {
            for req in res.particles {
                let vx = self.rng.gen_range(-5.0..5.0);
                let vy = self.rng.gen_range(-5.0..5.0);
                let life = self.rng.gen_range(0.2..0.8);
                self.particles.push(Particle { x: req.x, y: req.y, vx, vy, glyph: req.glyph, fg: req.fg, life });
            }
        }
    }

    pub fn take_log(&mut self) -> Vec<LogEntry> { std::mem::take(&mut self.script_log) }

    fn flush_audio(&mut self) {
        for path in self.script_engine.pending_sounds.drain(..) { self.audio.play_sound(&path, 1.0); }
        let cam_pos = self.camera.position;
        let max_dist = 20.0f32;
        for (path, x, y) in self.script_engine.pending_spatial_sounds.drain(..) {
            let dx = x - cam_pos.x;
            let dy = y - cam_pos.y;
            let dist = (dx*dx + dy*dy).sqrt();
            let volume = (1.0 - (dist / max_dist)).clamp(0.0, 1.0);
            if volume > 0.01 { self.audio.play_sound(&path, volume as f64); }
        }
        if self.script_engine.stop_music { self.audio.stop_music(); self.script_engine.stop_music = false; }
        if let Some(path) = self.script_engine.pending_music.take() { self.audio.play_music(&path); }
    }

    /// World position scripts see as the camera's origin — `get_camera_x/y`
    /// and (added to the mouse's screen cell) `get_mouse_world_x/y` both key
    /// off this. Rounded to whole cells, matching the precision scripts have
    /// always seen (e.g. `player.rhai`'s click-to-teleport lands on an exact
    /// cell); only the render path benefits from `self.camera`'s true float
    /// precision. A dedicated method so `update`/`late_update` don't
    /// hand-duplicate this formula — they used to, which is the same defect
    /// shape D5 was about, just for camera math instead of draw order.
    fn script_camera_origin(&self) -> Vec2 {
        let tl = self.camera.top_left();
        Vec2::new(tl.x.round(), tl.y.round())
    }
}

impl GameState for PlayState {
    fn on_start(&mut self, world: &mut World, events: &mut EventBus, viewport_width: usize, viewport_height: usize, persistent: &mut BTreeMap<String, rhai::Dynamic>) {
        if !self.is_loading_save {
            self.do_on_start(world, events, viewport_width, viewport_height, persistent);
        } else {
            for (_, script) in &world.scripts { self.script_engine.compile(&script.path, &mut self.script_log); }
            // Step 5g: `camera_entity` isn't itself part of `SaveState` (only
            // `World`/`persistent`/`globals`/`clips` are), so a freshly
            // constructed `PlayState::from_save` always starts with it
            // unset — this is what re-derives it, same as `do_on_start`'s
            // own `camera_follow` handling does for a fresh level load.
            if self.camera_entity.is_none() { self.camera_entity = local_player_ids(world).next(); }
        }
        // Step 5f: both branches above leave `world.actors` fully
        // populated (spawned fresh by `do_on_start`, or round-tripped
        // through `SaveState`/`World`'s own (de)serialization) — this is
        // the one place after either that's guaranteed true.
        self.rebuild_scheduler(world);
    }

    fn update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, input, mouse, delta_time, frame_delta_time, elapsed, viewport_width, viewport_height, events, turn_triggered, persistent, .. } = ctx;

        // FPS counter and shake-timer decay are presentation only (the F3
        // debug overlay, the render-time shake jitter) — never read back by
        // scripts — so they use frame_delta_time (real wall-clock), not
        // delta_time (the fixed sim step). Step 5d, docs/ember2d-phase5-plan.md;
        // see UpdateContext::frame_delta_time's own doc comment. Before this
        // split, turn-based mode's `delta_time` WAS real wall-clock time, so
        // this is unchanged there; realtime mode's frame_delta_time equals
        // delta_time by construction (see sim::step's caller in engine.rs),
        // so this is unchanged there too.
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

        // Advance every Animator before scripts run this frame, so
        // `clip_finished(id)` reflects this tick, not last frame's — an
        // entity whose clip has no matching registration (renamed,
        // never registered) simply doesn't advance and never finishes.
        for animator in world.animators.values_mut() {
            if let Some(clip) = self.clips.get(&animator.clip) { animator.advance(clip, delta_time); }
            else { animator.just_finished = false; }
        }

        let mut target_cam = self.camera_entity.map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
        if let Some(over) = self.camera_override { target_cam = over; }

        // Step 4g: the world gets the full viewport now — the two
        // hardcoded HUD bars that used to reserve row 0 and the last row
        // are gone; engine chrome is a toggleable F3 overlay drawn on top
        // instead of reserving space.
        let game_h = (viewport_height as i32).max(1) as f32;
        let half_w = viewport_width as f32 / 2.0;
        let half_h = game_h / 2.0;

        let min_x = half_w;
        let max_x = (self.level.width as f32 - half_w).max(min_x);
        let min_y = half_h;
        let max_y = (self.level.height as f32 - half_h).max(min_y);

        target_cam.x = target_cam.x.clamp(min_x, max_x);
        target_cam.y = target_cam.y.clamp(min_y, max_y);

        if self.camera.position == Vec2::ZERO { self.camera.position = target_cam; }
        else {
            // Presentation, not simulation — frame_delta_time (real
            // wall-clock), not delta_time, so the camera stays visually
            // smooth regardless of the sim's own clock. Also worth noting
            // for Phase 6 (§5.2 H2 in the refactor plan): `exp()` is a
            // named cross-platform determinism hazard, one more reason
            // camera position must never be read back into anything a
            // script or the sim depends on — see
            // docs/ember2d-scripting-api.md §3's "Camera" section.
            let lerp_speed = 5.0;
            self.camera.position = self.camera.position + (target_cam - self.camera.position) * (1.0 - (-lerp_speed * frame_delta_time).exp());
        }
        self.camera.viewport_width = viewport_width as f32;
        self.camera.viewport_height = game_h;
        self.camera.viewport_origin = Vec2::ZERO;
        self.camera.zoom = 1.0; // Phase 2 doesn't add a scripted zoom control yet

        let camera_origin = self.script_camera_origin();
        let input_snapshot = input.snapshot();
        // Step 5i (workspace split, docs/ember2d-phase5-plan.md): `scripting`
        // is sim-side and can't reference `&MouseState`/`&GamepadState`
        // directly (both own real winit/gilrs types) — see
        // `MouseSnapshot`/`GamepadSnapshot`'s own doc comments (command.rs).
        let mouse_snapshot = mouse.snapshot();
        let gamepad_snapshot = ctx.gamepad.snapshot();

        // Built once per step, shared (via cheap `Rc::clone`) across
        // on_input/on_update/on_turn below instead of each one rebuilding
        // its own full `World`-derived snapshot — see `WorldSnapshot`'s own
        // doc comment (scripting/state.rs) for the perf regression this
        // fixes (Step 5f: floor2 dropped to ~21 fps in a debug build,
        // reported live after this session's earlier 5e/5f work added the
        // extra on_input/on_turn passes that made it 2-3x worse).
        let world_snapshot = std::rc::Rc::new(ember2d_sim::scripting::WorldSnapshot::build(world));

        // ── Step 5f: the turn scheduler (docs/ember2d-phase5-plan.md) ──────
        // At most one actor's turn resolves per step. `on_input` (for a
        // `Local` actor awaiting a command) runs first; the housekeeping
        // `on_update` pass runs next, before `on_turn` — NOT after, even
        // though that means a killing blow's consequences (a death screen,
        // a stairs tile unlocking) show up one step later than the blow
        // itself. That ordering is required, not just simpler: on_update is
        // also where a script's own lazy-init lives (see player.rhai's
        // header comment), and `on_turn` reads that same persistent/global
        // state — running on_turn first would mean a brand new level's
        // very first turn reads pre-init values (e.g. hp as `()`/0,
        // misread as "already dead") before on_update ever got a chance to
        // set them. The 1-step lag this trades for is imperceptible at 60
        // fps; the lazy-init race was a real, immediate bug (confirmed by
        // hand: pressing a single movement key on a freshly loaded level
        // did nothing at all, because on_turn saw hp==0 before on_update's
        // lazy-init had ever run once).
        let front = self.scheduler.peek();
        let is_local = front.map(|f| matches!(world.actors.get(&f).map(|a| a.controller), Some(ember2d_sim::components::Controller::Local(_)))).unwrap_or(false);

        if let Some(front) = front {
            if is_local {
                // `on_input` is the only place this actor's raw input gets
                // read (see docs/ember2d-scripting-api.md's "Input"
                // section) — its own `apply_script_result` commits
                // whatever `ctx.submit()` queued into `self.commands`
                // below, which `run_on_turn` (and the housekeeping
                // `on_update` pass) read back via `command_action`/
                // `command_param`.
                let input_res = self.script_engine.run_on_input(
                    world, world_snapshot.clone(), &mut self.script_log, front, delta_time, elapsed, input_snapshot.clone(),
                    mouse_snapshot, gamepad_snapshot.clone(), &self.level.extra_spawns, self.globals.clone(), self.clips.clone(),
                    persistent, camera_origin, self.turn_number, (viewport_width, viewport_height),
                );
                self.apply_script_result(world, input_res, persistent);
            }
        }

        // Snapshot here, before the housekeeping pass below overwrites
        // `self.commands` with its own (always-empty — on_update never
        // submits) result: `ScriptUpdateResult::commands` doesn't
        // accumulate across passes by design (see its doc comment), so
        // without this snapshot on_turn below would see "" instead of
        // whatever on_input just queued.
        let turn_commands = self.commands.clone();

        // Housekeeping `on_update` pass — every scripted entity, every
        // step, regardless of whose turn it is (docs/ember2d-regression-checklist.md
        // §12: this is what lets a stairs tile's lock-state and an
        // enemy's death check update between turns).
        let res = self.script_engine.run_scripts(
            world, world_snapshot.clone(), events, &mut self.script_log, delta_time, elapsed, input_snapshot, mouse_snapshot, gamepad_snapshot,
            &self.level.extra_spawns, self.globals.clone(), self.clips.clone(), persistent, camera_origin,
            turn_commands.clone(), self.turn_number, (viewport_width, viewport_height),
        );
        self.apply_script_result(world, res, persistent);

        if let Some(front) = front {
            // A Local actor with nothing actionable queued yet stays
            // "awaiting" — render this frame as usual, but don't run
            // on_turn (no turn consumed, no late phase this step). An AI
            // actor is always ready; it never goes through on_input at
            // all.
            let has_command = turn_commands.get(&(front as i64)).map(|c| !c.action.is_empty()).unwrap_or(false);
            if !is_local || has_command {
                self.run_actor_turn(world, world_snapshot, front, is_local, turn_commands, delta_time, elapsed, persistent, camera_origin, turn_triggered, viewport_width, viewport_height);
            }
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
        let UpdateContext { world, events, prev_positions, delta_time, elapsed, viewport_width, viewport_height, persistent, .. } = ctx;
        let mut all_pairs = Vec::new();

        for event in events.events() {
            let GameEvent::Collision { entity_a, entity_b } = event else { continue };
            let (a, b) = (*entity_a, *entity_b);
            all_pairs.push((a, b));
            // Step 5g: fires for *any* local player, not one stored id —
            // see this file's "Player identity" section up top.
            let (player, other) = if is_local_player(world, a) { (a, b) } else if is_local_player(world, b) { (b, a) } else { continue };
            let solid = world.colliders.get(&other).map(|c| c.solid).unwrap_or(false);
            // Defect D12: this used to read the collider's `layer` string and
            // compare it against the magic value "locked", which corrupted
            // the layer field's real purpose (collision filtering) for any
            // locked exit tile. `locked` is now its own flag.
            let locked = world.colliders.get(&other).map(|c| c.locked).unwrap_or(false);

            if solid { world.resolve_solid_collision(player, other, prev_positions); }
            else if let Some(path) = self.exit_targets.get(&other).cloned() {
                if !locked {
                    let full_path = resolve_exit_path(&path, &self.level.path);
                    match LevelData::load(&full_path) {
                        Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                        Err(e)   => { self.script_log.push(LogEntry::warn(format!("Exit failed: {}", e))); }
                    }
                }
            }
        }

        // self.camera was already refreshed this step by the preceding
        // update() call (see engine.rs's per-step order: update, physics,
        // collisions, late_update) — no need to recompute it here.
        let camera_origin = self.script_camera_origin();
        let res = self.script_engine.run_collisions(
            world, &all_pairs, &mut self.script_log, delta_time, elapsed,
            &self.level.extra_spawns, self.globals.clone(), self.clips.clone(), persistent, camera_origin,
            (viewport_width, viewport_height),
        );
        self.apply_script_result(world, res, persistent);
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
        if let Some(shake) = self.shake_state.filter(|s| s.duration > 0.0) {
            let intensity = shake.intensity * (self.shake_timer / shake.duration);
            render_camera.position.x += self.rng.gen_range(-intensity..=intensity);
            render_camera.position.y += self.rng.gen_range(-intensity..=intensity);
        }

        let draw_list = DrawList::from_world(world);
        for cmd in draw_list.commands {
            let screen = render_camera.world_to_screen(cmd.world_pos);
            let (col, row) = (screen.x.round() as i32, screen.y.round() as i32);
            // Defect D13: the texture branch used to draw and `continue`
            // before this bounds check ran, so textured sprites bypassed
            // viewport culling entirely (glyph sprites were always culled
            // correctly). Both paths now share one check up front.
            if !in_viewport(col, row, renderer.width, renderer.height) { continue; }

            match cmd.source {
                SpriteSource::Glyph { ch, bg } => {
                    renderer.draw_char_world(&render_camera, cmd.world_pos, *ch, cmd.tint, *bg);
                }
                SpriteSource::Texture { path, src } => {
                    let id = assets.load(path);
                    if let Some(t) = assets.get(id) {
                        let size = sprite_size(cmd.size, t.width, t.height, self.pixels_per_unit);
                        renderer.draw_texture_world(&render_camera, cmd.world_pos, t, size, 0.0, cmd.tint, *src);
                    }
                }
                SpriteSource::Clip { name } => {
                    // ClipFrames::Rects isn't resolved here yet (Step 3c's
                    // scope, per the plan file, is glyph clips only — no
                    // demo content needs sheet animation, and there's no
                    // way to author the rects until Phase 8's asset
                    // tooling exists).
                    if let Some(ClipFrames::Glyphs { frames }) = self.clips.get(name).map(|c| &c.frames) {
                        if !frames.is_empty() {
                            let frame = world.animators.get(&cmd.id).map(|a| a.frame).unwrap_or(0) % frames.len();
                            renderer.draw_char_world(&render_camera, cmd.world_pos, frames[frame], cmd.tint, Color::Reset);
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
            // Step 5g: reads whatever the camera is actually following —
            // itself already singular by design (one viewport) — rather
            // than a specific stored player id. For today's one-player
            // content this is the same entity either way.
            let pos = self.camera_entity.map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
            renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::Black, Color::DarkBlue);
            renderer.draw_str(0, 0, &format!(" DEBUG: {}", self.level.name), Color::White, Color::DarkBlue);
            renderer.draw_str(38, 0, &format!("x:{:.1} y:{:.1}", pos.x, pos.y), Color::Green, Color::DarkBlue);
            renderer.draw_str(renderer.width.saturating_sub(18), 0, &format!("Mode:{}", renderer.backend_name()), Color::Cyan, Color::DarkBlue);
            renderer.draw_str(renderer.width.saturating_sub(6), 0, &format!("FPS:{}", self.fps.round()), Color::White, Color::DarkBlue);
        }

        // Render last 3 log messages at the bottom of the (now full-height)
        // viewport — used to sit just above the bottom bar; there's no bar
        // to sit above anymore.
        let log_len = self.script_log.len();
        for i in 0..log_len.min(3) {
            let entry = &self.script_log[log_len - 1 - i];
            let col = match entry.level {
                ember2d_sim::scripting::LogLevel::Error => Color::Red,
                ember2d_sim::scripting::LogLevel::Warning => Color::Yellow,
                ember2d_sim::scripting::LogLevel::Info => Color::Cyan,
            };
            renderer.draw_str(1, renderer.height - 1 - i, &entry.text, col, Color::Reset);
        }

        for hud in &self.script_engine.pending_hud_draws {
            match hud {
                HudDraw::Text { x, y, text, fg, bg } => if *x < renderer.width && *y < renderer.height { renderer.draw_str(*x, *y, text, *fg, *bg); }
                HudDraw::Box { x, y, w, h, fg, bg } => renderer.draw_rect_outline(*x, *y, *w, *h, *fg, *bg),
                HudDraw::Fill { x, y, w, h, ch, fg, bg } => renderer.draw_rect_filled(*x, *y, *w, *h, *ch, *fg, *bg),
                HudDraw::Menu { x, y, w, options, selected, fg, bg, sel_fg, sel_bg } => 
                    crate::ui::Menu::new(*x, *y, *w, options.clone(), *selected).with_colors(*fg, *bg, *sel_fg, *sel_bg).draw(renderer),
                HudDraw::Panel { x, y, w, h, title, fg, bg } =>
                    crate::ui::Panel::new(*x, *y, *w, *h).with_title(title).with_colors(*fg, *bg).draw(renderer),
            }
        }
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
