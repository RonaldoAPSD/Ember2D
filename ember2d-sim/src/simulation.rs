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
// `Simulation::step`/`late_step`/`run_actor_turn`/`apply_script_result`
// live in `simulation/step.rs` (R76, docs/ember2d-master-plan.md §3.2) —
// same child-module mechanics as `spawn` above; this file was 791 real
// lines, over CLAUDE.md's 750-line limit, and the per-step execution was
// the single largest remaining piece to pull out. See that file's own
// header comment.
mod step;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use crate::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::{AnimationClip, Controller};
use crate::layers::LayerRegistry;
use crate::level::LevelData;
use crate::math::Vec2;
use crate::save::SaveState;
use crate::scheduler::TurnScheduler;
use crate::scripting::{HudDraw, LogEntry, ScriptEngine, ShakeState};
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

/// True if `id` is any `Actor` (local, AI, or a future `Remote`) whose own
/// `physics` flag is set — Step 7.5-6 (docs/ember2d-master-plan.md §5.6):
/// what `late_step`'s solid-collision resolution gates on now, broader
/// than `is_local_player`'s narrower "who receives on_input" question.
/// `unwrap_or(false)`: an entity with no `Actor` at all (every enemy in
/// the shooter demo, deliberately — see `gen_shooter.rs`'s own header for
/// why giving them one would stall the local player's `on_input` under
/// `TurnScheduler`) never gets engine-side resolution this way either;
/// `physics: false` is the same "opt out" for an entity that DOES have an
/// `Actor` but manages its own collision by hand.
fn actor_has_physics(world: &World, id: EntityId) -> bool {
    world.actors.get(&id).map(|a| a.physics).unwrap_or(false)
}

