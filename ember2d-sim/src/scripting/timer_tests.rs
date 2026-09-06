// scripting/timer_tests.rs — ScriptEngine timer unit tests.
//
// Split into its own sibling file (via `#[path]` in engine.rs's `mod
// timer_tests;` declaration, right next to the existing `mod tests;` ->
// engine_tests.rs) rather than appended to engine_tests.rs — that file was
// already at 496/600 lines (CLAUDE.md's hard limit) before Phase 6 Step 9
// (docs/ember2d-phase6-plan.md) added timer coverage, and appending would
// have pushed it to 613. Same second-file-via-`#[path]` pattern this crate
// already uses for `apply.rs`/`api_spatial.rs`, applied to a test module
// instead of an `impl` block.
//
// Nothing shipped (`roguelike/`, `shooter/`) calls `start_timer`/`timer_done`/
// `cancel_timer` at all, so this is the only coverage the mechanism has. Each
// test seeds `engine.timers` directly (same "poke the field, don't go through
// the API" trick `engine_tests.rs`'s own
// `hot_reload_clears_only_the_reloaded_scripts_entities` already uses for
// `engine.scopes`) so it isolates the storage/decay/lifecycle mechanism
// itself from `on_start`/`start_timer` plumbing that isn't what this step
// changed.
//
// Deliberately NOT tested here: `cancel_timer` "preventing" `timer_done` from
// firing. It doesn't, today — see D22 (docs/ember2d-refactor-plan.md §3),
// logged (not fixed) as part of this step: `cancel_timer` and a just-consumed
// timer both resolve to the same -1.0 sentinel, which is itself still within
// `timer_done`'s own `val <= 0.0 && val > -500.0` "done" range. A test
// asserting cancellation prevents firing would simply be wrong about current
// behavior, not a useful regression guard.

use super::*;
use crate::components::Script;

/// Mirrors `engine_tests.rs`'s own `test_layers()` — duplicated rather than
/// shared across the two sibling test files, since sharing it would need its
/// own plumbing (a `pub(super)` helper module) for one line of code.
fn test_layers() -> crate::layers::LayerRegistry { crate::layers::LayerRegistry::new(&["solid".to_string()]) }

#[test]
fn a_timer_reports_done_only_once_decay_carries_it_to_zero_or_below() {
    let mut script = std::env::temp_dir();
    script.push("ember2d_test_timer_decay.rhai");
    std::fs::write(&script, r#"fn on_update(id, ctx) { ctx.set_global("done", ctx.timer_done("t")); }"#).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let entity = world.spawn();
    world.add_script(entity, Script::new(&path));

    // 1.5 sim steps' worth (delta = 1/60 in every call below) — deliberately
    // not exactly on a step boundary, so the first call leaves it positive
    // and the second reliably carries it negative, with no floating-point
    // near-zero ambiguity.
    engine.timers.entry(entity).or_default().insert("t".to_string(), 1.5 / 60.0);

    let mut persistent = BTreeMap::new();
    let snapshot1 = Rc::new(WorldSnapshot::build(&world, &engine.layers));
    let r1 = engine.run_scripts(
        &mut world, snapshot1, &mut log, 1.0 / 60.0, 0.0, crate::command::InputSnapshot::default(), crate::command::MouseSnapshot::default(), crate::command::GamepadSnapshot::default(),
        &[], BTreeMap::new(), BTreeMap::new(), &mut persistent, crate::math::Vec2::ZERO, BTreeMap::new(), 0, (80, 24),
    );
    assert_eq!(r1.globals.get("done").and_then(|d| d.as_bool().ok()), Some(false), "must not report done before decay carries it past zero");

    let snapshot2 = Rc::new(WorldSnapshot::build(&world, &engine.layers));
    let r2 = engine.run_scripts(
        &mut world, snapshot2, &mut log, 1.0 / 60.0, 0.0, crate::command::InputSnapshot::default(), crate::command::MouseSnapshot::default(), crate::command::GamepadSnapshot::default(),
        &[], BTreeMap::new(), BTreeMap::new(), &mut persistent, crate::math::Vec2::ZERO, BTreeMap::new(), 0, (80, 24),
    );
    assert_eq!(r2.globals.get("done").and_then(|d| d.as_bool().ok()), Some(true), "must report done once decay carries it to zero or below");

    let _ = std::fs::remove_file(&script);
}

#[test]
fn despawn_removes_the_entitys_timers() {
    let mut script = std::env::temp_dir();
    script.push("ember2d_test_despawn_timer_cleanup.rhai");
    std::fs::write(&script, "fn on_update(id, ctx) { ctx.despawn(id); }\n").unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let entity = world.spawn();
    world.add_script(entity, Script::new(&path));
    engine.timers.entry(entity).or_default().insert("t".to_string(), 5.0);

    let mut persistent = BTreeMap::new();
    let snapshot = Rc::new(WorldSnapshot::build(&world, &engine.layers));
    engine.run_scripts(
        &mut world, snapshot, &mut log, 1.0 / 60.0, 0.0, crate::command::InputSnapshot::default(), crate::command::MouseSnapshot::default(), crate::command::GamepadSnapshot::default(),
        &[], BTreeMap::new(), BTreeMap::new(), &mut persistent, crate::math::Vec2::ZERO, BTreeMap::new(), 0, (80, 24),
    );

    assert!(!engine.timers.contains_key(&entity), "a despawned entity's timers must not leak forever (mirrors the existing scopes cleanup)");
    let _ = std::fs::remove_file(&script);
}

#[test]
fn hot_reload_clears_only_the_reloaded_scripts_entities_timers() {
    let mut script_a = std::env::temp_dir();
    script_a.push("ember2d_test_hot_reload_timer_a.rhai");
    std::fs::write(&script_a, "fn on_update(id, ctx) {}\n").unwrap();

    let mut script_b = std::env::temp_dir();
    script_b.push("ember2d_test_hot_reload_timer_b.rhai");
    std::fs::write(&script_b, "fn on_update(id, ctx) {}\n").unwrap();

    let path_a = script_a.to_string_lossy().to_string();
    let path_b = script_b.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path_a, &mut log));
    assert!(engine.compile(&path_b, &mut log));

    let mut world = World::new();
    let entity_a = world.spawn();
    let entity_b = world.spawn();
    world.add_script(entity_a, Script::new(&path_a));
    world.add_script(entity_b, Script::new(&path_b));

    engine.timers.entry(entity_a).or_default().insert("t".to_string(), 5.0);
    engine.timers.entry(entity_b).or_default().insert("t".to_string(), 5.0);

    engine.mod_times.insert(path_a.clone(), std::time::SystemTime::UNIX_EPOCH);
    engine.check_hot_reload(&world, &mut log);

    assert!(!engine.timers.contains_key(&entity_a), "the reloaded script's entity must lose its stale timers, same as its scope");
    assert!(engine.timers.contains_key(&entity_b), "an unrelated entity's timers must survive another script's hot-reload");

    let _ = std::fs::remove_file(&script_a);
    let _ = std::fs::remove_file(&script_b);
}
