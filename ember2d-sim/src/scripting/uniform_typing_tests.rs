// scripting/uniform_typing_tests.rs — regression tests for 7.5-1
// (docs/ember2d-master-plan.md §5.6, R31/R32): every coordinate/size/
// layer-order function accepts both int and float literals, and
// `remove_global`/`clear_persistent`/`load_level` no longer alias other
// operations through a shared sentinel. Split into its own sibling file
// (via `#[path]` in engine.rs's `mod uniform_typing_tests;` declaration,
// next to the existing `mod tests;` -> engine_tests.rs, `mod timer_tests;`
// -> timer_tests.rs, and `mod safety_tests;` -> safety_tests.rs) rather
// than appended to engine_tests.rs, which this step's own coverage pushed
// to 762/750 lines (CLAUDE.md's hard limit) — same reasoning
// timer_tests.rs's and safety_tests.rs's own header comments give for
// their own splits.

use super::*;
use crate::components::Script;

/// Mirrors `engine_tests.rs`'s, `timer_tests.rs`'s, and `safety_tests.rs`'s
/// own `test_layers()`.
fn test_layers() -> crate::layers::LayerRegistry {
    crate::layers::LayerRegistry::new(&["solid".to_string()])
}

/// Mirrors `engine_tests.rs`'s own `test_temp_dir()`.
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
    );
}

/// Compile `source` to a temp file, attach it to a fresh scripted entity in
/// a fresh world, and run one update pass. Returns the world and the log —
/// mirrors `engine_tests.rs`'s own `run_source`.
fn run_source(name: &str, source: &str) -> (World, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_typing_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&script);
    (world, log)
}

/// Like `run_source`, but also hands back the full `ScriptUpdateResult` —
/// needed by tests below that check `globals`/`pending_level` directly
/// rather than a side effect on `World`.
fn run_source_with_result(name: &str, source: &str) -> (World, ScriptUpdateResult, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_typing_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_script(driver, Script::new(&path));

    let mut persistent = BTreeMap::new();
    let snapshot = Rc::new(WorldSnapshot::build(&world, &engine.layers));
    let result = engine.run_scripts(
        &mut world,
        snapshot,
        &mut log,
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
    );
    let _ = std::fs::remove_file(&script);
    (world, result, log)
}

/// R31: `draw_hud`'s `x`/`y` are `i64`-typed — Rhai never coerces int<->float
/// for a registered native function, so a script writing FLOAT literals used
/// to fail to resolve at all ("function not found"), the same class of error
/// `engine_tests.rs`'s own
/// `a_genuine_function_not_found_error_inside_on_update_is_logged_and_disables_the_script`
/// pins, just triggered by the right function name with the wrong argument
/// type instead of a nonexistent name.
#[test]
fn draw_hud_accepts_float_coordinate_literals_not_just_int() {
    let (_world, log) = run_source(
        "draw_hud_float",
        r#"
        fn on_update(id, ctx) { ctx.draw_hud(1.0, 2.0, "hp", "White", "Reset"); }
    "#,
    );
    assert!(
        !log.iter().any(|e| e.level == LogLevel::Error),
        "draw_hud must accept float coordinate literals, not just int: {:?}",
        log
    );
}

/// R31, the reverse direction: `set_position`'s `x`/`y` are `f64`-typed, so
/// INT literals used to fail the same way.
#[test]
fn set_position_accepts_int_coordinate_literals_not_just_float() {
    let (_world, log) = run_source(
        "set_position_int",
        r#"
        fn on_update(id, ctx) { ctx.set_position(id, 5, 7); }
    "#,
    );
    assert!(
        !log.iter().any(|e| e.level == LogLevel::Error),
        "set_position must accept int coordinate literals, not just float: {:?}",
        log
    );
}

/// R31 spot-check beyond `api.rs`: `get_entity_at` (`api_spatial.rs`) is
/// `f64`-typed too, and a script spatial-querying with int literals (the
/// common case — `ctx.get_entity_at(5, 5)`, not `5.0, 5.0`) is exactly the
/// R31 report's own shape.
#[test]
fn get_entity_at_accepts_int_coordinate_literals_not_just_float() {
    let (_world, log) = run_source(
        "get_entity_at_int",
        r#"
        fn on_update(id, ctx) { let _hit = ctx.get_entity_at(5, 5); }
    "#,
    );
    assert!(
        !log.iter().any(|e| e.level == LogLevel::Error),
        "get_entity_at must accept int coordinate literals, not just float: {:?}",
        log
    );
}

/// R32: `set_global`/`remove_global` used to both write `Dynamic::UNIT` into
/// the same pending-write map, so `apply.rs`'s own apply loop (`if
/// v.is_unit() { remove } else { insert }`) treated `set_global("k", ())` —
/// a script legitimately storing unit — exactly like `remove_global("k")`.
/// `PendingWrite::Set(())` vs `::Remove` now keep the two apart.
#[test]
fn set_global_can_store_unit_instead_of_silently_deleting_the_key() {
    let (_world, result, _log) = run_source_with_result(
        "set_global_unit",
        r#"
        fn on_update(id, ctx) { ctx.set_global("k", ()); }
    "#,
    );
    assert!(
        result.globals.contains_key("k"),
        "set_global(\"k\", ()) must store the key, not delete it"
    );
    assert!(result.globals.get("k").unwrap().is_unit());
}

/// R32: `load_level` used to guard with `if pending_level.is_none()` —
/// first-wins, inconsistent with `save_game`/`play_music`, which already
/// overwrite unconditionally (last-wins). A script calling `load_level`
/// twice in the same pass now gets the LAST one, matching those two.
#[test]
fn load_level_is_last_wins_not_first_wins() {
    let (_world, result, _log) = run_source_with_result(
        "load_level_last_wins",
        r#"
        fn on_update(id, ctx) {
            ctx.load_level("first.level");
            ctx.load_level("second.level");
        }
    "#,
    );
    assert_eq!(result.pending_level.as_deref(), Some("second.level"));
}
