// simulation.rs — Simulation: the device-free, headless-steppable
// simulation core (Phase 5.5, docs/ember2d-phase5.5-plan.md Part 2).
//
// This is the real seam-1/seam-2 closure the refactor plan's §5.4 asked
// for and `sim.rs`'s own header comment (in the `ember2d` crate) admitted
// never happened: "scaffolding toward the real `Simulation::step()` a
// later Phase 5 step builds... not that seam itself." `sim.rs` stays
// exactly where it is — it's the generic per-frame pump shared by
// `PlayState`/`EditorState`/`StartScreen`/`PauseMenuState`, all riding the
// same `Engine::run()` loop, and the editor genuinely needs the raw device
// types (`InputManager::take_text()`, mouse wheel, a much larger `Key` set
// than any snapshot covers) that pump threads through — see
// docs/ember2d-phase5.5-plan.md §0.4 for the verified findings behind that
// split. `Simulation` is the actual sim, extracted from what used to be
// spread across `ember2d::play::PlayState`'s methods: it owns every piece
// of state a script/turn/collision pass reads or writes, and its `step`/
// `late_step`/`on_start` take only device-free types (`InputSnapshot`,
// `MouseSnapshot`, `GamepadSnapshot`, `Command`) — no `InputManager`,
// `MouseState`, `GamepadState`, or `winit`/`gilrs` type anywhere in this
// file. `PlayState` now owns only presentation state (camera, particles,
// shake, fps, the debug flag, audio) and delegates to a `Simulation` it
// holds.
//
// `StepInput::external_commands` is the seam-2 fix: commands used to flow
// only internally (`ctx.submit()` in `on_input` -> `ScriptState::pending_commands`
// -> that same step's `on_turn`), with no way for anything outside a step
// to hand it a command. It's merged into `self.commands` at the exact point
// `PlayState::update` used to snapshot `self.commands` for `on_turn` to
// read — see `step`'s own body. A test drives this directly
// (`tests/external_commands.rs`); Phase 9's netcode is the eventual other
// caller.

// `Simulation::do_on_start` lives in `simulation/spawn.rs` — a genuine
// child module (Rust 2018+'s file+sibling-directory layout: `simulation.rs`
// and `simulation/` coexist, so `mod spawn;` here resolves to
// `simulation/spawn.rs`), not a `scripting`-style module-tree sibling. See
// that file's own header comment for why this one is a child instead.
mod spawn;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::{AnimationClip, Controller};
use crate::event::EventBus;
use crate::layers::LayerRegistry;
use crate::level::LevelData;
use crate::math::Vec2;
use crate::save::SaveState;
use crate::scheduler::{TurnScheduler, ALTERNATING_COST};
use crate::scripting::{HudDraw, LogEntry, ScriptEngine, ScriptUpdateResult, ShakeState, WorldSnapshot};
use crate::world::{EntityId, World};

// ── Path resolution ─────────────────────────────────────────────────────────
//
// Moved here from `ember2d::play` (Step 5a-era code) — it's pure string/Path
// logic with no engine dependency, and every one of its callers
// (`Simulation::do_on_start`, `late_step`'s exit-tile resolution) is sim-side
// now. `ember2d::play` keeps a `pub use` re-export so
// `ember2d-editor/src/editor/impl_state.rs`'s existing
// `use ember2d::play::resolve_exit_path` import is untouched.
pub fn resolve_exit_path(next: &str, current_level_path: &str) -> String {
    if Path::new(next).is_absolute() || current_level_path.is_empty() {
        return next.to_string();
    }
    if Path::new(next).exists() {
        return next.to_string();
    }
    match Path::new(current_level_path).parent() {
        Some(dir) if dir != Path::new("") => dir.join(next).to_string_lossy().into_owned(),
        _ => next.to_string(),
    }
}

/// True if `id` is any locally-controlled actor — see
/// `ember2d::play`'s former "Player identity" section (Step 5g,
/// docs/ember2d-phase5-plan.md) for why this is a query, not a stored id.
fn is_local_player(world: &World, id: EntityId) -> bool {
    matches!(world.actors.get(&id).map(|a| a.controller), Some(Controller::Local(_)))
}

