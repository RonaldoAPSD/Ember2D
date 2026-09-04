// scripting/engine_tests.rs — ScriptEngine unit tests.
//
// Split out of engine.rs (via #[path] in the `mod tests` declaration there)
// once engine.rs crossed the project's 600-line hard limit — see CLAUDE.md
// "Development Rules". This file's content is `scripting::engine::tests`,
// with full access to engine.rs's private items via `use super::*`.

use super::*;
use crate::components::Script;
use crate::renderer::color::Color;

// ── Tests: D3, D8 scoped hot-reload, D9 disable-on-error, D10 spawn_entity ──────
// (docs/ember2d-refactor-plan.md §3 — D3: the script RNG used to be
// `SmallRng::from_entropy()`, making random_* nondeterministic across runs.
// D8: `check_hot_reload` used to call `self.scopes.clear()` unconditionally,
// wiping every entity's persistent `let` state and __timer_* vars whenever
// ANY one script file changed. D9: a script that errored kept being called
// and failing every frame — only its own log message got suppressed after
// the first failure. D10: `spawn_entity` hardcoded white/z=2/1x1 trigger
// regardless of what the script asked for.)

#[test]
fn same_seed_produces_the_same_random_sequence() {
    use rand::Rng;
    let engine_a = ScriptEngine::new(1234);
    let engine_b = ScriptEngine::new(1234);
    let seq_a: Vec<u32> = (0..10).map(|_| engine_a.rng.borrow_mut().gen_range(0..1_000_000)).collect();
    let seq_b: Vec<u32> = (0..10).map(|_| engine_b.rng.borrow_mut().gen_range(0..1_000_000)).collect();
    assert_eq!(seq_a, seq_b, "the same seed must produce the same random_* sequence");
}

#[test]
fn different_seeds_produce_different_random_sequences() {
    use rand::Rng;
    let engine_a = ScriptEngine::new(1);
    let engine_b = ScriptEngine::new(2);
    let seq_a: Vec<u32> = (0..10).map(|_| engine_a.rng.borrow_mut().gen_range(0..1_000_000)).collect();
    let seq_b: Vec<u32> = (0..10).map(|_| engine_b.rng.borrow_mut().gen_range(0..1_000_000)).collect();
    assert_ne!(seq_a, seq_b, "different seeds should not produce the same sequence");
}

#[test]
fn hot_reload_clears_only_the_reloaded_scripts_entities() {
    let mut script_a = std::env::temp_dir();
    script_a.push("ember2d_test_hot_reload_a.rhai");
    std::fs::write(&script_a, "fn on_update(id, ctx) {}\n").unwrap();

    let mut script_b = std::env::temp_dir();
    script_b.push("ember2d_test_hot_reload_b.rhai");
    std::fs::write(&script_b, "fn on_update(id, ctx) {}\n").unwrap();

    let path_a = script_a.to_string_lossy().to_string();
    let path_b = script_b.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42);
    let mut log = Vec::new();
    assert!(engine.compile(&path_a, &mut log));
    assert!(engine.compile(&path_b, &mut log));

    let mut world = World::new();
    let entity_a = world.spawn();
    let entity_b = world.spawn();
    world.add_script(entity_a, Script::new(&path_a));
    world.add_script(entity_b, Script::new(&path_b));

    // Seed both entities' scopes with a marker, as if a script had set a
    // persistent `let` (or a timer via __timer_*) on a previous call.
    engine.scopes.entry(entity_a).or_insert_with(Scope::new).set_value("marker", true);
    engine.scopes.entry(entity_b).or_insert_with(Scope::new).set_value("marker", true);

    // Force script A to look stale without needing to race filesystem
    // mtime resolution — an artificially ancient recorded mtime is
    // guaranteed older than the file's real one.
    engine.mod_times.insert(path_a.clone(), std::time::SystemTime::UNIX_EPOCH);

    engine.check_hot_reload(&world, &mut log);

    assert!(!engine.scopes.contains_key(&entity_a), "the reloaded script's entity must get a fresh scope");
    assert_eq!(
        engine.scopes.get(&entity_b).and_then(|s| s.get_value::<bool>("marker")),
        Some(true),
        "an unrelated entity's scope must survive another script's hot-reload"
    );

    let _ = std::fs::remove_file(&script_a);
    let _ = std::fs::remove_file(&script_b);
}

fn run_scripts_once(engine: &mut ScriptEngine, world: &mut World, log: &mut Vec<LogEntry>) {
    let mut events = EventBus::new();
    let mut persistent = HashMap::new();
    engine.run_scripts(
        world, &mut events, log, 1.0 / 60.0, 0.0, None, None, None,
        &[], HashMap::new(), &mut persistent, crate::math::Vec2::ZERO, (80, 24),
    );
}

