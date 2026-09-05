// tests/common/mod.rs — shared headless test harness for driving a
// TurnBased-mode PlayState.
//
// Not auto-discovered as its own test target by Cargo — only files
// directly under `tests/`, not in subdirectories, are — so this is
// included via `mod common;` in whichever test file needs it.
//
// **Corrected in Step 5d** (docs/ember2d-phase5-plan.md): this used to say
// TurnBased mode's real step sequence (consume_step -> update ->
// [integrate_physics -> detect_collisions -> late_update], gated on
// turn_triggered -> decay) "lives inline in engine.rs's `run()` loop, not
// in any function a test could call directly" — true when this file was
// written, and exactly the hazard it names: getting any piece of that
// sequence wrong or out of order here would silently test different
// behavior than the real engine. Fixed by extracting that sequence into
// `ember2d::sim::step`, which both `engine.rs::run()` and `frame` below now
// call — there's only one copy of the sequence left to get wrong.

use std::collections::BTreeMap;
use ember2d::prelude::*;
use ember2d::sim;

pub struct TurnHarness {
    pub world: World,
    pub play: PlayState,
    pub events: EventBus,
    pub input: InputManager,
    pub mouse: MouseState,
    pub gamepad: GamepadState,
    pub persistent: BTreeMap<String, rhai::Dynamic>,
    pub elapsed: f32,
    pub viewport_width: usize,
    pub viewport_height: usize,
}

/// Fixed per-frame time this harness advances by — there's no real
/// wall-clock here, so this stands in for both `sim::step`'s `sim_dt` (what
/// the real engine calls `SIM_DT`) and its `frame_dt` (what a real frame's
/// measured wall-clock delta would be). Using the same constant for both
/// exactly matches what a headless test with no real timing should assume:
/// a perfectly steady simulated 60 fps.
const HARNESS_DT: f32 = 1.0 / 60.0;

/// Safety cap on `TurnHarness::turn`'s follow-up-frame drain — well above
/// anything the roguelike's own levels ever schedule (floor3, the busiest,
/// has 2 rats + 1 boss = 3 AI actors per round), so this only ever fires on
/// a genuine `TurnScheduler` bug (an actor whose turn never actually
/// consumes), turning a hang into a clear test failure instead.
const MAX_ROUND_FOLLOW_UP_FRAMES: u32 = 50;