/// Every locally-controlled actor, in `EntityId` order (`world.actors` is a
/// `BTreeMap`, Step 5b) — used only for `on_start`'s camera-entity fallback
/// on a loaded save, where more than one might plausibly need considering.
fn local_player_ids(world: &World) -> impl Iterator<Item = EntityId> + '_ {
    world.actors.iter().filter(|(_, a)| matches!(a.controller, Controller::Local(_))).map(|(&id, _)| id)
}

// ── Step input/output ────────────────────────────────────────────────────────

/// Everything one step needs from outside the simulation. A struct rather
/// than a parameter list: Phase 9 will add fields here (peer command
/// batches, authority id) and a struct means those additions don't touch
/// every call site.
pub struct StepInput<'a> {
    pub input: &'a InputSnapshot,
    pub mouse: MouseSnapshot,
    pub gamepad: &'a GamepadSnapshot,
    /// Commands injected from outside — a test harness today
    /// (`tests/external_commands.rs`), a network peer in Phase 9. Merged
    /// into `self.commands` at the point `ember2d::play::PlayState::update`
    /// used to snapshot it for `on_turn` to read (see `step`'s own body).
    /// This is the seam-2 fix.
    ///
    /// One command per actor per step: a matching actor id overwrites.
    /// Correct for `TurnModel::Alternating`; `Declared` and netcode will
    /// want a queue per actor, so revisit here rather than at the call site.
    pub external_commands: &'a [Command],
    /// Caller-supplied — `Simulation` does not own the presentation
    /// `Camera` (that stays in `ember2d::play::PlayState`).
    pub camera_origin: Vec2,
    pub sim_dt: f32,
    pub elapsed: f32,
    pub viewport_w: usize,
    pub viewport_h: usize,
}

/// What a caller needs back after a `step`/`late_step`/`on_start` call.
/// `pending_level`/`pending_load` are already-resolved (loaded from disk),
/// not raw path strings — the caller's only job is turning a `Some` here
/// into its own `Transition` (an `ember2d`-only type `Simulation` can't
/// reference). `camera_override`/`shake_state` report "a script asked for
/// this just now"; the caller is the one that decides whether that's sticky
/// (camera override, until changed again) or a fresh timer (shake) — see
/// `ember2d::play::PlayState::update`'s handling of each.
#[derive(Default)]
pub struct StepOutcome {
    pub turn_triggered: bool,
    pub camera_override: Option<Vec2>,
    pub shake_state: Option<ShakeState>,
    pub particles: Vec<crate::scripting::ParticleRequest>,
    pub pending_level: Option<LevelData>,
    pub pending_load: Option<SaveState>,
    /// Visual events queued this step (Phase 5.5 Part 3,
    /// docs/ember2d-phase5.5-plan.md) — `ember2d::play::PlayState` drains
    /// these into its own presentation-side playback queue; `Simulation`
    /// never reads them back.
    pub animations: Vec<crate::scripting::AnimationEvent>,
    // Phase 6: `logs`/`particles`/`animations` are a fresh Vec every call —
    // the exact per-step allocation category D11/Phase 6 exists to reduce.
    // Kept as-is here to keep this phase's diff readable; revisit with
    // reusable buffers or `&mut Vec` out-params if profiling ever calls
    // for it.
    pub logs: Vec<LogEntry>,
}

/// What `ember2d::play::PlayState::flush_audio` needs each call — a narrow
/// view into `ScriptEngine`'s pending-audio queues rather than exposing the
/// whole engine, so `Simulation` stays the only thing that touches it.
pub struct AudioRequests {
    pub sounds: Vec<String>,
    pub spatial_sounds: Vec<(String, f32, f32)>,
    pub music: Option<String>,
    pub stop_music: bool,
}

// ── Simulation ────────────────────────────────────────────────────────────────

