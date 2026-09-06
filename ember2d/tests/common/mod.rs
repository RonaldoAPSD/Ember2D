// tests/common/mod.rs — shared headless test harness for driving a
// TurnBased-mode Simulation directly.
//
// Not auto-discovered as its own test target by Cargo — only files
// directly under `tests/`, not in subdirectories, are — so this is
// included via `mod common;` in whichever test file needs it.
//
// **Rewritten in Phase 5.5** (docs/ember2d-phase5.5-plan.md Part 2, Step
// 2d): used to own a `PlayState` plus real `InputManager`/`MouseState`/
// `GamepadState` and drive them all through `ember2d::sim::step` — a real,
// if synthetic, device stack, because that was the only place the turn
// sequence lived at the time (see this file's own history before this
// rewrite, in `docs/HANDOFF.md`/git blame, for that hazard's original
// shape). Now that `ember2d_sim::simulation::Simulation` exists and owns
// the sequence directly, none of that is needed: this harness builds an
// `InputSnapshot` by hand and calls `Simulation::step`/`late_step`
// directly, with zero winit/gilrs types (`InputManager`, `MouseState`,
// `GamepadState`, `Key`) anywhere in it. This is strictly simpler for test
// authors and loses nothing functionally — the buffering/decay machinery
// those device types exist for is `input.rs`'s own concern (already tested
// there), not something a turn-logic test needs to re-exercise on every
// synthetic press.

use std::collections::{BTreeMap, BTreeSet};
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::simulation::{Simulation, StepInput};

pub struct TurnHarness {
    pub world: World,
    pub sim: Simulation,
    pub persistent: BTreeMap<String, rhai::Dynamic>,
    pub elapsed: f32,
    pub viewport_width: usize,
    pub viewport_height: usize,
    /// The most recent `load_level`/`load_game` request either `step` or
    /// `late_step` produced, if any — see `take_pending_level`.
    pending_level: Option<LevelData>,
}

/// Fixed per-frame time this harness advances by — there's no real
/// wall-clock here, so this stands in for both `Simulation::step`'s
/// `sim_dt` (what the real engine calls `SIM_DT`) and the `frame_dt` a real
/// frame's measured wall-clock delta would be. A perfectly steady simulated
/// 60 fps is exactly what a headless test with no real timing should assume.
const HARNESS_DT: f32 = 1.0 / 60.0;

/// Safety cap on `TurnHarness::turn`'s follow-up-frame drain — well above
/// anything the roguelike's own levels ever schedule (floor3, the busiest,
/// has 2 rats + 1 boss = 3 AI actors per round), so this only ever fires on
/// a genuine `TurnScheduler` bug (an actor whose turn never actually
/// consumes), turning a hang into a clear test failure instead.
const MAX_ROUND_FOLLOW_UP_FRAMES: u32 = 50;

/// `cargo test` runs each integration test binary with its CWD set to the
/// *package's own* directory (`ember2d/`), unlike `cargo run` (CWD = wherever
/// invoked from). Since Step 5i's workspace split moved this package one
/// level below the repo root, that quirk breaks more than just the
/// top-level level-file path: a level's own `tile.script`/`next_level`
/// fields are authored as repo-root-relative strings too, and
/// `resolve_exit_path` resolves them by checking `Path::new(next).exists()`
/// against CWD first. `set_current_dir` once, here, makes every one of
/// those paths resolve exactly as they did before the split. Idempotent and
/// safe to call from every test (including in parallel — they all set the
/// same target).
fn ensure_workspace_root_cwd() {
    let _ = std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
}

impl TurnHarness {
    /// Load a real `.level` file and run its `on_start` scripts, the same
    /// way `app.rs::run_play_app` does before the engine's first frame.
    pub fn load(path: &str) -> Self {
        ensure_workspace_root_cwd();
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let mut world = World::new();
        let mut persistent = BTreeMap::new();
        let mut sim = Simulation::new(data);
        let (viewport_width, viewport_height) = (80, 24);
        sim.on_start(&mut world, viewport_width, viewport_height, &mut persistent);

        TurnHarness { world, sim, persistent, elapsed: 0.0, viewport_width, viewport_height, pending_level: None }
    }

