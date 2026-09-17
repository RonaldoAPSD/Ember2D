// scripting/path_tests.rs — regression tests for 7.5-6 (docs/ember2d-
// master-plan.md §5.6): get_path's new `diagonal` option and its no-corner-
// cutting guard, and the new reachable_within movement-range query. Split
// into its own sibling file — same reasoning uniform_typing_tests.rs's/
// vars_tests.rs's/actor_stats_tests.rs's/set_script_tests.rs's own header
// comments give for their own splits.

use super::*;
use crate::components::{Collider, Script, Tag, Transform};

fn test_layers() -> crate::layers::LayerRegistry {
    crate::layers::LayerRegistry::new(&["solid".to_string()])
}

fn test_temp_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run_scripts_once(
    engine: &mut ScriptEngine,
    world: &mut World,
    log: &mut Vec<LogEntry>,
) -> ScriptUpdateResult {
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
    )
}

/// A solid 1x1 wall at (x, y), spawned directly on `world` (not via a
/// script) — an obstacle for `get_path`/`reachable_within` to route
/// around, present in the very first `WorldSnapshot` a test script sees.
fn wall_at(world: &mut World, x: f32, y: f32) {
    let id = world.spawn();
    world.add_transform(id, Transform::new(x, y));
    world.add_collider(id, Collider::unit());
}

/// Compiles `source` onto a driver entity, adds every wall in `walls`, and
/// runs one `on_update` pass. Returns the pass's own `ScriptUpdateResult`
/// (its `globals` is where every test below reads its answer back from)
/// and the log.
fn run_source_with_result(
    name: &str,
    source: &str,
    walls: &[(f32, f32)],
) -> (ScriptUpdateResult, Vec<LogEntry>) {
    let mut script = test_temp_dir();
    script.push(format!("ember2d_test_path_{}.rhai", name));
    std::fs::write(&script, source).unwrap();
    let path = script.to_string_lossy().to_string();

    let mut engine = ScriptEngine::new(42, test_layers());
    let mut log = Vec::new();
    assert!(engine.compile(&path, &mut log));

    let mut world = World::new();
    let driver = world.spawn();
    world.add_transform(driver, Transform::new(0.0, 0.0));
    world.add_script(driver, Script::new(&path));

    // A separate, tag-findable entity at the origin for `reachable_within`
    // tests to query by id (`ctx.find_by_tag("mover")`) — no `Collider`,
    // so it never blocks a path or itself. Harmless for the `get_path`
    // tests above, which never look it up.
    let mover = world.spawn();
    world.add_transform(mover, Transform::new(0.0, 0.0));
    world.add_tag(mover, Tag::new("mover"));

    for &(x, y) in walls {
        wall_at(&mut world, x, y);
    }

    let result = run_scripts_once(&mut engine, &mut world, &mut log);
    let _ = std::fs::remove_file(&path);
    (result, log)
}

fn global_int(result: &ScriptUpdateResult, key: &str) -> Option<i64> {
    result.globals.get(key).and_then(|d| d.as_int().ok())
}

#[test]
fn diagonal_true_finds_a_shorter_path_than_the_default_four_directional_one() {
    let (result, log) = run_source_with_result(
        "shorter_diagonal",
        r#"
        fn on_update(id, ctx) {
            let straight = ctx.get_path(0.0, 0.0, 2.0, 2.0, []);
            let diag = ctx.get_path(0.0, 0.0, 2.0, 2.0, [], true);
            ctx.set_global("straight_len", straight.len());
            ctx.set_global("diag_len", diag.len());
        }
    "#,
        &[],
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    // Manhattan distance from (0,0) to (2,2) is 4 (4-directional); the
    // diagonal shortcut covers the same displacement in 2 steps.
    assert_eq!(global_int(&result, "straight_len"), Some(4), "default get_path must stay 4-directional");
    assert_eq!(global_int(&result, "diag_len"), Some(2), "diagonal: true must find the 2-step diagonal route");
}

#[test]
fn diagonal_movement_cannot_cut_between_two_walls_that_share_only_a_corner() {
    // Walls at (1, 0) and (0, 1) leave (1, 1) open but flank it on both
    // orthogonal sides — a single diagonal hop from (0, 0) to (1, 1) would
    // otherwise squeeze between both wall corners.
    let (result, log) = run_source_with_result(
        "no_corner_cut",
        r#"
        fn on_update(id, ctx) {
            let path = ctx.get_path(0.0, 0.0, 1.0, 1.0, [], true);
            ctx.set_global("path_len", path.len());
        }
    "#,
        &[(1.0, 0.0), (0.0, 1.0)],
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    // A direct corner-cut would be exactly 1 step; the long way around
    // (going around both walls) is necessarily more than 1.
    let len = global_int(&result, "path_len").expect("path should still exist via the long way around");
    assert!(len > 1, "a diagonal move must not cut between two walls sharing only a corner, got path_len={len}");
}

#[test]
fn reachable_within_counts_every_cell_in_the_manhattan_diamond() {
    let (result, log) = run_source_with_result(
        "reach_open",
        r#"
        fn on_update(id, ctx) {
            let mover = ctx.find_by_tag("mover");
            let cells = ctx.reachable_within(mover, 2);
            ctx.set_global("reach_count", cells.len());
        }
    "#,
        &[],
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    // Manhattan-diamond cells at distance 1 (4) plus distance 2 (8) = 12,
    // never including the mover's own starting cell.
    assert_eq!(global_int(&result, "reach_count"), Some(12));
}

#[test]
fn reachable_within_excludes_a_blocked_neighbor() {
    let (result, log) = run_source_with_result(
        "reach_blocked",
        r#"
        fn on_update(id, ctx) {
            let mover = ctx.find_by_tag("mover");
            let cells = ctx.reachable_within(mover, 1);
            ctx.set_global("reach_count", cells.len());
        }
    "#,
        &[(0.0, 1.0)],
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    // 4 orthogonal neighbors at budget 1, minus the one wall directly north.
    assert_eq!(global_int(&result, "reach_count"), Some(3));
}

#[test]
fn reachable_within_a_non_positive_budget_or_an_unknown_id_returns_empty() {
    let (result, log) = run_source_with_result(
        "reach_empty",
        r#"
        fn on_update(id, ctx) {
            let mover = ctx.find_by_tag("mover");
            ctx.set_global("zero_budget", ctx.reachable_within(mover, 0).len());
            ctx.set_global("unknown_id", ctx.reachable_within(9999, 5).len());
        }
    "#,
        &[],
    );
    assert!(log.is_empty(), "unexpected script log: {:?}", log);
    assert_eq!(global_int(&result, "zero_budget"), Some(0));
    assert_eq!(global_int(&result, "unknown_id"), Some(0));
}
