// scripting/engine_tests.rs — ScriptEngine unit tests.
//
// Split out of engine.rs (via #[path] in the `mod tests` declaration there)
// once engine.rs crossed the project's 600-line hard limit — see CLAUDE.md
// "Development Rules". This file's content is `scripting::engine::tests`,
// with full access to engine.rs's private items via `use super::*`.

use super::*;
use crate::components::Script;
use crate::renderer::color::Color;

/// Unwraps a Glyph-sourced sprite's (char, bg) — panics for any other
/// source, which is correct for these tests since they all spawn glyphs.
fn glyph_and_bg(sp: &Sprite) -> (char, Color) {
    match &sp.source {
        SpriteSource::Glyph { ch, bg } => (*ch, *bg),
        other => panic!("expected a Glyph source, got {:?}", other),
    }
}

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
        &[], HashMap::new(), HashMap::new(), &mut persistent, crate::math::Vec2::ZERO, (80, 24),
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

// ── Tests: D15 — a real error inside on_update must not be mistaken for a
// missing optional lifecycle function (docs/ember2d-refactor-plan.md §3) ──
// (`e.to_string().contains("Function not found")` couldn't distinguish
// "on_update doesn't exist in this script" from "some call *inside*
// on_update genuinely has no matching function/operator" — both produce
// the identical Rhai error shape, since operators are functions
// internally. The fix matches ErrorFunctionNotFound's exact payload
// against the specific lifecycle function name instead of a substring of
// the whole error's Display text.)

#[test]
fn a_genuine_function_not_found_error_inside_on_update_is_logged_and_disables_the_script() {
    let (_world, log) = run_source("undefined_function_call", r#"
        fn on_update(id, ctx) {
            this_function_does_not_exist_anywhere(42);
        }
    "#);
    assert!(
        log.iter().any(|e| e.level == LogLevel::Error),
        "a genuine \"function not found\" error from a call inside on_update must be logged, not silently swallowed like a missing on_update itself"
    );
}

#[test]
fn a_script_with_no_on_update_function_is_silently_fine() {
    let (_world, log) = run_source("no_on_update_defined", "// intentionally defines nothing\n");
    assert!(
        !log.iter().any(|e| e.level == LogLevel::Error),
        "a script that simply doesn't define on_update must still be treated as fine, not an error"
    );
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
    let (ch, bg) = glyph_and_bg(sp);
    assert_eq!(ch, 'Q');
    assert_eq!(sp.tint, Color::White);
    assert_eq!(bg, Color::Reset);
    assert_eq!(sp.layer, 2);
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
    let (ch, _bg) = glyph_and_bg(sp);
    assert_eq!(ch, 'B');
    assert_eq!(sp.tint, Color::Red);
    assert_eq!(sp.layer, 9);
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
            ctx.set_layer_order(e, 42);
        }
    "#);

    let spawned = world.find_by_tag("thing").expect("spawned entity should exist");
    assert_eq!(world.sprites.get(&spawned).unwrap().layer, 42, "a setter called on the same frame as spawn_entity must not be dropped");
}

// ── Tests: Step 3e API renames (ember2d-refactor-plan.md Phase 3) ──────────────