    /// One simulation step in TurnBased mode. Returns whether a turn was
    /// triggered this step (mirrors `Simulation::StepOutcome::turn_triggered`).
    /// `press`, if given, is a key name from `InputSnapshot::snapshot()`'s
    /// `KEY_MAP` (e.g. `"w"`, `"space"`) — a synthetic press is a one-frame
    /// tap (`held` and `pressed` both true for exactly this one step), same
    /// as every existing test's usage pattern assumes.
    pub fn frame(&mut self, press: Option<&str>) -> bool {
        let prev_positions = self.world.snapshot_positions();

        let mut keys = BTreeSet::new();
        if let Some(k) = press { keys.insert(k.to_string()); }
        let input = InputSnapshot { held: keys.clone(), pressed: keys };
        let mouse = MouseSnapshot::default();
        let gamepad = GamepadSnapshot::default();

        let outcome = self.sim.step(&mut self.world, StepInput {
            input: &input,
            mouse,
            gamepad: &gamepad,
            external_commands: &[],
            camera_origin: Vec2::ZERO,
            sim_dt: HARNESS_DT,
            elapsed: self.elapsed,
            viewport_w: self.viewport_width,
            viewport_h: self.viewport_height,
        }, &mut self.persistent);

        if outcome.pending_level.is_some() { self.pending_level = outcome.pending_level; }
        let turn_triggered = outcome.turn_triggered;

        // Mirrors `ember2d::sim::step`'s TurnBased branch: the late phase
        // (collision detection + late_step) only runs on a step that
        // actually resolved a turn — physics itself never integrates in
        // turn mode at all (D7 fix, Step 5f), so there's nothing to
        // integrate here either.
        if turn_triggered {
            let mut events = EventBus::new();
            self.world.detect_collisions(&mut events);
            let late_outcome = self.sim.late_step(
                &mut self.world, &events, &prev_positions, Vec2::ZERO,
                HARNESS_DT, self.elapsed, self.viewport_width, self.viewport_height, &mut self.persistent,
            );
            if late_outcome.pending_level.is_some() { self.pending_level = late_outcome.pending_level; }
        }

        self.elapsed += HARNESS_DT;
        turn_triggered
    }

    /// A player action, plus every follow-up input-less frame it causes —
    /// one full round. `TurnScheduler` resolves "one actor per step"
    /// (scheduler.rs's own doc comment) — the player's press consumes one
    /// frame, then each AI actor `TurnScheduler` hands the turn to consumes
    /// one more frame of its own, however many there are. This drains every
    /// one of those follow-up frames (each of which returns `triggered:
    /// true`, since an AI actor's turn always counts) until control returns
    /// to the player awaiting its next command (a `frame(None)` that
    /// returns `false`), so callers still see "one player action -> the
    /// round it causes" as a single logical turn.
    #[allow(dead_code)]
    pub fn turn(&mut self, key: &str) -> bool {
        let triggered = self.frame(Some(key));
        let mut follow_up_frames = 0;
        while self.frame(None) {
            follow_up_frames += 1;
            assert!(
                follow_up_frames < MAX_ROUND_FOLLOW_UP_FRAMES,
                "turn() didn't return to the player awaiting its next command within {} follow-up frames — TurnScheduler may be stuck",
                MAX_ROUND_FOLLOW_UP_FRAMES
            );
        }
        triggered
    }

    /// The most recent `load_level`/`load_game` request a step produced, if
    /// any — `Simulation` reports these as an already-loaded `LevelData`
    /// (see `StepOutcome::pending_level`'s own doc comment); turning that
    /// into a `Transition` is `ember2d::play::PlayState`'s job, which this
    /// harness deliberately doesn't own. Unused until a stairs-transition
    /// test needs it.
    #[allow(dead_code)]
    pub fn take_pending_level(&mut self) -> Option<LevelData> {
        self.pending_level.take()
    }

    pub fn player_id(&self) -> EntityId {
        self.world.find_by_tag("player").expect("player should have spawned")
    }

    pub fn player_pos(&self) -> Vec2 {
        self.world.get_global_position(self.player_id())
    }
}

/// Find a tagged entity at an exact world-space cell — used to locate a
/// specific item/feature tile (e.g. "which gold pile is this one") when a
/// level has more than one entity sharing a tag. `World::find_by_tag` itself
/// deterministically returns the *lowest* `EntityId` sharing the tag, which
/// is a real, useful guarantee but not what a test that wants "the gold pile
/// specifically at (10, 6)" needs.
pub fn find_tagged_entity_at(world: &World, tag: &str, x: f32, y: f32) -> Option<EntityId> {
    world.tags.iter()
        .filter(|(_, t)| t.name == tag)
        .find_map(|(&id, _)| {
            let pos = world.transforms.get(&id)?.position;
            if (pos.x - x).abs() < 0.01 && (pos.y - y).abs() < 0.01 { Some(id) } else { None }
        })
}
