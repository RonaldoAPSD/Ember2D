// scripting/safety_tests.rs — regression tests for 7A-1
// (docs/ember2d-master-plan.md §5.1): a broken or malicious script must
// never crash or hang the engine (CLAUDE.md's "Error handling" rule). Split
// into its own sibling file (via `#[path]` in engine.rs's `mod
// safety_tests;` declaration, next to the existing `mod tests;` ->
// engine_tests.rs and `mod timer_tests;` -> timer_tests.rs) rather than
// appended to engine_tests.rs, which was already at 496/600 lines
// (CLAUDE.md's hard limit) before this step's coverage — same reasoning
// timer_tests.rs's own header comment gives for its own split.
//
// One test per defect in the R1-R10 group (docs/ember2d-master-plan.md §3.2)
// that 7A-1 fixes: R1 (unbounded script hang), R2/R3 (random_int/random_bool
// panics), R4 (parse_color panic on non-ASCII hex), R5 (Animator::advance
// runaway loop), R6 (NaN position -> inconsistent sort comparator), R9
// (clear_all_persistent no-op), R10 (set_tag/play_clip ghost components).

use super::*;
use crate::components::{ClipFrames, Script, Transform};

/// Mirrors `engine_tests.rs`'s and `timer_tests.rs`'s own `test_layers()` —
/// duplicated rather than shared, same reasoning `timer_tests.rs`'s own
/// comment on this gives.
fn test_layers() -> crate::layers::LayerRegistry { crate::layers::LayerRegistry::new(&["solid".to_string()]) }

/// Mirrors `engine_tests.rs`'s own `run_scripts_once` — duplicated for the
/// same reason `test_layers()` is; this file has no access to that one
/// (private to `engine_tests.rs`'s own module).
fn run_scripts_once(engine: &mut ScriptEngine, world: &mut World, log: &mut Vec<LogEntry>) {
    let mut persistent = BTreeMap::new();
    let snapshot = Rc::new(WorldSnapshot::build(world, &engine.layers));
    engine.run_scripts(
        world, snapshot, log, 1.0 / 60.0, 0.0, crate::command::InputSnapshot::default(), crate::command::MouseSnapshot::default(), crate::command::GamepadSnapshot::default(),
        &[], BTreeMap::new(), BTreeMap::new(), &mut persistent, crate::math::Vec2::ZERO, BTreeMap::new(), 0, (80, 24),
    );
}