/// `cargo test` runs each integration test binary with its CWD set to the
/// *package's own* directory (`ember2d/`), unlike `cargo run` (CWD = wherever
/// invoked from — see docs/HANDOFF.md-style Cargo quirks worth naming).
/// Since Step 5i's workspace split (docs/ember2d-phase5-plan.md) moved this
/// package one level below the repo root, that quirk breaks more than just
/// the top-level level-file path: a level's own `tile.script`/`next_level`
/// fields are authored as repo-root-relative strings too (e.g.
/// `"roguelike/scripts/enemy_rat.rhai"`), and `resolve_exit_path`
/// (play.rs) resolves them by checking `Path::new(next).exists()` against
/// CWD first. Fixing only the level-file path a test passes to `load()`
/// isn't enough — `set_current_dir` once, here, makes every one of those
/// paths resolve exactly as they did before the split, with no special-
/// casing needed anywhere else. Idempotent and safe to call from every
/// test (including in parallel — they all set the same target).
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
        let mut events = EventBus::new();
        let mut persistent = BTreeMap::new();
        let mut play = PlayState::from_level(data, BTreeMap::new());
        let (viewport_width, viewport_height) = (80, 24);
        play.on_start(&mut world, &mut events, viewport_width, viewport_height, &mut persistent);

        TurnHarness {
            world,
            play,
            events,
            input: InputManager::new(),
            mouse: MouseState::new(),
            gamepad: GamepadState::new(),
            persistent,
            elapsed: 0.0,
            viewport_width,
            viewport_height,
        }
    }

    /// One engine frame in TurnBased mode. Returns whether a turn was
    /// triggered this frame (mirrors `engine.rs`'s own `turn_triggered`).
    pub fn frame(&mut self, press: Option<Key>) -> bool {
        if let Some(k) = press { self.input.handle_pressed(k); }

        // `sim::step` is what claims a buffered press into this step's
        // just_pressed set (see input::INPUT_BUFFER_WINDOW) and runs the
        // real consume_step -> update -> [physics -> collisions ->
        // late_update] sequence — see this file's header comment.
        // `gate_late_phase_on_turn: true` matches engine.rs's own
        // TurnBased branch: the late phase only runs if this step actually
        // resolved an actor's turn (`PlayState::run_actor_turn` sets
        // `turn_triggered`, driven by `TurnScheduler` — Step 5f,
        // docs/ember2d-phase5-plan.md; there's no more `ctx.trigger_turn()`
        // to call). `physics_dt: 1.0` is passed but never actually used in
        // TurnBased mode as of Step 5f's D7 fix — `sim::step` only
        // integrates physics in realtime mode (`!gate_late_phase_on_turn`)
        // now, matching this demo's whole design (grid movement via
        // set_position, never velocity).
        let result = sim::step(
            &mut self.play,
            &mut self.world,
            &mut self.input,
            &mut self.mouse,
            &mut self.gamepad,
            &mut self.events,
            &mut self.persistent,
            HARNESS_DT,
            HARNESS_DT,
            1.0,
            self.elapsed,
            self.viewport_width,
            self.viewport_height,
            true,
        );

        if let Some(k) = press { self.input.handle_released(k); }
        // Decay runs every frame regardless of whether a key was pressed
        // this call, matching engine.rs exactly — a press buffered from an
        // earlier frame must still expire on schedule even on a frame this
        // harness calls with press: None.
        self.input.decay(HARNESS_DT);
        self.mouse.decay(HARNESS_DT);
        self.gamepad.decay(HARNESS_DT);
        self.elapsed += HARNESS_DT;

        result.turn_triggered
    }

    /// A player action, plus every follow-up input-less frame it causes —
    /// one full round. **Rewritten in Step 5f** (docs/ember2d-phase5-plan.md):
    /// used to be exactly `frame(Some(key))` plus one fixed follow-up
    /// `frame(None)`, back when every enemy's `on_update` ran in that same
    /// single follow-up frame (gated by comparing its own "acted_<id>"
    /// against the player's "turn" global). `TurnScheduler` now resolves
    /// "one actor per step" (scheduler.rs's own doc comment) — the player's
    /// press consumes one frame, then each AI actor TurnScheduler hands the
    /// turn to consumes one more frame of its own, however many there are.
    /// This drains every one of those follow-up frames (each of which
    /// returns `triggered: true`, since an AI actor's turn always counts —
    /// see `PlayState::run_actor_turn`) until control returns to the player
    /// awaiting its next command (a `frame(None)` that returns `false`),
    /// so callers still see "one player action -> the round it causes" as
    /// a single logical turn, same shape as before.
    #[allow(dead_code)]
    pub fn turn(&mut self, key: Key) -> bool {
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

    /// Unused until Step 4h/4i's stairs-transition tests exist.
    #[allow(dead_code)]
    pub fn take_transition(&mut self) -> Option<Transition> {
        self.play.take_transition()
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
/// deterministically returns the *lowest* `EntityId` sharing the tag (Step
/// 5b, docs/ember2d-phase5-plan.md — `world.tags` is a `BTreeMap`), which is
/// a real, useful guarantee but not what a test that wants "the gold pile
/// specifically at (10, 6)" needs.
pub fn find_tagged_entity_at(world: &World, tag: &str, x: f32, y: f32) -> Option<EntityId> {
    world.tags.iter()
        .filter(|(_, t)| t.name == tag)
        .find_map(|(&id, _)| {
            let pos = world.transforms.get(&id)?.position;
            if (pos.x - x).abs() < 0.01 && (pos.y - y).abs() < 0.01 { Some(id) } else { None }
        })
}
