// scripting/engine.rs — ScriptState and ScriptEngine core.

use rhai::{Engine, Scope, AST};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::rc::Rc;
use std::time::SystemTime;

use crate::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::AnimationClip;
use crate::world::{EntityId, World};

use super::api::ScriptCtx;
use super::types::*;

// ScriptState (the per-frame snapshot + write-queue scripts see through
// ScriptCtx) lives in state.rs, a sibling module declared in
// scripting/mod.rs (not here) since api.rs needs it too — see that file's
// header comment for why it was split out of engine.rs.
use super::state::{ScriptState, WorldSnapshot};

/// How often `run_scripts` actually invokes `check_hot_reload`, in calls
/// (one call per simulation step — see `run_scripts`'s own call site).
/// Phase 6 Step 6 (docs/ember2d-phase6-plan.md): `check_hot_reload` issues
/// one `fs::metadata` syscall per cached script *every single step* it
/// runs, real measured cost proportional to script count, not entity
/// count. A step counter throttles this deterministically — `Instant::now()`
/// in `ember2d-sim` would be a determinism violation (this crate's own
/// rule; see CLAUDE.md's Determinism section) since two machines' wall
/// clocks don't advance in lockstep the way step counts do under replay.
/// The tradeoff: a live script edit can take up to this many steps (0.5s at
/// 60 steps/s) to be noticed instead of showing up on the very next one —
/// an imperceptible delay for the dev-time-only workflow this exists for.
const HOT_RELOAD_CHECK_INTERVAL: u32 = 30;

pub struct ScriptEngine {
    engine: Engine,
    ast_cache: HashMap<String, AST>,
    /// `pub(super)` (not private) since `apply.rs`'s `apply_ctx` — a second
    /// `impl ScriptEngine` block in a sibling file, Phase 6 Step 2 — reads
    /// and removes entries directly (timer write-back, despawn cleanup).
    pub(super) scopes: HashMap<EntityId, Scope<'static>>,
    mod_times: HashMap<String, SystemTime>,
    /// Script paths that threw a runtime error and are no longer called —
    /// defect D9: previously an error only suppressed its own log message
    /// (`logged_runtime_errors`) while the script kept being invoked, and
    /// kept failing, every frame. A script stays disabled until it hot-reloads
    /// successfully (see `check_hot_reload`).
    disabled_scripts: HashSet<String>,
    rng: Rc<RefCell<rand::rngs::SmallRng>>,
    /// Phase 6 Step 6 (docs/ember2d-phase6-plan.md): counts calls to
    /// `run_scripts` so hot-reload checking can run every
    /// `HOT_RELOAD_CHECK_INTERVAL` of them instead of every one — see that
    /// constant's own doc comment for why a counter, not `Instant::now()`.
    hot_reload_counter: u32,
    /// Phase 6 Step 7 (docs/ember2d-phase6-plan.md): this engine's own copy
    /// of the level's layer name<->bit table — needed by `apply_ctx`'s
    /// `set_collider_layer`/`set_collider_mask` handling and by every
    /// `WorldSnapshot::build` call this file makes. `Simulation` holds an
    /// independent copy of the same registry (built from the same
    /// `LevelData.collision_layers`, passed in here at construction) rather
    /// than this being a shared reference — see `Simulation::layers`'s own
    /// doc comment for why. `pub(super)` (not private), same reasoning as
    /// `scopes` above: `apply.rs`'s `apply_ctx` — a second `impl
    /// ScriptEngine` block in a sibling file — reads it directly.
    pub(super) layers: crate::layers::LayerRegistry,
    /// Phase 6 Step 9 (docs/ember2d-phase6-plan.md): per-entity timer values,
    /// now plain engine-owned state instead of being smuggled through each
    /// entity's Rhai `Scope` as `__timer_<name>` variables scanned out by
    /// string-prefix every single script pass (five near-identical scan
    /// blocks, one per `run_*` method below, all deleted by this step).
    /// `BTreeMap`, not `HashMap` — matches this crate's blanket "no HashMap
    /// iteration in sim code" rule (CLAUDE.md's Determinism section) even
    /// though nothing here iterates it in an order-sensitive way today;
    /// consistent beats "provably safe this one time." Round-trips through
    /// `ScriptState.timers` via `mem::take`/put-back exactly like
    /// `globals`/`clips`/`persistent` already do (Step 3) — `apply_ctx`
    /// (`apply.rs`) is what puts it back. **Not part of `SaveState`:**
    /// timers are silently lost across a save/load, same as before this
    /// step (see `docs/ember2d-scripting-api.md`'s Timers section) —
    /// unchanged behavior, just now documented rather than an accident of
    /// where the state happened to live.
    pub(super) timers: BTreeMap<EntityId, BTreeMap<String, f64>>,
    pub pending_hud_draws: Vec<HudDraw>,
    pub pending_sounds: Vec<String>,
    pub pending_spatial_sounds: Vec<(String, f32, f32)>,
    pub pending_music: Option<String>,
    pub stop_music: bool,
}

