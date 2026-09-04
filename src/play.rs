// play.rs — Play mode: run a level built from a LevelData file.

mod spawn;

use std::collections::HashMap;
use std::path::Path;

use crate::camera::Camera;
use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::event::{EventBus, GameEvent};
use crate::input::Key;
use crate::level::LevelData;
use crate::math::Vec2;
use crate::renderer::color::Color;
use crate::audio::AudioEngine;
use crate::scripting::{LogEntry, ScriptEngine, HudDraw};
use crate::world::{EntityId, World};
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;

// ── Path resolution ───────────────────────────────────────────────────────────

fn resolve_exit_path(next: &str, current_level_path: &str) -> String {
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

// ── Z-order constants ─────────────────────────────────────────────────────────

const Z_FLOOR:  i32 = 0;
const Z_ITEM:   i32 = 1;
const Z_WALL:   i32 = 2;
const Z_PLAYER: i32 = 15;

const PLAYER_SPEED: f32 = 10.0;

/// Rows reserved for HUD chrome above the playable viewport (currently just
/// the top status bar). The single source of truth for what used to be a
/// bare `+1`/`-1` literal hand-duplicated between the render loop and
/// `get_mouse_world_y` (`scripting/api.rs`) — see `Camera::viewport_origin`,
/// which both now go through.
pub const HUD_TOP_ROWS: i32 = 1;

// ── Draw list ────────────────────────────────────────────────────────────────
//
// Step 2d of docs/ember2d-refactor-plan.md: everything the entity draw loop
// used to build inline (an ad hoc tuple, sorted only by (z, id) for defect
// D5's sake) is now a real DrawCommand/DrawList, sorted by (space, z,
// texture, id). The texture dimension is the point of this step: WgpuBackend
// only merges *consecutive* same-texture instances into one draw call
// (`ensure_batch`), so a list that happened to interleave glyphs and
// textures degenerated into one draw call per sprite. Sorting by texture
// before submission means every sprite sharing a texture (including the
// font atlas, which every glyph implicitly shares) lands adjacent.
//
// `layer` from the plan's (space, layer, z, texture) isn't included yet —
// `Sprite` has no field distinct from `z_order` to sort by, so there's
// nothing to add without inventing data that doesn't exist. `z_order`
// already folds in the tile's authored layer (`z_for_tag(tag) + layer*10`,
// see play/spawn.rs), so this isn't a functional gap, just a naming one
// Phase 3's sprite model may resolve.
//
// Step 2e: commands now carry a real world-space position instead of a
// pre-subtracted screen col/row — `Space::World` was a lie otherwise
// (screen coordinates labeled "World"). The camera conversion happens once,
// in `render`, via `Camera::world_to_screen`.

/// World vs. screen space — every command built today is `World`; `Screen`
/// exists so HUD/particle work in later phases has somewhere to go without
/// another format change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Space { World, Screen }

pub struct DrawCommand<'w> {
    pub space: Space,
    pub z: i32,
    /// Sort tiebreak only, not display data — see defect D5's rationale.
    pub id: EntityId,
    pub world_pos: Vec2,
    pub glyph: char,
    pub fg: Color,
    pub bg: Color,
    pub texture: Option<&'w str>,
}

pub struct DrawList<'w> {
    pub commands: Vec<DrawCommand<'w>>,
}

impl<'w> DrawList<'w> {
    /// Collect every visible sprite, sorted for rendering. Free
    /// function-shaped (an associated fn with no `&self`) so it's testable
    /// without a live GPU-backed `Renderer`. No camera involved here at
    /// all — that conversion happens per-command in `render`.
    fn from_world(world: &'w World) -> Self {
        let mut commands: Vec<DrawCommand<'w>> = world.transforms.keys().filter_map(|&id| {
            let pos = world.get_global_position(id);
            world.sprites.get(&id).and_then(|sp| {
                if !sp.visible { return None; }
                let glyph = if sp.frames.is_empty() { sp.glyph }
                else { let idx = (sp.frame_timer / sp.frame_rate) as usize % sp.frames.len(); sp.frames[idx] };
                Some(DrawCommand { space: Space::World, z: sp.z_order, id, world_pos: pos, glyph, fg: sp.fg, bg: sp.bg, texture: sp.texture.as_deref() })
            })
        }).collect();

        commands.sort_unstable_by_key(|c| (c.space, c.z, c.texture, c.id));
        DrawList { commands }
    }
}

