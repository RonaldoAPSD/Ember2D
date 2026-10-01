// scripting/atomic_arithmetic_tests.rs — regression tests for 7.5-2
// (docs/ember2d-master-plan.md §5.6): `add_global`/`add_persistent` read the
// CURRENT value (this pass's own already-queued write if there is one,
// falling back to the resolved store, falling back to 0 if the key has
// never been set) and add a delta, so N calls to the same key in one pass
// land as N additions — unlike a hand-rolled `set_global(k, get_global(k) +
// d)`, where a `get_global` never observes a same-pass `set_global` and a
// second call silently clobbers the first. Split into its own sibling file
// (via `#[path]` in engine.rs's `mod atomic_arithmetic_tests;`
// declaration) rather than appended to uniform_typing_tests.rs — see that
// file's own header comment for the general reasoning this mirrors (a
// distinct concern, not an extension of uniform int/float typing).

use super::*;
use crate::components::Script;
use rhai::Dynamic;

/// Mirrors `uniform_typing_tests.rs`'s own `test_layers()`.
fn test_layers() -> crate::layers::LayerRegistry {
    crate::layers::LayerRegistry::new(&["solid".to_string()])
}

/// Mirrors `uniform_typing_tests.rs`'s own `test_temp_dir()`.
fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Mirrors `uniform_typing_tests.rs`'s own `run_source_with_result` — the
/// only shape these tests need (they all inspect `globals`/`persistent`
/// after one pass, never a `World` side effect).
fn run_source_with_result(name: &str, source: &str) -> (ScriptUpdateResult, Vec<LogEntry>) {
    run_source_with_result_and_persistent(name, source, BTreeMap::new())
}

/// Like `run_source_with_result`, but seeds `persistent` with a value
/// already resolved from an EARLIER pass — needed by the
/// `clear_all_persistent` interaction test below, which is specifically
/// about a stale value already in the resolved store, not one just queued
/// this same pass (`pending_persistent` is a flat map keyed by name, so a
/// same-pass `set_persistent` before `clear_all_persistent` would just be
/// the last write to that key regardless of call order — a different,
/// already-covered case, not this one).
fn run_source_with_result_and_persistent(
    name: &str,
    source: &str,
    persistent: BTreeMap<String, Dynamic>,
) -> (ScriptUpdateResult, Vec<LogEntry>) {
    run_source_with_result_full(name, source, BTreeMap::new(), persistent)
}

/// Like `run_source_with_result`, but seeds `globals` with a value already
/// resolved from an EARLIER pass — same reasoning as
/// `run_source_with_result_and_persistent`'s own doc comment, against
/// `globals` instead of `persistent`.
fn run_source_with_result_and_globals(
    name: &str,
    source: &str,
    globals: BTreeMap<String, Dynamic>,
) -> (ScriptUpdateResult, Vec<LogEntry>) {
    run_source_with_result_full(name, source, globals, BTreeMap::new())
}

