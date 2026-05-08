// scripting/engine.rs — ScriptState and ScriptEngine core.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::SystemTime;
use rhai::{Engine, Scope, AST};

use crate::world::{EntityId, World};
use crate::input::InputManager;
use crate::event::EventBus;
use crate::components::{Collider, Sprite, Tag, Transform};
use crate::renderer::color::Color;

use super::types::*;
use super::api::ScriptCtx;

pub(super) struct ScriptState {
    pub(super) positions:        HashMap<i64, (f32, f32)>,
    pub(super) velocities:       HashMap<i64, (f32, f32)>,
    pub(super) tags:             HashMap<i64, String>,
    pub(super) tag_to_id:        HashMap<String, i64>,
    pub(super) tag_to_ids:       HashMap<String, Vec<i64>>,
    pub(super) delta_time:       f32,
    pub(super) elapsed:          f32,
    pub(super) next_spawn_id:    EntityId,
    pub(super) held_keys:        HashSet<String>,
    pub(super) just_pressed_keys: HashSet<String>,
    pub(super) extra_spawns:     HashMap<String, (f32, f32)>,

    pub(super) pending_velocities: Vec<(i64, f32, f32)>,
    pub(super) pending_positions:  Vec<(i64, f32, f32)>,
    pub(super) pending_glyphs:     Vec<(i64, char)>,
    pub(super) pending_colors:     Vec<(i64, String, String)>,
    pub(super) pending_hud_draws:  Vec<HudDraw>,
    pub(super) despawn_queue:      Vec<i64>,
    pub(super) spawn_queue:        Vec<SpawnRequest>,
    pub(super) pending_level:      Option<String>,
    pub(super) pending_logs:       Vec<String>,
    pub(super) pending_sounds:     Vec<String>,
    pub(super) pending_music:      Option<String>,
    pub(super) stop_music:         bool,
}

impl ScriptState {
    pub(super) fn from_world(world: &World, delta_time: f32, elapsed: f32, input: Option<&InputManager>, spawns: &[(String, f32, f32)]) -> Self {
        let mut positions  = HashMap::new();
        let mut velocities = HashMap::new();
        let mut tags       = HashMap::new();
        let mut tag_to_id  = HashMap::new();
        let mut tag_to_ids: HashMap<String, Vec<i64>> = HashMap::new();

        for (id, tf) in &world.transforms {
            let eid = *id as i64;
            positions.insert(eid,  (tf.position.x, tf.position.y));
            velocities.insert(eid, (tf.velocity.x, tf.velocity.y));
        }

        for (id, tag) in &world.tags {
            let eid = *id as i64;
            tags.insert(eid, tag.name.clone());
            tag_to_id.entry(tag.name.clone()).or_insert(eid);
            tag_to_ids.entry(tag.name.clone()).or_default().push(eid);
        }

        let (held_keys, just_pressed_keys) = input.map(snapshot_keys).unwrap_or_default();

        let extra_spawns: HashMap<String, (f32, f32)> = spawns.iter().map(|(name, x, y)| (name.clone(), (*x, *y))).collect();

        ScriptState {
            positions, velocities, tags, tag_to_id, tag_to_ids, delta_time, elapsed,
            next_spawn_id: world.next_id, held_keys, just_pressed_keys, extra_spawns,
            pending_velocities: Vec::new(), pending_positions: Vec::new(),
            pending_glyphs: Vec::new(), pending_colors: Vec::new(),
            pending_hud_draws: Vec::new(), despawn_queue: Vec::new(),
            spawn_queue: Vec::new(), pending_level: None, pending_logs: Vec::new(),
            pending_sounds: Vec::new(), pending_music: None, stop_music: false,
        }
    }
}

pub struct ScriptEngine {
    engine:    Engine,
    ast_cache: HashMap<String, AST>,
    scopes:    HashMap<EntityId, Scope<'static>>,
    mod_times: HashMap<String, SystemTime>,
    logged_runtime_errors: HashSet<String>,
    pub pending_hud_draws: Vec<HudDraw>,
    pub pending_sounds:    Vec<String>,
    pub pending_music:     Option<String>,
    pub stop_music:        bool,
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.register_type_with_name::<ScriptCtx>("Ctx");

