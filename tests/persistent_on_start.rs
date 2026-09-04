// Regression test for defect D2 (docs/ember2d-refactor-plan.md §3):
// `ctx.set_persistent` called from a script's `on_start` used to be silently
// discarded — PlayState::do_on_start ran on_start scripts against a throwaway
// local HashMap instead of the engine's real persistent store.

use std::collections::HashMap;
use ember2d::prelude::*;

#[test]
fn set_persistent_in_on_start_survives() {
    let mut script_path = std::env::temp_dir();
    script_path.push("ember2d_test_on_start_persist.rhai");
    std::fs::write(&script_path, r#"
        fn on_start(id, ctx) {
            ctx.set_persistent("marker", 42);
        }
    "#).expect("write temp script");

    let mut data = LevelData::empty(20, 10);
    data.player.script = Some(script_path.to_string_lossy().to_string());

    let mut play = PlayState::from_level(data, HashMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: HashMap<String, rhai::Dynamic> = HashMap::new();

    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let marker = persistent.get("marker").cloned().unwrap_or(rhai::Dynamic::UNIT);
    assert_eq!(
        marker.as_int().unwrap_or(-1),
        42,
        "set_persistent from on_start must survive into the caller's persistent map"
    );

    let _ = std::fs::remove_file(&script_path);
}