impl ScriptEngine {
    /// `seed` drives every `random_*` call scripts make. Defect D3: this
    /// used to be `SmallRng::from_entropy()`, making script randomness
    /// (and anything built on it — loot, AI decisions) different on every
    /// run. Callers should pass the owning level's stored seed so replays
    /// with the same level and inputs are reproducible; this is also the
    /// determinism §5 needs for netcode later.
    pub fn new(seed: u64, layers: crate::layers::LayerRegistry) -> Self {
        let mut engine = Engine::new();
        // R1 (7A-1, docs/ember2d-master-plan.md): an unbounded script
        // (`loop {}`, or any accidental infinite loop) used to hang this
        // thread forever — nothing here capped how long a single call into
        // Rhai could run. 2,000,000 is a generous budget (floor2's full
        // on_turn pass measures far under this via bench_sim); a script
        // that exceeds it gets Rhai's own `ErrorTooManyOperations`, which
        // every `run_*` call site below already treats like any other
        // runtime error (not `is_missing_optional_fn`, so it logs and
        // disables the script) — no separate handling needed here.
        engine.set_max_operations(2_000_000);
        // 7A-10 (docs/ember2d-master-plan.md §5.1, R43): the full Rhai
        // `register_fn` sequence (Rhai name -> ScriptCtx method) used to be
        // inline here — 7A-9's `cargo fmt --all` alone (no logic change)
        // pushed this file to 790/750 lines (CLAUDE.md's hard limit). Pulled
        // into its own file since it's pure mechanical wiring, same calls,
        // same order, same section comments, nothing renamed.
        super::registry::register_all(&mut engine);

        use rand::SeedableRng;
        ScriptEngine {
            engine,
            ast_cache: HashMap::new(),
            scopes: HashMap::new(),
            mod_times: HashMap::new(),
            disabled_scripts: HashSet::new(),
            rng: Rc::new(RefCell::new(rand::rngs::SmallRng::seed_from_u64(seed))),
            hot_reload_counter: 0,
            layers,
            timers: BTreeMap::new(),
            pending_hud_draws: Vec::new(),
            pending_sounds: Vec::new(),
            pending_spatial_sounds: Vec::new(),
            pending_music: None,
            stop_music: false,
        }
    }