        engine.register_fn("get_x",           ScriptCtx::get_x);
        engine.register_fn("get_y",           ScriptCtx::get_y);
        engine.register_fn("get_vel_x",       ScriptCtx::get_vel_x);
        engine.register_fn("get_vel_y",       ScriptCtx::get_vel_y);
        engine.register_fn("get_tag",         ScriptCtx::get_tag);
        engine.register_fn("has_tag",         ScriptCtx::has_tag);
        engine.register_fn("find_by_tag",     ScriptCtx::find_by_tag);
        engine.register_fn("find_all_by_tag", ScriptCtx::find_all_by_tag);
        engine.register_fn("is_held",         ScriptCtx::is_held);
        engine.register_fn("just_pressed",    ScriptCtx::just_pressed);
        engine.register_fn("get_spawn_point", ScriptCtx::get_spawn_point);
        engine.register_fn("get_delta",       ScriptCtx::get_delta);
        engine.register_fn("get_elapsed",     ScriptCtx::get_elapsed);
        engine.register_fn("set_velocity",    ScriptCtx::set_velocity);
        engine.register_fn("set_position",    ScriptCtx::set_position);
        engine.register_fn("set_glyph",       ScriptCtx::set_glyph);
        engine.register_fn("set_color",       ScriptCtx::set_color);
        engine.register_fn("despawn",         ScriptCtx::despawn);
        engine.register_fn("spawn",           ScriptCtx::spawn);
        engine.register_fn("load_level",      ScriptCtx::load_level);
        engine.register_fn("log",             ScriptCtx::log);
        engine.register_fn("draw_hud",        ScriptCtx::draw_hud);
        engine.register_fn("play_sound",      ScriptCtx::play_sound);
        engine.register_fn("play_music",      ScriptCtx::play_music);
        engine.register_fn("stop_music",      ScriptCtx::stop_music);

