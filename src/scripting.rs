// scripting.rs — Rhai scripting integration.
//
// ── HOW SCRIPTING WORKS ───────────────────────────────────────────────────────
//
// Scripts are plain text files with a `.rhai` extension. Each script defines
// callback functions that the engine calls at specific moments:
//
//   fn on_start(entity_id, ctx)             — called once when the level loads
//   fn on_update(entity_id, ctx)            — called every frame
//   fn on_collide(entity_id, other_id, ctx) — called when the entity overlaps another
//
// The `ctx` argument is a ScriptCtx object — a proxy that lets scripts safely
// read and write game state without direct access to the Rust World struct.
//
// ── FULL SCRIPT API ───────────────────────────────────────────────────────────
//
//   READ (position / velocity / tags)
//   ctx.get_x(id)                       → f64    position x of entity `id`
//   ctx.get_y(id)                       → f64    position y of entity `id`
//   ctx.get_vel_x(id)                   → f64    velocity x of entity `id`
//   ctx.get_vel_y(id)                   → f64    velocity y of entity `id`
//   ctx.get_tag(id)                     → String tag name of entity `id`
//   ctx.has_tag(id, "name")             → bool   true if entity has that tag
//   ctx.find_by_tag("name")             → i64    first entity ID with that tag, -1 if none
//   ctx.find_all_by_tag("name")         → Array  all entity IDs with that tag
//
//   READ (input)
//   ctx.is_held("space")                → bool   true while the key is held
//   ctx.just_pressed("e")               → bool   true only the frame the key went down
//   Keys: "w" "a" "s" "d" "e" "q" "r" "f" "z" "x" "c" "v"
//         "up" "down" "left" "right" "space" "enter" "escape"
//         "shift" "ctrl" "tab" "1"–"9" "0"
//
//   READ (time)
//   ctx.get_delta()                     → f64    seconds since last frame
//   ctx.get_elapsed()                   → f64    total seconds since level started
//
//   READ (spawn points)
//   ctx.get_spawn_point("name")         → Array  [x, y] or [] if not found
//
//   WRITE (deferred — applied after all scripts run this frame)
//   ctx.set_velocity(id, vx, vy)        → ()     set velocity
//   ctx.set_position(id, x, y)          → ()     teleport entity (knockback, respawn)
//   ctx.set_glyph(id, "X")             → ()     change the displayed character
//   ctx.set_color(id, "Red", "Reset")   → ()     change fg/bg color by name
//   ctx.despawn(id)                     → ()     remove entity next frame
//   ctx.spawn("*", x, y, "bullet")     → i64    create entity, returns its ID
//   ctx.load_level("path.level")       → ()     transition to another level
//   ctx.log("message")                 → ()     print to editor console
//   ctx.draw_hud(x, y, "text", fg, bg) → ()     draw text overlay (drawn after sprites)
//   ctx.play_sound("hit.ogg")          → ()     play a sound effect once
//   ctx.play_music("theme.ogg")        → ()     loop music (stops previous)
//   ctx.stop_music()                   → ()     stop current music
//
// ── SCRIPT PERSISTENCE ────────────────────────────────────────────────────────
//
// Each entity's Rhai scope is preserved between frames so `let` variables
// declared in on_update persist across calls (patrol timers, cooldowns, etc.).
// Scopes are cleared when the entity is despawned.
//
// ── HOT-RELOAD ────────────────────────────────────────────────────────────────
//
// During play mode, the engine checks .rhai files each frame. If a file's
// modification time changes, it is recompiled automatically and the result is
// logged to the editor console. This allows live tweaking without pressing Esc.
//
// ── THREAD SAFETY ────────────────────────────────────────────────────────────
//
// ScriptCtx wraps ScriptState in Arc<Mutex<>>. All clones of ScriptCtx for a
// given frame share the same pending mutations so writes from one entity's
// script are batched with writes from all others.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use rhai::{Array, Dynamic, Engine, Scope, AST};