pub struct Simulation {
    level: LevelData,
    script_engine: ScriptEngine,
    /// Deterministic turn order (Step 5f, docs/ember2d-phase5-plan.md) — see
    /// `scheduler.rs`'s header comment for the split of responsibility
    /// between it (a dumb ordering primitive) and `run_actor_turn` (the "is
    /// this actor local, does it have a command yet" policy). Rebuilt from
    /// scratch by `rebuild_scheduler` at level load.
    scheduler: TurnScheduler,
    globals: BTreeMap<String, rhai::Dynamic>,
    clips: BTreeMap<String, AnimationClip>,
    /// This step's commands, keyed by actor id — see `ScriptUpdateResult::commands`'s
    /// doc comment for why this doesn't accumulate across steps.
    commands: BTreeMap<i64, Command>,
    /// How many turns the local player has completed so far this level —
    /// what `ctx.get_turn_number()` reads.
    turn_number: i64,
    camera_entity: Option<EntityId>,
    exit_targets: HashMap<EntityId, String>,
    is_loading_save: bool,
    /// Phase 6 Step 7 (docs/ember2d-phase6-plan.md): built once here, from
    /// `level.collision_layers`, before anything else runs — see
    /// `crate::layers::LayerRegistry`'s own doc comment for why fixed at
    /// load and never grown. `ScriptEngine` holds its own independent copy
    /// (built identically, passed in at `ScriptEngine::new`) rather than
    /// sharing this one by reference, since threading a reference through
    /// every `WorldSnapshot`/`ScriptState` constructor would touch far more
    /// signatures for no real benefit — the registry is at most 31 entries.
    layers: LayerRegistry,
}

impl Simulation {
    pub fn new(level: LevelData) -> Self {
        let seed = level.seed;
        let layers = LayerRegistry::new(&level.collision_layers);
        Simulation {
            level,
            script_engine: ScriptEngine::new(seed, layers.clone()),
            scheduler: TurnScheduler::new(),
            globals: BTreeMap::new(),
            clips: BTreeMap::new(),
            commands: BTreeMap::new(),
            turn_number: 0,
            camera_entity: None,
            exit_targets: HashMap::new(),
            is_loading_save: false,
            layers,
        }
    }

    /// `globals`/`clips` come from a loaded `SaveState` — defect D17 fix
    /// (Step 5c, docs/ember2d-phase5-plan.md). `on_start`'s `is_loading_save`
    /// branch deliberately never re-runs a script's own `on_start`, so
    /// nothing else would populate these otherwise (several scripts'
    /// `on_start` writes are unconditional — re-running them on load would
    /// silently reset state like a re-healed enemy).
    pub fn from_save(level: LevelData, globals: BTreeMap<String, rhai::Dynamic>, clips: BTreeMap<String, AnimationClip>) -> Self {
        let mut sim = Self::new(level);
        sim.is_loading_save = true;
        sim.globals = globals;
        sim.clips = clips;
        sim
    }

    pub fn level(&self) -> &LevelData { &self.level }
    /// This simulation's layer name<->bit table (Phase 6 Step 7,
    /// docs/ember2d-phase6-plan.md) — exposed for `bench_sim`'s direct,
    /// isolated `WorldSnapshot::build` timing, which needs the same
    /// registry a real step would use without re-deriving it from
    /// `level().collision_layers` by hand.
    pub fn layers(&self) -> &LayerRegistry { &self.layers }
    pub fn camera_entity(&self) -> Option<EntityId> { self.camera_entity }
    /// Whose turn is up — lets a caller target `StepInput::external_commands`
    /// at the right actor.
    pub fn current_actor(&self) -> Option<EntityId> { self.scheduler.peek() }
    pub fn globals(&self) -> &BTreeMap<String, rhai::Dynamic> { &self.globals }
    pub fn clips(&self) -> &BTreeMap<String, AnimationClip> { &self.clips }
    pub fn pending_hud_draws(&self) -> &[HudDraw] { &self.script_engine.pending_hud_draws }

