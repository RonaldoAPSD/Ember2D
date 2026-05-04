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

use std::collections::HashMap;
use std::path::Path;

use crate::components::{Collider, Script, Sprite, Tag, Transform};
use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::event::{EventBus, GameEvent};
use crate::input::Key;
use crate::level::LevelData;
use crate::math::Vec2;
use crate::renderer::color::Color;
use crate::audio::AudioEngine;
use crate::scripting::{LogEntry, ScriptEngine};
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
const Z_PLAYER: i32 = 3;

/// Player movement speed in cells per second.
const PLAYER_SPEED: f32 = 10.0;

// ── PlayState ─────────────────────────────────────────────────────────────────

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
}

impl PlayState {
    /// Build a PlayState from a LevelData.
    ///
    /// The actual ECS world is not populated here — that happens in on_start()
    /// when the engine calls it. This constructor just stores the data.
    pub fn from_level(data: LevelData) -> Self {
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
        for path in self.script_engine.pending_sounds.drain(..) {
            self.audio.play_sound(&path);
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
    fn on_start(&mut self, world: &mut World, _events: &mut EventBus) {
        let mut item_count   = 0u32;
        let mut scripts_ok   = 0u32;
        let mut scripts_fail = 0u32;

        for tile in &self.level.tiles {
            if tile.tag == "spawn" { continue; }

            let id = world.spawn();
            world.add_transform(id, Transform::new(tile.x as f32, tile.y as f32));

            let z = Self::z_for_tag(&tile.tag);
            world.add_sprite(id, Sprite::new(tile.glyph, tile.fg, tile.bg, z));

            if tile.solid {
                world.add_collider(id, Collider::unit());
            } else if tile.trigger {
                world.add_collider(id, Collider::trigger(1.0, 1.0));
                if tile.tag == "item" || tile.tag == "chest" { item_count += 1; }
            }

            if !tile.tag.is_empty() { world.add_tag(id, Tag::new(&tile.tag)); }

            // Priority 1: node graph; 2: .rhai file
            let mut source = String::new();
            if let Some(ref graph) = tile.graph {
                source = crate::editor::node_graph::generate_graph(graph);
            }
            if !source.is_empty() {
                if let Some(ref path) = tile.script {
                    let full = resolve_exit_path(path, &self.level.path);
                    if let Ok(file_src) = std::fs::read_to_string(&full) {
                        source.push('\n');
                        source.push_str(&file_src);
                    }
                }
                let key = format!("__script_{}", id);
                if self.script_engine.compile_str(&key, &source, &mut self.script_log) {
                    scripts_ok += 1;
                } else {
                    scripts_fail += 1;
                }
                world.add_script(id, Script::new(&key));
            } else if let Some(script_path) = &tile.script {
                world.add_script(id, Script::new(script_path));
                if self.script_engine.compile(script_path, &mut self.script_log) {
                    scripts_ok += 1;
                } else {
                    scripts_fail += 1;
                }
            }

            if tile.camera_follow && self.camera_entity.is_none() {
                self.camera_entity = Some(id);
            }

            if let Some(ref path) = tile.next_level {
                self.exit_targets.insert(id, path.clone());
            }
        }

        self.total_items = item_count;

        // Spawn the player at the level's stored spawn point.
        let (sx, sy) = self.level.spawn_point;
        let player = world.spawn();
        world.add_transform(player, Transform::new(sx, sy));

        let pr = &self.level.player;
        world.add_sprite(player, Sprite::new(pr.glyph, pr.fg, pr.bg, Z_PLAYER));
        // Slightly smaller than 1×1 so the player has a 0.125-unit tolerance on each
        // side when squeezing through 1-cell corridors (walls are still 1×1).
        world.add_collider(player, Collider::new(0.75, 0.75));
        world.add_tag(player, Tag::new(&pr.tag));

        if let Some(ref script_path) = pr.script.clone() {
            world.add_script(player, Script::new(script_path));
            if self.script_engine.compile(script_path, &mut self.script_log) {
                scripts_ok += 1;
            } else {
                scripts_fail += 1;
            }
        }

        if pr.camera_follow && self.camera_entity.is_none() {
            self.camera_entity = Some(player);
        }

        self.player_id = player;

        // Summarise compilation results in the console log.
        if scripts_ok + scripts_fail > 0 {
            let msg = format!("{} script(s) compiled, {} failed", scripts_ok, scripts_fail);
            if scripts_fail > 0 {
                self.script_log.push(LogEntry::warn(msg));
            } else {
                self.script_log.push(LogEntry::info(msg));
            }
        }

        // Call on_start() for all scripted entities now that the world is fully populated.
        self.script_engine.run_on_start_all(world, &mut self.script_log, &self.level.extra_spawns);
    }

    /// Read input, update player velocity, track FPS.
    fn update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, input, events, quit, delta_time, elapsed, .. } = ctx;

        // Rolling FPS average.
        if delta_time > 0.0 {
            self.fps = self.fps * 0.9 + (1.0 / delta_time) * 0.1;
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

        // Run all entity scripts. Scripts read the world snapshot and write
        // deferred velocity/despawn commands that are applied inside run_scripts().
        // This runs AFTER player velocity is set so scripts can overwrite it.
        if let Some(level_path) = self.script_engine.run_scripts(world, events, &mut self.script_log, delta_time, elapsed, Some(input), &self.level.extra_spawns) {
            let full = resolve_exit_path(&level_path, &self.level.path);
            match LevelData::load(&full) {
                Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                Err(e)   => { self.script_log.push(LogEntry::warn(format!("load_level failed: {}", e))); }
            }
        }

        // Drain audio requests queued by scripts.
        self.flush_audio();

        let _ = quit;
    }

    /// Handle collision events: roll back solid hits, collect trigger items.
    fn late_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { world, events, prev_positions, delta_time, elapsed, .. } = ctx;

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
            let other_tag = world.tags.get(&other).map(|t| t.name.as_str());

            if solid {
                world.resolve_solid_collision(self.player_id, other, prev_positions);
            } else if matches!(other_tag, Some("item") | Some("chest")) {
                to_collect.push(other);
            } else if let Some(path) = self.exit_targets.get(&other).cloned() {
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

        for id in to_collect {
            if world.sprites.contains_key(&id) {
                world.despawn(id);
                self.score += 1;
            }
        }

        // Run on_collide callbacks for scripted entities involved in collisions.
        if let Some(level_path) = self.script_engine.run_collisions(
            world, &all_pairs, &mut self.script_log, delta_time, elapsed, &self.level.extra_spawns,
        ) {
            if self.pending_transition.is_none() {
                let full = resolve_exit_path(&level_path, &self.level.path);
                match LevelData::load(&full) {
                    Ok(next) => { self.pending_transition = Some(Transition::ToPlay(next)); }
                    Err(e)   => { self.script_log.push(LogEntry::warn(format!("load_level failed: {}", e))); }
                }
            }
        }

        self.flush_audio();
    }

    /// Draw the game world with HUD.
    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { world, renderer, elapsed, .. } = ctx;

        // ── Background ─────────────────────────────────────────────────────────
        renderer.draw_rect_filled(
            0, 0, renderer.width, renderer.height,
            ' ', Color::Reset, Color::Reset,
        );

        // ── Camera offset (entity with camera_follow=true) ────────────────────
        // Game area occupies rows 1..height-1 (top and bottom HUD bars excluded).
        let game_h = (renderer.height as i32 - 2).max(1);
        let (cam_x, cam_y) = self.camera_entity
            .and_then(|id| world.transforms.get(&id))
            .map(|tf| {
                let cx = (tf.position.x - renderer.width as f32 / 2.0).max(0.0).round() as i32;
                let cy = (tf.position.y - game_h as f32 / 2.0).max(0.0).round() as i32;
                (cx, cy)
            })
            .unwrap_or((0, 0));

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
                        Some((sp.z_order, col as usize, row as usize, sp.glyph, sp.fg, sp.bg))
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

        // ── Collectible pulse animation ────────────────────────────────────────
        if (elapsed * 3.0) as u32 % 2 == 0 {
            for (id, tf) in &world.transforms {
                let tag = world.tags.get(id).map(|t| t.name.as_str());
                if matches!(tag, Some("item") | Some("chest")) {
                    let col = tf.position.x.round() as i32 - cam_x;
                    let row = tf.position.y.round() as i32 - cam_y + 1; // +1 for top HUD bar
                    if col >= 0 && row >= 1
                        && (col as usize) < renderer.width
                        && (row as usize) < renderer.height - 1
                    {
                        renderer.draw_char(col as usize, row as usize, '$', Color::White, Color::Reset);
                    }
                }
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
            if hud.x < renderer.width && hud.y < renderer.height {
                renderer.draw_str(hud.x, hud.y, &hud.text, hud.fg, hud.bg);
            }
        }
        self.script_engine.pending_hud_draws.clear();

        // ── Victory banner ────────────────────────────────────────────────────
        if self.total_items > 0 && self.score >= self.total_items {
            let msg = " ALL COLLECTED!  Press Esc to return to editor. ";
            let col = (renderer.width / 2).saturating_sub(msg.len() / 2);
            let row = renderer.height / 2;
            renderer.draw_rect_filled(
                col.saturating_sub(1), row - 1, msg.len() + 2, 3,
                ' ', Color::Black, Color::Green,
            );
            renderer.draw_str(col, row, msg, Color::Black, Color::Green);
        }
    }

    /// Return any pending mode transition (set when Escape is pressed).
    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }
}
