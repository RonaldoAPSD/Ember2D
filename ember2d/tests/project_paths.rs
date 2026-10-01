// tests/project_paths.rs — Step 9-6 (docs/ember2d-master-plan.md §5.8): a
// project's paths are project-relative, so a copy of a project runs ITS
// OWN files wherever it sits — not the originals a repo-relative path
// would have pointed back to.

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::simulation::Simulation;
use std::collections::BTreeMap;
use std::path::Path;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let p = entry.path();
        let dest = to.join(entry.file_name());
        if p.is_dir() {
            copy_dir(&p, &dest);
        } else {
            std::fs::copy(&p, &dest).unwrap();
        }
    }
}

#[test]
fn r115_a_copied_project_runs_its_own_scripts_and_finds_its_own_audio() {
    let demo = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/classic_roguelike"));
    let copy = common::test_temp_dir().join("project_paths_copy");
    let _ = std::fs::remove_dir_all(&copy);
    copy_dir(demo, &copy);
    // The copy's player script differs from the original, so which one ran
    // is observable.
    std::fs::write(
        copy.join("scripts/player.rhai"),
        "fn on_start(id, ctx) { ctx.set_global(\"ran\", \"the copy\"); }",
    )
    .unwrap();

    let level_path = copy.join("floor1.level").to_string_lossy().into_owned();
    let data = LevelData::load(&level_path).expect("the copy's level loads");
    assert_eq!(data.player.script.as_deref(), Some("scripts/player.rhai"));
    let mut sim = Simulation::new(data);
    sim.set_level_source(Box::new(FsLevelSource));
    let mut world = World::new();
    let logs = sim.on_start(&mut world, 80, 24, &mut BTreeMap::new());
    assert!(logs.iter().all(|l| !l.text.contains("Compile")), "{logs:?}");
    let ran = sim.globals().get("ran").map(|v| v.to_string());
    assert_eq!(ran.as_deref(), Some("the copy"));

    // Audio a script names resolves inside the copy too.
    let music = ember2d::play::resolve_exit_path("audio/music.ogg", &level_path, &|p| {
        Path::new(p).exists()
    });
    assert!(Path::new(&music).starts_with(&copy), "{music}");
    assert!(Path::new(&music).exists());
    let _ = std::fs::remove_dir_all(&copy);
}
