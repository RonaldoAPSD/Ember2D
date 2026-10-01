// simulation/scenes.rs — the scene stack itself: pushing and popping scenes,
// running scene scripts each step, and telling the rest of the step whether
// the world is paused.
//
// Step 9-1 (docs/ember2d-master-plan.md §5.8). The scripting half (requests,
// reads, the pass that calls scene scripts) is `scripting/scene.rs`; this is
// the half that owns state. The stack is simulation state — deterministic,
// saved with the game (`SaveState::scenes`) — never presentation.
//
// The per-step rules, in `run_scene_step`:
//   1. A scene pushed since the last step gets `on_start(id, ctx)`.
//   2. The TOP scene gets `on_input(id, ctx)` — unless it was pushed this
//      same step, so the key press that opened a scene (Esc, Enter on an
//      NPC) can never also act inside it.
//   3. `on_update(id, ctx)` runs for every scene from the topmost
//      world-pausing one upward (bottom to top); scenes beneath a pausing
//      scene are paused along with the level.
// The level itself runs only when no scene on the stack pauses the world —
// decided once at the START of the step, so popping a pause scene with a
// key press doesn't hand that same press to the level a moment later.

use std::collections::BTreeMap;
use std::rc::Rc;

use rhai::Dynamic;
use serde::{Deserialize, Serialize};

use crate::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::Transform;
use crate::math::Vec2;
use crate::scripting::{
    HudDraw, LogEntry, PassArgs, SceneInfo, SceneOp, WorldSnapshot, BUILTIN_PAUSE_KEY,
    BUILTIN_PAUSE_SOURCE,
};
use crate::world::{EntityId, World};

use super::{resolve_exit_path, Simulation, StepOutcome};

/// One scene on the stack. Also what a save file stores for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneFrame {
    pub name: String,
    /// The script's file path, or `BUILTIN_PAUSE_KEY`.
    pub script: String,
    pub pauses_world: bool,
    #[serde(default)]
    pub data: Dynamic,
    /// The scene's own hidden entity (a bare `Transform`, nothing drawn) —
    /// the `id` its script functions receive.
    pub entity: EntityId,
    /// `on_start` has run. A loaded save restores scenes already started.
    #[serde(default = "started_default")]
    pub started: bool,
}

fn started_default() -> bool {
    true
}

/// The input-side pieces of a step a scene pass needs.
pub(super) struct SceneStepInput<'a> {
    pub input: &'a InputSnapshot,
    pub mouse: MouseSnapshot,
    pub gamepad: &'a GamepadSnapshot,
    pub camera_origin: Vec2,
    pub sim_dt: f32,
    pub elapsed: f32,
    pub viewport: (usize, usize),
}

impl Simulation {
    /// Whether any scene on the stack pauses the world right now.
    pub fn world_paused(&self) -> bool {
        self.scenes.iter().any(|s| s.pauses_world)
    }

    /// Whether the world was paused when the current step began — what
    /// play mode reads to hold physics for the rest of that step.
    pub fn paused_this_step(&self) -> bool {
        self.paused_this_step
    }

    /// The stack's scene names, bottom to top.
    pub fn scene_names(&self) -> Vec<String> {
        self.scenes.iter().map(|s| s.name.clone()).collect()
    }

    /// The HUD scene scripts drew — drawn above `pending_hud_draws`.
    pub fn scene_hud_draws(&self) -> &[HudDraw] {
        &self.script_engine.pending_scene_hud_draws
    }

    /// Marks this run as an editor preview (F5): `is_editor_preview()` reads
    /// it, and `return_to_editor()` only works when it's set.
    pub fn set_editor_preview(&mut self, on: bool) {
        self.editor_preview = on;
        self.sync_scene_view();
    }

    /// The scenes a loaded save had open — restored by `on_start`'s
    /// loading-save branch (the entities themselves come back with `World`).
    pub fn set_saved_scenes(&mut self, scenes: Vec<SceneFrame>) {
        self.scenes = scenes;
    }

    /// The scene stack as a save file stores it.
    pub fn scene_frames(&self) -> &[SceneFrame] {
        &self.scenes
    }

    /// Esc in play mode: opens the pause scene when no scene is open (a
    /// scene that is open handles Esc in its own `on_input`). The project's
    /// `scenes/pause.rhai` if it has one, else the built-in menu.
    pub fn request_pause(&mut self, world: &mut World, logs: &mut Vec<LogEntry>) {
        if self.scenes.is_empty() {
            self.push_scene_frame(world, "pause".to_string(), None, true, Dynamic::UNIT, logs);
        }
    }

