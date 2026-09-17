// level_source.rs — FsLevelSource: the real, disk-backed
// `ember2d_sim::level_source::LevelSource` implementation every actual
// game uses (Step 7.5-9, docs/ember2d-master-plan.md §5.6, R17 fix).
//
// `ember2d-sim` itself can never touch a filesystem (CLAUDE.md's
// Determinism section) — `Simulation` instead calls through whatever
// `LevelSource` it's been given. `ember2d` is where real disk access is
// allowed, so this is where the working implementation lives.
// `PlayState::new_with_sim` (play.rs) wires this up unconditionally on
// every `Simulation` it constructs; nothing else in this crate should ever
// need to.

use ember2d_sim::level::LevelData;
use ember2d_sim::level_source::LevelSource;
use std::path::Path;

pub struct FsLevelSource;

impl LevelSource for FsLevelSource {
    fn exists(&self, path: &str) -> bool {
        Path::new(path).exists()
    }
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        LevelData::load(path).map_err(|e| e.to_string())
    }
}
