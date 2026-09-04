// scripting/engine.rs — ScriptState and ScriptEngine core.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::SystemTime;
use std::rc::Rc;
use std::cell::RefCell;
use rhai::{Engine, Scope, AST};

use crate::world::{EntityId, World};
use crate::input::InputManager;
use crate::event::EventBus;
use crate::components::{Collider, Sprite, Tag, Transform};

use super::types::*;
use super::api::ScriptCtx;

pub(super) struct ScriptState {
    pub(super) positions:        HashMap<i64, (f32, f32)>,
    pub(super) velocities:       HashMap<i64, (f32, f32)>,
    pub(super) parents:          HashMap<i64, i64>,
    /// (width, height, solid, layer, mask, locked)
    pub(super) colliders:        HashMap<i64, (f32, f32, bool, String, Vec<String>, bool)>,
    pub(super) tags:             HashMap<i64, String>,
    pub(super) glyphs:           HashMap<i64, char>,
    pub(super) colors:           HashMap<i64, (String, String)>,
    pub(super) textures:         HashMap<i64, String>,
    pub(super) tag_to_id:        HashMap<String, i64>,
    pub(super) tag_to_ids:       HashMap<String, Vec<i64>>,
    pub(super) visibility:       HashMap<i64, bool>,
    pub(super) z_orders:         HashMap<i64, i32>,
    pub(super) delta_time:       f32,
    pub(super) elapsed:          f32,
    pub(super) next_spawn_id:    EntityId,
    pub(super) held_keys:        HashSet<String>,
    pub(super) just_pressed_keys: HashSet<String>,
    pub(super) extra_spawns:     HashMap<String, (f32, f32)>,
    pub(super) mouse_pos:        (f32, f32),
    pub(super) mouse_held:       (bool, bool),
    pub(super) mouse_pressed:    (bool, bool),

    pub(super) gamepad_held:     HashSet<(usize, String)>,
    pub(super) gamepad_pressed:  HashSet<(usize, String)>,
    pub(super) gamepad_axes:     HashMap<(usize, String), f32>,

    pub(super) globals:          HashMap<String, rhai::Dynamic>,
    pub(super) persistent:       HashMap<String, rhai::Dynamic>,
    pub(super) camera_pos:       (f32, f32),
    pub(super) viewport_size:    (usize, usize),
    pub(super) pending_velocities: Vec<(i64, f32, f32)>,
    pub(super) pending_positions:  Vec<(i64, f32, f32)>,
    pub(super) pending_parents:    Vec<(i64, i64, bool)>,
    pub(super) pending_glyphs:     Vec<(i64, char)>,
    pub(super) pending_colors:     Vec<(i64, String, String)>,
    pub(super) pending_animations: Vec<(i64, Vec<char>, f32)>,
    pub(super) pending_textures:   Vec<(i64, Option<String>)>,
    pub(super) pending_hud_draws:  Vec<HudDraw>,
    pub(super) pending_particles:  Vec<ParticleRequest>,
    pub(super) clear_hud:          bool,
    pub(super) despawn_queue:      Vec<i64>,
    pub(super) spawn_queue:        Vec<SpawnRequest>,
    pub(super) pending_level:      Option<String>,
    pub(super) pending_save:       Option<String>,
    pub(super) pending_load:       Option<String>,
    pub(super) pending_logs:       Vec<String>,
    pub(super) pending_sounds:     Vec<String>,
    pub(super) pending_spatial_sounds: Vec<(String, f32, f32)>,
    pub(super) pending_music:      Option<String>,
    pub(super) stop_music:         bool,
    pub(super) pending_globals:    HashMap<String, rhai::Dynamic>,
    pub(super) pending_persistent: HashMap<String, rhai::Dynamic>,
    pub(super) pending_camera:     Option<crate::math::Vec2>,
    pub(super) pending_shake:      Option<crate::play::ShakeState>,
    pub(super) pending_visibility: Vec<(i64, bool)>,
    pub(super) pending_z_order:    Vec<(i64, i32)>,
    pub(super) pending_tags:       Vec<(i64, String)>,
    pub(super) pending_collider_size:  Vec<(i64, f32, f32)>,
    pub(super) pending_collider_solid: Vec<(i64, bool)>,
    pub(super) pending_collider_layer: Vec<(i64, String)>,
    pub(super) pending_collider_locked: Vec<(i64, bool)>,
    pub(super) pending_collider_mask:  Vec<(i64, Vec<String>)>,
    pub(super) pending_timers:     Vec<(EntityId, String, f64)>,
    pub(super) timers:             HashMap<EntityId, HashMap<String, f64>>,
    pub(super) pending_turn:       bool,
}

