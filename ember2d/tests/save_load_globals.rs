// tests/save_load_globals.rs — regression test for defect D17
// (docs/ember2d-refactor-plan.md §3): script-set globals and clip
// definitions used to be silently dropped by save_game/load_game, since
// SaveState only ever carried `world` + `persistent`. The roguelike's whole
// combat model (hp_<id>, aware_<id>, …) lives in globals, so a
// mid-run save/load would silently reset every enemy to whatever `on_start`
// happens to (re-)initialize — or worse, since `on_start` was never re-run
// on load either, leave everything at Rhai's "no such global" default.
//
// Fixed in Phase 5 Step 5c (docs/ember2d-phase5-plan.md): SaveState now
// also carries `globals`/`clips`, and `PlayState::from_save` restores them
// directly. `on_start` deliberately still does NOT re-run on load — see
// `PlayState::from_save`'s own doc comment for why (some scripts' on_start
// writes are unconditional, e.g. enemy_rat.rhai's own hp lazy-init, and
// re-running it would reset every enemy back to full health).

use std::collections::{BTreeMap, HashMap};
use ember2d::prelude::*;
use ember2d_sim::simulation::Simulation;

mod common;
use common::TurnHarness;

// `CARGO_MANIFEST_DIR`-relative, not CWD-relative — see tests/replay.rs's
// own comment on this (Step 5i's workspace split moved this crate below
// `roguelike/`, and `cargo test` runs each integration test binary with
// CWD set to the package's own directory).
const FLOOR1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../roguelike/floor1.level");
const FLOOR2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../roguelike/floor2.level");

#[test]
fn a_scripts_set_global_survives_a_real_ron_round_trip_through_save_and_load() {
    let mut script_path = std::env::temp_dir();
    script_path.push("ember2d_test_save_load_globals.rhai");
    std::fs::write(&script_path, r#"
        fn on_update(id, ctx) {
            ctx.set_global("hp_" + id, 4);
        }
    "#).expect("write temp script");

    let mut data = LevelData::empty(10, 10);
    let mut tile = TileRecord::new(2, 2, 1, 'r', Color::Red, Color::Reset, false, false, "enemy");
    tile.script = Some(script_path.to_string_lossy().to_string());
    data.tiles.push(tile);

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 10, 10, &mut persistent);

    let rat_id = world.find_by_tag("enemy").expect("enemy tile should have spawned");

    // Run one real frame so on_update's set_global lands — deferred writes
    // apply at the end of the frame, same as everywhere else in this engine.
    let mut input = InputManager::new();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let prev_positions: HashMap<EntityId, Vec2> = HashMap::new();
    let mut quit = false;
    let mut turn_triggered = false;
    play.update(UpdateContext {
        world: &mut world,
        input: &mut input,
        mouse: &mouse,
        gamepad: &gamepad,
        events: &mut events,
        prev_positions: &prev_positions,
        delta_time: 1.0 / 60.0,
        frame_delta_time: 1.0 / 60.0,
        elapsed: 0.0,
        quit: &mut quit,
        turn_triggered: &mut turn_triggered,
        viewport_width: 10,
        viewport_height: 10,
        persistent: &mut persistent,
    });

    let key = format!("hp_{}", rat_id);
    assert_eq!(
        play.globals().get(&key).and_then(|d| d.as_int().ok()),
        Some(4),
        "the script's set_global write must be visible on PlayState.globals() after one real frame"
    );

    // The actual regression: round-trip through a REAL RON string, not just
    // an in-memory clone — this is what save_game/load_game do.
    let save = SaveState::new(world.clone(), persistent.clone(), play.globals().clone(), play.clips().clone(), "unused.level".to_string(), 0, Vec::new());
    let ron = save.to_ron().expect("SaveState must serialize");
    let restored = SaveState::from_ron(&ron).expect("SaveState must deserialize");

    assert_eq!(
        restored.globals.get(&key).and_then(|d| d.as_int().ok()),
        Some(4),
        "defect D17: a script-set global must survive a real save-to-RON/load-from-RON round trip, not just an in-memory clone"
    );

    // And the load-time reconstruction: from_save must actually populate
    // PlayState.globals from the restored save, not leave it empty like it
    // did before this fix.
    let loaded_play = PlayState::from_save(
        LevelData::empty(10, 10),
        restored.persistent.clone(),
        restored.globals.clone(),
        restored.clips.clone(),
        restored.turn_number,
        restored.scheduler.clone(),
    );
    assert_eq!(
        loaded_play.globals().get(&key).and_then(|d| d.as_int().ok()),
        Some(4),
        "PlayState::from_save must populate globals from the restored SaveState"
    );

    let _ = std::fs::remove_file(&script_path);
}