/// Every locally-controlled actor, in `EntityId` order (`world.actors` is a
/// `BTreeMap`, Step 5b) — used only for `on_start`'s camera-entity fallback
/// on a loaded save, where more than one might plausibly need considering.
fn local_player_ids(world: &World) -> impl Iterator<Item = EntityId> + '_ {
    world
        .actors
        .iter()
        .filter(|(_, a)| matches!(a.controller, Controller::Local(_)))
        .map(|(&id, _)| id)
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
    /// R16 (7A-5, docs/ember2d-master-plan.md): incremented once per `step`
    /// call, regardless of mode or whether a turn actually resolved —
    /// `step_count() * sim_dt` is what `ctx.get_elapsed()` reads (via
    /// `StepInput::elapsed`) instead of the caller's own wall-clock time.
    /// Unlike `turn_number` (which only counts a LOCAL actor's resolved
    /// turns, meaningful only in turn-based mode), this counts every call
    /// uniformly, which is what makes it the right basis for "how much sim
    /// time has passed" in realtime mode too — see this crate's `CLAUDE.md`
    /// Determinism section: replaying the same input sequence at a
    /// different real frame rate must produce the same `get_elapsed()`
    /// progression, which a wall-clock value can never guarantee.
    step_count: u64,
    camera_entity: Option<EntityId>,
    exit_targets: HashMap<EntityId, String>,
    is_loading_save: bool,
    /// R7 (7A-3, docs/ember2d-master-plan.md): the saved scheduler state,
    /// staged here by `from_save` for `on_start`'s loading-save branch to
    /// consume (`World` — needed to resolve each actor's `Controller` for
    /// `TurnScheduler::restore` — isn't available until `on_start` runs).
    /// Empty for a save written before this field existed (`SaveState`'s
    /// own `#[serde(default)]`) or a fresh (non-loaded) level, in which
    /// case `on_start` falls back to `rebuild_scheduler`.
    pending_scheduler: Vec<(EntityId, u64)>,
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
            step_count: 0,
            camera_entity: None,
            exit_targets: HashMap::new(),
            is_loading_save: false,
            pending_scheduler: Vec::new(),
            layers,
        }
    }

    /// `globals`/`clips` come from a loaded `SaveState` — defect D17 fix
    /// (Step 5c, docs/ember2d-phase5-plan.md). `on_start`'s `is_loading_save`
    /// branch deliberately never re-runs a script's own `on_start`, so
    /// nothing else would populate these otherwise (several scripts'
    /// `on_start` writes are unconditional — re-running them on load would
    /// silently reset state like a re-healed enemy).
    /// R7 (7A-3, docs/ember2d-master-plan.md): `turn_number`/`scheduler`
    /// come from the same `SaveState` as `globals`/`clips` — see
    /// `pending_scheduler`'s own doc comment for why the scheduler restore
    /// itself waits until `on_start`.
    pub fn from_save(
        level: LevelData,
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        turn_number: u64,
        scheduler: Vec<(EntityId, u64)>,
    ) -> Self {
        let mut sim = Self::new(level);
        sim.is_loading_save = true;
        sim.globals = globals;
        sim.clips = clips;
        sim.turn_number = turn_number as i64;
        sim.pending_scheduler = scheduler;
        sim
    }

    pub fn level(&self) -> &LevelData {
        &self.level
    }
    /// This simulation's layer name<->bit table (Phase 6 Step 7,
    /// docs/ember2d-phase6-plan.md) — exposed for `bench_sim`'s direct,
    /// isolated `WorldSnapshot::build` timing, which needs the same
    /// registry a real step would use without re-deriving it from
    /// `level().collision_layers` by hand.
    pub fn layers(&self) -> &LayerRegistry {
        &self.layers
    }
    pub fn camera_entity(&self) -> Option<EntityId> {
        self.camera_entity
    }
    /// Whose turn is up — lets a caller target `StepInput::external_commands`
    /// at the right actor.
    pub fn current_actor(&self) -> Option<EntityId> {
        self.scheduler.peek()
    }
    /// How many turns the local player has completed — the save-side
    /// counterpart to `ctx.get_turn_number()`, and one of the two fields
    /// R7 (7A-3, docs/ember2d-master-plan.md) adds to `SaveState`.
    pub fn turn_number(&self) -> i64 {
        self.turn_number
    }
    /// R16 (7A-5, docs/ember2d-master-plan.md): how many `step` calls have
    /// completed so far — see `step_count`'s own doc comment for why a
    /// caller building this step's `StepInput::elapsed` should multiply
    /// this (read BEFORE calling `step`) by its own fixed timestep instead
    /// of using wall-clock time.
    pub fn step_count(&self) -> u64 {
        self.step_count
    }
    /// The scheduler's exact (actor, due) state — see
    /// `TurnScheduler::snapshot`'s own doc comment.
    pub fn scheduler_snapshot(&self) -> Vec<(EntityId, u64)> {
        self.scheduler.snapshot()
    }
    pub fn globals(&self) -> &BTreeMap<String, rhai::Dynamic> {
        &self.globals
    }
    pub fn clips(&self) -> &BTreeMap<String, AnimationClip> {
        &self.clips
    }
    pub fn pending_hud_draws(&self) -> &[HudDraw] {
        &self.script_engine.pending_hud_draws
    }

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

    /// R7 (7A-3, docs/ember2d-master-plan.md): rebuilds `exit_targets`
    /// purely from `self.level.tiles`'s own order — the exact id sequence
    /// `do_on_start`'s spawn loop assigns (a fresh `World`'s ids start at 1
    /// and increment once per tile, in `level.tiles` order, with nothing
    /// else spawned in between). Reproducing that sequence here, without
    /// touching `world` at all, is what lets the SAME function run against
    /// a loaded save's `World` (already fully populated via
    /// deserialization — re-spawning would duplicate every tile) exactly
    /// as safely as it runs during a fresh spawn — see `on_start`'s two
    /// branches, and `simulation/spawn.rs`'s own call site for why this
    /// used to be an inline `self.exit_targets.insert(id, ...)` tied to
    /// that loop's real `world.spawn()` id instead.
    fn index_exits(&mut self) {
        self.exit_targets.clear();
        let mut id: EntityId = 1;
        for tile in &self.level.tiles {
            if let Some(ref path) = tile.next_level {
                self.exit_targets.insert(id, path.clone());
            }
            id += 1;
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
    pub fn on_start(
        &mut self,
        world: &mut World,
        viewport_w: usize,
        viewport_h: usize,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
    ) -> Vec<LogEntry> {
        let mut logs = Vec::new();
        if !self.is_loading_save {
            // `do_on_start` calls `index_exits` and this crate's normal
            // fresh-spawn scheduler build — see that function's own body.
            self.do_on_start(world, viewport_w, viewport_h, persistent, &mut logs);
            self.rebuild_scheduler(world);
        } else {
            for script in world.scripts.values() {
                self.script_engine.compile(&script.path, &mut logs);
            }
            if self.camera_entity.is_none() {
                self.camera_entity = local_player_ids(world).next();
            }
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
            // R7 (7A-3, docs/ember2d-master-plan.md): `exit_targets` used to
            // never get built on this branch at all — stairs were dead on
            // every loaded save. `index_exits` is a pure function of
            // `self.level`, so it's exactly as safe to call here as it is
            // during a fresh spawn.
            self.index_exits();
            // R7: restore the exact scheduler state a mid-round save
            // captured, rather than resetting every actor to the same due
            // time — see `pending_scheduler`'s own doc comment. An empty
            // list (a pre-7A-3 save, via `SaveState`'s `#[serde(default)]`)
            // falls back to the old rebuild-from-scratch behavior.
            let saved_schedule = std::mem::take(&mut self.pending_scheduler);
            if saved_schedule.is_empty() {
                self.rebuild_scheduler(world);
            } else {
                self.scheduler
                    .restore(&saved_schedule, |id| world.actors.get(&id).map(|a| a.controller));
            }
            // Step 7.5-5 (docs/ember2d-master-plan.md §5.6): `on_load`
            // fires exactly once here, for every scripted entity a save
            // deserialized — the loaded-save counterpart to `do_on_start`'s
            // own `on_start` call above, which this branch never runs (see
            // this branch's own opening comment). `cam_pos` is computed the
            // same way `do_on_start` (simulation/spawn.rs) computes it for
            // its own `run_on_start_all` call — `camera_entity` is already
            // resolved by this point (just above).
            let cam_pos =
                self.camera_entity.map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO);
            let game_h = (viewport_h as i32).max(1);
            let cam_x = (cam_pos.x - viewport_w as f32 / 2.0).max(0.0).round();
            let cam_y = (cam_pos.y - game_h as f32 / 2.0).max(0.0).round();
            let globals = std::mem::take(&mut self.globals);
            let clips = std::mem::take(&mut self.clips);
            let res = self.script_engine.run_on_load_all(
                world,
                &mut logs,
                &self.level.extra_spawns,
                globals,
                clips,
                persistent,
                Vec2::new(cam_x, cam_y),
                (viewport_w, viewport_h),
            );
            // Same as `do_on_start`'s own call: `on_start`/`on_load`'s
            // return type is a plain `Vec<LogEntry>`, not a `StepOutcome` —
            // this `outcome` only exists to satisfy `apply_script_result`'s
            // signature and is discarded (a level-load pass has nothing
            // that reads a camera override or a particle request yet).
            let mut outcome = StepOutcome::default();
            self.apply_script_result(world, res, persistent, &mut logs, &mut outcome);
        }
        logs
    }

    // step/late_step (and their run_actor_turn/apply_script_result helpers)
    // moved to simulation/step.rs (R76, docs/ember2d-master-plan.md §3.2) —
    // this file was over the project's 750-line hard limit (CLAUDE.md); the
    // per-step execution was the single largest, most self-contained unit
    // left to pull out. See that file's own header comment.
}
