// standalone.rs — launching an exported game.
//
// R120 (docs/ember2d-master-plan.md §3.2), found replaying the
// first-project tutorial (Step 9.5-6). File > Export Game copies this
// executable into `<Name>_Export/` next to the project's files and marks
// the folder with an empty `.standalone` file — but nothing ever read the
// marker. Started with no arguments, as a player would (double-clicking
// it), the "game" opened the editor's start screen.
//
// Now `main` asks `standalone_game` first: if the executable sits in a
// marked folder, it plays that folder's start level, exactly as
// `ember2d path/to.level` does. Any arguments still win, so the exported
// copy can still be pointed at a level by hand.

use std::path::{Path, PathBuf};

use ember2d::project::ProjectData;

/// The level an exported game should start on, if the executable at `exe`
/// is one: its folder holds Export Game's `.standalone` marker. The
/// project's `start_level`, else its first level by name. `None` for an
/// ordinary build (no marker) or a marked folder with no level at all.
pub fn standalone_game(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    if !dir.join(".standalone").is_file() {
        return None;
    }
    let folder = dir.to_string_lossy().into_owned();
    if let Some(start) = ProjectData::load(&folder).ok().and_then(|p| p.start_level) {
        let path = dir.join(start);
        if path.is_file() {
            return Some(path);
        }
    }
    ProjectData::levels_in(&folder).into_iter().next().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder per test (tests run in parallel threads).
    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
        dir
    }

    #[test]
    fn r120_an_exported_game_starts_on_its_start_level() {
        let dir = temp("r120_start");
        std::fs::write(dir.join(".standalone"), "").unwrap();
        std::fs::write(dir.join("a.level"), "").unwrap();
        std::fs::write(dir.join("title.level"), "").unwrap();
        std::fs::write(
            dir.join("project.ron"),
            "(name: \"G\", start_level: Some(\"title.level\"))",
        )
        .unwrap();
        assert_eq!(standalone_game(&dir.join("G.exe")), Some(dir.join("title.level")));
    }

    #[test]
    fn r120_without_a_start_level_the_first_level_by_name() {
        let dir = temp("r120_first");
        std::fs::write(dir.join(".standalone"), "").unwrap();
        std::fs::write(dir.join("b.level"), "").unwrap();
        std::fs::write(dir.join("a.level"), "").unwrap();
        let got = standalone_game(&dir.join("G.exe")).expect("a level");
        assert_eq!(got.file_name().unwrap(), "a.level");
    }

    #[test]
    fn r120_an_unmarked_folder_is_not_a_game() {
        let dir = temp("r120_plain");
        std::fs::write(dir.join("main.level"), "").unwrap();
        assert_eq!(standalone_game(&dir.join("ember2d.exe")), None);
    }
}