fn run_source_with_result_full(
    name: &str,
    source: &str,
    globals: BTreeMap<String, Dynamic>,
    mut persistent: BTreeMap<String, Dynamic>,
) -> (ScriptUpdateResult, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_atomic_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_script(driver, Script::new(&path));

    let snapshot = Rc::new(WorldSnapshot::build(&world, &engine.layers, &Default::default()));
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
            spawns: &Default::default(),
            globals,
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

/// The core 7.5-2 guarantee, and director.rhai's old "duplicate-tally"
/// hazard made concrete: two `add_global` calls to the same key in the SAME
/// pass must both land. A hand-rolled `set_global(k, get_global(k) + d)`
/// can't do this — see this file's header.
#[test]
fn two_add_global_calls_to_the_same_key_in_one_pass_both_land() {
    let (result, log) = run_source_with_result(
        "double_add",
        r#"
        fn on_update(id, ctx) {
            ctx.add_global("score", 10);
            let _n = ctx.add_global("score", 5);
        }
    "#,
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    let score = result.globals.get("score").unwrap().as_float().unwrap();
    assert_eq!(score, 15.0, "both add_global calls this pass must accumulate, not clobber");
}

/// A key that was never set reads as 0 — the same implicit-zero convention
/// `or_zero()` used to give the demo scripts by hand.
#[test]
fn add_global_on_a_never_set_key_treats_it_as_zero() {
    let (result, _log) =
        run_source_with_result("fresh_key", r#"fn on_update(id, ctx) { ctx.add_global("gold", 3); }"#);
    let gold = result.globals.get("gold").unwrap().as_float().unwrap();
    assert_eq!(gold, 3.0);
}

/// `remove_global` then `add_global` on the SAME key in one pass must not
/// resurrect the removed value — this is exactly `resolve_hits`'s "dead"
/// guard in director.rhai (7.5-2's own landed note): an enemy's "ehp_<id>"
/// key is removed the instant it dies, and a later hit on an already-dead
/// key must not read back the pre-removal HP and revive it. The stale
/// "ehp_1" of 5 is seeded as if resolved from an EARLIER pass (mirroring
/// `add_persistent_after_clear_all_persistent...`'s own reasoning for why
/// a same-pass `set_global` wouldn't discriminate this case at all: it
/// would just be overwritten by the following `remove_global` in the same
/// flat `pending_globals` map either way).
#[test]
fn add_global_after_remove_global_in_the_same_pass_starts_over_from_zero() {
    let mut seed = BTreeMap::new();
    seed.insert("ehp_1".to_string(), Dynamic::from(5_i64));
    let (result, _log) = run_source_with_result_and_globals(
        "remove_then_add",
        r#"
        fn on_update(id, ctx) {
            ctx.remove_global("ehp_1");
            ctx.add_global("ehp_1", -1);
        }
    "#,
        seed,
    );
    let hp = result.globals.get("ehp_1").unwrap().as_float().unwrap();
    assert_eq!(hp, -1.0, "add_global after remove_global must start over from 0, not resurrect 5");
}

/// `add_persistent` after `clear_all_persistent` in the same pass must
/// treat "current" as 0, even though the real store isn't cleared until
/// `apply_ctx` runs at the end of the pass — see `add_persistent`'s own doc
/// comment (api_ext.rs) for why reading `persistent` directly here would
/// get this wrong. The stale "gold" of 40 is seeded as if resolved from an
/// EARLIER pass (via `run_source_with_result_and_persistent` — see its own
/// doc comment for why a same-pass `set_persistent` wouldn't test this):
/// reading it directly would land this test on 41, not 1.
#[test]
fn add_persistent_after_clear_all_persistent_in_the_same_pass_starts_over_from_zero() {
    let mut seed = BTreeMap::new();
    seed.insert("gold".to_string(), Dynamic::from(40_i64));
    let (result, _log) = run_source_with_result_and_persistent(
        "clear_all_then_add",
        r#"
        fn on_update(id, ctx) {
            ctx.clear_all_persistent();
            ctx.add_persistent("gold", 1);
        }
    "#,
        seed,
    );
    let gold = result.persistent.get("gold").unwrap().as_float().unwrap();
    assert_eq!(gold, 1.0, "add_persistent must not see the pre-clear resolved value");
}

/// The `i64` overload — see `registry.rs`'s own note on why every
/// coordinate/size/layer-order function gets one (7.5-1, R31); `delta` is
/// exactly that kind of argument, and director.rhai's own
/// `ctx.add_global("score", 25 * cleared)` (an int expression) is the real
/// call shape this exists for.
#[test]
fn add_global_accepts_an_int_delta_literal_not_just_float() {
    let (result, log) = run_source_with_result(
        "int_delta",
        r#"fn on_update(id, ctx) { let _n = ctx.add_global("score", 25 * 2); }"#,
    );
    assert!(
        !log.iter().any(|e| e.level == LogLevel::Error),
        "add_global must accept an int delta, not just float: {:?}",
        log
    );
    let score = result.globals.get("score").unwrap().as_float().unwrap();
    assert_eq!(score, 50.0);
}
