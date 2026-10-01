// simulation/step.rs — Simulation's per-step execution: `step`/`late_step`,
// their `run_actor_turn` helper, and `apply_script_result` (split out of
// simulation.rs at R76, docs/ember2d-master-plan.md §3.2 — simulation.rs
// was 791 real lines, over CLAUDE.md's 750-line limit; this is purely a
// file split, no behavior change). A genuine child module, same mechanics
// as `simulation/spawn.rs` (see that file's own header comment): Rust
// 2018+'s file+sibling-directory layout lets `simulation.rs` and
// `simulation/` coexist, so `mod step;` in simulation.rs resolves here.
// simulation.rs itself keeps the `Simulation` struct, its constructors,
// accessors, `rebuild_scheduler`/`restore_legacy_exits`, and `on_start` — this file
// is what a caller drives every step afterward.

use std::collections::{BTreeMap, HashMap};

use crate::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::Controller;
use crate::event::EventBus;
use crate::math::Vec2;
use crate::save::SaveState;
use crate::scheduler::{TurnModel, ALTERNATING_COST};
use crate::scripting::{LogEntry, PassArgs, ScriptUpdateResult, WorldSnapshot};
use crate::world::{EntityId, World};

use super::{
    actor_has_physics, drain_diagnostics_into, is_local_player, resolve_exit_path, Simulation,
    StepInput, StepOutcome,
};