/// Mirrors `engine_tests.rs`'s own `run_source`.
fn run_source(name: &str, source: &str) -> (World, Vec<LogEntry>) {
    let mut script = std::env::temp_dir();
    script.push(format!("ember2d_test_safety_{}.rhai", name));
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

/// Mirrors `engine_tests.rs`'s own `run_source_with_driver`, and returns the
/// engine too (some tests here need to keep driving it — `run_source`'s own
/// version, above, doesn't since none of that file's tests need to). The
/// driver gets a `Transform` for the same reason `run_source_with_driver`'s
/// own doc comment (engine_tests.rs) gives — R10's ghost-entity guard in
/// `apply_ctx` treats "has a Transform" as "exists."
fn run_source_with_engine(name: &str, source: &str) -> (ScriptEngine, World, EntityId, Vec<LogEntry>) {
    let mut script = std::env::temp_dir();
    script.push(format!("ember2d_test_safety_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&script);
    (engine, world, driver, log)
}

// ── R1: an unbounded script must not hang the process ──────────────────────

#[test]
fn a_script_with_an_unbounded_loop_is_disabled_after_one_step_with_an_error_logged() {
    let (engine, _world, _driver, log) = run_source_with_engine("infinite_loop", r#"
        fn on_update(id, ctx) { loop {} }
    "#);
    assert!(log.iter().any(|e| e.level == LogLevel::Error), "hitting the operation limit must log an error, same as any other runtime failure");
    // The path is whatever run_source_with_engine wrote to temp — recover it
    // the same way the disabled-script check elsewhere in this crate does:
    // by asking the engine directly rather than re-deriving the path here.
    assert_eq!(engine.disabled_scripts.len(), 1, "the runaway script must be disabled, not left running every subsequent step");
}

// ── R2/R3: random_int/random_bool must not panic on bad arguments ──────────

#[test]
fn random_int_with_max_less_than_min_still_returns_a_value_in_the_implied_range() {
    let (_world, log) = run_source("random_int_reversed", r#"
        fn on_update(id, ctx) { ctx.log(ctx.random_int(5, 1).to_string()); }
    "#);
    let msg = log.iter().find(|e| e.level == LogLevel::Info).expect("random_int(5, 1) must return a value, not panic");
    let n: i64 = msg.text.parse().expect("logged value must be an integer");
    assert!((1..=5).contains(&n), "random_int(5, 1) must still draw from {{1..=5}}, got {}", n);
}

#[test]
fn random_bool_with_a_nan_chance_returns_false_without_panicking() {
    let (_world, log) = run_source("random_bool_nan", r#"
        fn on_update(id, ctx) { ctx.log(ctx.random_bool(0.0 / 0.0).to_string()); }
    "#);
    let msg = log.iter().find(|e| e.level == LogLevel::Info).expect("random_bool(NaN) must return a value, not panic");
    assert_eq!(msg.text, "false", "a non-finite chance must be treated as 0.0 (never true)");
}

// ── R4: set_tint with a malformed color must not panic or corrupt state ────

#[test]
fn set_tint_with_non_ascii_hex_leaves_the_tint_unchanged() {
    let (world, log) = run_source("set_tint_non_ascii", r##"
        fn on_update(id, ctx) {
            let e = ctx.spawn_entity("Q", 0.0, 0.0, "thing");
            ctx.set_tint(e, "#€€", "White");
        }
    "##);

    let spawned = world.find_by_tag("thing").expect("spawned entity should exist");
    let sp = world.sprites.get(&spawned).unwrap();
    assert_eq!(sp.tint, crate::color::Color::White, "a malformed color must leave the previous tint (the spawn default) unchanged, not overwrite it with Reset or panic");
    assert!(log.iter().any(|e| e.level == LogLevel::Info), "the malformed string must still be logged once so the author can find it");
}

// ── R5: an extreme clip speed must not hang Animator::advance ──────────────

#[test]
fn set_clip_speed_with_an_extreme_value_terminates_within_a_bounded_number_of_steps() {
    let (_engine, mut world, driver, _log) = run_source_with_engine("extreme_clip_speed", r#"
        fn on_update(id, ctx) {
            ctx.register_clip("spin", "abc", 10.0, true);
            ctx.play_clip(id, "spin");
            ctx.set_clip_speed(id, 1000000000.0);
        }
    "#);

    let animator = world.animators.get_mut(&driver).expect("play_clip must create an Animator");
    assert!(animator.speed <= 64.0, "set_clip_speed must clamp an extreme scripted value before it reaches Animator.speed, got {}", animator.speed);

    let clip = crate::components::AnimationClip { frames: ClipFrames::Glyphs { frames: vec!['a', 'b', 'c'] }, fps: 10.0, looping: true };
    // R5's actual bug was Animator::advance never returning at a large
    // enough speed (f32 precision absorption made `elapsed -= frame_duration`
    // a no-op) — 1,000 steps completing at all, not any particular frame
    // value, is the regression guard here.
    for _ in 0..1000 {
        animator.advance(&clip, 1.0 / 60.0);
    }
}

// ── R6: a non-finite position must not reach the collision sort ────────────

#[test]
fn set_position_with_a_nan_coordinate_is_a_no_op() {
    let (world, _log) = run_source("set_position_nan", r#"
        fn on_update(id, ctx) {
            let e = ctx.spawn_entity("Q", 3.0, 4.0, "thing");
            ctx.set_position(e, 0.0 / 0.0, 0.0 / 0.0);
        }
    "#);

    let spawned = world.find_by_tag("thing").expect("spawned entity should exist");
    let tf = world.transforms.get(&spawned).unwrap();
    assert_eq!((tf.position.x, tf.position.y), (3.0, 4.0), "a non-finite set_position call must leave the entity's position unchanged");
}

#[test]
fn detect_collisions_does_not_panic_when_a_collider_has_a_nan_position() {
    // Belt-and-suspenders alongside the no-op guard above: even if a NaN
    // position ever reached World directly (not through the script API this
    // guards), the sort itself must not panic. total_cmp is a genuine total
    // order over every f32 bit pattern, NaN included.
    let mut world = World::new();
    let a = world.spawn();
    world.add_transform(a, crate::components::Transform::new(f32::NAN, 0.0));
    world.add_collider(a, crate::components::Collider::unit());
    let b = world.spawn();
    world.add_transform(b, crate::components::Transform::new(0.0, 0.0));
    world.add_collider(b, crate::components::Collider::unit());

    let mut events = crate::event::EventBus::new();
    world.detect_collisions(&mut events); // must not panic
}

// ── R9: clear_all_persistent must empty the real store, not the write queue ─

#[test]
fn clear_all_persistent_empties_a_populated_store() {
    let mut script = std::env::temp_dir();
    script.push("ember2d_test_safety_clear_all_persistent.rhai");
    std::fs::write(&script, r#"
        fn on_update(id, ctx) {
            if ctx.has_global("phase2") {
                ctx.clear_all_persistent();
            } else {
                ctx.set_persistent("foo", 1);
                ctx.set_persistent("bar", 2);
                ctx.set_global("phase2", true);
            }
        }
    "#).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(1, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let entity = world.spawn();
    world.add_script(entity, Script::new(&path));

    let mut globals = BTreeMap::new();
    let mut persistent = BTreeMap::new();

    // Pass 1: populate the store.
    let snapshot = Rc::new(WorldSnapshot::build(&world, &engine.layers));
    let result = engine.run_scripts(
        &mut world, snapshot, &mut log, 1.0 / 60.0, 0.0, crate::command::InputSnapshot::default(), crate::command::MouseSnapshot::default(), crate::command::GamepadSnapshot::default(),
        &[], globals, BTreeMap::new(), &mut persistent, crate::math::Vec2::ZERO, BTreeMap::new(), 0, (80, 24),
    );
    globals = result.globals;
    persistent = result.persistent;
    assert_eq!(persistent.len(), 2, "the store must be populated after pass 1");

    // Pass 2: before this step's fix, clear_all_persistent cleared the
    // (already-empty) pending write queue instead of this store — a no-op.
    let snapshot = Rc::new(WorldSnapshot::build(&world, &engine.layers));
    let result = engine.run_scripts(
        &mut world, snapshot, &mut log, 1.0 / 60.0, 0.0, crate::command::InputSnapshot::default(), crate::command::MouseSnapshot::default(), crate::command::GamepadSnapshot::default(),
        &[], globals, BTreeMap::new(), &mut persistent, crate::math::Vec2::ZERO, BTreeMap::new(), 0, (80, 24),
    );
    persistent = result.persistent;
    assert!(persistent.is_empty(), "clear_all_persistent must empty the real store");

    let _ = std::fs::remove_file(&script);
}

// ── R10: set_tag/play_clip on a missing entity must not create one ─────────

#[test]
fn set_tag_on_a_missing_entity_does_not_create_a_ghost_entity() {
    let (world, _log) = run_source("set_tag_missing_entity", r#"
        fn on_update(id, ctx) { ctx.set_tag(9999, "x"); }
    "#);
    assert!(!world.tags.contains_key(&9999), "set_tag on a nonexistent id must not insert a ghost Tag component");
    assert!(!world.entity_ids().contains(&9999), "no entity at all should exist at that id afterward");
}

#[test]
fn play_clip_on_a_missing_entity_does_not_create_a_ghost_entity() {
    let (world, _log) = run_source("play_clip_missing_entity", r#"
        fn on_update(id, ctx) {
            ctx.register_clip("flicker", "*+#", 6.0, true);
            ctx.play_clip(9999, "flicker");
        }
    "#);
    assert!(!world.animators.contains_key(&9999), "play_clip on a nonexistent id must not insert a ghost Animator component");
    assert!(!world.entity_ids().contains(&9999), "no entity at all should exist at that id afterward");
}