#[test]
fn set_tint_writes_the_same_fields_the_removed_set_color_used_to() {
    let (world, _log) = run_source("set_tint", r#"
        fn on_update(id, ctx) {
            let e = ctx.spawn_entity("Q", 0.0, 0.0, "thing");
            ctx.set_tint(e, "Cyan", "DarkBlue");
        }
    "#);

    let spawned = world.find_by_tag("thing").expect("spawned entity should exist");
    let sp = world.sprites.get(&spawned).unwrap();
    let (_, bg) = glyph_and_bg(sp);
    assert_eq!(sp.tint, Color::Cyan);
    assert_eq!(bg, Color::DarkBlue);
}

#[test]
fn set_tint_accepts_an_explicit_hex_value() {
    let (world, _log) = run_source("set_tint_hex", r##"
        fn on_update(id, ctx) {
            let e = ctx.spawn_entity("Q", 0.0, 0.0, "thing");
            ctx.set_tint(e, "#4A90E2", "Reset");
        }
    "##);

    let spawned = world.find_by_tag("thing").expect("spawned entity should exist");
    assert_eq!(world.sprites.get(&spawned).unwrap().tint, Color::Rgb(0x4A, 0x90, 0xE2));
}

#[test]
fn api_version_reports_the_current_breaking_change_generation() {
    let (_world, log) = run_source("api_version", r#"
        fn on_update(id, ctx) { ctx.log(ctx.api_version().to_string()); }
    "#);

    let msg = log.iter().find(|e| e.level == LogLevel::Info).expect("api_version() should be loggable like any other return value");
    assert_eq!(msg.text, API_VERSION.to_string());
}

// ── Test: Step 4g HUD-survives-pause fix (docs/HANDOFF.md) ─────────────────────

#[test]
fn pending_hud_draws_are_cleared_at_the_start_of_run_scripts_not_by_the_renderer() {
    // PlayState::render used to call ScriptEngine::pending_hud_draws.clear()
    // itself, every frame, regardless of whether a script actually ran that
    // frame. Since PlayState::update (and therefore run_scripts) only runs
    // for the top-of-stack GameState, pausing the game (pushing
    // PauseMenuState on top) meant render kept running and clearing every
    // frame while nothing ever refilled the queue — a script's own drawn
    // HUD text vanished the instant the game paused. The fix: clear at the
    // START of run_scripts instead, so the queue only resets when a real
    // script pass actually happens.
    let mut script = std::env::temp_dir();
    script.push("ember2d_test_hud_persist.rhai");
    std::fs::write(&script, r#"
        fn on_update(id, ctx) { ctx.draw_hud(1, 1, "hp: 10", "White", "Reset"); }
    "#).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(1);
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let entity = world.spawn();
    world.add_script(entity, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    assert_eq!(engine.pending_hud_draws.len(), 1, "a script's draw_hud call must land in the engine's queue after a real pass");

    // Simulate a paused frame: no run_scripts call at all (PlayState::update
    // doesn't run for a state that isn't on top of the stack). Nothing else
    // in this headless test touches the queue, pinning that only
    // run_scripts itself may clear it.
    assert_eq!(engine.pending_hud_draws.len(), 1, "skipping a script pass must not clear the queue");

    // A second real pass must reset the queue before repopulating it —
    // otherwise draws would accumulate across frames instead of reflecting
    // only the latest pass.
    run_scripts_once(&mut engine, &mut world, &mut log);
    assert_eq!(engine.pending_hud_draws.len(), 1, "a fresh pass must clear stale draws before adding this frame's");

    let _ = std::fs::remove_file(&script);
}

// ── Tests: Step 3c named animation clips (ember2d-refactor-plan.md Phase 3) ────

/// Compile `source` to a temp file, attach it to a fresh scripted entity in
/// a fresh world, and run one update pass — like `run_source`, but also
/// hands back the driver entity's own id, which a script with no tag/sprite
/// of its own (as the clip tests below need) has no other way to recover.
fn run_source_with_driver(name: &str, source: &str) -> (World, EntityId, Vec<LogEntry>) {
    let mut script = std::env::temp_dir();
    script.push(format!("ember2d_test_clip_{}.rhai", name));
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
    (world, driver, log)
}

#[test]
fn play_clip_starts_an_animator_and_points_the_sprite_at_the_clip() {
    let (world, driver, _log) = run_source_with_driver("play_clip", r#"
        fn on_update(id, ctx) {
            ctx.register_clip("flicker", "*+#", 6.0, true);
            ctx.play_clip(id, "flicker");
        }
    "#);

    let animator = world.animators.get(&driver).expect("play_clip must create an Animator");
    assert_eq!(animator.clip, "flicker");
    assert_eq!(animator.frame, 0);
    assert!(animator.playing);
    assert!(!animator.oneshot, "play_clip must not set the play_clip_once override");
}

#[test]
fn play_clip_once_sets_the_oneshot_override() {
    let (world, driver, _log) = run_source_with_driver("play_clip_once", r#"
        fn on_update(id, ctx) {
            ctx.register_clip("swing", "ab", 6.0, true);
            ctx.play_clip_once(id, "swing");
        }
    "#);

    assert!(world.animators.get(&driver).unwrap().oneshot, "play_clip_once must set the oneshot override even for a looping clip");
}

#[test]
fn clip_finished_reports_true_for_entities_whose_animator_just_finished_this_tick() {
    // Animator ticking itself lives in PlayState::update (Step 3c wires
    // Animator::advance in there, not in the script engine), so this pins
    // just the read side: clip_finished(id) must reflect whatever
    // Animator::just_finished the World already carries into this frame.
    let mut script = std::env::temp_dir();
    script.push("ember2d_test_clip_finished.rhai");
    std::fs::write(&script, r#"
        fn on_update(id, ctx) { ctx.set_global("finished", ctx.clip_finished(id)); }
    "#).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(1);
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let entity = world.spawn();
    world.add_script(entity, Script::new(&path));
    let mut animator = Animator::new("swing");
    animator.just_finished = true;
    world.animators.insert(entity, animator);

    let mut events = EventBus::new();
    let mut persistent = HashMap::new();
    let result = engine.run_scripts(
        &mut world, &mut events, &mut log, 1.0 / 60.0, 0.0, None, None, None,
        &[], HashMap::new(), HashMap::new(), &mut persistent, crate::math::Vec2::ZERO, (80, 24),
    );

    assert_eq!(result.globals.get("finished").and_then(|d| d.as_bool().ok()), Some(true));
    let _ = std::fs::remove_file(&script);
}
