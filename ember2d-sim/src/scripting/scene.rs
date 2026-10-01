// scripting/scene.rs — the script-visible scene stack's scripting half:
// what a script can ask for (`push_scene`/`pop_scene`/`quit_game`/...),
// what it can read back (`current_scene`/`scene_data`/...), and the pass
// that runs scene scripts.
//
// ── WHY (Step 9-1, docs/ember2d-master-plan.md §5.8) ─────────────────────────
//
// Before 9-1 a level was the only thing a script could run inside: a battle,
// a pause menu or a dialogue had to be faked with globals and `if` chains in
// the level's own scripts, and the one real overlay — the Esc pause menu —
// was a Rust `GameState` scripts couldn't see, change or replace. A SCENE is
// a named, script-owned state layered over the level: its own script, its
// own hidden entity (so `set_var`/timers work on `id` like any entity), and
// a `pauses_world` flag. While a pausing scene is on top, the level's
// `on_input`/`on_update`/`on_turn`/`on_collide` don't run (and physics,
// animators and collisions are held — see `Simulation::world_paused`); the
// scene's own `on_input` gets the keyboard instead.
//
// The stack itself is simulation state and lives on `Simulation`
// (`simulation/scenes.rs`), which applies the `SceneOp`s queued here at the
// end of each pass — the same deferred-write rule as every other `ctx` call.
// This file is the part `ScriptEngine` owns: the request/read types, the
// `ScriptCtx` methods, and `run_scene_pass`.
//
// The engine's own Esc pause menu is now just a scene: `BUILTIN_PAUSE_SOURCE`
// below, pushed as "pause" unless the project ships its own
// `scenes/pause.rhai`.

use std::collections::BTreeMap;
use std::rc::Rc;

use rhai::{Dynamic, Map};

use crate::world::{EntityId, World};

use super::api::ScriptCtx;
use super::engine::ScriptEngine;
use super::state::{PassArgs, ScriptState, WorldSnapshot};
use super::types::*;

/// The built-in pause scene's script, compiled under `BUILTIN_PAUSE_KEY`
/// when a project has no `scenes/pause.rhai` of its own.
pub const BUILTIN_PAUSE_SOURCE: &str = include_str!("builtin_pause.rhai");
/// The `ScriptEngine` AST-cache key the built-in pause script compiles
/// under — angle brackets so it can never collide with a real file path.
pub const BUILTIN_PAUSE_KEY: &str = "<builtin:pause>";

/// A scene request a script queued this pass. Applied by `Simulation` once
/// the pass ends, in queue order.
#[derive(Debug, Clone)]
pub enum SceneOp {
    Push {
        name: String,
        /// `None`: `scenes/<name>.rhai`, found the way exit paths are.
        script: Option<String>,
        pauses_world: bool,
        data: Dynamic,
    },
    Pop,
}

/// A request to leave play mode, from `quit_game`/`return_to_editor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowRequest {
    Quit,
    ToEditor,
}

/// One scene on the stack, as scripts read it this pass.
#[derive(Debug, Clone)]
pub struct SceneInfo {
    pub name: String,
    pub entity: i64,
    pub pauses_world: bool,
    pub data: Dynamic,
}

/// Everything scene-related a `ScriptState` carries: the stack as it stood
/// when the pass began (read-only), and this pass's own requests.
#[derive(Default)]
pub struct SceneCtx {
    pub(super) stack: Rc<Vec<SceneInfo>>,
    pub(super) editor_preview: bool,
    pub(super) ops: Vec<SceneOp>,
    pub(super) flow: Option<FlowRequest>,
}

impl ScriptEngine {
    /// What every pass's `ScriptState` sees of the scene stack — set by
    /// `Simulation` whenever the stack changes, cloned (an `Rc` bump) into
    /// each pass.
    pub fn set_scene_view(&mut self, stack: Vec<SceneInfo>, editor_preview: bool) {
        self.scene_view = Rc::new(stack);
        self.editor_preview = editor_preview;
    }

    pub(super) fn scene_ctx(&self) -> SceneCtx {
        SceneCtx {
            stack: self.scene_view.clone(),
            editor_preview: self.editor_preview,
            ops: Vec::new(),
            flow: None,
        }
    }