use crate::components::{Collider, Sprite, Tag, Transform};
use crate::event::EventBus;
use crate::input::{InputManager, Key};
use crate::renderer::color::Color;
use crate::world::{EntityId, World};

// ── Console log types (used by editor console panel) ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel { Error, Warning, Info }

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text:  String,
}

impl LogEntry {
    pub fn error(text: impl Into<String>) -> Self { LogEntry { level: LogLevel::Error,   text: text.into() } }
    pub fn warn(text:  impl Into<String>) -> Self { LogEntry { level: LogLevel::Warning, text: text.into() } }
    pub fn info(text:  impl Into<String>) -> Self { LogEntry { level: LogLevel::Info,    text: text.into() } }
}

// ── HudDraw ───────────────────────────────────────────────────────────────────

/// A draw-text request queued by a script during update, consumed during render.
pub struct HudDraw {
    pub x:    usize,
    pub y:    usize,
    pub text: String,
    pub fg:   Color,
    pub bg:   Color,
}

// ── SpawnRequest ──────────────────────────────────────────────────────────────

struct SpawnRequest {
    id:    EntityId,
    glyph: char,
    x:     f32,
    y:     f32,
    tag:   String,
}

// ── Color name → Color enum ───────────────────────────────────────────────────

fn parse_color(name: &str) -> Color {
    match name.trim() {
        "Black"               => Color::Black,
        "White"               => Color::White,
        "Red"                 => Color::Red,
        "Green"               => Color::Green,
        "Yellow"              => Color::Yellow,
        "Cyan"                => Color::Cyan,
        "DarkBlue"            => Color::DarkBlue,
        "DarkGrey"|"DarkGray" => Color::DarkGrey,
        "DarkGreen"           => Color::DarkGreen,
        "Grey"|"Gray"         => Color::Grey,
        _                     => Color::Reset,
    }
}

// ── Key name snapshot ─────────────────────────────────────────────────────────

/// Build held/just-pressed sets of lowercase key name strings from InputManager.
/// Scripts call ctx.is_held("space"), ctx.just_pressed("e"), etc.
fn snapshot_keys(input: &InputManager) -> (HashSet<String>, HashSet<String>) {
    // Map of (Key variant, script name) for every key scripts can query.
    const KEY_MAP: &[(Key, &str)] = &[
        (Key::W, "w"), (Key::A, "a"), (Key::S, "s"), (Key::D, "d"),
        (Key::Q, "q"), (Key::E, "e"), (Key::R, "r"), (Key::F, "f"),
        (Key::Z, "z"), (Key::X, "x"), (Key::C, "c"), (Key::V, "v"),
        (Key::Up, "up"), (Key::Down, "down"), (Key::Left, "left"), (Key::Right, "right"),
        (Key::Space, "space"), (Key::Enter, "enter"), (Key::Escape, "escape"),
        (Key::LeftShift, "shift"), (Key::RightShift, "shift"),
        (Key::LeftCtrl, "ctrl"),  (Key::RightCtrl, "ctrl"),
        (Key::Key1, "1"), (Key::Key2, "2"), (Key::Key3, "3"),
        (Key::Key4, "4"), (Key::Key5, "5"), (Key::Key6, "6"),
        (Key::Key7, "7"), (Key::Key8, "8"), (Key::Key9, "9"), (Key::Key0, "0"),
        (Key::Tab, "tab"),
    ];

    let mut held         = HashSet::new();
    let mut just_pressed = HashSet::new();
    for (key, name) in KEY_MAP {
        if input.is_held(*key)      { held.insert(name.to_string()); }
        if input.just_pressed(*key) { just_pressed.insert(name.to_string()); }
    }
    (held, just_pressed)
}

// ── ScriptState ───────────────────────────────────────────────────────────────