impl ScriptState {
    pub(super) fn from_world(
        world: &World, delta_time: f32, elapsed: f32,
        input: Option<&InputManager>, mouse: Option<&crate::mouse::MouseState>,
        gamepad: Option<&crate::gamepad::GamepadState>,
        spawns: &[(String, f32, f32)], globals: HashMap<String, rhai::Dynamic>,
        persistent: HashMap<String, rhai::Dynamic>, camera_pos: crate::math::Vec2,
        viewport_size: (usize, usize),
    ) -> Self {
        let mut positions  = HashMap::new();
        let mut velocities = HashMap::new();
        let mut parents    = HashMap::new();
        let mut colliders  = HashMap::new();
        let mut tags       = HashMap::new();
        let mut glyphs     = HashMap::new();
        let mut colors     = HashMap::new();
        let mut textures   = HashMap::new();
        let mut tag_to_id  = HashMap::new();
        let mut tag_to_ids: HashMap<String, Vec<i64>> = HashMap::new();
        let mut visibility = HashMap::new();
        let mut z_orders   = HashMap::new();

        for (id, tf) in &world.transforms {
            let eid = *id as i64;
            positions.insert(eid,  (tf.position.x, tf.position.y));
            velocities.insert(eid, (tf.velocity.x, tf.velocity.y));
            if let Some(pid) = tf.parent { parents.insert(eid, pid as i64); }
            if let Some(sp) = world.sprites.get(id) {
                glyphs.insert(eid, sp.glyph);
                colors.insert(eid, (crate::scripting::types::color_to_name(sp.fg), crate::scripting::types::color_to_name(sp.bg)));
                textures.insert(eid, sp.texture.clone().unwrap_or_default());
                visibility.insert(eid, sp.visible);
                z_orders.insert(eid, sp.z_order);
            }
        }
        for (id, col) in &world.colliders { colliders.insert(*id as i64, (col.width, col.height, col.solid, col.layer.clone(), col.mask.clone(), col.locked)); }
        for (id, tag) in &world.tags {
            let eid = *id as i64;
            tags.insert(eid, tag.name.clone());
            tag_to_id.entry(tag.name.clone()).or_insert(eid);
            tag_to_ids.entry(tag.name.clone()).or_default().push(eid);
        }
        let (held_keys, just_pressed_keys) = input.map(snapshot_keys).unwrap_or_default();
        let mouse_pos = mouse.map(|m| (m.cell_x as f32, m.cell_y as f32)).unwrap_or((0.0, 0.0));
        let mouse_held = mouse.map(|m| (m.left_held(), m.right_held())).unwrap_or((false, false));
        let mouse_pressed = mouse.map(|m| (m.left_just_pressed(), m.right_just_pressed())).unwrap_or((false, false));

        let mut gamepad_held = HashSet::new();
        let mut gamepad_pressed = HashSet::new();
        let mut gamepad_axes = HashMap::new();

        if let Some(gp) = gamepad {
            for &(id, btn) in &gp.held { gamepad_held.insert((id, btn.to_string())); }
            for &(id, btn) in &gp.consumed { gamepad_pressed.insert((id, btn.to_string())); }
            for (&(id, ax), &val) in &gp.axes { gamepad_axes.insert((id, ax.to_string()), val); }
        }

        let extra_spawns: HashMap<String, (f32, f32)> = spawns.iter().map(|(name, x, y)| (name.clone(), (*x, *y))).collect();

        ScriptState {
            positions, velocities, parents, colliders, tags, glyphs, colors, textures, tag_to_id, tag_to_ids, visibility, z_orders,
            delta_time, elapsed, next_spawn_id: world.next_id, held_keys, just_pressed_keys, extra_spawns,
            mouse_pos, mouse_held, mouse_pressed,
            gamepad_held, gamepad_pressed, gamepad_axes,
            globals, persistent, camera_pos: (camera_pos.x, camera_pos.y), viewport_size,
            pending_velocities: Vec::new(), pending_positions: Vec::new(), pending_parents: Vec::new(),
            pending_glyphs: Vec::new(), pending_colors: Vec::new(), pending_animations: Vec::new(), pending_textures: Vec::new(),
            pending_hud_draws: Vec::new(), pending_particles: Vec::new(), clear_hud: false, despawn_queue: Vec::new(),
            spawn_queue: Vec::new(), pending_level: None, pending_save: None, pending_load: None, pending_logs: Vec::new(),
            pending_sounds: Vec::new(), pending_spatial_sounds: Vec::new(),
            pending_music: None, stop_music: false, pending_globals: HashMap::new(), pending_persistent: HashMap::new(),
            pending_camera: None, pending_shake: None, pending_visibility: Vec::new(), pending_z_order: Vec::new(), pending_tags: Vec::new(),
            pending_collider_size: Vec::new(), pending_collider_solid: Vec::new(), pending_collider_layer: Vec::new(), pending_collider_mask: Vec::new(),
            pending_collider_locked: Vec::new(),
            pending_timers: Vec::new(), timers: HashMap::new(), pending_turn: false,
        }
    }
}

