// tests/common/mod.rs — shared headless test harness for driving a
// TurnBased-mode PlayState exactly the way engine.rs's `run()` does.
//
// Not auto-discovered as its own test target by Cargo — only files
// directly under `tests/`, not in subdirectories, are — so this is
// included via `mod common;` in whichever test file needs it.
//
// This exists because TurnBased mode's real step sequence
// (consume_step -> update -> [integrate_physics -> detect_collisions ->
// late_update], gated on turn_triggered -> decay) lives inline in
// engine.rs's `run()` loop, not in any function a test could call
// directly. Getting any piece of it wrong or out of order silently tests
// different behavior than the real engine — see the comments in `frame`
// for the specific consequence of getting each piece wrong.

use std::collections::HashMap;
use ember2d::prelude::*;

pub struct TurnHarness {
    pub world: World,
    pub play: PlayState,
    pub events: EventBus,
    pub input: InputManager,
    pub mouse: MouseState,
    pub gamepad: GamepadState,
    pub persistent: HashMap<String, rhai::Dynamic>,
    pub elapsed: f32,
    pub viewport_width: usize,
    pub viewport_height: usize,
}

impl TurnHarness {
    /// Load a real `.level` file and run its `on_start` scripts, the same
    /// way `app.rs::run_play_app` does before the engine's first frame.
    pub fn load(path: &str) -> Self {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {}: {}", path, e));
        let mut world = World::new();
        let mut events = EventBus::new();
        let mut persistent = HashMap::new();
        let mut play = PlayState::from_level(data, HashMap::new());
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
        self.events.clear();
        let prev_positions = self.world.snapshot_positions();

        if let Some(k) = press { self.input.handle_pressed(k); }
        // consume_step() must run BEFORE update(): it's what moves a
        // buffered press into this step's just_pressed set (see
        // input::INPUT_BUFFER_WINDOW). Skip this, or call it after
        // update(), and just_pressed is never true — no script would ever
        // observe the keypress, and every test below would silently pass
        // for the wrong reason (nothing moves because nothing runs).
        self.input.consume_step();
        self.mouse.consume_step();
        self.gamepad.consume_step();

        let mut turn_triggered = false;
        let mut quit = false;
        self.play.update(UpdateContext {
            world: &mut self.world,
            input: &mut self.input,
            mouse: &self.mouse,
            gamepad: &self.gamepad,
            events: &mut self.events,
            prev_positions: &prev_positions,
            delta_time: 1.0 / 60.0,
            elapsed: self.elapsed,
            quit: &mut quit,
            turn_triggered: &mut turn_triggered,
            viewport_width: self.viewport_width,
            viewport_height: self.viewport_height,
            persistent: &mut self.persistent,
        });

        if turn_triggered {
            // The real TurnBased dt is 1.0, never SIM_DT — see engine.rs's
            // else-branch of run(). This demo's whole design (grid
            // movement via set_position, never velocity) exists
            // specifically so this dt never multiplies a nonzero velocity
            // into a teleport (defect D7).
            self.world.integrate_physics(1.0);
            self.world.detect_collisions(&mut self.events);

            self.play.late_update(UpdateContext {
                world: &mut self.world,
                input: &mut self.input,
                mouse: &self.mouse,
                gamepad: &self.gamepad,
                events: &mut self.events,
                prev_positions: &prev_positions,
                delta_time: 1.0 / 60.0,
                elapsed: self.elapsed,
                quit: &mut quit,
                turn_triggered: &mut turn_triggered,
                viewport_width: self.viewport_width,
                viewport_height: self.viewport_height,
                persistent: &mut self.persistent,
            });
        }

        if let Some(k) = press { self.input.handle_released(k); }
        // Decay runs every frame regardless of whether a key was pressed
        // this call, matching engine.rs exactly — a press buffered from an
        // earlier frame must still expire on schedule even on a frame this
        // harness calls with press: None.
        self.input.decay(1.0 / 60.0);
        self.mouse.decay(1.0 / 60.0);
        self.gamepad.decay(1.0 / 60.0);
        self.elapsed += 1.0 / 60.0;

        turn_triggered
    }

    /// A player action (one frame with `press`) plus the follow-up
    /// input-less frame in which enemies get to act. Enemy scripts compare
    /// their own "acted_<id>" global against the player's "turn" global,
    /// which only changes once this frame's apply_ctx lands the player's
    /// write — so an enemy never acts in the SAME frame the player moved,
    /// only the next one. See the Phase 4 plan / docs/HANDOFF.md.
    ///
    /// Unused until Step 4h adds enemies — floor 1 has none, so every
    /// current test only ever needs `frame`.
    #[allow(dead_code)]
    pub fn turn(&mut self, key: Key) -> bool {
        let triggered = self.frame(Some(key));
        self.frame(None);
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
/// level has more than one entity sharing a tag, since `World::find_by_tag`
/// itself returns an arbitrary match in that case (its `tag_to_id` map is
/// filled from `HashMap` iteration order — see the Phase 4 plan's
/// determinism notes).
pub fn find_tagged_entity_at(world: &World, tag: &str, x: f32, y: f32) -> Option<EntityId> {
    world.tags.iter()
        .filter(|(_, t)| t.name == tag)
        .find_map(|(&id, _)| {
            let pos = world.transforms.get(&id)?.position;
            if (pos.x - x).abs() < 0.01 && (pos.y - y).abs() < 0.01 { Some(id) } else { None }
        })
}