impl Simulation {
    /// One simulation step: advance animators, run `on_input` for whichever
    /// local actor the scheduler is waiting on, merge in
    /// `external_commands` (the seam-2 fix), run the housekeeping
    /// `on_update` pass for every scripted entity, then `on_turn` for
    /// whichever single actor is due (if it has a command, or is AI).
    /// Mirrors `ember2d::play::PlayState::update`'s former body exactly —
    /// see that history for why the pass order (on_update before on_turn,
    /// not after) is load-bearing, not arbitrary.
    pub fn step(
        &mut self,
        world: &mut World,
        input: StepInput<'_>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
    ) -> StepOutcome {
        let StepInput {
            input: input_snapshot,
            mouse: mouse_snapshot,
            gamepad: gamepad_snapshot,
            external_commands,
            animating,
            camera_origin,
            sim_dt,
            elapsed,
            viewport_w,
            viewport_h,
        } = input;

        // R16 (7A-5, docs/ember2d-master-plan.md): counts this call — see
        // `step_count`'s own doc comment.
        self.step_count += 1;

        let mut outcome = StepOutcome::default();
        let mut logs = Vec::new();

        // Step 9-1 (docs/ember2d-master-plan.md §5.8): a world-pausing scene
        // holds the level — animators, every level pass, collisions (see
        // `late_step`). Decided once, here, at the start of the step.
        let paused = self.world_paused();
        self.paused_this_step = paused;

        // Step 9-3: open menus / the dialogue box get the keyboard first;
        // the keys they use are removed from what scripts see this step.
        let filtered_input = self.ui.handle_input(input_snapshot);
        if filtered_input.is_some() {
            self.script_engine.set_ui_view(self.ui.clone());
        }
        let input_snapshot = filtered_input.as_ref().unwrap_or(input_snapshot);

        // Advance every Animator before scripts run this step, so
        // `clip_finished(id)` reflects this tick, not last step's.
        for animator in world.animators.values_mut().filter(|_| !paused) {
            if let Some(clip) = self.clips.get(&animator.clip) {
                animator.advance(clip, sim_dt);
            } else {
                animator.just_finished = false;
            }
        }

        // Built once per step, shared (via cheap `Rc::clone`) across
        // on_input/on_update/on_turn below — see `WorldSnapshot`'s own doc
        // comment (scripting/state.rs) for the perf regression this fixes.
        let world_snapshot =
            std::rc::Rc::new(WorldSnapshot::build(world, &self.layers, &self.level.extra_spawns));

        // Step 9-1: scene scripts run first; a paused level stops here.
        self.run_scene_step(
            world,
            world_snapshot.clone(),
            super::scenes::SceneStepInput {
                input: input_snapshot,
                mouse: mouse_snapshot,
                gamepad: gamepad_snapshot,
                camera_origin,
                sim_dt,
                elapsed,
                viewport: (viewport_w, viewport_h),
            },
            persistent,
            &mut logs,
            &mut outcome,
        );
        if paused {
            drain_diagnostics_into(world, &mut logs);
            outcome.logs = logs;
            return outcome;
        }

        let front = self.scheduler.peek();
        let is_local = front
            .map(|f| {
                matches!(world.actors.get(&f).map(|a| a.controller), Some(Controller::Local(_)))
            })
            .unwrap_or(false);

        if let Some(front) = front {
            if is_local {
                let globals = std::mem::take(&mut self.globals);
                let clips = std::mem::take(&mut self.clips);
                let input_res = self.script_engine.run_on_input(
                    world,
                    world_snapshot.clone(),
                    &mut logs,
                    front,
                    persistent,
                    PassArgs {
                        delta_time: sim_dt,
                        elapsed,
                        input: input_snapshot.clone(),
                        mouse: mouse_snapshot,
                        gamepad: gamepad_snapshot.clone(),
                        spawns: &self.level.extra_spawns,
                        globals,
                        clips,
                        camera_pos: camera_origin,
                        commands: BTreeMap::new(),
                        turn_number: self.turn_number,
                        viewport_size: (viewport_w, viewport_h),
                    },
                    animating,
                );
                self.apply_script_result(world, input_res, persistent, &mut logs, &mut outcome);
            }
        }

        // Seam-2 fix: merge externally-supplied commands in — same effect
        // as an entity's own `ctx.submit()` during `on_input`, just sourced
        // from outside instead. Overwrites on a matching actor id, same
        // "last write wins" rule `ScriptState::pending_commands` already has.
        for cmd in external_commands {
            self.commands.insert(cmd.actor as i64, cmd.clone());
        }

        // `mem::take`, not `.clone()`: the housekeeping pass below always
        // overwrites `self.commands` with its own (always-empty) result
        // regardless, so the pre-pass value never needs to survive
        // alongside a copy — but `turn_commands` itself is still read
        // twice below (once by run_scripts, once for `run_actor_turn`), so
        // that second use still needs its own clone.
        let turn_commands = std::mem::take(&mut self.commands);

        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_scripts(
            world,
            world_snapshot.clone(),
            &mut logs,
            persistent,
            PassArgs {
                delta_time: sim_dt,
                elapsed,
                input: input_snapshot.clone(),
                mouse: mouse_snapshot,
                gamepad: gamepad_snapshot.clone(),
                spawns: &self.level.extra_spawns,
                globals,
                clips,
                camera_pos: camera_origin,
                commands: turn_commands.clone(),
                turn_number: self.turn_number,
                viewport_size: (viewport_w, viewport_h),
            },
            animating,
        );
        self.apply_script_result(world, res, persistent, &mut logs, &mut outcome);

        if let Some(front) = front {
            let has_command =
                turn_commands.get(&(front as i64)).map(|c| !c.action.is_empty()).unwrap_or(false);
            if !is_local || has_command {
                self.run_actor_turn(
                    world,
                    world_snapshot,
                    front,
                    is_local,
                    turn_commands,
                    sim_dt,
                    elapsed,
                    persistent,
                    camera_origin,
                    viewport_w,
                    viewport_h,
                    animating,
                    &mut logs,
                    &mut outcome,
                );
            }
        }

        drain_diagnostics_into(world, &mut logs);
        outcome.logs = logs;
        outcome
    }

