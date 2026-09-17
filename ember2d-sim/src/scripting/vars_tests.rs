// scripting/vars_tests.rs — regression tests for 7.5-3 (docs/ember2d-
// master-plan.md §5.6): set_var/get_var/has_var/remove_var, the per-entity
// `Vars` component that replaced the `"hp_" + id`/`"aware_" + id`/
// `"ehp_" + id` global-key-concatenation convention in the demo scripts.
// Split into its own sibling file (via `#[path]` in engine.rs's `mod
// vars_tests;` declaration) — same reasoning uniform_typing_tests.rs's own
// header comment gives for its own split.

use super::*;
use crate::components::Script;
use rhai::Dynamic;

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
    let snapshot = Rc::new(WorldSnapshot::build(world, &engine.layers, &[]));
    engine.run_scripts(
        world,
        snapshot,
        log,
        &mut persistent,
        PassArgs {
            delta_time: 1.0 / 60.0,
            elapsed: 0.0,
            input: crate::command::InputSnapshot::default(),
            mouse: crate::command::MouseSnapshot::default(),
            gamepad: crate::command::GamepadSnapshot::default(),
            spawns: &[],
            globals: BTreeMap::new(),
            clips: BTreeMap::new(),
            camera_pos: crate::math::Vec2::ZERO,
            commands: BTreeMap::new(),
            turn_number: 0,
            viewport_size: (80, 24),
        },
        &[],
    );
}

/// Compiles `source` onto a fresh scripted driver entity in a fresh world
/// and runs one `on_update` pass. Returns the world (so a test can inspect
/// `World::vars` directly, the way `run_source` in the sibling test files
/// returns it to inspect `World`'s other components) and the log.
fn run_source(name: &str, source: &str) -> (World, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_vars_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, crate::components::Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&script);
    (world, log)
}

/// Like `run_source`, but also hands back the `ScriptUpdateResult` (needed
/// to check a pass's own `globals` write, not a later pass's `World`
/// state) — mirrors `uniform_typing_tests.rs`'s own `run_source_with_result`.
fn run_source_with_result(name: &str, source: &str) -> (World, ScriptUpdateResult, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_vars_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, crate::components::Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    let mut persistent = BTreeMap::new();
    let snapshot = Rc::new(WorldSnapshot::build(&world, &engine.layers, &[]));
    let result = engine.run_scripts(
        &mut world,
        snapshot,
        &mut log,
        &mut persistent,
        PassArgs {
            delta_time: 1.0 / 60.0,
            elapsed: 0.0,
            input: crate::command::InputSnapshot::default(),
            mouse: crate::command::MouseSnapshot::default(),
            gamepad: crate::command::GamepadSnapshot::default(),
            spawns: &[],
            globals: BTreeMap::new(),
            clips: BTreeMap::new(),
            camera_pos: crate::math::Vec2::ZERO,
            commands: BTreeMap::new(),
            turn_number: 0,
            viewport_size: (80, 24),
        },
        &[],
    );
    let _ = std::fs::remove_file(&script);
    (world, result, log)
}

/// The core round trip: a `set_var` this pass is invisible to a `get_var`
/// later in the SAME pass (same convention `get_global` already has — see
/// `api_ext.rs`'s own doc comment on `set_var`), but is visible to a LATER
/// pass, once `apply_ctx` has folded it into `World::vars`.
#[test]
fn set_var_is_invisible_this_pass_but_visible_the_next() {
    let mut script = test_temp_dir();
    script.push("ember2d_test_vars_round_trip.rhai");
    std::fs::write(
        &script,
        r#"
        fn on_update(id, ctx) {
            let before = ctx.get_var(id, "hp");
            ctx.set_var(id, "hp", 6);
            let after = ctx.get_var(id, "hp");
            ctx.set_global("before", before);
            ctx.set_global("after", after);
        }
    "#,
    )
    .unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, crate::components::Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        world.vars.get(&driver).and_then(|v| v.values.get("hp")).and_then(|d| d.as_int().ok()),
        Some(6),
        "set_var must have landed in World::vars after apply_ctx"
    );

    // Second pass: get_var now sees last pass's write.
    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&script);
    assert!(log.is_empty(), "unexpected script log on pass 2: {:?}", log);
}

#[test]
fn has_var_is_false_until_set_var_lands() {
    let (_world, result, log) = run_source_with_result(
        "has_var",
        r#"
        fn on_update(id, ctx) {
            ctx.set_global("before", ctx.has_var(id, "aware"));
            ctx.set_var(id, "aware", true);
        }
    "#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        result.globals.get("before").and_then(|d| d.as_bool().ok()),
        Some(false),
        "has_var must read false within the SAME pass set_var was called in"
    );
}