        ScriptEngine {
            engine, ast_cache: HashMap::new(), scopes: HashMap::new(),
            mod_times: HashMap::new(), logged_runtime_errors: HashSet::new(),
            pending_hud_draws: Vec::new(), pending_sounds: Vec::new(),
            pending_music: None, stop_music: false,
        }
    }

    pub fn compile_str(&mut self, key: &str, source: &str, log: &mut Vec<LogEntry>) -> bool {
        if self.ast_cache.contains_key(key) { return true; }
        match self.engine.compile(source) {
            Ok(ast) => { self.ast_cache.insert(key.to_string(), ast); true }
            Err(e) => { log.push(LogEntry::error(format!("Compile rules '{}': {}", key, e))); false }
        }
    }

    pub fn compile(&mut self, path: &str, log: &mut Vec<LogEntry>) -> bool {
        if self.ast_cache.contains_key(path) { return true; }
        match self.engine.compile_file(path.into()) {
            Ok(ast) => {
                if let Ok(meta) = fs::metadata(path) {
                    if let Ok(t) = meta.modified() { self.mod_times.insert(path.to_string(), t); }
                }
                self.ast_cache.insert(path.to_string(), ast);
                true
            }
            Err(e) => { log.push(LogEntry::error(format!("Compile '{}': {}", path, e))); false }
        }
    }

    pub fn run_on_start_all(&mut self, world: &mut World, log: &mut Vec<LogEntry>, extra_spawns: &[(String, f32, f32)]) {
        let scripted: Vec<(i64, String)> = world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let ctx = ScriptCtx::new(ScriptState::from_world(world, 0.0, 0.0, None, extra_spawns));
        for (entity_id, path) in &scripted {
            let Some(ast) = self.ast_cache.get(path) else { continue };
            let scope = self.scopes.entry(*entity_id as EntityId).or_insert_with(Scope::new);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_start", (*entity_id, ctx.clone())) {
                if !e.to_string().contains("Function not found") { log.push(LogEntry::error(format!("on_start '{}': {}", path, e))); }
            }
        }
        self.apply_ctx(ctx, world, log);
    }

    pub fn run_scripts(&mut self, world: &mut World, _events: &mut EventBus, log: &mut Vec<LogEntry>, delta_time: f32, elapsed: f32, input: Option<&InputManager>, extra_spawns: &[(String, f32, f32)]) -> Option<String> {
        self.check_hot_reload(log);
        let ctx = ScriptCtx::new(ScriptState::from_world(world, delta_time, elapsed, input, extra_spawns));
        let scripted: Vec<(i64, String)> = world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        for (entity_id, path) in scripted {
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_insert_with(Scope::new);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_update", (entity_id, ctx.clone())) {
                if !self.logged_runtime_errors.contains(&path) {
                    log.push(LogEntry::error(format!("Runtime '{}': {}", path, e)));
                    self.logged_runtime_errors.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    pub fn run_collisions(&mut self, world: &mut World, pairs: &[(EntityId, EntityId)], log: &mut Vec<LogEntry>, delta_time: f32, elapsed: f32, extra_spawns: &[(String, f32, f32)]) -> Option<String> {
        if pairs.is_empty() { return None; }
        let scripted_paths: HashMap<EntityId, String> = world.scripts.iter().map(|(id, s)| (*id, s.path.clone())).collect();
        let mut calls: Vec<(i64, i64, String)> = Vec::new();
        for &(a, b) in pairs {
            if let Some(p) = scripted_paths.get(&a) { calls.push((a as i64, b as i64, p.clone())); }
            if let Some(p) = scripted_paths.get(&b) { calls.push((b as i64, a as i64, p.clone())); }
        }
        if calls.is_empty() { return None; }
        let ctx = ScriptCtx::new(ScriptState::from_world(world, delta_time, elapsed, None, extra_spawns));
        for (entity_id, other_id, path) in calls {
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_insert_with(Scope::new);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_collide", (entity_id, other_id, ctx.clone())) {
                let msg = e.to_string();
                if !msg.contains("Function not found") && !self.logged_runtime_errors.contains(&path) {
                    log.push(LogEntry::error(format!("on_collide '{}': {}", path, e)));
                    self.logged_runtime_errors.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    fn check_hot_reload(&mut self, log: &mut Vec<LogEntry>) {
        let paths: Vec<String> = self.ast_cache.keys().cloned().collect();
        for path in paths {
            let Ok(meta) = fs::metadata(&path) else { continue };
            let Ok(t)    = meta.modified()     else { continue };
            let changed  = self.mod_times.get(&path).map(|old| t > *old).unwrap_or(false);
            if !changed { continue; }
            match self.engine.compile_file(path.clone().into()) {
                Ok(new_ast) => {
                    self.ast_cache.insert(path.clone(), new_ast);
                    self.mod_times.insert(path.clone(), t);
                    self.logged_runtime_errors.remove(&path);
                    self.scopes.clear();
                    log.push(LogEntry::info(format!("Hot-reloaded: {}", path)));
                }
                Err(e) => { log.push(LogEntry::error(format!("Reload '{}': {}", path, e))); }
            }
        }
    }

    fn apply_ctx(&mut self, ctx: ScriptCtx, world: &mut World, log: &mut Vec<LogEntry>) -> Option<String> {
        let mut state = ctx.inner.lock().unwrap();
        for &(id, vx, vy) in &state.pending_velocities { if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) { tf.velocity.x = vx; tf.velocity.y = vy; } }
        for &(id, x, y) in &state.pending_positions { if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) { tf.position.x = x; tf.position.y = y; } }
        for &(id, ch) in &state.pending_glyphs { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.glyph = ch; } }
        for (id, fg_str, bg_str) in state.pending_colors.drain(..) {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.fg = parse_color(&fg_str); sp.bg = parse_color(&bg_str);
            }
        }
        for req in state.spawn_queue.drain(..) {
            world.next_id = req.id + 1;
            let id = req.id;
            world.transforms.insert(id, Transform::new(req.x, req.y));
            world.sprites.insert(id, Sprite::new(req.glyph, Color::White, Color::Reset, 2));
            if !req.tag.is_empty() { world.add_tag(id, Tag::new(&req.tag)); }
            world.add_collider(id, Collider::trigger(1.0, 1.0));
        }
        self.pending_hud_draws.extend(state.pending_hud_draws.drain(..));
        self.pending_sounds.extend(state.pending_sounds.drain(..));
        if state.pending_music.is_some() { self.pending_music = state.pending_music.take(); }
        if state.stop_music { self.stop_music = true; state.stop_music = false; }
        for msg in state.pending_logs.drain(..) { log.push(LogEntry::info(msg)); }
        let pending_level = state.pending_level.take();
        let despawn_ids: Vec<i64> = state.despawn_queue.clone();
        drop(state);
        for id in despawn_ids { world.despawn(id as EntityId); self.scopes.remove(&(id as EntityId)); }
        pending_level
    }
}