    /// Runs `actor`'s `on_turn`, applies the result, and — unless a `Local`
    /// actor's action was rejected — advances `self.scheduler` and marks
    /// this step as having consumed a turn.
    #[allow(clippy::too_many_arguments)]
    fn run_actor_turn(
        &mut self,
        world: &mut World,
        snapshot: std::rc::Rc<WorldSnapshot>,
        actor: EntityId,
        is_local: bool,
        commands: BTreeMap<i64, Command>,
        sim_dt: f32,
        elapsed: f32,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_origin: Vec2,
        viewport_w: usize,
        viewport_h: usize,
        animating: &[EntityId],
        logs: &mut Vec<LogEntry>,
        outcome: &mut StepOutcome,
    ) {
        // Step 7.5-7 (docs/ember2d-master-plan.md §5.6): captured before
        // `commands` moves into `run_on_turn` below — `TurnModel::ActionCost`'s
        // own fallback (see the cost computation further down) needs this
        // actor's own queued command cost, if it set one.
        let command_cost = commands.get(&(actor as i64)).and_then(|c| c.cost);

        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_on_turn(
            world,
            snapshot,
            logs,
            actor,
            persistent,
            PassArgs {
                delta_time: sim_dt,
                elapsed,
                input: InputSnapshot::default(),
                mouse: MouseSnapshot::default(),
                gamepad: GamepadSnapshot::default(),
                spawns: &self.level.extra_spawns,
                globals,
                clips,
                camera_pos: camera_origin,
                commands,
                turn_number: self.turn_number,
                viewport_size: (viewport_w, viewport_h),
            },
            animating,
        );
        let act_cost = res.act_cost;
        self.apply_script_result(world, res, persistent, logs, outcome);

        // An AI actor's turn always counts; a Local actor's turn counts only
        // if it called `ctx.act` — see `docs/ember2d-scripting-api.md`'s
        // "The command boundary" for why (a rejected action like a wall
        // bump costs nothing). Guarded on the scheduler's front still being
        // `actor`: `apply_script_result` above may already have removed it
        // (a self-despawn during its own on_turn).
        let consumed = !is_local || act_cost.is_some();
        if consumed && self.scheduler.peek() == Some(actor) {
            let controller =
                world.actors.get(&actor).map(|a| a.controller).unwrap_or(Controller::Ai);
            // Step 7.5-7: `ctx.act(cost)` always wins when a script calls
            // it (unchanged); this is only the FALLBACK when it doesn't —
            // see `TurnModel`'s own doc comment (scheduler.rs) for what
            // each model means.
            let model_default = match self.turn_model {
                TurnModel::Alternating => ALTERNATING_COST as f64,
                TurnModel::Energy => {
                    let speed =
                        world.actors.get(&actor).map(|a| a.speed).unwrap_or(100).max(1);
                    ALTERNATING_COST as f64 * 100.0 / speed as f64
                }
                TurnModel::ActionCost => command_cost.unwrap_or(ALTERNATING_COST as f64),
            };
            let cost = act_cost.unwrap_or(model_default).max(1.0) as u64;
            self.scheduler.advance(actor, controller, cost);
            outcome.turn_triggered = true;
            if is_local {
                self.turn_number += 1;
            }
        }
    }