/// True if a screen cell at (col, row) falls inside the playable viewport —
/// i.e. on screen and above the bottom HUD bar (the last row is reserved).
///
/// Shared by both the glyph and texture draw paths in `render` (defect D13:
/// the texture path used to skip this check entirely, since it `continue`d
/// before the bounds test ran).
fn in_viewport(col: i32, row: i32, width: usize, height: usize) -> bool {
    col >= 0 && row >= 0 && (col as usize) < width && (row as usize) < height.saturating_sub(1)
}

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

#[derive(Clone, Copy)]
pub struct ShakeState {
    pub intensity: f32,
    pub duration:  f32,
}

pub struct PlayState {
    player_id: EntityId,
    score: u32,
    total_items: u32,
    fps: f32,
    level: LevelData,
    pending_transition: Option<Transition>,
    script_engine: ScriptEngine,
    audio:         AudioEngine,
    script_log: Vec<LogEntry>,
    camera_entity: Option<EntityId>,
    exit_targets: HashMap<EntityId, String>,
    pub globals: HashMap<String, rhai::Dynamic>,
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
}

/// A different constant offset from the level's seed for `PlayState::rng`
/// than the one `ScriptEngine` seeds from directly (see `from_level`), so
/// script randomness and particle/shake randomness are two independent
/// deterministic streams instead of mirroring each other's sequence.
const PLAYSTATE_RNG_SEED_OFFSET: u64 = 0x9E3779B97F4A7C15; // splitmix64's golden-ratio constant

impl PlayState {
    pub fn from_level(data: LevelData, _persistent: HashMap<String, rhai::Dynamic>) -> Self {
        let seed = data.seed;
        PlayState {
            player_id:          0,
            score:              0,
            total_items:        0,
            fps:                0.0,
            level:              data,
            pending_transition: None,
            script_engine:      ScriptEngine::new(seed),
            audio:              AudioEngine::new(),
            script_log:         Vec::new(),
            camera_entity:      None,
            exit_targets:       HashMap::new(),
            globals:            HashMap::new(),
            camera_override:    None,
            shake_state:        None,
            shake_timer:        0.0,
            camera:             Camera::new(0.0, 0.0), // real dimensions set every update()
            particles:          Vec::new(),
            is_loading_save:    false,
            rng:                SmallRng::seed_from_u64(seed.wrapping_add(PLAYSTATE_RNG_SEED_OFFSET)),
        }
    }

    pub fn from_save(level_data: LevelData, _persistent: HashMap<String, rhai::Dynamic>) -> Self {
        let mut ps = Self::from_level(level_data, _persistent);
        ps.is_loading_save = true;
        ps
    }