pub struct ScriptEngine {
    engine:    Engine,
    ast_cache: HashMap<String, AST>,
    scopes:    HashMap<EntityId, Scope<'static>>,
    mod_times: HashMap<String, SystemTime>,
    /// Script paths that threw a runtime error and are no longer called —
    /// defect D9: previously an error only suppressed its own log message
    /// (`logged_runtime_errors`) while the script kept being invoked, and
    /// kept failing, every frame. A script stays disabled until it hot-reloads
    /// successfully (see `check_hot_reload`).
    disabled_scripts: HashSet<String>,
    rng:       Rc<RefCell<rand::rngs::SmallRng>>,
    pub pending_hud_draws: Vec<HudDraw>,
    pub pending_sounds:    Vec<String>,
    pub pending_spatial_sounds: Vec<(String, f32, f32)>,
    pub pending_music:     Option<String>,
    pub stop_music:        bool,
}

impl ScriptEngine {
    /// `seed` drives every `random_*` call scripts make. Defect D3: this
    /// used to be `SmallRng::from_entropy()`, making script randomness
    /// (and anything built on it — loot, AI decisions) different on every
    /// run. Callers should pass the owning level's stored seed so replays
    /// with the same level and inputs are reproducible; this is also the
    /// determinism §5 needs for netcode later.
    pub fn new(seed: u64) -> Self {
        let mut engine = Engine::new();
        engine.register_type_with_name::<ScriptCtx>("Ctx");
        engine.register_fn("get_x",           ScriptCtx::get_x);
        engine.register_fn("get_y",           ScriptCtx::get_y);
        engine.register_fn("get_position",    ScriptCtx::get_position);
        engine.register_fn("get_vel_x",       ScriptCtx::get_vel_x);
        engine.register_fn("get_vel_y",       ScriptCtx::get_vel_y);
        engine.register_fn("get_velocity",    ScriptCtx::get_velocity);
        engine.register_fn("get_tag",         ScriptCtx::get_tag);
        engine.register_fn("set_tag",         ScriptCtx::set_tag);
        engine.register_fn("has_tag",         ScriptCtx::has_tag);
        engine.register_fn("get_glyph",       ScriptCtx::get_glyph);
        engine.register_fn("get_color",       ScriptCtx::get_color);
        engine.register_fn("get_texture",     ScriptCtx::get_texture);
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
        engine.register_fn("set_animation",   ScriptCtx::set_animation);
        engine.register_fn("set_texture",     ScriptCtx::set_texture);
        engine.register_fn("trigger_turn",    ScriptCtx::trigger_turn);
        engine.register_fn("despawn",         ScriptCtx::despawn);
        engine.register_fn("spawn_entity",    ScriptCtx::spawn_entity);
        engine.register_fn("spawn_entity",    ScriptCtx::spawn_entity_full);
        engine.register_fn("load_level",      ScriptCtx::load_level);
        engine.register_fn("log",             ScriptCtx::log);
        engine.register_fn("draw_hud",        ScriptCtx::draw_hud);
        engine.register_fn("draw_menu",       ScriptCtx::draw_menu);
        engine.register_fn("draw_panel",      ScriptCtx::draw_panel);
        engine.register_fn("play_sound",      ScriptCtx::play_sound);
        engine.register_fn("play_sound_at",   ScriptCtx::play_sound_at);
        engine.register_fn("play_music",      ScriptCtx::play_music);
        engine.register_fn("stop_music",      ScriptCtx::stop_music);
        engine.register_fn("emit_particles",  ScriptCtx::emit_particles);
        engine.register_fn("set_global",      ScriptCtx::set_global);
        engine.register_fn("get_global",      ScriptCtx::get_global);
        engine.register_fn("has_global",      ScriptCtx::has_global);
        engine.register_fn("remove_global",   ScriptCtx::remove_global);
        engine.register_fn("random_int",      ScriptCtx::random_int);
        engine.register_fn("random_float",    ScriptCtx::random_float);
        engine.register_fn("random_bool",     ScriptCtx::random_bool);
        engine.register_fn("random_choice",   ScriptCtx::random_choice);
        engine.register_fn("get_entity_at",   ScriptCtx::get_entity_at);
        engine.register_fn("is_solid_at",     ScriptCtx::is_solid_at);
        engine.register_fn("find_entities_in_rect", ScriptCtx::find_entities_in_rect);
        engine.register_fn("get_distance",    ScriptCtx::get_distance);
        engine.register_fn("get_angle_to",    ScriptCtx::get_angle_to);
        engine.register_fn("entity_exists",   ScriptCtx::entity_exists);
        engine.register_fn("count_by_tag",    ScriptCtx::count_by_tag);
        engine.register_fn("get_collider_w",  ScriptCtx::get_collider_w);
        engine.register_fn("get_collider_h",  ScriptCtx::get_collider_h);
        engine.register_fn("set_collider_size", ScriptCtx::set_collider_size);
        engine.register_fn("is_collider_solid", ScriptCtx::is_collider_solid);
        engine.register_fn("set_collider_solid", ScriptCtx::set_collider_solid);
        engine.register_fn("is_visible",      ScriptCtx::is_visible);
        engine.register_fn("set_visible",     ScriptCtx::set_visible);
        engine.register_fn("get_z_order",     ScriptCtx::get_z_order);
        engine.register_fn("set_z_order",     ScriptCtx::set_z_order);
        engine.register_fn("get_mouse_x",     ScriptCtx::get_mouse_x);
        engine.register_fn("get_mouse_y",     ScriptCtx::get_mouse_y);
        engine.register_fn("mouse_left_pressed", ScriptCtx::mouse_left_pressed);
        engine.register_fn("mouse_right_pressed", ScriptCtx::mouse_right_pressed);
        engine.register_fn("mouse_left_held", ScriptCtx::mouse_left_held);
        engine.register_fn("mouse_right_held", ScriptCtx::mouse_right_held);
        engine.register_fn("get_mouse_world_x", ScriptCtx::get_mouse_world_x);
        engine.register_fn("get_mouse_world_y", ScriptCtx::get_mouse_world_y);
        engine.register_fn("get_camera_x",    ScriptCtx::get_camera_x);
        engine.register_fn("get_camera_y",    ScriptCtx::get_camera_y);
        engine.register_fn("set_camera",      ScriptCtx::set_camera);
        engine.register_fn("shake_camera",    ScriptCtx::shake_camera);
        engine.register_fn("set_persistent",   ScriptCtx::set_persistent);
        engine.register_fn("get_persistent",   ScriptCtx::get_persistent);
        engine.register_fn("has_persistent",   ScriptCtx::has_persistent);
        engine.register_fn("clear_persistent", ScriptCtx::clear_persistent);
        engine.register_fn("clear_all_persistent", ScriptCtx::clear_all_persistent);
        engine.register_fn("draw_box",        ScriptCtx::draw_box);
        engine.register_fn("fill_rect",       ScriptCtx::fill_rect);
        engine.register_fn("clear_hud",       ScriptCtx::clear_hud);
        engine.register_fn("save_game",       ScriptCtx::save_game);
        engine.register_fn("load_game",       ScriptCtx::load_game);
        engine.register_fn("get_collider_layer", ScriptCtx::get_collider_layer);
        engine.register_fn("set_collider_layer", ScriptCtx::set_collider_layer);
        engine.register_fn("is_collider_locked", ScriptCtx::is_collider_locked);
        engine.register_fn("set_collider_locked", ScriptCtx::set_collider_locked);
        engine.register_fn("get_collider_mask", ScriptCtx::get_collider_mask);
        engine.register_fn("set_collider_mask", ScriptCtx::set_collider_mask);
        engine.register_fn("raycast",         ScriptCtx::raycast);
        engine.register_fn("get_path",        ScriptCtx::get_path);
        engine.register_fn("get_viewport_width", ScriptCtx::get_viewport_width);
        engine.register_fn("get_viewport_height", ScriptCtx::get_viewport_height);
        engine.register_fn("start_timer",     ScriptCtx::start_timer);
        engine.register_fn("timer_done",      ScriptCtx::timer_done);
        engine.register_fn("cancel_timer",    ScriptCtx::cancel_timer);
        engine.register_fn("get_parent",      ScriptCtx::get_parent);
        engine.register_fn("set_parent",      ScriptCtx::set_parent);
        engine.register_fn("set_parent_keep_world", ScriptCtx::set_parent_keep_world);
        engine.register_fn("get_world_x",     ScriptCtx::get_world_x);
        engine.register_fn("get_world_y",     ScriptCtx::get_world_y);

        // V0.5 Gamepad Extensions
        engine.register_fn("gp_is_held",      ScriptCtx::gp_is_held);
        engine.register_fn("gp_just_pressed", ScriptCtx::gp_just_pressed);
        engine.register_fn("gp_axis",         ScriptCtx::gp_axis);

        use rand::SeedableRng;
        ScriptEngine {
            engine, ast_cache: HashMap::new(), scopes: HashMap::new(), mod_times: HashMap::new(),
            disabled_scripts: HashSet::new(), rng: Rc::new(RefCell::new(rand::rngs::SmallRng::seed_from_u64(seed))),
            pending_hud_draws: Vec::new(), pending_sounds: Vec::new(), pending_spatial_sounds: Vec::new(),
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
                if let Ok(meta) = fs::metadata(path) { if let Ok(t) = meta.modified() { self.mod_times.insert(path.to_string(), t); } }
                self.ast_cache.insert(path.to_string(), ast); true
            }
            Err(e) => { log.push(LogEntry::error(format!("Compile '{}': {}", path, e))); false }
        }
    }

    pub fn run_on_start_all(&mut self, world: &mut World, log: &mut Vec<LogEntry>, extra_spawns: &[(String, f32, f32)], globals: HashMap<String, rhai::Dynamic>, persistent: &mut HashMap<String, rhai::Dynamic>, camera_pos: crate::math::Vec2, viewport_size: (usize, usize)) -> ScriptUpdateResult {
        let scripted: Vec<(i64, String)> = world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let mut ctx_state = ScriptState::from_world(world, 0.0, 0.0, None, None, None, extra_spawns, globals, persistent.clone(), camera_pos, viewport_size);
        for (entity_id, _) in &scripted {
            let scope = self.scopes.entry(*entity_id as EntityId).or_insert_with(Scope::new);
            let mut entity_timers = HashMap::new();
            for (name, _, val) in scope.iter() {
                if name.starts_with("__timer_") { if let Ok(f) = val.as_float() { entity_timers.insert(name.to_string().replace("__timer_", ""), f); } }
            }
            ctx_state.timers.insert(*entity_id as EntityId, entity_timers);
        }
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, path) in &scripted {
            if self.disabled_scripts.contains(path) { continue; }
            let Some(ast) = self.ast_cache.get(path) else { continue };
            let scope = self.scopes.entry(*entity_id as EntityId).or_insert_with(Scope::new);
            let entity_ctx = ctx.with_entity(*entity_id);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_start", (*entity_id, entity_ctx)) {
                if !e.to_string().contains("Function not found") {
                    log.push(LogEntry::error(format!("on_start '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    pub fn run_scripts(&mut self, world: &mut World, _events: &mut EventBus, log: &mut Vec<LogEntry>, delta_time: f32, elapsed: f32, input: Option<&InputManager>, mouse: Option<&crate::mouse::MouseState>, gamepad: Option<&crate::gamepad::GamepadState>, spawns: &[(String, f32, f32)], globals: HashMap<String, rhai::Dynamic>, persistent: &mut HashMap<String, rhai::Dynamic>, camera_pos: crate::math::Vec2, viewport_size: (usize, usize)) -> ScriptUpdateResult {
        self.check_hot_reload(world, log);
        let mut ctx_state = ScriptState::from_world(world, delta_time, elapsed, input, mouse, gamepad, spawns, globals, persistent.clone(), camera_pos, viewport_size);
        let scripted: Vec<(i64, String)> = world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        for (entity_id, _) in &scripted {
            let scope = self.scopes.entry(*entity_id as EntityId).or_insert_with(Scope::new);
            let timer_keys: Vec<String> = scope.iter().filter(|(n, _, _)| n.starts_with("__timer_")).map(|(n, _, _)| n.to_string()).collect();
            for key in timer_keys { if let Some(val) = scope.get_value::<f64>(&key) { scope.set_value(&key, val - delta_time as f64); } }
            let mut entity_timers = HashMap::new();
            for (name, _, val) in scope.iter() {
                if name.starts_with("__timer_") { if let Ok(f) = val.as_float() { entity_timers.insert(name.to_string().replace("__timer_", ""), f); } }
            }
            ctx_state.timers.insert(*entity_id as EntityId, entity_timers);
        }
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, path) in scripted {
            if self.disabled_scripts.contains(&path) { continue; }
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_insert_with(Scope::new);
            let entity_ctx = ctx.with_entity(entity_id);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_update", (entity_id, entity_ctx)) {
                if !e.to_string().contains("Function not found") {
                    log.push(LogEntry::error(format!("Runtime '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    pub fn run_collisions(&mut self, world: &mut World, pairs: &[(EntityId, EntityId)], log: &mut Vec<LogEntry>, delta_time: f32, elapsed: f32, spawns: &[(String, f32, f32)], globals: HashMap<String, rhai::Dynamic>, persistent: &mut HashMap<String, rhai::Dynamic>, camera_pos: crate::math::Vec2, viewport_size: (usize, usize)) -> ScriptUpdateResult {
        let scripted_paths: HashMap<EntityId, String> = world.scripts.iter().map(|(id, s)| (*id, s.path.clone())).collect();
        let mut calls: Vec<(i64, i64, String)> = Vec::new();
        for &(a, b) in pairs {
            if let Some(p) = scripted_paths.get(&a) { calls.push((a as i64, b as i64, p.clone())); }
            if let Some(p) = scripted_paths.get(&b) { calls.push((b as i64, a as i64, p.clone())); }
        }
        let mut ctx_state = ScriptState::from_world(world, delta_time, elapsed, None, None, None, spawns, globals, persistent.clone(), camera_pos, viewport_size);
        for (entity_id, _, _) in &calls {
            let scope = self.scopes.entry(*entity_id as EntityId).or_insert_with(Scope::new);
            let mut entity_timers = HashMap::new();
            for (name, _, val) in scope.iter() {
                if name.starts_with("__timer_") { if let Ok(f) = val.as_float() { entity_timers.insert(name.to_string().replace("__timer_", ""), f); } }
            }
            ctx_state.timers.insert(*entity_id as EntityId, entity_timers);
        }
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, other_id, path) in calls {
            if self.disabled_scripts.contains(&path) { continue; }
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_insert_with(Scope::new);
            let entity_ctx = ctx.with_entity(entity_id);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_collide", (entity_id, other_id, entity_ctx)) {
                if !e.to_string().contains("Function not found") {
                    log.push(LogEntry::error(format!("on_collide '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    fn check_hot_reload(&mut self, world: &World, log: &mut Vec<LogEntry>) {
        let paths: Vec<String> = self.ast_cache.keys().cloned().collect();
        for path in paths {
            let Ok(meta) = fs::metadata(&path) else { continue };
            let Ok(t)    = meta.modified()     else { continue };
            if self.mod_times.get(&path).map(|old| t > *old).unwrap_or(false) {
                match self.engine.compile_file(path.clone().into()) {
                    Ok(ast) => {
                        self.ast_cache.insert(path.clone(), ast);
                        self.mod_times.insert(path.clone(), t);
                        // Defect D9: a script disabled by a prior runtime
                        // error gets one more chance once its source changes —
                        // it re-enables here rather than staying dead forever.
                        self.disabled_scripts.remove(&path);
                        // Defect D8: this used to be `self.scopes.clear()`,
                        // wiping every entity's persistent `let` state (and
                        // __timer_* vars) whenever ANY script reloaded — not
                        // just entities running the script that changed.
                        // Only those entities need a fresh scope; everyone
                        // else's state must survive untouched.
                        let affected: Vec<EntityId> = world.scripts.iter()
                            .filter(|(_, s)| s.path == path)
                            .map(|(&id, _)| id)
                            .collect();
                        for id in affected { self.scopes.remove(&id); }
                        log.push(LogEntry::info(format!("Hot-reloaded: {}", path)));
                    }
                    Err(e) => { log.push(LogEntry::error(format!("Reload '{}': {}", path, e))); }
                }
            }
        }
    }

    fn apply_ctx(&mut self, ctx: ScriptCtx, world: &mut World, log: &mut Vec<LogEntry>) -> ScriptUpdateResult {
        let mut state = ctx.inner.borrow_mut();
        let trigger_turn = state.pending_turn; state.pending_turn = false;

        // Spawns are applied first, ahead of every other pending_* queue below:
        // a script that spawns an entity and immediately calls a setter on the
        // returned id (e.g. `set_z_order`) needs that entity to already exist
        // by the time this pass reaches the setter's queue, or the setter
        // silently no-ops against a nonexistent entity until next frame.
        for req in state.spawn_queue.drain(..) {
            world.next_id = req.id + 1; let id = req.id;
            world.transforms.insert(id, Transform::new(req.x, req.y));
            world.sprites.insert(id, Sprite::new(req.glyph, req.fg, req.bg, req.z));
            if !req.tag.is_empty() { world.add_tag(id, Tag::new(&req.tag)); }
            let mut col = Collider::new(req.w, req.h);
            col.solid = req.solid;
            col.layer = req.layer;
            world.add_collider(id, col);
        }

        for &(id, vx, vy) in &state.pending_velocities { if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) { tf.velocity.x = vx; tf.velocity.y = vy; } }
        for &(id, x, y) in &state.pending_positions { if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) { tf.position.x = x; tf.position.y = y; } }
        for &(id, pid, keep_world) in &state.pending_parents { let parent = if pid < 0 { None } else { Some(pid as EntityId) }; world.set_parent(id as EntityId, parent, keep_world); }
        for &(id, ch) in &state.pending_glyphs { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.glyph = ch; } }
        for (id, f, b) in state.pending_colors.drain(..) { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.fg = parse_color(&f); sp.bg = parse_color(&b); } }
        for (id, v) in state.pending_visibility.drain(..) { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.visible = v; } }
        for (id, z) in state.pending_z_order.drain(..) { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.z_order = z; } }
        for (id, t) in state.pending_tags.drain(..) { world.add_tag(id as EntityId, Tag::new(&t)); }
        for (id, w, h) in state.pending_collider_size.drain(..) { if let Some(col) = world.colliders.get_mut(&(id as EntityId)) { col.width = w; col.height = h; } }
        for (id, s) in state.pending_collider_solid.drain(..) { if let Some(col) = world.colliders.get_mut(&(id as EntityId)) { col.solid = s; } }
        for (id, l) in state.pending_collider_layer.drain(..) { if let Some(col) = world.colliders.get_mut(&(id as EntityId)) { col.layer = l; } }
        for (id, l) in state.pending_collider_locked.drain(..) { if let Some(col) = world.colliders.get_mut(&(id as EntityId)) { col.locked = l; } }
        for (id, m) in state.pending_collider_mask.drain(..) { if let Some(col) = world.colliders.get_mut(&(id as EntityId)) { col.mask = m; } }
        for (id, f, r) in state.pending_animations.drain(..) { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.frames = f; sp.frame_rate = r; sp.frame_timer = 0.0; } }
        for (id, p) in state.pending_textures.drain(..) { if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) { sp.texture = p; } }
        self.pending_hud_draws.extend(state.pending_hud_draws.drain(..));
        self.pending_sounds.extend(state.pending_sounds.drain(..));
        self.pending_spatial_sounds.extend(state.pending_spatial_sounds.drain(..));
        if state.pending_music.is_some() { self.pending_music = state.pending_music.take(); }
        if state.stop_music { self.stop_music = true; state.stop_music = false; }
        for msg in state.pending_logs.drain(..) { log.push(LogEntry::info(msg)); }
        let globals_to_apply: Vec<(String, rhai::Dynamic)> = state.pending_globals.drain().collect();
        for (k, v) in globals_to_apply { if v.is_unit() { state.globals.remove(&k); } else { state.globals.insert(k, v); } }

        let persistent_to_apply: Vec<(String, rhai::Dynamic)> = state.pending_persistent.drain().collect();
        for (k, v) in persistent_to_apply { if v.is_unit() { state.persistent.remove(&k); } else { state.persistent.insert(k, v); } }
        for (id, name, duration) in state.pending_timers.drain(..) { if let Some(scope) = self.scopes.get_mut(&id) { let key = format!("__timer_{}", name); if duration < -900.0 { scope.set_value(key, -1.0f64); } else { scope.set_value(key, duration); } } }
        let result = ScriptUpdateResult { pending_level: state.pending_level.take(), pending_save: state.pending_save.take(), pending_load: state.pending_load.take(), globals: state.globals.clone(), persistent: state.persistent.clone(), camera_override: state.pending_camera.take(), shake_state: state.pending_shake.take(), clear_hud: state.clear_hud, particles: state.pending_particles.drain(..).collect(), trigger_turn };
        state.clear_hud = false; let despawn_ids = state.despawn_queue.clone(); drop(state);
        for id in despawn_ids { world.despawn(id as EntityId); self.scopes.remove(&(id as EntityId)); }
        result
    }
}

// Tests split into engine_tests.rs — see that file's header comment — once
// this file crossed the project's 600-line hard limit (CLAUDE.md).
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
