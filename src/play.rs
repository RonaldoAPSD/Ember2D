// play.rs — Play mode: run a level built from a LevelData file.
//
// ── HOW PLAY MODE WORKS ───────────────────────────────────────────────────────
//
// PlayState implements GameState exactly like EmberDemo does, except that
// instead of hard-coding the world layout it reads it from a LevelData value
// (which was either loaded from a .level file on disk, or exported live from
// the editor by pressing F5).
//
// When play mode starts, on_start() iterates every TileRecord in the LevelData
// and spawns an ECS entity for each one:
//
//   solid: true  → wall entity (Transform + Sprite + Collider(solid) + Tag)
//   trigger: true → item/zone entity (Transform + Sprite + Collider(trigger) + Tag)
//   otherwise    → floor/decor entity (Transform + Sprite + Tag)
//   tag == "spawn" → skip (the player is spawned at level.spawn_point instead)
//
// The player entity is always spawned at the level's spawn_point.
//
// ── RETURNING TO THE EDITOR ───────────────────────────────────────────────────
//
// Pressing Escape stores a Transition::ToEditor in `self.pending_transition`.
// The engine's `run_until_transition()` call picks this up at the end of the
// frame and returns it to run_editor_app() in src/app.rs, which loops back
// to running the editor.

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

/// Resolve `next` relative to the directory of `current_level_path`.
/// If `next` is absolute, or if `current_level_path` is empty (e.g. playing
/// an unsaved level), it is returned as-is.
fn resolve_exit_path(next: &str, current_level_path: &str) -> String {
    if Path::new(next).is_absolute() || current_level_path.is_empty() {
        return next.to_string();
    }
    match Path::new(current_level_path).parent() {
        Some(dir) if dir != Path::new("") => {
            dir.join(next).to_string_lossy().into_owned()
        }
        _ => next.to_string(),
    }
}

// ── Z-order constants ─────────────────────────────────────────────────────────

const Z_FLOOR:  i32 = 0;
const Z_ITEM:   i32 = 1;
const Z_WALL:   i32 = 2;
const Z_PLAYER: i32 = 15; // In the middle of layer 1 (Main)

/// Player movement speed in cells per second.
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

// ── PlayState ─────────────────────────────────────────────────────────────────

/// State for the screen shake effect.
#[derive(Clone, Copy)]
pub struct ShakeState {
    pub intensity: f32,
    pub duration:  f32,
}

/// The play/preview mode: runs a level loaded from LevelData.
///
/// Created by `PlayState::from_level(data)` and passed to `engine.run_until_transition()`.
pub struct PlayState {
    /// Entity ID of the player, cached at spawn time.
    player_id: EntityId,

    /// Number of collectible items picked up so far.
    score: u32,

    /// Total collectible items present at level start.
    total_items: u32,

    /// Rolling-average FPS counter.
    fps: f32,

    /// The level data used to build this play session.
    /// Stored so on_start() can build the world from it.
    level: LevelData,

    /// Pending mode transition set when Escape is pressed.
    /// Picked up by the engine via take_transition().
    pending_transition: Option<Transition>,

    script_engine: ScriptEngine,
    audio:         AudioEngine,

    /// Errors and info messages collected during this play session.
    /// Transferred to the editor console when play mode ends.
    script_log: Vec<LogEntry>,

    /// Entity whose transform the camera centers on each frame, if any.
    camera_entity: Option<EntityId>,

    /// Maps exit-trigger entity IDs → target level file path.
    exit_targets: HashMap<EntityId, String>,

    /// Global variables accessible by all scripts in the current level.
    /// Reset on level load.
    pub globals: HashMap<String, rhai::Dynamic>,

    /// Persistent variables that survive level transitions.
    pub persistent: HashMap<String, rhai::Dynamic>,

    /// Camera override set by scripts this frame.
    pub camera_override: Option<Vec2>,

    /// Current active camera shake.
    pub shake_state: Option<ShakeState>,

    /// Timer for camera shake duration.
    pub shake_timer: f32,

    /// Viewport dimensions in character cells, updated each frame from UpdateContext.
    /// Used by on_start() for the initial camera calculation before the first update().
    viewport_w: usize,
    viewport_h: usize,

    /// Smoothed camera position in world space.
    pub camera_pos: Vec2,

    /// Active particles (short-lived, non-solid effects).
    pub particles: Vec<Particle>,
}