    pub fn take_audio_requests(&mut self) -> AudioRequests {
        AudioRequests {
            sounds: std::mem::take(&mut self.script_engine.pending_sounds),
            spatial_sounds: std::mem::take(&mut self.script_engine.pending_spatial_sounds),
            music: self.script_engine.pending_music.take(),
            stop_music: std::mem::replace(&mut self.script_engine.stop_music, false),
        }
    }

    /// (Re)populate `self.scheduler` from every entity `World` currently has
    /// an `Actor` component for. Iterates `world.actors` (a `BTreeMap`, Step
    /// 5b) in ascending `EntityId` order for a reproducible insertion
    /// sequence, though `TurnScheduler::insert`'s own rank/id tiebreak
    /// already makes the *outcome* order-independent.
    fn rebuild_scheduler(&mut self, world: &World) {
        self.scheduler = TurnScheduler::new();
        for (&id, actor) in &world.actors {
            self.scheduler.insert(id, actor.controller);
        }
    }

    // do_on_start moved to simulation/spawn.rs (Phase 6 Step 7,
    // docs/ember2d-phase6-plan.md) — this file was at the project's
    // 600-line hard limit (CLAUDE.md); spawning is the single largest,
    // most self-contained method here. Pure relocation — see that file's
    // own header comment for the module-tree mechanics (a genuine child
    // module, not a scripting-style sibling).

    /// Spawns the level (or, on a loaded save, just compiles scripts and
    /// re-derives `camera_entity` — `SaveState` doesn't carry it) and builds
    /// the turn scheduler. Returns whatever got logged.
    pub fn on_start(&mut self, world: &mut World, viewport_w: usize, viewport_h: usize, persistent: &mut BTreeMap<String, rhai::Dynamic>) -> Vec<LogEntry> {
        let mut logs = Vec::new();
        if !self.is_loading_save {
            self.do_on_start(world, viewport_w, viewport_h, persistent, &mut logs);
        } else {
            for (_, script) in &world.scripts { self.script_engine.compile(&script.path, &mut logs); }
            if self.camera_entity.is_none() { self.camera_entity = local_player_ids(world).next(); }
            // Phase 6 Step 7 (docs/ember2d-phase6-plan.md): a loaded save's
            // `World` round-tripped through serialization, which skips
            // `Collider::layer_bits`/`mask_bits` (`#[serde(skip)]`) — every
            // collider's bits are zero at this point even though `layer`/
            // `mask` themselves survived intact. Without this, every loaded
            // save would silently stop filtering collisions at all (`mask_bits
            // == 0` reads as "matches everything," indistinguishable from a
            // mask that legitimately matched) until something else happened
            // to rewrite the layer/mask through a script setter. See
            // `Collider`'s own header comment (components/collider.rs) for
            // why this is the one mistake here with no visible symptom.
            world.refresh_collider_bits(&self.layers);
        }
        // Both branches above leave `world.actors` fully populated (spawned
        // fresh, or round-tripped through `World`'s own (de)serialization) —
        // this is the one place after either that's guaranteed true.
        self.rebuild_scheduler(world);
        logs
    }

