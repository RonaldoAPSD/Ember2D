// Regression test for R18 (docs/ember2d-master-plan.md, 7C-7): script
// errors used to vanish the instant a PlayState was popped off Engine's
// stack, because `pop_state` hands the caller back only a type-erased
// `Box<dyn GameState>` — there was no way to reach `PlayState::take_log`
// (a concrete method) once that happened. `GameState::take_script_log` is
// the new default-no-op trait method `PlayState` overrides to make its
// script log reachable through the trait object instead; this pins that
// override's behavior directly (see ember2d-editor's own
// `receive_script_log` test for the other half of the round trip, and
// ember2d-app's `run_editor_app` for where both halves meet on a real F5
// return — untestable here without a live window).

use ember2d::prelude::*;
use std::collections::BTreeMap;

mod common;

#[test]
fn take_script_log_drains_and_clears_the_log() {
    let mut script_path = common::test_temp_dir();
    script_path.push("ember2d_test_take_script_log.rhai");
    std::fs::write(
        &script_path,
        r#"
        fn on_start(id, ctx) {
            throw "boom";
        }
    "#,
    )
    .expect("write temp script");

    let mut data = LevelData::empty(20, 10);
    data.player.script = Some(script_path.to_string_lossy().to_string());

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();

    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let logs = play.take_script_log();
    assert!(!logs.is_empty(), "on_start's runtime error must be captured in the script log");
    assert!(
        logs.iter().any(|l| l.text.contains("boom")),
        "the log must mention the thrown error, got {:?}",
        logs.iter().map(|l| &l.text).collect::<Vec<_>>()
    );

    let drained_again = play.take_script_log();
    assert!(drained_again.is_empty(), "take_script_log must drain the log, not clone it");

    let _ = std::fs::remove_file(&script_path);
}