/// Snapshot of world state for a single frame, plus deferred mutation queues.
/// Built fresh each frame; scripts read from the snapshot and write to the queues.
struct ScriptState {
    // ── Read-only snapshot ────────────────────────────────────────────────────
    positions:        HashMap<i64, (f32, f32)>,
    velocities:       HashMap<i64, (f32, f32)>,
    tags:             HashMap<i64, String>,
    tag_to_id:        HashMap<String, i64>,       // first entity with this tag
    tag_to_ids:       HashMap<String, Vec<i64>>,  // all entities with this tag
    delta_time:       f32,
    elapsed:          f32,
    next_spawn_id:    EntityId,   // pre-allocated counter for ctx.spawn() return values
    held_keys:        HashSet<String>,
    just_pressed_keys: HashSet<String>,
    extra_spawns:     HashMap<String, (f32, f32)>,

    // ── Deferred writes (applied after all scripts run) ───────────────────────
    pending_velocities: Vec<(i64, f32, f32)>,
    pending_positions:  Vec<(i64, f32, f32)>,
    pending_glyphs:     Vec<(i64, char)>,
    pending_colors:     Vec<(i64, String, String)>,
    pending_hud_draws:  Vec<HudDraw>,
    despawn_queue:      Vec<i64>,
    spawn_queue:        Vec<SpawnRequest>,
    pending_level:      Option<String>,
    pending_logs:       Vec<String>,
    pending_sounds:     Vec<String>,
    pending_music:      Option<String>,
    stop_music:         bool,
}

impl ScriptState {
    fn from_world(
        world:        &World,
        delta_time:   f32,
        elapsed:      f32,
        input:        Option<&InputManager>,
        spawns:       &[(String, f32, f32)],
    ) -> Self {
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

        let (held_keys, just_pressed_keys) = input
            .map(snapshot_keys)
            .unwrap_or_default();

        let extra_spawns: HashMap<String, (f32, f32)> = spawns
            .iter()
            .map(|(name, x, y)| (name.clone(), (*x, *y)))
            .collect();

        ScriptState {
            positions,
            velocities,
            tags,
            tag_to_id,
            tag_to_ids,
            delta_time,
            elapsed,
            next_spawn_id:     world.next_id,
            held_keys,
            just_pressed_keys,
            extra_spawns,
            pending_velocities: Vec::new(),
            pending_positions:  Vec::new(),
            pending_glyphs:     Vec::new(),
            pending_colors:     Vec::new(),
            pending_hud_draws:  Vec::new(),
            despawn_queue:      Vec::new(),
            spawn_queue:        Vec::new(),
            pending_level:      None,
            pending_logs:       Vec::new(),
            pending_sounds:     Vec::new(),
            pending_music:      None,
            stop_music:         false,
        }
    }
}

// ── ScriptCtx ─────────────────────────────────────────────────────────────────

/// The context object scripts receive. Clone + Send + Sync so Rhai can copy it
/// freely; all clones share the same Arc<Mutex<ScriptState>>.
#[derive(Clone)]
pub struct ScriptCtx {
    inner: Arc<Mutex<ScriptState>>,
}

impl ScriptCtx {
    fn new(state: ScriptState) -> Self {
        ScriptCtx { inner: Arc::new(Mutex::new(state)) }
    }

    // ── Read: position / velocity ─────────────────────────────────────────────
    // All floats are f64 at the Rhai boundary — Rhai's native float type is f64.