/// `remove_var` then a later pass's `get_var` must read back `()`, not the
/// pre-removal value — the exact "ehp_<id>" lifecycle `resolve_hits`
/// (director.rhai) needs: a dead enemy's var must actually go away.
#[test]
fn remove_var_actually_removes_the_key() {
    let mut script = test_temp_dir();
    script.push("ember2d_test_vars_remove.rhai");
    std::fs::write(
        &script,
        r#"
        fn on_update(id, ctx) {
            if !ctx.has_var(id, "seeded") {
                ctx.set_var(id, "seeded", true);
                ctx.set_var(id, "ehp", 5);
            } else {
                ctx.remove_var(id, "ehp");
            }
        }
    "#,
    )
    .unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, crate::components::Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log); // seeds "ehp" = 5
    assert_eq!(world.vars.get(&driver).unwrap().values.get("ehp").unwrap().as_int().unwrap(), 5);

    run_scripts_once(&mut engine, &mut world, &mut log); // removes it
    let _ = std::fs::remove_file(&script);
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert!(
        !world.vars.get(&driver).unwrap().values.contains_key("ehp"),
        "remove_var must actually remove the key, not leave the pre-removal value"
    );
}

/// `World::despawn` clears that entity's `Vars` entirely — see
/// `components/vars.rs`'s own doc comment on why this is the whole point
/// (no per-script `remove_global` bookkeeping needed on death, unlike the
/// `"hp_" + id`/`"ehp_" + id` convention this replaced).
#[test]
fn despawn_clears_the_entitys_vars() {
    let mut world = World::new();
    let id = world.spawn();
    world.add_transform(id, crate::components::Transform::new(0.0, 0.0));
    world.vars.insert(id, crate::components::Vars {
        values: BTreeMap::from([("hp".to_string(), Dynamic::from(6_i64))]),
    });
    assert!(world.vars.contains_key(&id));

    world.despawn(id);
    assert!(!world.vars.contains_key(&id), "despawn must remove the entity's Vars entirely");
}

/// `add_var` (mirrors `add_global`'s own 7.5-2 coverage in
/// `atomic_arithmetic_tests.rs`): two calls to the same `(id, key)` in one
/// pass must both land — director.rhai's `resolve_hits` needs exactly this
/// once its per-enemy HP total moves from a global to a `Vars` entry.
#[test]
fn two_add_var_calls_to_the_same_key_in_one_pass_both_land() {
    let (world, log) = run_source(
        "double_add_var",
        r#"
        fn on_update(id, ctx) {
            ctx.add_var(id, "hp", 10);
            let _n = ctx.add_var(id, "hp", 5);
        }
    "#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    let driver = 1;
    let hp = world.vars.get(&driver).unwrap().values.get("hp").unwrap().as_float().unwrap();
    assert_eq!(hp, 15.0, "both add_var calls this pass must accumulate, not clobber");
}

/// `add_var` after `remove_var` in the same pass starts over from 0, not
/// the pre-removal value — mirrors `add_global_after_remove_global...` in
/// `atomic_arithmetic_tests.rs`, same reasoning.
#[test]
fn add_var_after_remove_var_in_the_same_pass_starts_over_from_zero() {
    let mut script = test_temp_dir();
    script.push("ember2d_test_vars_remove_then_add.rhai");
    std::fs::write(
        &script,
        r#"
        fn on_update(id, ctx) {
            if !ctx.has_var(id, "seeded") {
                ctx.set_var(id, "seeded", true);
                ctx.set_var(id, "hp", 5);
            } else {
                ctx.remove_var(id, "hp");
                let _n = ctx.add_var(id, "hp", -1);
            }
        }
    "#,
    )
    .unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, crate::components::Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log); // seeds "hp" = 5
    run_scripts_once(&mut engine, &mut world, &mut log); // remove_var then add_var(-1)
    let _ = std::fs::remove_file(&script);
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    let hp = world.vars.get(&driver).unwrap().values.get("hp").unwrap().as_float().unwrap();
    assert_eq!(hp, -1.0, "add_var after remove_var must start over from 0, not resurrect 5");
}

/// R10-style ghost-component guard (7A-1, mirrored from `pending_tags`'s
/// own): `set_var` on an entity nothing else spawned this pass must not
/// create a `Vars` component out of nowhere.
#[test]
fn set_var_on_a_nonexistent_entity_is_a_no_op() {
    let (world, log) = run_source(
        "ghost_entity",
        r#"fn on_update(id, ctx) { ctx.set_var(9999, "hp", 6); }"#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert!(
        !world.vars.contains_key(&9999),
        "set_var on a nonexistent entity must not create a ghost Vars component"
    );
}