impl PlayState {
    /// Build a PlayState from a LevelData.
    ///
    /// The actual ECS world is not populated here — that happens in on_start()
    /// when the engine calls it. This constructor just stores the data.
    pub fn from_level(data: LevelData, persistent: HashMap<String, rhai::Dynamic>) -> Self {
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
            persistent,
            camera_override:    None,
            shake_state:        None,
            shake_timer:        0.0,
            viewport_w:         80,
            viewport_h:         40,
            camera_pos:         Vec2::ZERO,
            particles:          Vec::new(),
        }
    }

    /// Helper to process the result of a script engine run.
    fn apply_script_result(&mut self, res: crate::scripting::ScriptUpdateResult) {
        if let Some(level_path) = res.pending_level {
            let full = resolve_exit_path(&level_path, &self.level.path);
            match LevelData::load(&full) {
                Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                Err(e)   => { self.script_log.push(LogEntry::warn(format!("load_level failed: {}", e))); }
            }
        }
        self.globals = res.globals;
        self.persistent = res.persistent;
        if res.camera_override.is_some() { self.camera_override = res.camera_override; }
        if let Some(shake) = res.shake_state {
            self.shake_state = Some(shake);
            self.shake_timer = shake.duration;
        }

        // Emit requested particles
        if !res.particles.is_empty() {
            use rand::{Rng, SeedableRng};
            let mut rng = rand::rngs::SmallRng::from_entropy();
            for req in res.particles {
                let vx = rng.gen_range(-5.0..5.0);
                let vy = rng.gen_range(-5.0..5.0);
                let life = rng.gen_range(0.2..0.8);
                self.particles.push(Particle {
                    x: req.x, y: req.y, vx, vy, glyph: req.glyph, fg: req.fg, life,
                });
            }
        }
    }

    /// Map a tile's tag to a z-order draw layer.
    ///
    /// Determines what draws on top of what when multiple entities share a cell.
    fn z_for_tag(tag: &str) -> i32 {
        match tag {
            "floor" | "water" => Z_FLOOR,
            "item"  | "chest" | "danger" => Z_ITEM,
            _                 => Z_WALL,
        }
    }

    /// Drain the script log so the editor can display it in its console panel.
    pub fn take_log(&mut self) -> Vec<LogEntry> {
        std::mem::take(&mut self.script_log)
    }

    /// Apply audio requests queued by scripts this frame.
    fn flush_audio(&mut self) {
        // 1. Regular fire-and-forget sounds (full volume)
        for path in self.script_engine.pending_sounds.drain(..) {
            self.audio.play_sound(&path, 1.0);
        }

        // 2. Spatial sounds (attenuated by distance to camera)
        let cam_pos = self.camera_pos;
        let max_dist = 20.0f32;

        for (path, x, y) in self.script_engine.pending_spatial_sounds.drain(..) {
            let dx = x - cam_pos.x;
            let dy = y - cam_pos.y;
            let dist = (dx*dx + dy*dy).sqrt();
            
            // Linear falloff: 1.0 at 0 dist, 0.0 at max_dist
            let volume = (1.0 - (dist / max_dist)).clamp(0.0, 1.0);
            
            if volume > 0.01 {
                self.audio.play_sound(&path, volume as f64);
            }
        }

        if self.script_engine.stop_music {
            self.audio.stop_music();
            self.script_engine.stop_music = false;
        }
        if let Some(path) = self.script_engine.pending_music.take() {
            self.audio.play_music(&path);
        }
    }
}

// ── GameState ─────────────────────────────────────────────────────────────────

impl GameState for PlayState {
    /// Populate the ECS World with entities built from the stored LevelData.
    fn on_start(&mut self, world: &mut World, events: &mut EventBus) {
        self.do_on_start(world, events);
    }