    pub fn compile_str(&mut self, key: &str, source: &str, log: &mut Vec<LogEntry>) -> bool {
        if self.ast_cache.contains_key(key) {
            return true;
        }
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

    pub fn compile(&mut self, path: &str, log: &mut Vec<LogEntry>) -> bool {
        if self.ast_cache.contains_key(path) {
            return true;
        }
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

    /// True only if `err` is Rhai's report that the optional lifecycle
    /// function `fn_name` ("on_start"/"on_update"/"on_collide") doesn't
    /// exist in this script at all — the one case every call site below is
    /// meant to ignore silently.
    ///
    /// Defect D15 (docs/ember2d-refactor-plan.md): this used to be
    /// `e.to_string().contains("Function not found")`, a bare substring
    /// check on the error's Display output. Rhai reports a genuinely
    /// missing top-level function as `ErrorFunctionNotFound` whose payload
    /// is the bare function name (`"on_start"`) — but it reports a failed
    /// operator or function call *inside* a function that DOES exist as
    /// the exact same error variant, just with the payload extended to
    /// `"name (arg, types)"` (see rhai's `gen_fn_call_signature`). Both
    /// stringify to a message containing "Function not found", so the old
    /// check silently swallowed genuine runtime errors — e.g. a script
    /// reading back a value it had just written via set_global/
    /// set_persistent earlier in the same pass: deferred writes mean that
    /// read still sees the old value, and an arithmetic op against the
    /// resulting `()` has no operator overload, which is exactly this
    /// error shape — wherever that happened, with no log entry, no
    /// disabled script, nothing: just a function that silently stopped
    /// executing partway through, every single call. (Found for real
    /// authoring `demos/roguelike/scripts/player.rhai` — see docs/HANDOFF.md.)
    /// Matching the error's exact payload instead of a substring of its
    /// Display text distinguishes the two cases correctly.
    fn is_missing_optional_fn(err: &rhai::EvalAltResult, fn_name: &str) -> bool {
        matches!(err, rhai::EvalAltResult::ErrorFunctionNotFound(sig, _) if sig.as_str() == fn_name)
    }

    pub fn run_on_start_all(
        &mut self,
        world: &mut World,
        log: &mut Vec<LogEntry>,
        extra_spawns: &[(String, f32, f32)],
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_pos: crate::math::Vec2,
        viewport_size: (usize, usize),
    ) -> ScriptUpdateResult {
        let scripted: Vec<(i64, String)> =
            world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let mut ctx_state = ScriptState::from_world(
            world,
            &self.layers,
            0.0,
            0.0,
            InputSnapshot::default(),
            MouseSnapshot::default(),
            GamepadSnapshot::default(),
            extra_spawns,
            globals,
            clips,
            std::mem::take(persistent),
            camera_pos,
            BTreeMap::new(),
            0,
            viewport_size,
        );
        ctx_state.timers = std::mem::take(&mut self.timers);
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, path) in &scripted {
            if self.disabled_scripts.contains(path) {
                continue;
            }
            let Some(ast) = self.ast_cache.get(path) else { continue };
            let scope = self.scopes.entry(*entity_id as EntityId).or_default();
            let entity_ctx = ctx.with_entity(*entity_id);
            if let Err(e) =
                self.engine.call_fn::<()>(scope, ast, "on_start", (*entity_id, entity_ctx))
            {
                if !Self::is_missing_optional_fn(&e, "on_start") {
                    log.push(LogEntry::error(format!("on_start '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    /// Step 5e's `on_input` pass (docs/ember2d-phase5-plan.md): runs once
    /// per step, for a single locally-controlled actor only — callers pass
    /// whichever `EntityId` `TurnScheduler` is currently resolving (see
    /// `PlayState::update`), not a stored id: player identity comes from
    /// `Actor::controller` being `Controller::Local` (Step 5g), and even
    /// with more than one local player only one can be "up" at a time. Its
    /// own `apply_ctx` commits `ctx.submit()`'s queued command into
    /// `ScriptUpdateResult.commands` *before* `run_scripts`'s `on_update`
    /// pass runs — that ordering (caller's responsibility: call this,
    /// apply its result, then call `run_scripts`) is what lets `on_update`
    /// read the command back out via `command_action`/`command_param`.
    #[allow(clippy::too_many_arguments)]
    pub fn run_on_input(
        &mut self,
        world: &mut World,
        snapshot: Rc<WorldSnapshot>,
        log: &mut Vec<LogEntry>,
        actor_id: EntityId,
        delta_time: f32,
        elapsed: f32,
        input: InputSnapshot,
        mouse: MouseSnapshot,
        gamepad: GamepadSnapshot,
        spawns: &[(String, f32, f32)],
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_pos: crate::math::Vec2,
        turn_number: i64,
        viewport_size: (usize, usize),
    ) -> ScriptUpdateResult {
        let path = world.scripts.get(&actor_id).map(|s| s.path.clone());
        let mut ctx_state = ScriptState::from_snapshot(
            snapshot,
            world.next_id,
            delta_time,
            elapsed,
            input,
            mouse,
            gamepad,
            spawns,
            globals,
            clips,
            std::mem::take(persistent),
            camera_pos,
            BTreeMap::new(),
            turn_number,
            viewport_size,
        );
        ctx_state.timers = std::mem::take(&mut self.timers);
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        if let Some(path) = path {
            if !self.disabled_scripts.contains(&path) {
                if let Some(ast) = self.ast_cache.get(&path) {
                    let scope = self.scopes.entry(actor_id).or_default();
                    let entity_ctx = ctx.with_entity(actor_id as i64);
                    if let Err(e) = self.engine.call_fn::<()>(
                        scope,
                        ast,
                        "on_input",
                        (actor_id as i64, entity_ctx),
                    ) {
                        if !Self::is_missing_optional_fn(&e, "on_input") {
                            log.push(LogEntry::error(format!("on_input '{}': {}", path, e)));
                            self.disabled_scripts.insert(path.clone());
                        }
                    }
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    /// Step 5f's `on_turn` pass (docs/ember2d-phase5-plan.md): runs for the
    /// single actor `TurnScheduler` just handed the turn to — the AI
    /// equivalent of `on_input` above, except it's called for every kind of
    /// actor (not just `Local`), never reads raw input, and is where the
    /// actual game-state mutation (movement, attacks, quaffing) happens.
    /// `ctx.command_action()`/`command_param()` inside it read whatever the
    /// *same* step's `on_input` pass (for a `Local` actor) already
    /// committed — see `PlayState::update`'s call order.
    #[allow(clippy::too_many_arguments)]
    pub fn run_on_turn(
        &mut self,
        world: &mut World,
        snapshot: Rc<WorldSnapshot>,
        log: &mut Vec<LogEntry>,
        actor_id: EntityId,
        delta_time: f32,
        elapsed: f32,
        spawns: &[(String, f32, f32)],
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_pos: crate::math::Vec2,
        commands: BTreeMap<i64, Command>,
        turn_number: i64,
        viewport_size: (usize, usize),
    ) -> ScriptUpdateResult {
        let path = world.scripts.get(&actor_id).map(|s| s.path.clone());
        let mut ctx_state = ScriptState::from_snapshot(
            snapshot,
            world.next_id,
            delta_time,
            elapsed,
            InputSnapshot::default(),
            MouseSnapshot::default(),
            GamepadSnapshot::default(),
            spawns,
            globals,
            clips,
            std::mem::take(persistent),
            camera_pos,
            commands,
            turn_number,
            viewport_size,
        );
        ctx_state.timers = std::mem::take(&mut self.timers);
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        if let Some(path) = path {
            if !self.disabled_scripts.contains(&path) {
                if let Some(ast) = self.ast_cache.get(&path) {
                    let scope = self.scopes.entry(actor_id).or_default();
                    let entity_ctx = ctx.with_entity(actor_id as i64);
                    if let Err(e) = self.engine.call_fn::<()>(
                        scope,
                        ast,
                        "on_turn",
                        (actor_id as i64, entity_ctx),
                    ) {
                        if !Self::is_missing_optional_fn(&e, "on_turn") {
                            log.push(LogEntry::error(format!("on_turn '{}': {}", path, e)));
                            self.disabled_scripts.insert(path.clone());
                        }
                    }
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    #[allow(clippy::too_many_arguments)]
    // Phase 6 Step 3 (docs/ember2d-phase6-plan.md): dropped the `_events:
    // &mut EventBus` parameter — it was never read (the leading underscore
    // already said so), so every caller had to allocate a throwaway
    // `EventBus::new()` just to satisfy the signature. Collision events are
    // populated by `World::detect_collisions` and consumed by `late_step`
    // directly; this pass never touched them.
    pub fn run_scripts(
        &mut self,
        world: &mut World,
        snapshot: Rc<WorldSnapshot>,
        log: &mut Vec<LogEntry>,
        delta_time: f32,
        elapsed: f32,
        input: InputSnapshot,
        mouse: MouseSnapshot,
        gamepad: GamepadSnapshot,
        spawns: &[(String, f32, f32)],
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_pos: crate::math::Vec2,
        commands: BTreeMap<i64, Command>,
        turn_number: i64,
        viewport_size: (usize, usize),
    ) -> ScriptUpdateResult {
        // Phase 6 Step 6: throttled here, at the one call site (`run_scripts`
        // runs exactly once per simulation step), rather than inside
        // `check_hot_reload` itself — that function's own unit test
        // (`hot_reload_clears_only_the_reloaded_scripts_entities`,
        // engine_tests.rs) calls it directly and expects it to always check
        // immediately, so its unconditional behavior stays intact; only
        // this caller decides how often to actually ask.
        self.hot_reload_counter += 1;
        if self.hot_reload_counter >= HOT_RELOAD_CHECK_INTERVAL {
            self.hot_reload_counter = 0;
            self.check_hot_reload(world, log);
        }
        // Step 4g: cleared here (once per real frame — `run_scripts` is the
        // one call site the engine's own `update()` invokes, and it only
        // fires for the top-of-stack GameState) rather than after drawing
        // in `PlayState::render`. `render` runs for every stacked state
        // every frame regardless of pause, but `update` (and therefore
        // `run_scripts`) does not — clearing in render meant a script's HUD
        // text vanished the instant a `PauseMenuState` was pushed on top,
        // since nothing ever refilled the queue while paused. Clearing here
        // instead means a skipped pass just leaves last frame's draws
        // rendering unchanged.
        self.pending_hud_draws.clear();
        let mut ctx_state = ScriptState::from_snapshot(
            snapshot,
            world.next_id,
            delta_time,
            elapsed,
            input,
            mouse,
            gamepad,
            spawns,
            globals,
            clips,
            std::mem::take(persistent),
            camera_pos,
            commands,
            turn_number,
            viewport_size,
        );
        ctx_state.timers = std::mem::take(&mut self.timers);
        // Decay happens exactly once per real step, here — `run_scripts` is
        // the one call site the engine's own `update()` invokes unconditionally
        // (see `check_hot_reload`'s throttle comment above for the same
        // "exactly once per step" property) — unlike the old scope-scan
        // version, every entry in this map IS a timer by construction now
        // (no more `__timer_` prefix filtering needed: this map holds nothing
        // else), so decaying is a plain nested `values_mut()` walk.
        for entity_timers in ctx_state.timers.values_mut() {
            for val in entity_timers.values_mut() {
                *val -= delta_time as f64;
            }
        }
        let scripted: Vec<(i64, String)> =
            world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, path) in scripted {
            if self.disabled_scripts.contains(&path) {
                continue;
            }
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_default();
            let entity_ctx = ctx.with_entity(entity_id);
            if let Err(e) =
                self.engine.call_fn::<()>(scope, ast, "on_update", (entity_id, entity_ctx))
            {
                if !Self::is_missing_optional_fn(&e, "on_update") {
                    log.push(LogEntry::error(format!("Runtime '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_collisions(
        &mut self,
        world: &mut World,
        pairs: &[(EntityId, EntityId)],
        log: &mut Vec<LogEntry>,
        delta_time: f32,
        elapsed: f32,
        spawns: &[(String, f32, f32)],
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_pos: crate::math::Vec2,
        viewport_size: (usize, usize),
    ) -> ScriptUpdateResult {
        let scripted_paths: HashMap<EntityId, String> =
            world.scripts.iter().map(|(id, s)| (*id, s.path.clone())).collect();
        let mut calls: Vec<(i64, i64, String)> = Vec::new();
        for &(a, b) in pairs {
            if let Some(p) = scripted_paths.get(&a) {
                calls.push((a as i64, b as i64, p.clone()));
            }
            if let Some(p) = scripted_paths.get(&b) {
                calls.push((b as i64, a as i64, p.clone()));
            }
        }

        // Phase 6 Step 5 (docs/ember2d-phase6-plan.md): `calls` is built
        // BEFORE the `WorldSnapshot` a `ScriptState` would carry, specifically
        // so this can bail out here — with no `on_collide` about to run,
        // there's nothing for a snapshot to back, and building one is a full
        // O(entities) pass (`WorldSnapshot::build`'s own doc comment) for a
        // return value nothing would read. On most turn-resolving steps a
        // colliding pair doesn't involve a scripted entity at all, so this
        // is the common case, not an edge case — this is deliberately NOT
        // done by sharing `step`'s own snapshot instead: `late_step` calls
        // `resolve_solid_collision` on `world` between building `pairs` and
        // calling this function, so a snapshot taken before that would carry
        // stale positions.
        //
        // `globals`/`clips`/`persistent` still have to come straight back
        // out unlanded, exactly what `apply_ctx` would return from a pass
        // that ran zero scripts — every other field here is that same
        // pass's quiescent default (nothing spawned, despawned, drawn, or
        // submitted).
        if calls.is_empty() {
            return ScriptUpdateResult {
                pending_level: None,
                pending_save: None,
                pending_load: None,
                globals,
                clips,
                persistent: std::mem::take(persistent),
                camera_override: None,
                shake_state: None,
                clear_hud: false,
                particles: Vec::new(),
                commands: BTreeMap::new(),
                act_cost: None,
                despawned: Vec::new(),
                animations: Vec::new(),
            };
        }

        let mut ctx_state = ScriptState::from_world(
            world,
            &self.layers,
            delta_time,
            elapsed,
            InputSnapshot::default(),
            MouseSnapshot::default(),
            GamepadSnapshot::default(),
            spawns,
            globals,
            clips,
            std::mem::take(persistent),
            camera_pos,
            BTreeMap::new(),
            0,
            viewport_size,
        );
        ctx_state.timers = std::mem::take(&mut self.timers);
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, other_id, path) in calls {
            if self.disabled_scripts.contains(&path) {
                continue;
            }
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_default();
            let entity_ctx = ctx.with_entity(entity_id);
            if let Err(e) = self.engine.call_fn::<()>(
                scope,
                ast,
                "on_collide",
                (entity_id, other_id, entity_ctx),
            ) {
                if !Self::is_missing_optional_fn(&e, "on_collide") {
                    log.push(LogEntry::error(format!("on_collide '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }

    fn check_hot_reload(&mut self, world: &World, log: &mut Vec<LogEntry>) {
        // Phase 6 Step 6: `__script_<id>` keys are synthetic — a node
        // graph's generated Rhai source, cached via `compile_str` under
        // this key (`Simulation::do_on_start`), never backed by a real file
        // on disk. `fs::metadata` on one is a guaranteed-failing syscall,
        // every time, for every graph-scripted tile. Skipping them here
        // means this loop only ever stats a path that's actually a file.
        let paths: Vec<String> =
            self.ast_cache.keys().filter(|k| !k.starts_with("__script_")).cloned().collect();
        for path in paths {
            let Ok(meta) = fs::metadata(&path) else { continue };
            let Ok(t) = meta.modified() else { continue };
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
                        // wiping every entity's persistent `let` state (and,
                        // before Step 9, its `__timer_*` vars too) whenever
                        // ANY script reloaded — not just entities running the
                        // script that changed. Only those entities need a
                        // fresh scope; everyone else's state must survive
                        // untouched.
                        let affected: Vec<EntityId> = world
                            .scripts
                            .iter()
                            .filter(|(_, s)| s.path == path)
                            .map(|(&id, _)| id)
                            .collect();
                        // Phase 6 Step 9 (docs/ember2d-phase6-plan.md): timers
                        // now live in `self.timers`, not the scope being
                        // dropped here — must be cleaned up in lockstep with
                        // it, or a reloaded script's entity inherits a stale
                        // timer from before the reload (the same leak-on-
                        // despawn hazard `apply_ctx`'s despawn loop already
                        // guards against, here on the hot-reload path instead).
                        for id in affected {
                            self.scopes.remove(&id);
                            self.timers.remove(&id);
                        }
                        log.push(LogEntry::info(format!("Hot-reloaded: {}", path)));
                    }
                    Err(e) => {
                        log.push(LogEntry::error(format!("Reload '{}': {}", path, e)));
                    }
                }
            }
        }
    }

    // apply_ctx moved to apply.rs (Phase 6 Step 2, docs/ember2d-phase6-plan.md)
    // — a second `impl ScriptEngine` block in a sibling file, same pattern
    // api_animation.rs already established for ScriptCtx in Phase 5.5. This
    // file was at the project's 600-line hard limit (CLAUDE.md) and Phase 6
    // edits it directly across several more steps.
}

// Tests split into engine_tests.rs — see that file's header comment — once
// this file crossed the project's 600-line hard limit (CLAUDE.md).
#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

// Phase 6 Step 9 (docs/ember2d-phase6-plan.md): timer tests split into their
// own sibling file rather than appended to engine_tests.rs — that file was
// already at 496/600 lines before this step's coverage, which would have
// pushed it to 613. See timer_tests.rs's own header comment.
#[cfg(test)]
#[path = "timer_tests.rs"]
mod timer_tests;

// 7A-1 (docs/ember2d-master-plan.md §5.1): safety-regression coverage split
// into its own sibling file rather than appended to engine_tests.rs — see
// safety_tests.rs's own header comment for why (same 600-line reasoning
// timer_tests.rs's own header comment gives).
#[cfg(test)]
#[path = "safety_tests.rs"]
mod safety_tests;