    pub fn get_x(&mut self, id: i64) -> f64 {
        self.inner.lock().unwrap().positions.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0)
    }
    pub fn get_y(&mut self, id: i64) -> f64 {
        self.inner.lock().unwrap().positions.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0)
    }
    pub fn get_vel_x(&mut self, id: i64) -> f64 {
        self.inner.lock().unwrap().velocities.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0)
    }
    pub fn get_vel_y(&mut self, id: i64) -> f64 {
        self.inner.lock().unwrap().velocities.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0)
    }

    // ── Read: tags ────────────────────────────────────────────────────────────

    pub fn get_tag(&mut self, id: i64) -> String {
        self.inner.lock().unwrap().tags.get(&id).cloned().unwrap_or_default()
    }
    pub fn has_tag(&mut self, id: i64, name: String) -> bool {
        self.inner.lock().unwrap().tags.get(&id).map(|t| t == &name).unwrap_or(false)
    }
    pub fn find_by_tag(&mut self, tag: String) -> i64 {
        self.inner.lock().unwrap().tag_to_id.get(&tag).copied().unwrap_or(-1)
    }
    pub fn find_all_by_tag(&mut self, tag: String) -> Array {
        self.inner.lock().unwrap()
            .tag_to_ids.get(&tag)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(Dynamic::from)
            .collect()
    }

    // ── Read: input ───────────────────────────────────────────────────────────

    pub fn is_held(&mut self, key: String) -> bool {
        self.inner.lock().unwrap().held_keys.contains(&key)
    }
    pub fn just_pressed(&mut self, key: String) -> bool {
        self.inner.lock().unwrap().just_pressed_keys.contains(&key)
    }

    // ── Read: named spawn points ──────────────────────────────────────────────

    /// Returns [x, y] for the named spawn point, or an empty array if not found.
    pub fn get_spawn_point(&mut self, name: String) -> Array {
        self.inner.lock().unwrap()
            .extra_spawns.get(&name)
            .map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)])
            .unwrap_or_default()
    }

    // ── Read: time ────────────────────────────────────────────────────────────

    pub fn get_delta(&mut self) -> f64 {
        self.inner.lock().unwrap().delta_time as f64
    }
    pub fn get_elapsed(&mut self) -> f64 {
        self.inner.lock().unwrap().elapsed as f64
    }

    // ── Write: motion / appearance ────────────────────────────────────────────

    pub fn set_velocity(&mut self, id: i64, vx: f64, vy: f64) {
        self.inner.lock().unwrap().pending_velocities.push((id, vx as f32, vy as f32));
    }
    pub fn set_position(&mut self, id: i64, x: f64, y: f64) {
        self.inner.lock().unwrap().pending_positions.push((id, x as f32, y as f32));
    }
    pub fn set_glyph(&mut self, id: i64, glyph_str: String) {
        if let Some(ch) = glyph_str.chars().next() {
            self.inner.lock().unwrap().pending_glyphs.push((id, ch));
        }
    }
    pub fn set_color(&mut self, id: i64, fg: String, bg: String) {
        self.inner.lock().unwrap().pending_colors.push((id, fg, bg));
    }

    // ── Write: entity lifecycle ───────────────────────────────────────────────

    pub fn despawn(&mut self, id: i64) {
        self.inner.lock().unwrap().despawn_queue.push(id);
    }
    /// Spawn a new trigger entity. It appears in the world next frame.
    /// Find it by tag using `ctx.find_by_tag` after the frame completes.
    /// Spawn a new trigger entity. Returns the entity ID it will receive.
    /// The ID is valid immediately — use it in set_velocity/set_position the same frame.
    pub fn spawn(&mut self, glyph_str: String, x: f64, y: f64, tag: String) -> i64 {
        let glyph = glyph_str.chars().next().unwrap_or('?');
        let mut s = self.inner.lock().unwrap();
        let id = s.next_spawn_id;
        s.next_spawn_id += 1;
        s.spawn_queue.push(SpawnRequest { id, glyph, x: x as f32, y: y as f32, tag });
        id as i64
    }

    // ── Write: game flow ──────────────────────────────────────────────────────

    /// Request a level transition. Only the first call per frame takes effect.
    pub fn load_level(&mut self, path: String) {
        let mut s = self.inner.lock().unwrap();
        if s.pending_level.is_none() { s.pending_level = Some(path); }
    }
    /// Write a message to the editor console (visible after returning from play).
    pub fn log(&mut self, msg: String) {
        self.inner.lock().unwrap().pending_logs.push(msg);
    }
    /// Draw text on screen this frame at cell (x, y). Drawn after sprites so always on top.
    pub fn draw_hud(&mut self, x: i64, y: i64, text: String, fg: String, bg: String) {
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw {
            x: x as usize,
            y: y as usize,
            text,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
        });
    }

    // ── Write: audio ──────────────────────────────────────────────────────────

    /// Play a sound file once (fire-and-forget).
    pub fn play_sound(&mut self, path: String) {
        self.inner.lock().unwrap().pending_sounds.push(path);
    }
    /// Start looping music. Stops any currently playing music first.
    pub fn play_music(&mut self, path: String) {
        let mut s = self.inner.lock().unwrap();
        s.pending_music = Some(path);
    }
    /// Stop the current music track.
    pub fn stop_music(&mut self) {
        self.inner.lock().unwrap().stop_music = true;
    }
}