    /// Read input, update player velocity, track FPS.
    fn update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, input, events, mouse, delta_time, elapsed, viewport_width, viewport_height, .. } = ctx;

        self.viewport_w = viewport_width;
        self.viewport_h = viewport_height;

        // Rolling FPS average.
        if delta_time > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / delta_time) * 0.1;
        }

        // Camera shake timer
        if self.shake_timer > 0.0 {
            self.shake_timer -= delta_time;
            if self.shake_timer <= 0.0 { self.shake_state = None; }
        }

        // Escape → return to editor (not quit the whole app).
        if input.just_pressed(Key::Escape) {
            self.pending_transition = Some(Transition::ToEditor);
            return;
        }

        // Movement: WASD + arrows, normalized for diagonal.
        let mut dir = Vec2::ZERO;
        if input.is_held(Key::Up)    || input.is_held(Key::W) { dir.y -= 1.0; }
        if input.is_held(Key::Down)  || input.is_held(Key::S) { dir.y += 1.0; }
        if input.is_held(Key::Left)  || input.is_held(Key::A) { dir.x -= 1.0; }
        if input.is_held(Key::Right) || input.is_held(Key::D) { dir.x += 1.0; }

        if let Some(tf) = world.transforms.get_mut(&self.player_id) {
            tf.velocity = dir.normalized() * PLAYER_SPEED;

            // Corridor alignment: when moving along one axis only, snap the
            // perpendicular coordinate to the nearest integer grid line in one
            // physics step. This keeps the player centered in single-cell corridors
            // and prevents them from getting snagged on wall corners.
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

        // Update sprite animations
        for sprite in world.sprites.values_mut() {
            if !sprite.frames.is_empty() {
                sprite.frame_timer += delta_time;
            }
        }

        // ── Camera Update ─────────────────────────────────────────────────────
        let mut target_cam = self.camera_entity
            .and_then(|id| world.transforms.get(&id))
            .map(|tf| tf.position)
            .unwrap_or(Vec2::ZERO);

        if let Some(over) = self.camera_override {
            target_cam = over;
        }

        // Apply map boundaries
        let game_h = (viewport_height as i32 - 2).max(1) as f32;
        let half_w = viewport_width as f32 / 2.0;
        let half_h = game_h / 2.0;

        let min_x = half_w;
        let max_x = (self.level.width as f32 - half_w).max(min_x);
        let min_y = half_h;
        let max_y = (self.level.height as f32 - half_h).max(min_y);

        target_cam.x = target_cam.x.clamp(min_x, max_x);
        target_cam.y = target_cam.y.clamp(min_y, max_y);

        // Smooth lerp (or snap if it's the first frame)
        if self.camera_pos == Vec2::ZERO {
            self.camera_pos = target_cam;
        } else {
            let lerp_speed = 5.0;
            self.camera_pos = self.camera_pos + (target_cam - self.camera_pos) * (1.0 - (-lerp_speed * delta_time).exp());
        }

        let cam_x = (self.camera_pos.x - viewport_width as f32 / 2.0).round();
        let cam_y = (self.camera_pos.y - game_h / 2.0).round();

        // Run all entity scripts.
        let res = self.script_engine.run_scripts(
            world, events, &mut self.script_log, delta_time, elapsed, Some(input), Some(mouse),
            &self.level.extra_spawns, self.globals.clone(), self.persistent.clone(), Vec2::new(cam_x, cam_y),
            (viewport_width, viewport_height),
        );
        self.apply_script_result(res);

        // Update particles
        self.particles.retain_mut(|p| {
            p.x += p.vx * delta_time;
            p.y += p.vy * delta_time;
            p.life -= delta_time;
            p.life > 0.0
        });

        // Drain audio requests queued by scripts.
        self.flush_audio();
    }

    /// Handle collision events: roll back solid hits, collect trigger items.
    fn late_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, events, prev_positions, delta_time, elapsed, viewport_width, viewport_height, .. } = ctx;

        let mut to_collect: Vec<EntityId> = Vec::new();
        let mut all_pairs:  Vec<(EntityId, EntityId)> = Vec::new();

        for event in events.events() {
            let GameEvent::Collision { entity_a, entity_b } = event else { continue };
            let (a, b) = (*entity_a, *entity_b);
            all_pairs.push((a, b));

            let other = if a == self.player_id {
                b
            } else if b == self.player_id {
                a
            } else {
                continue;
            };

            let solid     = world.colliders.get(&other).map(|c| c.solid).unwrap_or(false);
            let layer     = world.colliders.get(&other).map(|c| c.layer.as_str()).unwrap_or("");
            let other_tag = world.tags.get(&other).map(|t| t.name.as_str());

            if solid {
                world.resolve_solid_collision(self.player_id, other, prev_positions);
            } else if matches!(other_tag, Some("item") | Some("chest")) {
                to_collect.push(other);
            } else if let Some(path) = self.exit_targets.get(&other).cloned() {
                if layer != "locked" {
                    let full_path = resolve_exit_path(&path, &self.level.path);
                    match LevelData::load(&full_path) {
                        Ok(next_level) => {
                            self.pending_transition = Some(Transition::ToPlay(next_level));
                        }
                        Err(e) => {
                            self.script_log.push(LogEntry::warn(
                                format!("Exit failed — could not load '{}': {}", full_path, e)
                            ));
                        }
                    }
                }
            }
        }

        for id in to_collect {
            if world.sprites.contains_key(&id) {
                world.despawn(id);
                self.score += 1;
            }
        }

        // Run on_collide callbacks for scripted entities involved in collisions.
        let game_h = (viewport_height as i32 - 2).max(1) as f32;
        let cam_x = (self.camera_pos.x - viewport_width as f32 / 2.0).round();
        let cam_y = (self.camera_pos.y - game_h / 2.0).round();

        let res = self.script_engine.run_collisions(
            world, &all_pairs, &mut self.script_log, delta_time, elapsed,
            &self.level.extra_spawns, self.globals.clone(), self.persistent.clone(), Vec2::new(cam_x, cam_y),
            (viewport_width, viewport_height),
        );
        self.apply_script_result(res);

        self.flush_audio();
    }

    /// Draw the game world with HUD.
    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { world, renderer, .. } = ctx;

        // ── Background ─────────────────────────────────────────────────────────
        renderer.draw_rect_filled(
            0, 0, renderer.width, renderer.height,
            ' ', Color::Reset, Color::Reset,
        );

        // ── Camera setup ───────────────────────────────────────────────────────
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

        // ── Sprites sorted by z_order ──────────────────────────────────────────
        let mut draw_list: Vec<(i32, usize, usize, char, Color, Color)> = world
            .transforms
            .iter()
            .filter_map(|(id, tf)| {
                world.sprites.get(id).and_then(|sp| {
                    if !sp.visible { return None; }
                    let col = tf.position.x.round() as i32 - cam_x;
                    let row = tf.position.y.round() as i32 - cam_y + 1; // +1 for top HUD bar
                    if col >= 0 && row >= 1
                        && (col as usize) < renderer.width
                        && (row as usize) < renderer.height - 1
                    {
                        let glyph = if sp.frames.is_empty() {
                            sp.glyph
                        } else {
                            let frame_idx = (sp.frame_timer / sp.frame_rate) as usize % sp.frames.len();
                            sp.frames[frame_idx]
                        };
                        Some((sp.z_order, col as usize, row as usize, glyph, sp.fg, sp.bg))
                    } else {
                        None
                    }
                })
            })
            .collect();

        draw_list.sort_unstable_by_key(|(z, ..)| *z);

        for (_, col, row, glyph, fg, bg) in draw_list {
            renderer.draw_char(col, row, glyph, fg, bg);
        }

        // ── Particles ──────────────────────────────────────────────────────────
        for p in &self.particles {
            let col = p.x.round() as i32 - cam_x;
            let row = p.y.round() as i32 - cam_y + 1;
            if col >= 0 && row >= 1
                && (col as usize) < renderer.width
                && (row as usize) < renderer.height - 1
            {
                renderer.draw_char(col as usize, row as usize, p.glyph, p.fg, Color::Reset);
            }
        }

        // ── HUD — top bar ──────────────────────────────────────────────────────
        renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::Black, Color::DarkBlue);

        let title = format!(" PLAYING: {}", self.level.name);
        renderer.draw_str(0, 0, &title, Color::White, Color::DarkBlue);

        let score_str = format!("  Items {}/{}", self.score, self.total_items);
        renderer.draw_str(20, 0, &score_str, Color::Yellow, Color::DarkBlue);

        if let Some(tf) = world.transforms.get(&self.player_id) {
            let pos_str = format!("  x:{:.1} y:{:.1}", tf.position.x, tf.position.y);
            renderer.draw_str(36, 0, &pos_str, Color::Green, Color::DarkBlue);
        }

        let fps_str = format!("FPS:{:3.0}", self.fps);
        let fps_col = renderer.width.saturating_sub(fps_str.len() + 1);
        renderer.draw_str(fps_col, 0, &fps_str, Color::Cyan, Color::DarkBlue);

        // ── HUD — bottom bar ───────────────────────────────────────────────────
        renderer.draw_rect_filled(0, renderer.height - 1, renderer.width, 1,
            ' ', Color::Black, Color::DarkGrey);
        renderer.draw_str(1, renderer.height - 1,
            "WASD / Arrows: Move   Esc: Back to editor",
            Color::White, Color::DarkGrey);

        // ── Script HUD draws ──────────────────────────────────────────────────
        for hud in &self.script_engine.pending_hud_draws {
            match hud {
                HudDraw::Text { x, y, text, fg, bg } => {
                    if *x < renderer.width && *y < renderer.height {
                        renderer.draw_str(*x, *y, text, *fg, *bg);
                    }
                }
                HudDraw::Box { x, y, w, h, fg, bg } => {
                    renderer.draw_rect_outline(*x, *y, *w, *h, *fg, *bg);
                }
                HudDraw::Fill { x, y, w, h, ch, fg, bg } => {
                    renderer.draw_rect_filled(*x, *y, *w, *h, *ch, *fg, *bg);
                }
                HudDraw::Menu { x, y, w, options, selected, fg, bg, sel_fg, sel_bg } => {
                    crate::ui::Menu::new(*x, *y, *w, options.clone(), *selected)
                        .with_colors(*fg, *bg, *sel_fg, *sel_bg)
                        .draw(renderer);
                }
                HudDraw::Panel { x, y, w, h, title, fg, bg } => {
                    crate::ui::Panel::new(*x, *y, *w, *h)
                        .with_title(title)
                        .with_colors(*fg, *bg)
                        .draw(renderer);
                }
            }
        }
        self.script_engine.pending_hud_draws.clear();
    }

    /// Return any pending mode transition (set when Escape is pressed).
    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }
}