#[test]
fn a_script_that_errors_is_disabled_and_stops_being_called() {
    let mut script = std::env::temp_dir();
    script.push("ember2d_test_disable_on_error.rhai");
    std::fs::write(&script, "fn on_update(id, ctx) { throw \"boom\"; }\n").unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42);
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let entity = world.spawn();
    world.add_script(entity, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    assert!(engine.disabled_scripts.contains(&path), "a script that throws must be disabled after its first error");
    let errors_after_first_call = log.iter().filter(|e| e.level == LogLevel::Error).count();
    assert_eq!(errors_after_first_call, 1);

    // If the script were still being called, this would throw and log
    // again — the count must not move.
    run_scripts_once(&mut engine, &mut world, &mut log);
    run_scripts_once(&mut engine, &mut world, &mut log);
    let errors_after_more_calls = log.iter().filter(|e| e.level == LogLevel::Error).count();
    assert_eq!(errors_after_more_calls, errors_after_first_call, "a disabled script must not be invoked again, so it can't log another error");

    // Fix the script on disk and force it to look stale (see the hot-reload
    // test above for why UNIX_EPOCH rather than racing real fs mtimes).
    std::fs::write(&script, "fn on_update(id, ctx) {}\n").unwrap();
    engine.mod_times.insert(path.clone(), std::time::SystemTime::UNIX_EPOCH);

    run_scripts_once(&mut engine, &mut world, &mut log);
    assert!(!engine.disabled_scripts.contains(&path), "a fixed script must re-enable itself once it hot-reloads successfully");
    let errors_after_fix = log.iter().filter(|e| e.level == LogLevel::Error).count();
    assert_eq!(errors_after_fix, errors_after_first_call, "the fixed script must run cleanly with no new errors");

    let _ = std::fs::remove_file(&script);
}

/// Compile `source` to a temp file, attach it to a fresh scripted entity
/// in a fresh world, and run one update pass. Returns the world (for the
/// caller to inspect whatever the script spawned) and the log.
///
/// `name` must be unique per call site — tests run in parallel threads
/// within one process, so anything derived from the process id alone
/// would collide across the tests that share this helper.
fn run_source(name: &str, source: &str) -> (World, Vec<LogEntry>) {
    let mut script = std::env::temp_dir();
    script.push(format!("ember2d_test_spawn_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42);
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&script);
    (world, log)
}

#[test]
fn spawn_entity_default_overload_keeps_the_legacy_appearance() {
    // Defect D10 regression guard: the 4-arg overload must still produce
    // exactly what used to be hardcoded, so existing scripts (and the
    // demo) see no behavior change.
    let (world, _log) = run_source("default_overload", r#"
        fn on_update(id, ctx) { ctx.spawn_entity("Q", 3.0, 4.0, "widget"); }
    "#);

    let spawned = world.find_by_tag("widget").expect("spawned entity should exist");
    let tf = world.transforms.get(&spawned).unwrap();
    assert_eq!((tf.position.x, tf.position.y), (3.0, 4.0));
    let sp = world.sprites.get(&spawned).unwrap();
    assert_eq!(sp.glyph, 'Q');
    assert_eq!(sp.fg, Color::White);
    assert_eq!(sp.bg, Color::Reset);
    assert_eq!(sp.z_order, 2);
    let col = world.colliders.get(&spawned).unwrap();
    assert!(!col.solid);
    assert_eq!((col.width, col.height), (1.0, 1.0));
    assert_eq!(col.layer, "");
}

#[test]
fn spawn_entity_extended_overload_honors_every_parameter() {
    let (world, _log) = run_source("extended_overload", r#"
        fn on_update(id, ctx) {
            ctx.spawn_entity("B", 1.0, 2.0, "bullet", "Red", "Reset", 9, true, 0.5, 0.5, "projectile");
        }
    "#);

    let spawned = world.find_by_tag("bullet").expect("spawned entity should exist");
    let sp = world.sprites.get(&spawned).unwrap();
    assert_eq!(sp.glyph, 'B');
    assert_eq!(sp.fg, Color::Red);
    assert_eq!(sp.z_order, 9);
    let col = world.colliders.get(&spawned).unwrap();
    assert!(col.solid);
    assert_eq!((col.width, col.height), (0.5, 0.5));
    assert_eq!(col.layer, "projectile");
}

#[test]
fn a_script_can_configure_the_entity_it_just_spawned_in_the_same_frame() {
    // Spawns are now applied before every other pending_* queue in
    // apply_ctx specifically so this pattern works on the very first
    // frame, not just from the next frame onward.
    let (world, _log) = run_source("same_frame_setter", r#"
        fn on_update(id, ctx) {
            let e = ctx.spawn_entity("Z", 0.0, 0.0, "thing");
            ctx.set_z_order(e, 42);
        }
    "#);

    let spawned = world.find_by_tag("thing").expect("spawned entity should exist");
    assert_eq!(world.sprites.get(&spawned).unwrap().z_order, 42, "a setter called on the same frame as spawn_entity must not be dropped");
}