    /// One simulation step: advance animators, run `on_input` for whichever
    /// local actor the scheduler is waiting on, merge in
    /// `external_commands` (the seam-2 fix), run the housekeeping
    /// `on_update` pass for every scripted entity, then `on_turn` for
    /// whichever single actor is due (if it has a command, or is AI).
    /// Mirrors `ember2d::play::PlayState::update`'s former body exactly —
    /// see that history for why the pass order (on_update before on_turn,
    /// not after) is load-bearing, not arbitrary.
    pub fn step(&mut self, world: &mut World, input: StepInput<'_>, persistent: &mut BTreeMap<String, rhai::Dynamic>) -> StepOutcome {
        let StepInput { input: input_snapshot, mouse: mouse_snapshot, gamepad: gamepad_snapshot, external_commands, camera_origin, sim_dt, elapsed, viewport_w, viewport_h } = input;

        let mut outcome = StepOutcome::default();
        let mut logs = Vec::new();

        // Advance every Animator before scripts run this step, so
        // `clip_finished(id)` reflects this tick, not last step's.
        for animator in world.animators.values_mut() {
            if let Some(clip) = self.clips.get(&animator.clip) { animator.advance(clip, sim_dt); }
            else { animator.just_finished = false; }
        }

        // Built once per step, shared (via cheap `Rc::clone`) across
        // on_input/on_update/on_turn below — see `WorldSnapshot`'s own doc
        // comment (scripting/state.rs) for the perf regression this fixes.
        let world_snapshot = std::rc::Rc::new(WorldSnapshot::build(world, &self.layers));

        let front = self.scheduler.peek();
        let is_local = front.map(|f| matches!(world.actors.get(&f).map(|a| a.controller), Some(Controller::Local(_)))).unwrap_or(false);

        if let Some(front) = front {
            if is_local {
                let globals = std::mem::take(&mut self.globals);
                let clips = std::mem::take(&mut self.clips);
                let input_res = self.script_engine.run_on_input(
                    world, world_snapshot.clone(), &mut logs, front, sim_dt, elapsed, input_snapshot.clone(),
                    mouse_snapshot, gamepad_snapshot.clone(), &self.level.extra_spawns, globals, clips,
                    persistent, camera_origin, self.turn_number, (viewport_w, viewport_h),
                );
                self.apply_script_result(world, input_res, persistent, &mut logs, &mut outcome);
            }
        }

        // Seam-2 fix: merge externally-supplied commands in — same effect
        // as an entity's own `ctx.submit()` during `on_input`, just sourced
        // from outside instead. Overwrites on a matching actor id, same
        // "last write wins" rule `ScriptState::pending_commands` already has.
        for cmd in external_commands { self.commands.insert(cmd.actor as i64, cmd.clone()); }

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
            world, world_snapshot.clone(), &mut logs, sim_dt, elapsed, input_snapshot.clone(), mouse_snapshot, gamepad_snapshot.clone(),
            &self.level.extra_spawns, globals, clips, persistent, camera_origin,
            turn_commands.clone(), self.turn_number, (viewport_w, viewport_h),
        );
        self.apply_script_result(world, res, persistent, &mut logs, &mut outcome);

        if let Some(front) = front {
            let has_command = turn_commands.get(&(front as i64)).map(|c| !c.action.is_empty()).unwrap_or(false);
            if !is_local || has_command {
                self.run_actor_turn(world, world_snapshot, front, is_local, turn_commands, sim_dt, elapsed, persistent, camera_origin, viewport_w, viewport_h, &mut logs, &mut outcome);
            }
        }