    /// Runs one batch of scene-script calls — `(entity, script key,
    /// lifecycle fn)` in order — inside ONE `ScriptCtx`, so they share the
    /// same deferred-write pass the way every level pass does. The HUD a
    /// scene draws goes into `pending_scene_hud_draws`, drawn above the
    /// level's own HUD and cleared here whenever this batch runs any
    /// `on_update` (the level's queue is left alone, so a paused level's
    /// HUD stays on screen under the scene — the D16 rule the old pause
    /// menu kept too).
    pub fn run_scene_pass(
        &mut self,
        world: &mut World,
        snapshot: Rc<WorldSnapshot>,
        log: &mut Vec<LogEntry>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
        calls: &[(EntityId, String, &'static str)],
    ) -> ScriptUpdateResult {
        if calls.iter().any(|(_, _, f)| *f == "on_update") {
            self.pending_scene_hud_draws.clear();
        }
        let mut ctx_state =
            ScriptState::from_snapshot(snapshot, world.next_id, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx();
        ctx_state.camera_view = self.camera_view;
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity, key, f) in calls {
            let entity_ctx = ctx.with_entity(*entity as i64);
            self.call_lifecycle_fn(key, f, f, (*entity as i64, entity_ctx), log);
        }
        // The level's HUD queue is set aside while `apply_ctx` appends this
        // pass's draws, then restored — scene draws land in their own queue.
        let level_hud = std::mem::take(&mut self.pending_hud_draws);
        let res = self.apply_ctx(ctx, world, log);
        let scene_draws = std::mem::replace(&mut self.pending_hud_draws, level_hud);
        self.pending_scene_hud_draws.extend(scene_draws);
        res
    }
}

/// `push_scene`'s options map: `script` (a path), `pauses_world` (bool,
/// default true), `data` (anything, read back with `scene_data()`).
fn push_op(name: String, opts: &Map) -> SceneOp {
    SceneOp::Push {
        name,
        script: opts.get("script").and_then(|v| v.clone().into_string().ok()),
        pauses_world: opts.get("pauses_world").and_then(|v| v.as_bool().ok()).unwrap_or(true),
        data: opts.get("data").cloned().unwrap_or(Dynamic::UNIT),
    }
}

impl ScriptCtx {
    /// Push scene `name` (script `scenes/<name>.rhai`, pausing the world).
    pub fn push_scene(&mut self, name: String) {
        self.push_scene_with(name, Map::new());
    }
    /// Push scene `name` with options — see `push_op`.
    pub fn push_scene_with(&mut self, name: String, opts: Map) {
        self.inner.borrow_mut().scene.ops.push(push_op(name, &opts));
    }
    /// Pop the top scene (a no-op with none on the stack).
    pub fn pop_scene(&mut self) {
        self.inner.borrow_mut().scene.ops.push(SceneOp::Pop);
    }
    /// The top scene's name, or `""` when only the level is running.
    pub fn current_scene(&mut self) -> String {
        self.inner.borrow().scene.stack.last().map(|s| s.name.clone()).unwrap_or_default()
    }
    /// How many scenes are on the stack.
    pub fn scene_count(&mut self) -> i64 {
        self.inner.borrow().scene.stack.len() as i64
    }
    /// The `data` the calling scene was pushed with (the top scene's, when
    /// called from a level script); `()` if none.
    pub fn scene_data(&mut self) -> Dynamic {
        let me = self.entity_id;
        let state = self.inner.borrow();
        let stack = &state.scene.stack;
        stack
            .iter()
            .find(|s| s.entity == me)
            .or_else(|| stack.last())
            .map(|s| s.data.clone())
            .unwrap_or(Dynamic::UNIT)
    }
    /// Leave play mode and close the game.
    pub fn quit_game(&mut self) {
        self.inner.borrow_mut().scene.flow = Some(FlowRequest::Quit);
    }
    /// Return to the editor (a no-op outside an editor preview — check
    /// `is_editor_preview()`).
    pub fn return_to_editor(&mut self) {
        let mut state = self.inner.borrow_mut();
        if state.scene.editor_preview {
            state.scene.flow = Some(FlowRequest::ToEditor);
        }
    }
    /// True when this run was started from the editor (F5).
    pub fn is_editor_preview(&mut self) -> bool {
        self.inner.borrow().scene.editor_preview
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("push_scene", ScriptCtx::push_scene);
    engine.register_fn("push_scene", ScriptCtx::push_scene_with);
    engine.register_fn("pop_scene", ScriptCtx::pop_scene);
    engine.register_fn("current_scene", ScriptCtx::current_scene);
    engine.register_fn("scene_count", ScriptCtx::scene_count);
    engine.register_fn("scene_data", ScriptCtx::scene_data);
    engine.register_fn("quit_game", ScriptCtx::quit_game);
    engine.register_fn("return_to_editor", ScriptCtx::return_to_editor);
    engine.register_fn("is_editor_preview", ScriptCtx::is_editor_preview);
}