// ── Tests: R7 (7A-3, docs/ember2d-master-plan.md) — save/load is a
// faithful sim round trip, not just a globals/clips one ──────────────────

#[test]
fn a_saved_and_loaded_session_still_transitions_when_the_player_steps_onto_the_stairs() {
    // Before this fix, `exit_targets` was never rebuilt on the loading-save
    // branch of `Simulation::on_start` at all — stairs were permanently
    // dead after any load, in every level, forever.
    let mut h = TurnHarness::load(FLOOR1);
    let player = h.player_id();

    // floor1's real stairs tile (roguelike/floor1.level) — walking there
    // for real isn't this test's point, so jump the player straight onto
    // it rather than scripting a route.
    h.world.transforms.get_mut(&player).unwrap().position = Vec2::new(36.0, 16.0);

    // A REAL RON round trip — SaveState::to_ron/from_ron, matching
    // save_game/load_game exactly, not an in-memory clone.
    let save = SaveState::new(
        h.world.clone(), h.persistent.clone(), h.sim.globals().clone(), h.sim.clips().clone(),
        FLOOR1.to_string(), h.sim.turn_number().max(0) as u64, h.sim.scheduler_snapshot(),
    );
    let ron = save.to_ron().expect("SaveState must serialize");
    let restored = SaveState::from_ron(&ron).expect("SaveState must deserialize");

    // Load into a fresh Simulation exactly like PlayState::from_save /
    // app.rs's real load-game flow does: is_loading_save = true, no
    // do_on_start re-spawn — restored.world's entities are the only ones
    // that will ever exist in this session.
    let level = LevelData::load(FLOOR1).expect("floor1 must load");
    let mut loaded_world = restored.world;
    let mut loaded_persistent = restored.persistent;
    let mut loaded_sim = Simulation::from_save(level, restored.globals, restored.clips, restored.turn_number, restored.scheduler);
    loaded_sim.on_start(&mut loaded_world, h.viewport_width, h.viewport_height, &mut loaded_persistent);

    let mut events = EventBus::new();
    loaded_world.detect_collisions(&mut events);
    let prev_positions = loaded_world.snapshot_positions();
    let outcome = loaded_sim.late_step(
        &mut loaded_world, &events, &prev_positions, Vec2::ZERO,
        1.0 / 60.0, 0.0, h.viewport_width, h.viewport_height, &mut loaded_persistent,
    );

    assert!(outcome.pending_level.is_some(), "stepping onto the stairs after a save/load must still trigger a level transition — exit_targets must survive the load");
}

#[test]
fn loading_a_mid_round_save_resumes_with_the_same_current_actor() {
    // floor2 has three AI actors (docs/ember2d-master-plan.md §2.3's own
    // bench_sim numbers) — floor1 has none, so it can't produce a
    // divergent mid-round state at all.
    let mut h = TurnHarness::load(FLOOR2);

    // Resolve the player's turn, then exactly one AI actor's — deliberately
    // NOT a full round (`h.turn()` would drain every follow-up frame back
    // to the player, landing on a round BOUNDARY every time, which a plain
    // rebuild-from-scratch would reproduce by accident). Stopping partway
    // through the round is what actually needs the fix: some actors have
    // already used this round's turn, others haven't.
    h.frame(Some("w"));
    h.frame(None);
    let expected_actor = h.sim.current_actor();

    let save = SaveState::new(
        h.world.clone(), h.persistent.clone(), h.sim.globals().clone(), h.sim.clips().clone(),
        FLOOR2.to_string(), h.sim.turn_number().max(0) as u64, h.sim.scheduler_snapshot(),
    );
    let ron = save.to_ron().expect("SaveState must serialize");
    let restored = SaveState::from_ron(&ron).expect("SaveState must deserialize");

    let level = LevelData::load(FLOOR2).expect("floor2 must load");
    let mut loaded_world = restored.world;
    let mut loaded_persistent = restored.persistent;
    let mut loaded_sim = Simulation::from_save(level, restored.globals, restored.clips, restored.turn_number, restored.scheduler);
    loaded_sim.on_start(&mut loaded_world, h.viewport_width, h.viewport_height, &mut loaded_persistent);

    assert_eq!(
        loaded_sim.current_actor(), expected_actor,
        "a mid-round save must resume with the same actor about to act, not reset everyone to the same due time via a fresh scheduler rebuild"
    );
}