        outcome.logs = logs;
        outcome
    }

    /// Runs `actor`'s `on_turn`, applies the result, and — unless a `Local`
    /// actor's action was rejected — advances `self.scheduler` and marks
    /// this step as having consumed a turn.
    #[allow(clippy::too_many_arguments)]
    fn run_actor_turn(
        &mut self, world: &mut World, snapshot: std::rc::Rc<WorldSnapshot>, actor: EntityId, is_local: bool, commands: BTreeMap<i64, Command>,
        sim_dt: f32, elapsed: f32, persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_origin: Vec2, viewport_w: usize, viewport_h: usize, logs: &mut Vec<LogEntry>, outcome: &mut StepOutcome,
    ) {
        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_on_turn(
            world, snapshot, logs, actor, sim_dt, elapsed,
            &self.level.extra_spawns, globals, clips,
            persistent, camera_origin, commands, self.turn_number,
            (viewport_w, viewport_h),
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
            let controller = world.actors.get(&actor).map(|a| a.controller).unwrap_or(Controller::Ai);
            let cost = act_cost.unwrap_or(ALTERNATING_COST as f64).max(1.0) as u64;
            self.scheduler.advance(actor, controller, cost);
            outcome.turn_triggered = true;
            if is_local { self.turn_number += 1; }
        }
    }

    /// The late phase: resolve solid collisions and exit triggers from this
    /// step's collision events, then run `on_collide` for every pair
    /// involving a scripted entity. `camera_origin` is the same value the
    /// preceding `step()` call already computed this step (the camera
    /// doesn't move between the two) — a small, deliberate addition beyond
    /// docs/ember2d-phase5.5-plan.md's literal `late_step` sketch, since
    /// `ScriptEngine::run_collisions` requires one exactly like `step` does.
    pub fn late_step(&mut self, world: &mut World, events: &EventBus, prev_positions: &HashMap<EntityId, Vec2>, camera_origin: Vec2, sim_dt: f32, elapsed: f32, viewport_w: usize, viewport_h: usize, persistent: &mut BTreeMap<String, rhai::Dynamic>) -> StepOutcome {
        let mut outcome = StepOutcome::default();
        let mut logs = Vec::new();
        let mut all_pairs = Vec::new();

        for event in events.events() {
            let crate::event::GameEvent::Collision { entity_a, entity_b } = event else { continue };
            let (a, b) = (*entity_a, *entity_b);
            all_pairs.push((a, b));
            let (player, other) = if is_local_player(world, a) { (a, b) } else if is_local_player(world, b) { (b, a) } else { continue };
            let solid = world.colliders.get(&other).map(|c| c.solid).unwrap_or(false);
            let locked = world.colliders.get(&other).map(|c| c.locked).unwrap_or(false);

            if solid { world.resolve_solid_collision(player, other, prev_positions); }
            else if let Some(path) = self.exit_targets.get(&other).cloned() {
                if !locked {
                    let full_path = resolve_exit_path(&path, &self.level.path);
                    match LevelData::load(&full_path) {
                        Ok(next) => { outcome.pending_level = Some(next); }
                        Err(e) => { logs.push(LogEntry::warn(format!("Exit failed: {}", e))); }
                    }
                }
            }
        }

        let globals = std::mem::take(&mut self.globals);
        let clips = std::mem::take(&mut self.clips);
        let res = self.script_engine.run_collisions(
            world, &all_pairs, &mut logs, sim_dt, elapsed,
            &self.level.extra_spawns, globals, clips, persistent, camera_origin,
            (viewport_w, viewport_h),
        );
        self.apply_script_result(world, res, persistent, &mut logs, &mut outcome);
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
    fn apply_script_result(&mut self, world: &mut World, res: ScriptUpdateResult, persistent: &mut BTreeMap<String, rhai::Dynamic>, logs: &mut Vec<LogEntry>, outcome: &mut StepOutcome) {
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
        debug_assert!(self.globals.is_empty() && self.clips.is_empty(), "apply_script_result called without a preceding mem::take of self.globals/self.clips");
        // A despawned actor must not keep cycling a dead turn slot forever.
        for &id in &res.despawned { self.scheduler.remove(id); }
        if let Some(level_path) = res.pending_level {
            let full = resolve_exit_path(&level_path, &self.level.path);
            match LevelData::load(&full) {
                Ok(next) => { outcome.pending_level = Some(next); }
                Err(e) => { logs.push(LogEntry::warn(format!("load_level failed: {}", e))); }
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
            let state = SaveState::new(world.clone(), persistent.clone(), self.globals.clone(), self.clips.clone(), self.level.path.clone());
            if let Err(e) = state.save_to_file(&save_path) {
                logs.push(LogEntry::error(format!("save_game failed: {}", e)));
            } else {
                logs.push(LogEntry::info(format!("Game saved to {}", save_path)));
            }
        }

        if let Some(load_path) = res.pending_load {
            match SaveState::load_from_file(&load_path) {
                Ok(state) => { outcome.pending_load = Some(state); }
                Err(e) => { logs.push(LogEntry::error(format!("load_game failed: {}", e))); }
            }
        }

        if res.camera_override.is_some() { outcome.camera_override = res.camera_override; }
        if let Some(shake) = res.shake_state { outcome.shake_state = Some(shake); }
        outcome.particles.extend(res.particles);
        outcome.animations.extend(res.animations);
    }
}
