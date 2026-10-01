// simulation/path_tests.rs — `resolve_exit_path`'s search order (Step 9-6,
// docs/ember2d-master-plan.md §5.8): beside the level, then each folder up
// to the project root (the first with a `project.ron`), then the working
// directory, else beside the level.

use super::resolve_exit_path;
use std::collections::BTreeSet;
use std::path::Path;

/// A fake filesystem: the set of paths that "exist", compared with `/`
/// separators so the test reads the same on every OS.
fn fs(paths: &[&str]) -> impl Fn(&str) -> bool {
    let set: BTreeSet<String> = paths.iter().map(|p| p.to_string()).collect();
    move |p: &str| set.contains(&p.replace('\\', "/"))
}

fn norm(p: String) -> String {
    p.replace('\\', "/")
}

#[test]
fn a_path_beside_the_level_wins_over_one_in_the_working_directory() {
    let exists = fs(&["game/scripts/a.rhai", "scripts/a.rhai", "game/project.ron"]);
    assert_eq!(
        norm(resolve_exit_path("scripts/a.rhai", "game/town.level", &exists)),
        "game/scripts/a.rhai"
    );
}

#[test]
fn a_level_in_a_subfolder_finds_the_project_roots_files() {
    let exists = fs(&["game/project.ron", "game/scripts/a.rhai"]);
    assert_eq!(
        norm(resolve_exit_path("scripts/a.rhai", "game/levels/cave.level", &exists)),
        "game/scripts/a.rhai"
    );
}

#[test]
fn the_search_stops_at_the_project_root() {
    // `outer/scripts/a.rhai` exists, but `game/` is the project: never look
    // above it. The working directory has nothing either, so the answer is
    // the expected place beside the level.
    let exists = fs(&["outer/game/project.ron", "outer/scripts/a.rhai"]);
    assert_eq!(
        norm(resolve_exit_path("scripts/a.rhai", "outer/game/town.level", &exists)),
        "outer/game/scripts/a.rhai"
    );
}

#[test]
fn an_old_repo_relative_path_still_resolves_from_the_working_directory() {
    let exists = fs(&["demos/roguelike/project.ron", "demos/roguelike/scripts/p.rhai"]);
    assert_eq!(
        norm(resolve_exit_path(
            "demos/roguelike/scripts/p.rhai",
            "demos/roguelike/floor1.level",
            &exists
        )),
        "demos/roguelike/scripts/p.rhai"
    );
}

#[test]
fn absolute_paths_and_levels_without_a_path_are_left_alone() {
    let exists = fs(&[]);
    let abs = if cfg!(windows) { "C:/x/a.rhai" } else { "/x/a.rhai" };
    assert!(Path::new(abs).is_absolute());
    assert_eq!(resolve_exit_path(abs, "game/town.level", &exists), abs);
    assert_eq!(resolve_exit_path("a.rhai", "", &exists), "a.rhai");
}
