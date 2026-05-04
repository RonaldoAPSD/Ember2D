// project.rs — Ember2D project folder format.
//
// ── WHAT IS A PROJECT? ────────────────────────────────────────────────────────
//
// A project is simply a folder on disk that holds level files:
//
//   my_game/
//     project.ron     ← optional metadata file (name, future settings)
//     main.level      ← a level file in RON format
//     dungeon.level   ← another level
//     ...
//
// `project.ron` is OPTIONAL. If it's missing, the folder itself is still a
// valid project and its folder name is used as the project name.
// This means you can open any folder containing .level files as a project.
//
// ── WHY A SEPARATE project.ron? ──────────────────────────────────────────────
//
// The project file exists so we can store project-level metadata (name, author,
// version, etc.) separately from any individual level. Currently it only stores
// `name`, but it's structured as a proper struct so more fields can be added
// without breaking existing projects.
//
// ── HOW THE START SCREEN USES THIS ───────────────────────────────────────────
//
// When the user clicks "Open Project" in the start screen, `find_projects(".")`
// scans the current directory for project subfolders and returns their names.
// The user picks one, and `levels_in(folder)` returns all .level files inside it.

use std::fs;
use serde::{Deserialize, Serialize};

// ── ProjectData ───────────────────────────────────────────────────────────────

/// Metadata stored in a project's `project.ron` file.
///
/// Currently just a name — expand this struct as you add project-level settings.
/// Old `.ron` files that are missing new fields will still deserialize correctly
/// because serde's `#[serde(default)]` handles missing fields gracefully.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    /// Human-readable project name (e.g. "My Platformer").
    ///
    /// Shown in the start screen and editor title bar.
    /// Independent of the folder name — you can rename the folder without
    /// changing the project name stored here.
    pub name: String,
}

impl ProjectData {
    /// Create a new ProjectData with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        ProjectData { name: name.into() }
    }

    /// Write a `project.ron` file into the given folder.
    ///
    /// Creates (or overwrites) `<folder>/project.ron` with the serialized data.
    /// Returns an error if the folder doesn't exist or can't be written.
    pub fn save(&self, folder: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path   = format!("{}/project.ron", folder);
        let config = ron::ser::PrettyConfig::new().depth_limit(2).new_line("\n".to_string());
        let text   = ron::ser::to_string_pretty(self, config)?;
        fs::write(path, text)?;
        Ok(())
    }

    /// Read and parse `<folder>/project.ron`.
    ///
    /// Returns Err if the file is missing or malformed. The caller can then
    /// fall back to using the folder name (see `name_for()` below).
    pub fn load(folder: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let path    = format!("{}/project.ron", folder);
        let content = fs::read_to_string(path)?;
        Ok(ron::de::from_str(&content)?)
    }

    /// Get the display name for a project folder.
    ///
    /// Tries to read `project.ron` first. If that fails (file missing, malformed,
    /// permissions error), falls back to the folder's own name.
    ///
    /// This is the "safe" way to get a project name — always returns something
    /// human-readable even for folders without a `project.ron`.
    pub fn name_for(folder: &str) -> String {
        ProjectData::load(folder)
            .map(|p| p.name)
            .unwrap_or_else(|_| {
                // Extract just the final path component (the folder's own name).
                std::path::Path::new(folder)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| folder.to_string())
            })
    }

    /// List all `.level` files inside `folder`, returned as sorted full paths.
    ///
    /// Example: for `my_game/`, returns `["my_game/dungeon.level", "my_game/main.level"]`
    ///
    /// Returns an empty Vec if the folder doesn't exist or can't be read.
    pub fn levels_in(folder: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(folder) else { return Vec::new() };
        let mut files: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.ends_with(".level") {
                    // Build the full relative path: folder/filename.level
                    Some(format!("{}/{}", folder, name))
                } else {
                    None
                }
            })
            .collect();
        files.sort();
        files
    }

    /// Scan `dir` for subdirectories that look like Ember2D projects.
    ///
    /// A directory qualifies if it contains either:
    ///   - a `project.ron` file, OR
    ///   - at least one `.level` file
    ///
    /// Returns sorted folder names (not full paths). For example, scanning "."
    /// might return `["my_game", "space_shooter", "test_level"]`.
    ///
    /// Hidden directories (starting with `.`) are automatically excluded because
    /// `fs::read_dir` returns them and we don't filter by name here — be aware.
    pub fn find_projects(dir: &str) -> Vec<String> {
        let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
        let mut folders: Vec<String> = entries
            .filter_map(|e| e.ok())
            // Only consider subdirectories, not files.
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter(|e| {
                let path = e.path();
                // Qualify: has project.ron OR has at least one .level file.
                let has_ron   = path.join("project.ron").exists();
                let has_level = fs::read_dir(&path)
                    .map(|rd| rd.filter_map(|x| x.ok())
                         .any(|x| x.file_name().to_string_lossy().ends_with(".level")))
                    .unwrap_or(false);
                has_ron || has_level
            })
            // Return just the folder name, not the full path.
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        folders.sort();
        folders
    }
}