// ── ScriptEngine ──────────────────────────────────────────────────────────────

/// Owns the Rhai engine, compiled ASTs, per-entity scopes, and hot-reload state.
/// Lives inside `PlayState` for the duration of a play session.
pub struct ScriptEngine {
    engine:    Engine,
    ast_cache: HashMap<String, AST>,
    /// Per-entity persistent variable scopes — reused across frames so
    /// `let` variables in on_update() survive between calls.
    scopes:    HashMap<EntityId, Scope<'static>>,
    /// File modification times for hot-reload detection.
    mod_times: HashMap<String, SystemTime>,
    /// Tracks scripts that have already logged a runtime error to avoid flooding.
    logged_runtime_errors: HashSet<String>,
    /// HUD draw calls queued during update, consumed in PlayState::render().
    pub pending_hud_draws: Vec<HudDraw>,
    /// Sound effect paths to play this frame (fire-and-forget).
    pub pending_sounds:    Vec<String>,
    /// Music path to start looping, if any.
    pub pending_music:     Option<String>,
    /// If true, stop current music this frame.
    pub stop_music:        bool,
}

impl ScriptEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine.register_type_with_name::<ScriptCtx>("Ctx");

        // ── Read: position / velocity ─────────────────────────────────────────
        engine.register_fn("get_x",           ScriptCtx::get_x);
        engine.register_fn("get_y",           ScriptCtx::get_y);
        engine.register_fn("get_vel_x",       ScriptCtx::get_vel_x);
        engine.register_fn("get_vel_y",       ScriptCtx::get_vel_y);
        // ── Read: tags ────────────────────────────────────────────────────────
        engine.register_fn("get_tag",         ScriptCtx::get_tag);
        engine.register_fn("has_tag",         ScriptCtx::has_tag);
        engine.register_fn("find_by_tag",     ScriptCtx::find_by_tag);
        engine.register_fn("find_all_by_tag", ScriptCtx::find_all_by_tag);
        // ── Read: input ───────────────────────────────────────────────────────
        engine.register_fn("is_held",         ScriptCtx::is_held);
        engine.register_fn("just_pressed",    ScriptCtx::just_pressed);
        // ── Read: named spawn points ──────────────────────────────────────────
        engine.register_fn("get_spawn_point", ScriptCtx::get_spawn_point);
        // ── Read: time ────────────────────────────────────────────────────────
        engine.register_fn("get_delta",       ScriptCtx::get_delta);
        engine.register_fn("get_elapsed",     ScriptCtx::get_elapsed);
        // ── Write: motion / appearance ────────────────────────────────────────
        engine.register_fn("set_velocity",    ScriptCtx::set_velocity);
        engine.register_fn("set_position",    ScriptCtx::set_position);
        engine.register_fn("set_glyph",       ScriptCtx::set_glyph);
        engine.register_fn("set_color",       ScriptCtx::set_color);
        // ── Write: entity lifecycle ───────────────────────────────────────────
        engine.register_fn("despawn",         ScriptCtx::despawn);
        engine.register_fn("spawn",           ScriptCtx::spawn);
        // ── Write: game flow ──────────────────────────────────────────────────
        engine.register_fn("load_level",      ScriptCtx::load_level);
        engine.register_fn("log",             ScriptCtx::log);
        engine.register_fn("draw_hud",        ScriptCtx::draw_hud);
        // ── Write: audio ──────────────────────────────────────────────────────
        engine.register_fn("play_sound",      ScriptCtx::play_sound);
        engine.register_fn("play_music",      ScriptCtx::play_music);
        engine.register_fn("stop_music",      ScriptCtx::stop_music);

        ScriptEngine {
            engine,
            ast_cache:             HashMap::new(),
            scopes:                HashMap::new(),
            mod_times:             HashMap::new(),
            logged_runtime_errors: HashSet::new(),
            pending_hud_draws:     Vec::new(),
            pending_sounds:        Vec::new(),
            pending_music:         None,
            stop_music:            false,
        }
    }

    /// Compile a Rhai source string and cache the AST under `key`.
    /// Unlike `compile`, this never touches the filesystem and never hot-reloads.
    /// Used for rules-generated scripts that live only in memory.
    pub fn compile_str(&mut self, key: &str, source: &str, log: &mut Vec<LogEntry>) -> bool {
        if self.ast_cache.contains_key(key) { return true; }
        match self.engine.compile(source) {
            Ok(ast) => {
                self.ast_cache.insert(key.to_string(), ast);
                true
            }
            Err(e) => {
                log.push(LogEntry::error(format!("Compile rules '{}': {}", key, e)));
                false
            }
        }
    }

    /// Compile a .rhai script and cache the AST. Records the modification time
    /// for hot-reload. Returns true on success; errors go to `log`.
    pub fn compile(&mut self, path: &str, log: &mut Vec<LogEntry>) -> bool {
        if self.ast_cache.contains_key(path) { return true; }
        match self.engine.compile_file(path.into()) {
            Ok(ast) => {
                if let Ok(meta) = fs::metadata(path) {
                    if let Ok(t) = meta.modified() {
                        self.mod_times.insert(path.to_string(), t);
                    }
                }
                self.ast_cache.insert(path.to_string(), ast);
                true
            }
            Err(e) => {
                log.push(LogEntry::error(format!("Compile '{}': {}", path, e)));
                false
            }
        }
    }

    /// Called once after all level entities are spawned.
    /// Calls `on_start(entity_id, ctx)` in each script that defines it.
    pub fn run_on_start_all(
        &mut self,
        world:        &mut World,
        log:          &mut Vec<LogEntry>,
        extra_spawns: &[(String, f32, f32)],
    ) {
        let scripted: Vec<(i64, String)> = world.scripts
            .iter()
            .map(|(id, s)| (*id as i64, s.path.clone()))
            .collect();

        let ctx = ScriptCtx::new(ScriptState::from_world(world, 0.0, 0.0, None, extra_spawns));

        for (entity_id, path) in &scripted {
            let Some(ast) = self.ast_cache.get(path) else { continue };
            let scope = self.scopes.entry(*entity_id as EntityId).or_insert_with(Scope::new);
            if let Err(e) = self.engine.call_fn::<()>(scope, ast, "on_start", (*entity_id, ctx.clone())) {
                // on_start is optional — only log genuine runtime errors
                if !e.to_string().contains("Function not found") {
                    log.push(LogEntry::error(format!("on_start '{}': {}", path, e)));
                }
            }
        }

        self.apply_ctx(ctx, world, log);
    }

    /// Called every frame from PlayState::update().
    /// Runs `on_update(entity_id, ctx)` for every scripted entity.
    /// Returns Some(path) if a script called `ctx.load_level(...)`.
    pub fn run_scripts(
        &mut self,
        world:        &mut World,
        _events:      &mut EventBus,
        log:          &mut Vec<LogEntry>,
        delta_time:   f32,
        elapsed:      f32,
        input:        Option<&InputManager>,
        extra_spawns: &[(String, f32, f32)],
    ) -> Option<String> {
        self.check_hot_reload(log);

        let ctx = ScriptCtx::new(ScriptState::from_world(world, delta_time, elapsed, input, extra_spawns));

        let scripted: Vec<(i64, String)> = world.scripts
            .iter()
            .map(|(id, s)| (*id as i64, s.path.clone()))
            .collect();

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

    /// Called from PlayState::late_update() with all collision pairs for this frame.
    /// Calls `on_collide(entity_id, other_id, ctx)` on any scripted entity involved
    /// in a collision. Returns Some(path) if a script called `ctx.load_level(...)`.
    pub fn run_collisions(
        &mut self,
        world:        &mut World,
        pairs:        &[(EntityId, EntityId)],
        log:          &mut Vec<LogEntry>,
        delta_time:   f32,
        elapsed:      f32,
        extra_spawns: &[(String, f32, f32)],
    ) -> Option<String> {
        if pairs.is_empty() { return None; }

        // Build the list of (scripted_entity, other_entity, script_path) calls.
        let scripted_paths: HashMap<EntityId, String> = world.scripts
            .iter()
            .map(|(id, s)| (*id, s.path.clone()))
            .collect();

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
                // on_collide is optional — skip "not found" quietly
                if !msg.contains("Function not found") && !self.logged_runtime_errors.contains(&path) {
                    log.push(LogEntry::error(format!("on_collide '{}': {}", path, e)));
                    self.logged_runtime_errors.insert(path.clone());
                }
            }
        }

        self.apply_ctx(ctx, world, log)
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    /// Check all cached scripts for file changes and recompile modified ones.
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
                    self.scopes.clear(); // clear all scopes; stale vars could misbehave
                    log.push(LogEntry::info(format!("Hot-reloaded: {}", path)));
                }
                Err(e) => {
                    log.push(LogEntry::error(format!("Reload '{}': {}", path, e)));
                }
            }
        }
    }

    /// Apply all pending mutations from a ScriptCtx to the world.
    /// Returns Some(path) if any script called `ctx.load_level(path)`.
    fn apply_ctx(&mut self, ctx: ScriptCtx, world: &mut World, log: &mut Vec<LogEntry>) -> Option<String> {
        let mut state = ctx.inner.lock().unwrap();

        // Velocities
        for &(id, vx, vy) in &state.pending_velocities {
            if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) {
                tf.velocity.x = vx;
                tf.velocity.y = vy;
            }
        }

        // Positions (teleport / knockback)
        for &(id, x, y) in &state.pending_positions {
            if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) {
                tf.position.x = x;
                tf.position.y = y;
            }
        }

        // Glyph changes
        for &(id, ch) in &state.pending_glyphs {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.glyph = ch;
            }
        }

        // Color changes
        for (id, fg_str, bg_str) in state.pending_colors.drain(..) {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.fg = parse_color(&fg_str);
                sp.bg = parse_color(&bg_str);
            }
        }

        // Spawn new entities using pre-allocated IDs so ctx.spawn() return values are valid.
        for req in state.spawn_queue.drain(..) {
            // Advance world.next_id to match the pre-allocated ID.
            world.next_id = req.id + 1;
            let id = req.id;
            world.transforms.insert(id, Transform::new(req.x, req.y));
            world.sprites.insert(id, Sprite::new(req.glyph, Color::White, Color::Reset, 2));
            if !req.tag.is_empty() { world.add_tag(id, Tag::new(&req.tag)); }
            world.add_collider(id, Collider::trigger(1.0, 1.0));
        }

        // HUD draws — collected into ScriptEngine so render() can read them
        self.pending_hud_draws.extend(state.pending_hud_draws.drain(..));

        // Audio — merge into ScriptEngine fields; play.rs drains them after run_scripts
        self.pending_sounds.extend(state.pending_sounds.drain(..));
        if state.pending_music.is_some() { self.pending_music = state.pending_music.take(); }
        if state.stop_music { self.stop_music = true; state.stop_music = false; }

        // Collect logs before releasing the lock
        for msg in state.pending_logs.drain(..) {
            log.push(LogEntry::info(msg));
        }

        let pending_level = state.pending_level.take();

        // Release lock before despawn (which borrows world mutably)
        let despawn_ids: Vec<i64> = state.despawn_queue.clone();
        drop(state);

        for id in despawn_ids {
            world.despawn(id as EntityId);
            self.scopes.remove(&(id as EntityId)); // clean up persistent scope
        }

        pending_level
    }
}
