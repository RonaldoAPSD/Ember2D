// scripting/engine.rs — ScriptState and ScriptEngine core.

use rhai::{Engine, Scope, AST};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::rc::Rc;
use std::time::SystemTime;

use crate::world::{EntityId, World};

use super::api::ScriptCtx;
use super::types::*;

// ScriptState (the per-frame snapshot + write-queue scripts see through
// ScriptCtx) lives in state.rs, a sibling module declared in
// scripting/mod.rs (not here) since api.rs needs it too — see that file's
// header comment for why it was split out of engine.rs.
use super::state::{PassArgs, ScriptState, WorldSnapshot};

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
    /// `pub(super)` (not private) since `lifecycle.rs`'s `run_on_start_all`/
    /// `run_on_load_all` and `collisions.rs`'s `run_collisions` — second
    /// `impl ScriptEngine` blocks in sibling files, Steps 7.5-5/7.5-8 — call
    /// `call_lifecycle_fn` on it directly.
    pub(super) engine: Engine,
    /// `pub(super)`, same reasoning as `engine` immediately above.
    pub(super) ast_cache: HashMap<String, AST>,
    mod_times: HashMap<String, SystemTime>,
    /// Script paths that threw a runtime error and are no longer called —
    /// defect D9: previously an error only suppressed its own log message
    /// (`logged_runtime_errors`) while the script kept being invoked, and
    /// kept failing, every frame. A script stays disabled until it hot-reloads
    /// successfully (see `check_hot_reload`). `pub(super)`, same reasoning
    /// as `engine` above.
    pub(super) disabled_scripts: HashSet<String>,
    /// `pub(super)`, same reasoning as `engine` above.
    pub(super) rng: Rc<RefCell<rand::rngs::SmallRng>>,
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
    /// `engine` above: `apply.rs`'s `apply_ctx` — a second `impl
    /// ScriptEngine` block in a sibling file — reads it directly.
    pub(super) layers: crate::layers::LayerRegistry,
    /// Phase 6 Step 9 (docs/ember2d-phase6-plan.md): per-entity timer values,
    /// now plain engine-owned state instead of being smuggled through each
    /// entity's Rhai `Scope` as `__timer_<name>` variables scanned out by
    /// string-prefix every single script pass (five near-identical scan
    /// blocks, one per `run_*` method below, all deleted by that step —
    /// and the per-entity `Scope` map itself followed them into deletion at
    /// Step 7.5-10, R22: `rhai::Engine::call_fn`'s default `CallFnOptions`
    /// rewinds the `Scope` it's given after every call, so nothing a script
    /// declared into it — global `let`, or a called function's own locals —
    /// ever survived past that same call. The map was dead weight
    /// masquerading as per-entity persistence from the moment this crate's
    /// real persistence primitives (`globals`/`persistent`/`Vars`/this field)
    /// existed to replace whatever it once did.
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
    /// Values are `TimerState` (Step 7.5-8, D22 fix), not a raw `f64`
    /// sentinel — see that type's own doc comment (scripting/types.rs).
    pub(super) timers: BTreeMap<EntityId, BTreeMap<String, TimerState>>,
    /// Entities `apply_ctx` just attached a script to via `ctx.set_script`
    /// (Step 7.5-5, docs/ember2d-master-plan.md §5.6) — drained by the next
    /// `run_scripts` call, which calls each one's `on_start` before this
    /// step's own `on_update` pass runs for it. `pub(super)`, same
    /// reasoning as `layers`/`timers` above: `apply.rs`'s
    /// `apply_ctx` pushes onto it directly. A plain `Vec`, not a `BTreeSet`
    /// — `set_script` twice on the same entity in one pass (unusual, but
    /// not guarded against) should call `on_start` twice next step, same
    /// as calling it would from two separate passes; de-duplicating would
    /// silently drop the second attach's own init.
    pub(super) pending_on_start: Vec<EntityId>,
    pub pending_hud_draws: Vec<HudDraw>,
    /// Step 9-1 (docs/ember2d-master-plan.md §5.8): the HUD scene scripts
    /// draw, kept apart from the level's own so a paused level's HUD stays
    /// up under a scene — see `run_scene_pass` (scripting/scene.rs).
    pub pending_scene_hud_draws: Vec<HudDraw>,
    /// Step 9-1: the scene stack as scripts read it — see `set_scene_view`.
    pub(super) scene_view: Rc<Vec<super::scene::SceneInfo>>,
    pub(super) editor_preview: bool,
    /// Step 9-2: the camera as scripts have set it — see `set_camera_view`.
    pub(super) camera_view: super::camera::CameraSettings,
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
            mod_times: HashMap::new(),
            disabled_scripts: HashSet::new(),
            rng: Rc::new(RefCell::new(rand::rngs::SmallRng::seed_from_u64(seed))),
            hot_reload_counter: 0,
            layers,
            timers: BTreeMap::new(),
            pending_on_start: Vec::new(),
            pending_hud_draws: Vec::new(),
            pending_scene_hud_draws: Vec::new(),
            scene_view: Rc::new(Vec::new()),
            editor_preview: false,
            camera_view: Default::default(),
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

    // Step 7.5-9 (docs/ember2d-master-plan.md §5.6): `fs::metadata` below
    // is a pre-existing, dev-time-only use (recording a script's own
    // mtime for `check_hot_reload`'s later comparison) — out of THIS
    // step's scope, which is level/exit-path resolution (`LevelSource`,
    // level_source.rs), not script compilation. `compile_file` two lines
    // up ALSO reads the script file, via `rhai`'s own internal
    // `std::fs::read_to_string` — invisible to clippy's `disallowed-
    // methods` (it fires on code in THIS crate, not inside a dependency)
    // and not something this crate could intercept without replacing
    // `rhai::Engine::compile_file` entirely. Not fixed here; flagged as a
    // real gap, not silently ignored.
    #[allow(clippy::disallowed_methods)]
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
    pub(super) fn is_missing_optional_fn(err: &rhai::EvalAltResult, fn_name: &str) -> bool {
        matches!(err, rhai::EvalAltResult::ErrorFunctionNotFound(sig, _) if sig.as_str() == fn_name)
    }

    /// Calls `fn_name` on the script at `path`, applying every call site's
    /// identical error handling: a genuinely missing optional lifecycle
    /// function (`is_missing_optional_fn`) is skipped silently; any other
    /// error logs under `log_label` and disables the script (D9). Step
    /// 7.5-10 (docs/ember2d-master-plan.md §5.6, R22) replaces five
    /// near-identical `call_fn` blocks that used to differ only in the
    /// label they logged under (`on_update`'s is historically "Runtime",
    /// not "on_update" — preserved here via the separate `log_label`
    /// parameter) and the argument tuple they passed (`on_collide`'s
    /// `(id, other_id, ctx)` vs. every other lifecycle function's plain
    /// `(id, ctx)`, which is why `args` is generic rather than a fixed
    /// tuple). Takes a fresh, throwaway `Scope::new()` per call rather than
    /// a stored per-entity one — seeded by the same discovery that deleted
    /// `ScriptEngine.scopes` (see `layers`'s field doc comment above): a
    /// `Scope` passed to `call_fn` never keeps anything written into it
    /// past that same call anyway, since rhai's default `CallFnOptions`
    /// rewinds it on the way out, so persisting one across calls bought
    /// nothing.
    pub(super) fn call_lifecycle_fn(
        &mut self,
        path: &str,
        fn_name: &str,
        log_label: &str,
        args: impl rhai::FuncArgs,
        log: &mut Vec<LogEntry>,
    ) {
        if self.disabled_scripts.contains(path) {
            return;
        }
        let Some(ast) = self.ast_cache.get(path) else { return };
        let mut scope = Scope::new();
        if let Err(e) = self.engine.call_fn::<()>(&mut scope, ast, fn_name, args) {
            if !Self::is_missing_optional_fn(&e, fn_name) {
                log.push(LogEntry::error(format!("{} '{}': {}", log_label, path, e)));
                self.disabled_scripts.insert(path.to_string());
            }
        }
    }

    // run_on_start_all/run_on_load_all moved to lifecycle.rs (Step 7.5-5,
    // docs/ember2d-master-plan.md §5.6) — this file was over CLAUDE.md's
    // 750-line hard limit once `run_on_load_all` (7.5-5's own addition)
    // landed alongside it. The two are a matched pair (fresh-spawn vs.
    // loaded-save, each called exactly once by `Simulation::on_start`'s two
    // branches) and together were the single largest, most self-contained
    // pair of methods left in this file — same second-`impl ScriptEngine`-
    // in-a-sibling-file pattern `apply.rs` already established (Phase 6
    // Step 2). Pure relocation: nothing about either method changed, only
    // location; `is_missing_optional_fn` above went from private to
    // `pub(super)` as the one visibility bump this split needed.

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
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
        animating: &[EntityId],
    ) -> ScriptUpdateResult {
        let path = world.scripts.get(&actor_id).map(|s| s.path.clone());
        let mut ctx_state =
            ScriptState::from_snapshot(snapshot, world.next_id, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx(); // Step 9-1
        ctx_state.camera_view = self.camera_view; // Step 9-2
        ctx_state.animating = animating.iter().map(|&id| id as i64).collect(); // 7.5-7
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        if let Some(path) = path {
            let entity_ctx = ctx.with_entity(actor_id as i64);
            self.call_lifecycle_fn(
                &path,
                "on_input",
                "on_input",
                (actor_id as i64, entity_ctx),
                log,
            );
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
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
        animating: &[EntityId],
    ) -> ScriptUpdateResult {
        let path = world.scripts.get(&actor_id).map(|s| s.path.clone());
        let mut ctx_state =
            ScriptState::from_snapshot(snapshot, world.next_id, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx(); // Step 9-1
        ctx_state.camera_view = self.camera_view; // Step 9-2
        ctx_state.animating = animating.iter().map(|&id| id as i64).collect();
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        if let Some(path) = path {
            let entity_ctx = ctx.with_entity(actor_id as i64);
            self.call_lifecycle_fn(&path, "on_turn", "on_turn", (actor_id as i64, entity_ctx), log);
        }
        self.apply_ctx(ctx, world, log)
    }

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
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
        animating: &[EntityId],
    ) -> ScriptUpdateResult {
        let delta_time = args.delta_time;
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
        let mut ctx_state =
            ScriptState::from_snapshot(snapshot, world.next_id, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx(); // Step 9-1
        ctx_state.camera_view = self.camera_view; // Step 9-2
        ctx_state.animating = animating.iter().map(|&id| id as i64).collect();
        // Decay happens exactly once per real step, here — `run_scripts` is
        // the one call site the engine's own `update()` invokes unconditionally
        // (see `check_hot_reload`'s throttle comment above for the same
        // "exactly once per step" property) — unlike the old scope-scan
        // version, every entry in this map IS a timer by construction now
        // (no more `__timer_` prefix filtering needed: this map holds nothing
        // else), so decaying is a plain nested `values_mut()` walk. Step
        // 7.5-8 (D22): only a `Running` timer decays; crossing zero
        // transitions it to `Fired` right here rather than leaving it as a
        // negative `Running` value for `timer_done` to interpret later —
        // `Fired`/`Cancelled`/`Consumed` are all terminal until a script
        // acts on them, never touched by decay.
        for entity_timers in ctx_state.timers.values_mut() {
            for state in entity_timers.values_mut() {
                if let TimerState::Running(remaining) = state {
                    *remaining -= delta_time;
                    if *remaining <= 0.0 {
                        *state = TimerState::Fired;
                    }
                }
            }
        }
        let scripted: Vec<(i64, String)> =
            world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        // Step 7.5-5 (docs/ember2d-master-plan.md §5.6): `ctx.set_script`'s
        // "next step boundary" contract — an entity `apply_ctx` attached a
        // script to during a PRIOR pass gets its `on_start` called here,
        // once, before this same call's `on_update` loop below reaches it
        // for the first time (`scripted` above already includes it, since
        // the attach landed in `World` before this call started). Uses
        // this call's own `ctx`, same as every entry in `scripted` below —
        // one `apply_ctx` flush covers both the on_start and on_update
        // writes this step.
        for entity_id in std::mem::take(&mut self.pending_on_start) {
            let Some(path) = world.scripts.get(&entity_id).map(|s| s.path.clone()) else {
                continue;
            };
            let entity_ctx = ctx.with_entity(entity_id as i64);
            self.call_lifecycle_fn(
                &path,
                "on_start",
                "on_start",
                (entity_id as i64, entity_ctx),
                log,
            );
        }
        for (entity_id, path) in scripted {
            let entity_ctx = ctx.with_entity(entity_id);
            self.call_lifecycle_fn(&path, "on_update", "Runtime", (entity_id, entity_ctx), log);
        }
        self.apply_ctx(ctx, world, log)
    }

    // run_collisions moved to collisions.rs (Step 7.5-8, docs/ember2d-
    // master-plan.md §5.6) — this file crossed CLAUDE.md's 750-line limit
    // again once the D22 timer fix's own doc comments landed; same
    // second-`impl ScriptEngine`-in-a-sibling-file pattern `apply.rs`/
    // `lifecycle.rs` already established. Pure relocation, no behavior
    // change.

    // Step 7.5-9: same pre-existing, dev-time-only `fs::metadata` exemption
    // `compile`'s own doc comment above explains.
    #[allow(clippy::disallowed_methods)]
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
                        // wiping every entity's `__timer_*` scope vars (Step
                        // 9 moved those into `self.timers` below; the
                        // `Scope` itself was later found to be dead state
                        // entirely and deleted, Step 7.5-10 R22 — see
                        // `ScriptEngine.layers`'s field doc comment) whenever
                        // ANY script reloaded — not just entities running the
                        // script that changed. Only those entities' timers
                        // need clearing; everyone else's must survive
                        // untouched.
                        let affected: Vec<EntityId> = world
                            .scripts
                            .iter()
                            .filter(|(_, s)| s.path == path)
                            .map(|(&id, _)| id)
                            .collect();
                        // Phase 6 Step 9 (docs/ember2d-phase6-plan.md): a
                        // reloaded script's entity must not inherit a stale
                        // timer from before the reload (the same leak-on-
                        // despawn hazard `apply_ctx`'s despawn loop already
                        // guards against, here on the hot-reload path instead).
                        for id in affected {
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

// 7.5-1 (docs/ember2d-master-plan.md §5.6): uniform-typing/sentinel
// regression coverage split into its own sibling file rather than appended
// to engine_tests.rs — see uniform_typing_tests.rs's own header comment for
// why (same 750-line reasoning timer_tests.rs's/safety_tests.rs's own
// header comments give, raised from 600 since their own splits).
#[cfg(test)]
#[path = "uniform_typing_tests.rs"]
mod uniform_typing_tests;

// 7.5-2 (docs/ember2d-master-plan.md §5.6): add_global/add_persistent
// regression coverage split into its own sibling file — same reasoning
// uniform_typing_tests.rs's own header comment gives for its own split, and
// a distinct concern from that file's (uniform int/float typing) rather
// than an extension of it.
#[cfg(test)]
#[path = "atomic_arithmetic_tests.rs"]
mod atomic_arithmetic_tests;

// 7.5-3 (docs/ember2d-master-plan.md §5.6): set_var/get_var/has_var/
// remove_var regression coverage split into its own sibling file — same
// reasoning uniform_typing_tests.rs's/atomic_arithmetic_tests.rs's own
// header comments give for their own splits.
#[cfg(test)]
#[path = "vars_tests.rs"]
mod vars_tests;

// 7.5-4 (docs/ember2d-master-plan.md §5.6): get_stat/get_tint_aware/
// get_tint_asleep regression coverage split into its own sibling file —
// same reasoning uniform_typing_tests.rs's/atomic_arithmetic_tests.rs's/
// vars_tests.rs's own header comments give for their own splits.
#[cfg(test)]
#[path = "actor_stats_tests.rs"]
mod actor_stats_tests;

// 7.5-5 (docs/ember2d-master-plan.md §5.6): set_script/on_load regression
// coverage split into its own sibling file — same reasoning
// uniform_typing_tests.rs's/atomic_arithmetic_tests.rs's/vars_tests.rs's/
// actor_stats_tests.rs's own header comments give for their own splits.
#[cfg(test)]
#[path = "set_script_tests.rs"]
mod set_script_tests;

// 7.5-6 (docs/ember2d-master-plan.md §5.6): get_path's diagonal option and
// reachable_within regression coverage split into its own sibling file —
// same reasoning uniform_typing_tests.rs's/vars_tests.rs's/
// actor_stats_tests.rs's/set_script_tests.rs's own header comments give for
// their own splits.
#[cfg(test)]
#[path = "path_tests.rs"]
mod path_tests;
