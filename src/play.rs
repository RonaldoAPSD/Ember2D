// play.rs — Play mode: run a level built from a LevelData file.

mod spawn;

use std::collections::HashMap;
use std::path::Path;

use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::event::{EventBus, GameEvent};
use crate::input::Key;
use crate::level::LevelData;
use crate::math::Vec2;
use crate::renderer::color::Color;
use crate::audio::AudioEngine;
use crate::scripting::{LogEntry, ScriptEngine, HudDraw};
use crate::world::{EntityId, World};

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
    pub camera_pos: Vec2,
    pub particles: Vec<Particle>,
    pub is_loading_save: bool,
}

impl PlayState {
    pub fn from_level(data: LevelData, _persistent: HashMap<String, rhai::Dynamic>) -> Self {
        PlayState {
            player_id:          0,
            score:              0,
            total_items:        0,
            fps:                0.0,
            level:              data,
            pending_transition: None,
            script_engine:      ScriptEngine::new(),
            audio:              AudioEngine::new(),
            script_log:         Vec::new(),
            camera_entity:      None,
            exit_targets:       HashMap::new(),
            globals:            HashMap::new(),
            camera_override:    None,
            shake_state:        None,
            shake_timer:        0.0,
            camera_pos:         Vec2::ZERO,
            particles:          Vec::new(),
            is_loading_save:    false,
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
            use rand::{Rng, SeedableRng};
            let mut rng = rand::rngs::SmallRng::from_entropy();
            for req in res.particles {
                let vx = rng.gen_range(-5.0..5.0);
                let vy = rng.gen_range(-5.0..5.0);
                let life = rng.gen_range(0.2..0.8);
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
        let cam_pos = self.camera_pos;
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
}

impl GameState for PlayState {
    fn on_start(&mut self, world: &mut World, events: &mut EventBus, viewport_width: usize, viewport_height: usize) {
        if !self.is_loading_save {
            self.do_on_start(world, events, viewport_width, viewport_height);
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

        if self.camera_pos == Vec2::ZERO { self.camera_pos = target_cam; } 
        else {
            let lerp_speed = 5.0;
            self.camera_pos = self.camera_pos + (target_cam - self.camera_pos) * (1.0 - (-lerp_speed * delta_time).exp());
        }

        let cam_x = (self.camera_pos.x - viewport_width as f32 / 2.0).round();
        let cam_y = (self.camera_pos.y - game_h / 2.0).round();

        let res = self.script_engine.run_scripts(
            world, events, &mut self.script_log, delta_time, elapsed, Some(input), Some(mouse), Some(ctx.gamepad),
            &self.level.extra_spawns, self.globals.clone(), persistent, Vec2::new(cam_x, cam_y),
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
            let layer = world.colliders.get(&other).map(|c| c.layer.as_str()).unwrap_or("");
            let other_tag = world.tags.get(&other).map(|t| t.name.as_str());

            if solid { world.resolve_solid_collision(self.player_id, other, prev_positions); } 
            else if matches!(other_tag, Some("item") | Some("chest")) { to_collect.push(other); } 
            else if let Some(path) = self.exit_targets.get(&other).cloned() {
                if layer != "locked" {
                    let full_path = resolve_exit_path(&path, &self.level.path);
                    match LevelData::load(&full_path) {
                        Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                        Err(e)   => { self.script_log.push(LogEntry::warn(format!("Exit failed: {}", e))); }
                    }
                }
            }
        }
        for id in to_collect { if world.sprites.contains_key(&id) { world.despawn(id); self.score += 1; } }

        let game_h = (viewport_height as i32 - 2).max(1) as f32;
        let cam_x = (self.camera_pos.x - viewport_width as f32 / 2.0).round();
        let cam_y = (self.camera_pos.y - game_h / 2.0).round();

        let res = self.script_engine.run_collisions(
            world, &all_pairs, &mut self.script_log, delta_time, elapsed,
            &self.level.extra_spawns, self.globals.clone(), persistent, Vec2::new(cam_x, cam_y),
            (viewport_width, viewport_height),
        );
        self.apply_script_result(world, res, turn_triggered, persistent);
        self.flush_audio();
    }

    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { world, renderer, assets, .. } = ctx;
        renderer.draw_rect_filled(0, 0, renderer.width, renderer.height, ' ', Color::Reset, Color::Reset);

        let game_h = (renderer.height as i32 - 2).max(1);
        let mut cam_x = (self.camera_pos.x - renderer.width as f32 / 2.0).round() as i32;
        let mut cam_y = (self.camera_pos.y - game_h as f32 / 2.0).round() as i32;

        if let Some(shake) = self.shake_state.filter(|s| s.duration > 0.0) {
            use rand::{Rng, SeedableRng};
            let mut rng = rand::rngs::SmallRng::from_entropy();
            let intensity = shake.intensity * (self.shake_timer / shake.duration);
            cam_x += rng.gen_range(-intensity..=intensity).round() as i32;
            cam_y += rng.gen_range(-intensity..=intensity).round() as i32;
        }

        let mut draw_list: Vec<(i32, i32, i32, char, Color, Color, Option<&str>)> = world.transforms.keys().filter_map(|&id| {
            let pos = world.get_global_position(id);
            world.sprites.get(&id).and_then(|sp| {
                if !sp.visible { return None; }
                let col = pos.x.round() as i32 - cam_x;
                let row = pos.y.round() as i32 - cam_y + 1;
                let glyph = if sp.frames.is_empty() { sp.glyph } 
                else { let idx = (sp.frame_timer / sp.frame_rate) as usize % sp.frames.len(); sp.frames[idx] };
                Some((sp.z_order, col, row, glyph, sp.fg, sp.bg, sp.texture.as_deref()))
            })
        }).collect();

        draw_list.sort_unstable_by_key(|(z, ..)| *z);
        for (_, col, row, glyph, fg, bg, tex) in draw_list {
            if let Some(path) = tex { if let Ok(t) = assets.load_texture(path) { renderer.draw_texture(col * 8, row * 16, t, 32.0); continue; } }
            if col >= 0 && row >= 0 && (col as usize) < renderer.width && (row as usize) < renderer.height - 1 {
                renderer.draw_char(col as usize, row as usize, glyph, fg, bg);
            }
        }

        for p in &self.particles {
            let col = p.x.round() as i32 - cam_x; let row = p.y.round() as i32 - cam_y + 1;
            if col >= 0 && row >= 1 && (col as usize) < renderer.width && (row as usize) < renderer.height - 1 {
                renderer.draw_char(col as usize, row as usize, p.glyph, p.fg, Color::Reset);
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
