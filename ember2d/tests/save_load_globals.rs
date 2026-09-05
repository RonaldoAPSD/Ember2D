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
        play.globals.get(&key).and_then(|d| d.as_int().ok()),
        Some(4),
        "the script's set_global write must be visible on PlayState.globals after one real frame"
    );

    // The actual regression: round-trip through a REAL RON string, not just
    // an in-memory clone — this is what save_game/load_game do.
    let save = SaveState::new(world.clone(), persistent.clone(), play.globals.clone(), play.clips.clone(), "unused.level".to_string());
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
    );
    assert_eq!(
        loaded_play.globals.get(&key).and_then(|d| d.as_int().ok()),
        Some(4),
        "PlayState::from_save must populate globals from the restored SaveState"
    );

    let _ = std::fs::remove_file(&script_path);
}