    /// The late phase: resolve solid collisions and exit triggers from this
    /// step's collision events, then run `on_collide` for every pair
    /// involving a scripted entity. `camera_origin` is the same value the
    /// preceding `step()` call already computed this step (the camera
    /// doesn't move between the two) — a small, deliberate addition beyond
    /// docs/ember2d-phase5.5-plan.md's literal `late_step` sketch, since
    /// `ScriptEngine::run_collisions` requires one exactly like `step` does.
    pub fn late_step(
        &mut self,
        world: &mut World,
        events: &EventBus,
        prev_positions: &HashMap<EntityId, Vec2>,
        camera_origin: Vec2,
        sim_dt: f32,
        elapsed: f32,
        viewport_w: usize,
        viewport_h: usize,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
    ) -> StepOutcome {
        let mut outcome = StepOutcome::default();
        let mut logs = Vec::new();
        let mut all_pairs = Vec::new();
        // Step 9-1: a paused world has no collisions to resolve this step.
        if self.paused_this_step {
            return outcome;
        }

        for event in events.events() {
            let crate::event::GameEvent::Collision { entity_a, entity_b } = event else { continue };
            let (a, b) = (*entity_a, *entity_b);
            all_pairs.push((a, b));

            // 7.5-6 (docs/ember2d-master-plan.md §5.6): solid-collision
            // resolution now covers any `Actor` with `physics` set
            // (default true), not just the local player — split out from
            // the exit-tile check below, which stays local-player-only (an
            // AI actor stepping onto stairs must never trigger a level
            // transition; `actor_has_physics` says nothing about who
            // should be allowed to change levels). The local player's own
            // resolution used to happen inline in the branch below; it
            // still does, just through this shared path now instead of a
            // second, duplicate call.
            let physics_pair = if actor_has_physics(world, a) {
                Some((a, b))
            } else if actor_has_physics(world, b) {
                Some((b, a))
            } else {
                None
            };
            if let Some((mover, obstacle)) = physics_pair {
                if world.colliders.get(&obstacle).map(|c| c.solid).unwrap_or(false) {
                    world.resolve_solid_collision(mover, obstacle, prev_positions);
                } else if world.tilemaps.contains_key(&obstacle) {
                    // Step 8-1: the obstacle is a whole tilemap — push out
                    // of each overlapped solid cell, the same way this
                    // branch above pushes out of one wall entity (see
                    // world/tilemap_collision.rs's header for the contract).
                    world.resolve_tilemap_collision(mover, obstacle);
                }
            }

            let other = if is_local_player(world, a) {
                b
            } else if is_local_player(world, b) {
                a
            } else {
                continue;
            };
            // Solid pairs already got their resolution above (`physics_pair`
            // covers the local player too, since `Actor::local` defaults
            // `physics: true`) — only a non-solid collider (an exit tile)
            // still needs handling here.
            if world.colliders.get(&other).map(|c| c.solid).unwrap_or(false) {
                continue;
            }
            let locked = world.colliders.get(&other).map(|c| c.locked).unwrap_or(false);
            // Step 8-1 (R93): keyed by the exit entity's real id on `World`
            // itself, not a Simulation-side map rebuilt from tile order.
            if let Some(path) = world.exits.get(&other).cloned() {
                if !locked {
                    let full_path =
                        resolve_exit_path(&path, &self.level.path, &|p| self.level_source_exists(p));
                    match self.level_source.load_level(&full_path) {
                        Ok(next) => {
                            outcome.pending_level = Some(next);
                        }
                        Err(e) => {
                            logs.push(LogEntry::warn(format!("Exit failed: {}", e)));
                        }
                    }
                }
            }
        }

        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_collisions(
            world,
            &all_pairs,
            &mut logs,
            persistent,
            PassArgs {
                delta_time: sim_dt,
                elapsed,
                input: InputSnapshot::default(),
                mouse: MouseSnapshot::default(),
                gamepad: GamepadSnapshot::default(),
                spawns: &self.level.extra_spawns,
                globals,
                clips,
                camera_pos: camera_origin,
                commands: BTreeMap::new(),
                turn_number: 0,
                viewport_size: (viewport_w, viewport_h),
            },
        );
        self.apply_script_result(world, res, persistent, &mut logs, &mut outcome);
        drain_diagnostics_into(world, &mut logs);
        outcome.logs = logs;
        outcome
    }