    /// Recompiles every restored scene's script (loading-save branch of
    /// `on_start`) and republishes the stack to scripts.
    pub(super) fn restore_scenes(&mut self, logs: &mut Vec<LogEntry>) {
        for i in 0..self.scenes.len() {
            let key = self.scenes[i].script.clone();
            if key == BUILTIN_PAUSE_KEY {
                self.script_engine.compile_str(BUILTIN_PAUSE_KEY, BUILTIN_PAUSE_SOURCE, logs);
            } else {
                self.script_engine.compile(&key, logs);
            }
        }
        self.sync_scene_view();
    }

    pub(super) fn sync_scene_view(&mut self) {
        let view = self
            .scenes
            .iter()
            .map(|s| SceneInfo {
                name: s.name.clone(),
                entity: s.entity as i64,
                pauses_world: s.pauses_world,
                data: s.data.clone(),
            })
            .collect();
        self.script_engine.set_scene_view(view, self.editor_preview);
    }

    /// Applies one pass's `push_scene`/`pop_scene` requests, in order.
    pub(super) fn apply_scene_ops(
        &mut self,
        world: &mut World,
        ops: Vec<SceneOp>,
        logs: &mut Vec<LogEntry>,
    ) {
        for op in ops {
            match op {
                SceneOp::Push { name, script, pauses_world, data } => {
                    self.push_scene_frame(world, name, script, pauses_world, data, logs);
                }
                SceneOp::Pop => {
                    if let Some(frame) = self.scenes.pop() {
                        world.despawn(frame.entity);
                        // Step 9-3: the scene's own menus/dialogue go too.
                        self.ui.close_owned_by(frame.entity as i64);
                        self.script_engine.set_ui_view(self.ui.clone());
                    }
                }
            }
        }
        self.sync_scene_view();
    }

    /// Finds and compiles scene `name`'s script and pushes it. Logs a
    /// warning and pushes nothing if the script can't be found.
    fn push_scene_frame(
        &mut self,
        world: &mut World,
        name: String,
        script: Option<String>,
        pauses_world: bool,
        data: Dynamic,
        logs: &mut Vec<LogEntry>,
    ) {
        let explicit = script.is_some();
        let wanted = script.unwrap_or_else(|| format!("scenes/{name}.rhai"));
        let full = resolve_exit_path(&wanted, &self.level.path, &|p| self.level_source_exists(p));
        let key = if self.level_source_exists(&full) {
            if !self.script_engine.compile(&full, logs) {
                return;
            }
            full
        } else if name == "pause" && !explicit {
            self.script_engine.compile_str(BUILTIN_PAUSE_KEY, BUILTIN_PAUSE_SOURCE, logs);
            BUILTIN_PAUSE_KEY.to_string()
        } else {
            logs.push(LogEntry::warn(format!("push_scene('{name}'): no script at {wanted}")));
            return;
        };
        let entity = world.spawn();
        world.add_transform(entity, Transform::new(0.0, 0.0));
        self.scenes.push(SceneFrame {
            name,
            script: key,
            pauses_world,
            data,
            entity,
            started: false,
        });
        self.sync_scene_view();
    }

    /// The scene half of `step` — see this file's header for the rules.
    pub(super) fn run_scene_step(
        &mut self,
        world: &mut World,
        snapshot: Rc<WorldSnapshot>,
        input: SceneStepInput<'_>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        logs: &mut Vec<LogEntry>,
        outcome: &mut StepOutcome,
    ) {
        if self.scenes.is_empty() {
            return;
        }
        let mut calls: Vec<(EntityId, String, &'static str)> = Vec::new();
        let top_was_started = self.scenes.last().map(|s| s.started).unwrap_or(false);
        for frame in self.scenes.iter_mut().filter(|f| !f.started) {
            calls.push((frame.entity, frame.script.clone(), "on_start"));
            frame.started = true;
        }
        if top_was_started {
            if let Some(top) = self.scenes.last() {
                calls.push((top.entity, top.script.clone(), "on_input"));
            }
        }
        let first_active = self.scenes.iter().rposition(|s| s.pauses_world).unwrap_or(0);
        for frame in &self.scenes[first_active..] {
            calls.push((frame.entity, frame.script.clone(), "on_update"));
        }

        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_scene_pass(
            world,
            snapshot,
            logs,
            persistent,
            PassArgs {
                delta_time: input.sim_dt,
                elapsed: input.elapsed,
                input: input.input.clone(),
                mouse: input.mouse,
                gamepad: input.gamepad.clone(),
                spawns: &self.level.extra_spawns,
                globals,
                clips,
                camera_pos: input.camera_origin,
                commands: BTreeMap::new(),
                turn_number: self.turn_number,
                viewport_size: input.viewport,
            },
            &calls,
        );
        self.apply_script_result(world, res, persistent, logs, outcome);
    }
}
