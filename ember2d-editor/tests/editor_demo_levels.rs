// ember2d-editor/tests/editor_demo_levels.rs — Step 9-8 (docs/ember2d-
// master-plan.md §5.8): every shipped demo level is exactly what the editor
// would save. Each one is opened the way the editor opens a level
// (`LevelGrid::from_level_data`) and written back the way it saves one
// (`to_level_data`, which bakes the tilemap); the text must come out
// unchanged. That's what lets a demo's levels be authored, and edited, in
// the editor alone — no field the editor can't hold, no generator-only
// layout.

use ember2d_editor::editor::grid::LevelGrid;
use ember2d_sim::level::LevelData;

fn ron(level: &LevelData) -> String {
    let config = ron::ser::PrettyConfig::new().depth_limit(4).new_line("\n".to_string());
    ron::ser::to_string_pretty(level, config).expect("serialize")
}

#[test]
fn every_demo_level_round_trips_through_the_editor_unchanged() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos");
    let mut checked = 0;
    for demo in ["rpg", "roguelike", "shooter"] {
        let dir = std::path::Path::new(root).join(demo);
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "level"))
            .collect();
        files.sort();
        for path in files {
            let loaded = LevelData::load(&path.to_string_lossy()).unwrap();
            let saved = LevelGrid::from_level_data(&loaded).to_level_data();
            assert_eq!(
                ron(&saved),
                ron(&loaded),
                "{} changes when the editor saves it",
                path.display()
            );
            checked += 1;
        }
    }
    // The RPG's four, the roguelike's title + generated dungeon (9.5-3),
    // the shooter's arena.
    assert!(checked >= 7, "every demo's levels were checked ({checked})");
}