    /// Folds one `ScriptUpdateResult` into `self` (globals/clips/commands/
    /// persistent, scheduler cleanup on despawn, save/load side effects) and
    /// into the in-progress `outcome`/`logs` a caller is accumulating across
    /// however many script passes one `step`/`late_step`/`on_start` call
    /// makes. `pending_level`/`pending_save`/`pending_load` are resolved
    /// (loaded/written) right here rather than left as raw paths — the
    /// caller only ever sees an already-loaded `LevelData`/`SaveState`.
    /// `pub(super)`, not private: `simulation/spawn.rs`'s own `do_on_start`
    /// — a sibling child module, not a descendant of this one — calls this
    /// too, to apply an `on_start` script's result the same way `step`/
    /// `late_step` apply theirs.
    pub(super) fn apply_script_result(
        &mut self,
        world: &mut World,
        mut res: ScriptUpdateResult,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        logs: &mut Vec<LogEntry>,
        outcome: &mut StepOutcome,
    ) {
        // Phase 6 Step 3 (docs/ember2d-phase6-plan.md): every call site
        // above `mem::take`s `self.globals`/`self.clips` immediately before
        // its `run_*` call, specifically so this function's own
        // `self.globals = res.globals` below is a pointer swap rather than
        // a clone. That means `self.globals`/`self.clips` MUST already be
        // empty by the time this function runs — if they're not, either a
        // future caller reverted to `.clone()` (which never empties the
        // source, so this fires immediately) or a take happened without
        // its matching call reaching this point (an early return in
        // between), either of which would otherwise silently duplicate or
        // permanently lose script state with no visible symptom.
        debug_assert!(
            self.globals.is_empty() && self.clips.is_empty(),
            "apply_script_result called without a preceding mem::take of self.globals/self.clips"
        );
        // Step 9-1: scene requests and flow requests from this pass.
        if !res.scene_ops.is_empty() {
            let ops = std::mem::take(&mut res.scene_ops);
            self.apply_scene_ops(world, ops, logs);
        }
        if res.flow.is_some() {
            outcome.flow = res.flow;
        }
        // Step 9-3: menu/dialogue requests.
        if !res.ui_ops.is_empty() {
            self.ui.apply(std::mem::take(&mut res.ui_ops));
            self.script_engine.set_ui_view(self.ui.clone());
        }
        // A despawned actor must not keep cycling a dead turn slot forever.
        for &id in &res.despawned {
            self.scheduler.remove(id);
        }
        if let Some(level_path) = res.pending_level {
            let full = resolve_exit_path(&level_path, &self.level.path, &|p| self.level_source_exists(p));
            match self.level_source.load_level(&full) {
                Ok(next) => {
                    outcome.pending_level = Some(next);
                }
                Err(e) => {
                    logs.push(LogEntry::warn(format!("load_level failed: {}", e)));
                }
            }
        }
        self.globals = res.globals;
        self.clips = res.clips;
        self.commands = res.commands;
        *persistent = res.persistent;

        if let Some(save_path) = res.pending_save {
            // globals/clips were just refreshed from `res` above, so this
            // captures the exact state a script saw the moment it called
            // save_game — defect D17 fix (Step 5c, docs/ember2d-phase5-plan.md).
            // R7 (7A-3, docs/ember2d-master-plan.md): turn_number/scheduler
            // are what makes this a faithful mid-round save — see
            // `SaveState::turn_number`/`::scheduler`'s own doc comments.
            let state = SaveState::new(
                world.clone(),
                persistent.clone(),
                self.globals.clone(),
                self.clips.clone(),
                self.level.path.clone(),
                self.turn_number.max(0) as u64,
                self.scheduler.snapshot(),
            );
            let mut state = state;
            state.scenes = self.scenes.clone(); // Step 9-1
            state.ui = self.ui.clone(); // Step 9-3
            if let Err(e) = state.save_to_file(&save_path) {
                logs.push(LogEntry::error(format!("save_game failed: {}", e)));
            } else {
                logs.push(LogEntry::info(format!("Game saved to {}", save_path)));
            }
        }

        if let Some(load_path) = res.pending_load {
            match SaveState::load_from_file(&load_path) {
                Ok(state) => {
                    outcome.pending_load = Some(state);
                }
                Err(e) => {
                    logs.push(LogEntry::error(format!("load_game failed: {}", e)));
                }
            }
        }

        // Step 9-2: camera requests land on `self.camera` (was a sticky
        // `outcome.camera_override` for `set_camera` alone).
        if !res.camera.is_empty() {
            res.camera.apply_to(&mut self.camera);
            self.script_engine.set_camera_view(self.camera);
        }
        if let Some(shake) = res.shake_state {
            outcome.shake_state = Some(shake);
        }
        outcome.particles.extend(res.particles);
        outcome.animations.extend(res.animations);
    }
}
