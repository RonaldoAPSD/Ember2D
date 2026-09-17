// scripting/set_script_tests.rs — regression tests for 7.5-5 (docs/ember2d-
// master-plan.md §5.6): `ctx.set_script` (attaches a script to an
// already-spawned entity, deferred like every other setter) and the
// `pending_on_start` mechanism that gives the newly-attached entity its own
// `on_start` call at the next step boundary. Split into its own sibling
// file (via `#[path]` in engine.rs's `mod set_script_tests;` declaration) —
// same reasoning uniform_typing_tests.rs's/vars_tests.rs's/
// actor_stats_tests.rs's own header comments give for their own splits.
//
// `on_load` (the loaded-save counterpart to `on_start`, also new this step)
// is NOT covered here — it needs a real `Simulation::from_save` round trip
// to exercise (`Simulation::on_start`'s `is_loading_save` branch, in
// simulation.rs), which this file's `ScriptEngine`-only harness has no way
// to drive. See `ember2d/tests/save_load_globals.rs`'s own `on_load`-named
// test for that coverage instead.

use super::*;
use crate::components::{Script, Transform};

fn test_layers() -> crate::layers::LayerRegistry {
    crate::layers::LayerRegistry::new(&["solid".to_string()])
}

fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_scripts_once(engine: &mut ScriptEngine, world: &mut World, log: &mut Vec<LogEntry>) {
    let mut persistent = BTreeMap::new();
    let snapshot = Rc::new(WorldSnapshot::build(world, &engine.layers));
    engine.run_scripts(
        world,
        snapshot,
        log,
        1.0 / 60.0,
        0.0,
        crate::command::InputSnapshot::default(),
        crate::command::MouseSnapshot::default(),
        crate::command::GamepadSnapshot::default(),
        &[],
        BTreeMap::new(),
        BTreeMap::new(),
        &mut persistent,
        crate::math::Vec2::ZERO,
        BTreeMap::new(),
        0,
        (80, 24),
        &[],
    );
}

fn write_script(name: &str, source: &str) -> String {
    let mut path = test_temp_dir();
    path.push(format!("ember2d_test_set_script_{}.rhai", name));
    std::fs::write(&path, source).unwrap();
    path.to_string_lossy().to_string()
}

/// The core contract: `set_script` attaches immediately (via `apply_ctx`,
/// same pass), but the attached entity's `on_start` does not run until the
/// FOLLOWING `run_scripts` call — not this one, and not by re-scanning
/// `scripted` mid-pass. `on_update` for the newly-attached entity is
/// naturally in the same boat (it wasn't in `scripted` this pass either,
/// since that list is built before the attach lands) but catches up on the
/// same next call `on_start` does, once `world.scripts` already lists it.
#[test]
fn set_script_attaches_this_pass_but_on_start_waits_for_the_next_one() {
    let path_b = write_script(
        "target",
        r#"
        fn on_start(id, ctx) { ctx.set_global("b_on_start_ran", true); }
        fn on_update(id, ctx) { ctx.set_global("b_on_update_ran", true); }
    "#,
    );
    // Gated on `find_by_tag` (real `World` state, always accurate every
    // pass) rather than a global flag — `run_scripts_once` below rebuilds
    // globals/persistent from scratch on every call (it doesn't thread
    // `ScriptEngine`'s own `self.globals` through the way `Simulation::step`
    // does), so a global-flag guard would never actually latch across the
    // two passes this test drives.
    let path_driver = write_script(
        "driver",
        &format!(
            r#"
        fn on_update(id, ctx) {{
            if ctx.find_by_tag("target") == -1 {{
                let e = ctx.spawn_entity("B", 0.0, 0.0, "target");
                ctx.set_script(e, "{path_b}");
            }}
        }}
    "#,
            path_b = path_b.replace('\\', "\\\\")
        ),
    );

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path_driver, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path_driver));

    // Pass 1: driver spawns B and calls set_script on it.
    run_scripts_once(&mut engine, &mut world, &mut log);
    assert!(log.is_empty(), "unexpected script log after pass 1: {:?}", log);
    let target = world.find_by_tag("target").expect("B should have spawned");
    assert!(world.scripts.contains_key(&target), "set_script must attach within the same pass");
    assert_eq!(
        engine.pending_on_start,
        vec![target],
        "the newly-attached entity must be queued for on_start, not run immediately"
    );

    // Pass 2: B's on_start fires (queued above), then its on_update — both
    // in this one call, since world.scripts already lists B by the time
    // this pass's own `scripted` snapshot is built.
    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&path_b);
    let _ = std::fs::remove_file(&path_driver);
    assert!(log.is_empty(), "unexpected script log after pass 2: {:?}", log);
    assert!(
        engine.pending_on_start.is_empty(),
        "pending_on_start must drain once its entity's on_start has run"
    );
}

/// R10-style ghost-component guard (mirrored from `pending_tags`'s own,
/// 7A-1): `set_script` on an entity nothing else spawned this pass must not
/// create a `Script` component out of nowhere.
#[test]
fn set_script_on_a_nonexistent_entity_is_a_no_op() {
    let path = write_script("unused", "fn on_update(id, ctx) {}\n");
    let driver_path = write_script(
        "ghost_driver",
        &format!(r#"fn on_update(id, ctx) {{ ctx.set_script(9999, "{}"); }}"#, path.replace('\\', "\\\\")),
    );

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&driver_path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&driver_path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&driver_path);
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert!(
        !world.scripts.contains_key(&9999),
        "set_script on a nonexistent entity must not create a ghost Script component"
    );
    assert!(engine.pending_on_start.is_empty(), "a no-op attach must not queue an on_start call");
}

/// A path that fails to compile is never attached — matches the sim
/// boundary's "a script can never crash the editor" rule: a typo'd path in
/// `set_script` logs a compile error (the same one a bad `script:` field in
/// a level file already produces) rather than leaving `World::scripts`
/// pointing at an AST that doesn't exist.
#[test]
fn set_script_with_a_path_that_fails_to_compile_never_attaches() {
    let missing_path = test_temp_dir().join("ember2d_test_set_script_does_not_exist.rhai");
    let driver_path = write_script(
        "compile_fail_driver",
        &format!(
            r#"
        fn on_update(id, ctx) {{
            let e = ctx.spawn_entity("B", 0.0, 0.0, "target");
            ctx.set_script(e, "{}");
        }}
    "#,
            missing_path.to_string_lossy().replace('\\', "\\\\")
        ),
    );

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&driver_path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&driver_path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&driver_path);

    assert!(!log.is_empty(), "a nonexistent script path must log a compile error");
    let target = world.find_by_tag("target").expect("B should still have spawned");
    assert!(
        !world.scripts.contains_key(&target),
        "a script that fails to compile must never be attached"
    );
    assert!(
        engine.pending_on_start.is_empty(),
        "a failed attach must not queue an on_start call"
    );
}