    fn apply_script_result(&mut self, world: &mut World, res: crate::scripting::ScriptUpdateResult, turn_triggered: &mut bool, persistent: &mut HashMap<String, rhai::Dynamic>) {
        if res.trigger_turn { *turn_triggered = true; }
        if let Some(level_path) = res.pending_level {
            let full = resolve_exit_path(&level_path, &self.level.path);
            match LevelData::load(&full) {
                Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                Err(e)   => { self.script_log.push(LogEntry::warn(format!("load_level failed: {}", e))); }
            }
        }
        self.globals = res.globals;
        *persistent = res.persistent;

        if let Some(save_path) = res.pending_save {
            let state = crate::save::SaveState::new(world.clone(), persistent.clone(), self.level.path.clone());
            if let Err(e) = state.save_to_file(&save_path) {
                self.script_log.push(LogEntry::error(format!("save_game failed: {}", e)));
            } else {
                self.script_log.push(LogEntry::info(format!("Game saved to {}", save_path)));
            }
        }

        if let Some(load_path) = res.pending_load {
            match crate::save::SaveState::load_from_file(&load_path) {
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

    fn z_for_tag(tag: &str) -> i32 {
        match tag {
            "floor" | "water" => Z_FLOOR,
            "item"  | "chest" | "danger" => Z_ITEM,
            _                 => Z_WALL,
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
    fn on_start(&mut self, world: &mut World, events: &mut EventBus, viewport_width: usize, viewport_height: usize, persistent: &mut HashMap<String, rhai::Dynamic>) {
        if !self.is_loading_save {
            self.do_on_start(world, events, viewport_width, viewport_height, persistent);
        } else {
            if let Some(id) = world.find_by_tag("player") { self.player_id = id; }
            for (_, script) in &world.scripts { self.script_engine.compile(&script.path, &mut self.script_log); }
            if self.camera_entity.is_none() { self.camera_entity = Some(self.player_id); }
        }
    }

    fn update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, input, mouse, delta_time, elapsed, viewport_width, viewport_height, events, turn_triggered, persistent, .. } = ctx;

        if delta_time > 0.0 { self.fps = self.fps * 0.9 + (1.0 / delta_time) * 0.1; }
        if self.shake_timer > 0.0 {
            self.shake_timer -= delta_time;
            if self.shake_timer <= 0.0 { self.shake_state = None; }
        }

        if input.just_pressed(Key::Escape) {
            self.pending_transition = Some(Transition::Push(Box::new(PauseMenuState::new())));
            return;
        }

        let mut dir = Vec2::ZERO;
        if input.is_held(Key::Up)    || input.is_held(Key::W) { dir.y -= 1.0; }
        if input.is_held(Key::Down)  || input.is_held(Key::S) { dir.y += 1.0; }
        if input.is_held(Key::Left)  || input.is_held(Key::A) { dir.x -= 1.0; }
        if input.is_held(Key::Right) || input.is_held(Key::D) { dir.x += 1.0; }

        if let Some(tf) = world.transforms.get_mut(&self.player_id) {
            tf.velocity = dir.normalized() * PLAYER_SPEED;
            if delta_time > 0.0 {
                if dir.x != 0.0 && dir.y == 0.0 {
                    let snap = tf.position.y.round() - tf.position.y;
                    tf.velocity.y = snap / delta_time;
                } else if dir.y != 0.0 && dir.x == 0.0 {
                    let snap = tf.position.x.round() - tf.position.x;
                    tf.velocity.x = snap / delta_time;
                }
            }
        }

        for sprite in world.sprites.values_mut() { if !sprite.frames.is_empty() { sprite.frame_timer += delta_time; } }

        let mut target_cam = self.camera_entity.map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
        if let Some(over) = self.camera_override { target_cam = over; }

        let game_h = (viewport_height as i32 - 2).max(1) as f32;
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
            let lerp_speed = 5.0;
            self.camera.position = self.camera.position + (target_cam - self.camera.position) * (1.0 - (-lerp_speed * delta_time).exp());
        }
        self.camera.viewport_width = viewport_width as f32;
        self.camera.viewport_height = game_h;
        self.camera.viewport_origin = Vec2::new(0.0, HUD_TOP_ROWS as f32);
        self.camera.zoom = 1.0; // Phase 2 doesn't add a scripted zoom control yet

        let camera_origin = self.script_camera_origin();
        let res = self.script_engine.run_scripts(
            world, events, &mut self.script_log, delta_time, elapsed, Some(input), Some(mouse), Some(ctx.gamepad),
            &self.level.extra_spawns, self.globals.clone(), persistent, camera_origin,
            (viewport_width, viewport_height),
        );
        self.apply_script_result(world, res, turn_triggered, persistent);

        self.particles.retain_mut(|p| {
            p.x += p.vx * delta_time; p.y += p.vy * delta_time; p.life -= delta_time; p.life > 0.0
        });
        self.flush_audio();
    }

    fn late_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, events, prev_positions, delta_time, elapsed, viewport_width, viewport_height, turn_triggered, persistent, .. } = ctx;
        let mut to_collect = Vec::new();
        let mut all_pairs = Vec::new();

        for event in events.events() {
            let GameEvent::Collision { entity_a, entity_b } = event else { continue };
            let (a, b) = (*entity_a, *entity_b);
            all_pairs.push((a, b));
            let other = if a == self.player_id { b } else if b == self.player_id { a } else { continue };
            let solid = world.colliders.get(&other).map(|c| c.solid).unwrap_or(false);
            // Defect D12: this used to read the collider's `layer` string and
            // compare it against the magic value "locked", which corrupted
            // the layer field's real purpose (collision filtering) for any
            // locked exit tile. `locked` is now its own flag.
            let locked = world.colliders.get(&other).map(|c| c.locked).unwrap_or(false);
            let other_tag = world.tags.get(&other).map(|t| t.name.as_str());

            if solid { world.resolve_solid_collision(self.player_id, other, prev_positions); }
            else if matches!(other_tag, Some("item") | Some("chest")) { to_collect.push(other); }
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
        for id in to_collect { if world.sprites.contains_key(&id) { world.despawn(id); self.score += 1; } }

        // self.camera was already refreshed this step by the preceding
        // update() call (see engine.rs's per-step order: update, physics,
        // collisions, late_update) — no need to recompute it here.
        let camera_origin = self.script_camera_origin();
        let res = self.script_engine.run_collisions(
            world, &all_pairs, &mut self.script_log, delta_time, elapsed,
            &self.level.extra_spawns, self.globals.clone(), persistent, camera_origin,
            (viewport_width, viewport_height),
        );
        self.apply_script_result(world, res, turn_triggered, persistent);
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
            if let Some(path) = cmd.texture {
                if let Ok(t) = assets.load_texture(path) {
                    // Preserves the exact size the old `draw_texture(.., 32.0)`
                    // magic-scale call produced at zoom 1 — real per-sprite
                    // sizing is Phase 3's Sprite::size, not this step's job.
                    let size = Vec2::new(t.width as f32 * 4.0, t.height as f32 * 4.0) * (1.0 / render_camera.zoom);
                    renderer.draw_texture_world(&render_camera, cmd.world_pos, t, size, 0.0, Color::White);
                    continue;
                }
            }
            renderer.draw_char_world(&render_camera, cmd.world_pos, cmd.glyph, cmd.fg, cmd.bg);
        }

        for p in &self.particles {
            let world_pos = Vec2::new(p.x, p.y);
            let screen = render_camera.world_to_screen(world_pos);
            let (col, row) = (screen.x.round() as i32, screen.y.round() as i32);
            if in_viewport(col, row, renderer.width, renderer.height) {
                renderer.draw_char_world(&render_camera, world_pos, p.glyph, p.fg, Color::Reset);
            }
        }

        renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::Black, Color::DarkBlue);
        renderer.draw_str(0, 0, &format!(" PLAYING: {}", self.level.name), Color::White, Color::DarkBlue);
        renderer.draw_str(24, 0, &format!("Items {}/{}", self.score, self.total_items), Color::Yellow, Color::DarkBlue);
        let pos = world.get_global_position(self.player_id);
        renderer.draw_str(38, 0, &format!("x:{:.1} y:{:.1}", pos.x, pos.y), Color::Green, Color::DarkBlue);
        renderer.draw_str(renderer.width.saturating_sub(18), 0, &format!("Mode:{}", renderer.backend_name()), Color::Cyan, Color::DarkBlue);
        renderer.draw_str(renderer.width.saturating_sub(6), 0, &format!("FPS:{}", self.fps.round()), Color::White, Color::DarkBlue);

        renderer.draw_rect_filled(0, renderer.height - 1, renderer.width, 1, ' ', Color::Black, Color::DarkGrey);
        renderer.draw_str(1, renderer.height - 1, "WASD / Arrows: Move   Esc: Pause", Color::White, Color::DarkGrey);

        // Render last 3 log messages above the bottom bar
        let log_len = self.script_log.len();
        for i in 0..log_len.min(3) {
            let entry = &self.script_log[log_len - 1 - i];
            let col = match entry.level {
                crate::scripting::LogLevel::Error => Color::Red,
                crate::scripting::LogLevel::Warning => Color::Yellow,
                crate::scripting::LogLevel::Info => Color::Cyan,
            };
            renderer.draw_str(1, renderer.height - 2 - i, &entry.text, col, Color::Reset);
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
        self.script_engine.pending_hud_draws.clear();
    }

    fn take_transition(&mut self) -> Option<Transition> { self.pending_transition.take() }
}

// Tests split into play/tests.rs — see that file's header comment — once
// this file approached the project's 600-line hard limit (CLAUDE.md).
#[cfg(test)]
mod tests;
