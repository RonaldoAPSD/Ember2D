// scripting/actor_stats_tests.rs — regression tests for 7.5-4 (docs/ember2d-
// master-plan.md §5.6): ctx.get_stat/get_tint_aware/get_tint_asleep, the
// data-driven replacement for the numbers `enemy_rat.rhai`/`enemy_boss.rhai`
// used to hardcode per-role. Split into its own sibling file (via `#[path]`
// in engine.rs's `mod actor_stats_tests;` declaration) — same reasoning
// uniform_typing_tests.rs's own header comment gives for its own split.

use super::*;
use crate::color::Color;
use crate::components::{Actor, Script};

fn test_layers() -> crate::layers::LayerRegistry {
    crate::layers::LayerRegistry::new(&["solid".to_string()])
}

fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compiles `source` onto a fresh driver entity with the given `Actor` and
/// runs one `on_update` pass, handing back the `ScriptUpdateResult` so a
/// test can inspect a `set_global` write from that same pass — mirrors
/// `vars_tests.rs`'s own `run_source_with_result`, plus the driver's
/// `Actor` component this file's tests all need that one doesn't.
fn run_source_with_actor(
    name: &str,
    actor: Actor,
    source: &str,
) -> (ScriptUpdateResult, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_actor_stats_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, crate::components::Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));
    world.add_actor(driver, actor);

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
    (result, log)
}

fn ai_actor_with_stats(stats: &[(&str, f64)]) -> Actor {
    let mut actor = Actor::ai(100);
    for (k, v) in stats {
        actor.stats.insert(k.to_string(), *v);
    }
    actor
}

#[test]
fn get_stat_reads_an_authored_stat_from_the_actor() {
    let (result, log) = run_source_with_actor(
        "reads_stat",
        ai_actor_with_stats(&[("hp", 6.0), ("atk", 2.0)]),
        r#"fn on_update(id, ctx) { ctx.set_global("hp", ctx.get_stat(id, "hp")); }"#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        result.globals.get("hp").and_then(|d| d.as_float().ok()),
        Some(6.0),
        "get_stat must read back the value authored on the tile's ActorRecord"
    );
}

#[test]
fn get_stat_on_a_missing_key_returns_zero() {
    let (result, log) = run_source_with_actor(
        "missing_key",
        ai_actor_with_stats(&[("hp", 6.0)]),
        r#"fn on_update(id, ctx) { ctx.set_global("awareness_range", ctx.get_stat(id, "awareness_range")); }"#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        result.globals.get("awareness_range").and_then(|d| d.as_float().ok()),
        Some(0.0),
        "a key never authored must read back 0.0, the same neutral-default convention every other get_* uses (R32, 7.5-1)"
    );
}

#[test]
fn get_stat_on_a_non_actor_entity_returns_zero() {
    let mut script = test_temp_dir();
    script.push("ember2d_test_actor_stats_no_actor.rhai");
    std::fs::write(
        &script,
        r#"fn on_update(id, ctx) { ctx.set_global("hp", ctx.get_stat(id, "hp")); }"#,
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
    // Deliberately no `world.add_actor` — this entity has no Actor at all.

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
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        result.globals.get("hp").and_then(|d| d.as_float().ok()),
        Some(0.0),
        "get_stat on an entity with no Actor component at all must not panic or crash the editor — it reads back 0.0, same as a missing key"
    );
}

#[test]
fn get_tint_aware_and_asleep_read_the_authored_colors() {
    let mut actor = ai_actor_with_stats(&[]);
    actor.tint_aware = Color::Red;
    actor.tint_asleep = Color::DarkRed;
    let (result, log) = run_source_with_actor(
        "reads_tint",
        actor,
        r#"
        fn on_update(id, ctx) {
            ctx.set_global("aware", ctx.get_tint_aware(id));
            ctx.set_global("asleep", ctx.get_tint_asleep(id));
        }
    "#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        result.globals.get("aware").and_then(|d| d.clone().into_string().ok()),
        Some("Red".to_string())
    );
    assert_eq!(
        result.globals.get("asleep").and_then(|d| d.clone().into_string().ok()),
        Some("DarkRed".to_string())
    );
}

#[test]
fn get_tint_on_a_non_actor_entity_returns_reset() {
    let mut script = test_temp_dir();
    script.push("ember2d_test_actor_stats_tint_no_actor.rhai");
    std::fs::write(
        &script,
        r#"fn on_update(id, ctx) { ctx.set_global("aware", ctx.get_tint_aware(id)); }"#,
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
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(
        result.globals.get("aware").and_then(|d| d.clone().into_string().ok()),
        Some("Reset".to_string()),
        "get_tint_aware on a non-actor entity must not panic — it reads back Reset, no override"
    );
}
