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

use serde::{Deserialize, Serialize};
use std::fs;

// ── ProjectData ───────────────────────────────────────────────────────────────

// There is no "visual style" setting (ASCII vs sprites) any more: a level
// mixes glyphs and sprites freely, so the choice never selected anything —
// all it did was pick a new project's starting `world_cell`. It was removed
// after Step 9.5-5 (the New Project wizard's style step, the Project
// Settings row and this field with it). A `project.ron` that still has a
// `visual_style:` line loads fine: serde skips a key the struct doesn't
// have (`an_old_project_ron_with_visual_style_still_loads`). A sprite game
// sets its square cell in Project Settings (World cell, Pixels per unit).

/// The core gameplay execution model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum GameplayLoop {
    /// Standard 60 FPS update/render loop.
    RealTime,
    /// Logic only updates when the player or events trigger it.
    TurnBased,
}

/// Metadata stored in a project's `project.ron` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    /// Human-readable project name (e.g. "My Platformer").
    pub name: String,

    /// Whether the game is real-time or turn-based.
    #[serde(default = "default_gameplay_loop")]
    pub gameplay_loop: GameplayLoop,

    /// The default level to load when opening this project.
    #[serde(default = "default_start_level")]
    pub start_level: Option<String>,

    /// Source-texture pixels per world unit, for a `Sprite` whose `size` is
    /// `None` ("natural size" — Phase 3, docs/ember2d-refactor-plan.md).
    /// Defaults to 8 (the font atlas's cell width in pixels), so an 8x8
    /// pixel-art sprite sized to match the ASCII grid occupies exactly one
    /// world unit — the same footprint a glyph would.
    #[serde(default = "default_pixels_per_unit")]
    pub pixels_per_unit: f32,

    /// Step 7.5-7 (docs/ember2d-master-plan.md §5.6): which `TurnModel`
    /// `Simulation::run_actor_turn`'s cost fallback uses when a script
    /// doesn't call `ctx.act(cost)` itself — see that type's own doc
    /// comment (`ember2d_sim::scheduler`). `#[serde(default)]` (not a named
    /// default fn — `TurnModel` derives its own `Default`, `Alternating`)
    /// reads `Alternating` for every pre-7.5-7 `project.ron`, matching the
    /// only behavior that ever existed before this step.
    #[serde(default)]
    pub turn_model: ember2d_sim::scheduler::TurnModel,

    /// Step 9-5 (docs/ember2d-master-plan.md §5.8): the size of one WORLD
    /// cell in logical pixels, (width, height). Defaults to the glyph cell
    /// (8×16), which is every project before this step — so an ASCII
    /// project changes nothing. A sprite project whose art is square sets
    /// it square (the RPG demo: 16×16), and its tiles, sprites and world
    /// glyphs then draw square. Only the world stretches: the HUD, menus,
    /// dialogue and every script screen coordinate stay on the 8×16 glyph
    /// grid. Each axis should be a whole multiple of the glyph cell's
    /// (8 and 16) for crisp glyphs; `cell_scale` accepts anything positive.
    #[serde(default = "default_world_cell")]
    pub world_cell: (u32, u32),

    /// Step 9.5-3: how many turns one simulation step may resolve while
    /// the next actor due is an AI one (`Simulation::
    /// set_ai_turns_per_step`). 1 — every project before this step — is one
    /// actor per step; a turn-based game with many monsters sets it high
    /// (the roguelike: 256) so they all answer the player's move at once.
    #[serde(default = "default_ai_turns_per_step")]
    pub ai_turns_per_step: u32,
}

fn default_ai_turns_per_step() -> u32 {
    1
}

/// Play-mode settings a project carries, bundled — Step 9-5: these used to
/// travel as separate arguments (loop, pixels per unit, turn model) through
/// every launch path in `ember2d-app`, and `world_cell` would have made a
/// fourth. `Default` is what a level with no `project.ron` gets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaySettings {
    pub gameplay_loop: GameplayLoop,
    pub pixels_per_unit: f32,
    pub turn_model: ember2d_sim::scheduler::TurnModel,
    pub world_cell: (u32, u32),
    pub ai_turns_per_step: u32, // Step 9.5-3
}

impl Default for PlaySettings {
    fn default() -> Self {
        PlaySettings {
            gameplay_loop: GameplayLoop::RealTime,
            pixels_per_unit: default_pixels_per_unit(),
            turn_model: Default::default(),
            world_cell: default_world_cell(),
            ai_turns_per_step: default_ai_turns_per_step(),
        }
    }
}

impl PlaySettings {
    /// Screen (glyph) cells one world unit spans at zoom 1, per axis —
    /// `world_cell / (CELL_W, CELL_H)`. A zero or absurd size falls back to
    /// the glyph cell rather than dividing by zero or vanishing.
    pub fn cell_scale(&self) -> (f32, f32) {
        let axis = |v: u32, glyph: usize| -> f32 {
            if v == 0 || v > 1024 {
                1.0
            } else {
                v as f32 / glyph as f32
            }
        };
        (
            axis(self.world_cell.0, crate::renderer::CELL_W),
            axis(self.world_cell.1, crate::renderer::CELL_H),
        )
    }

    /// The world cell in logical pixels after `cell_scale`'s validation —
    /// what the editor canvas sizes a level cell with.
    pub fn world_cell_px(&self) -> (f32, f32) {
        let (kx, ky) = self.cell_scale();
        (kx * crate::renderer::CELL_W as f32, ky * crate::renderer::CELL_H as f32)
    }
}

fn default_gameplay_loop() -> GameplayLoop {
    GameplayLoop::RealTime
}
fn default_start_level() -> Option<String> {
    Some("main.level".to_string())
}
// pub so both PlayState (play.rs, same crate) and main.rs (the binary
// crate) can use this exact constant as their own fallback default, instead
// of a second hardcoded "8.0" drifting from this one.
pub fn default_pixels_per_unit() -> f32 {
    8.0
}
/// The glyph cell, 8×16 — see `ProjectData::world_cell`.
pub fn default_world_cell() -> (u32, u32) {
    (crate::renderer::CELL_W as u32, crate::renderer::CELL_H as u32)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StartTemplate {
    Empty,
    BasicRoom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartResult {
    pub project_folder: String,
    pub project_name: String,
    pub level_path: String,
    pub template: Option<StartTemplate>,
    pub gameplay_loop: GameplayLoop,
}

impl ProjectData {
    /// Create a new ProjectData with the given settings. Every new project
    /// starts on the glyph cell (8x16) and 8 pixels per unit; a sprite game
    /// whose art is square sets both in Project Settings (16x16 and 16 for
    /// 16 px tiles — the RPG tutorial's first step).
    pub fn new(name: impl Into<String>, gameplay_loop: GameplayLoop) -> Self {
        ProjectData {
            name: name.into(),
            gameplay_loop,
            start_level: Some("main.level".to_string()),
            pixels_per_unit: default_pixels_per_unit(),
            turn_model: ember2d_sim::scheduler::TurnModel::default(),
            world_cell: default_world_cell(),
            // A new turn-based game answers the player's move with every
            // monster at once (Step 9.5-3); a `project.ron` without the key
            // keeps 1, one actor per step, as it always ran.
            ai_turns_per_step: match gameplay_loop {
                GameplayLoop::TurnBased => 256,
                GameplayLoop::RealTime => default_ai_turns_per_step(),
            },
        }
    }

    /// This project's play-mode settings (Step 9-5).
    pub fn play_settings(&self) -> PlaySettings {
        PlaySettings {
            gameplay_loop: self.gameplay_loop,
            pixels_per_unit: self.pixels_per_unit,
            turn_model: self.turn_model,
            world_cell: self.world_cell,
            ai_turns_per_step: self.ai_turns_per_step,
        }
    }

    /// Write a `project.ron` file into the given folder.
    ///
    /// Creates (or overwrites) `<folder>/project.ron` with the serialized data.
    /// Returns an error if the folder doesn't exist or can't be written.
    pub fn save(&self, folder: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = format!("{}/project.ron", folder);
        let config = ron::ser::PrettyConfig::new().depth_limit(2).new_line("\n".to_string());
        let text = ron::ser::to_string_pretty(self, config)?;
        fs::write(path, text)?;
        Ok(())
    }

    /// Read and parse `<folder>/project.ron`.
    ///
    /// Returns Err if the file is missing or malformed. The caller can then
    /// fall back to using the folder name (see `name_for()` below).
    pub fn load(folder: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let path = format!("{}/project.ron", folder);
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
        ProjectData::load(folder).map(|p| p.name).unwrap_or_else(|_| {
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
                let has_ron = path.join("project.ron").exists();
                let has_level = fs::read_dir(&path)
                    .map(|rd| {
                        rd.filter_map(|x| x.ok())
                            .any(|x| x.file_name().to_string_lossy().ends_with(".level"))
                    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Step 9-5: a `project.ron` written before `world_cell` existed loads
    /// with the glyph cell, so every existing project draws as before.
    #[test]
    fn an_old_project_ron_gets_the_glyph_world_cell() {
        let p: ProjectData =
            ron::de::from_str("(name: \"Old\", visual_style: ClassicASCII, gameplay_loop: TurnBased)")
                .expect("an old project.ron parses");
        assert_eq!(p.world_cell, (8, 16));
        assert_eq!(p.play_settings().cell_scale(), (1.0, 1.0));
        assert_eq!(p.play_settings().gameplay_loop, GameplayLoop::TurnBased);
    }

    #[test]
    fn a_square_world_cell_scales_the_world_two_to_one() {
        let s = PlaySettings { world_cell: (16, 16), ..Default::default() };
        assert_eq!(s.cell_scale(), (2.0, 1.0));
        let s = PlaySettings { world_cell: (32, 32), ..Default::default() };
        assert_eq!(s.cell_scale(), (4.0, 2.0));
    }

    #[test]
    fn a_zero_or_absurd_world_cell_falls_back_to_the_glyph_cell() {
        let s = PlaySettings { world_cell: (0, 5000), ..Default::default() };
        assert_eq!(s.cell_scale(), (1.0, 1.0));
    }

    /// A new project starts on the glyph cell, whatever its loop; a
    /// turn-based one answers the player with every monster at once.
    #[test]
    fn a_new_project_starts_on_the_glyph_cell() {
        let p = ProjectData::new("A", GameplayLoop::RealTime);
        assert_eq!(p.world_cell, (8, 16));
        assert_eq!(p.pixels_per_unit, 8.0);
        assert_eq!(p.ai_turns_per_step, 1);
        let p = ProjectData::new("T", GameplayLoop::TurnBased);
        assert_eq!(p.world_cell, (8, 16));
        assert_eq!(p.ai_turns_per_step, 256);
    }

    /// The visual-style option's removal: a `project.ron` written while it
    /// existed (every demo's, and every project made before) still loads,
    /// keeping what it set — the stale `visual_style` key is just skipped.
    #[test]
    fn an_old_project_ron_with_visual_style_still_loads() {
        let p: ProjectData = ron::de::from_str(
            "(name: \"Sq\", visual_style: Sprites2D, gameplay_loop: TurnBased, world_cell: (16, 16), pixels_per_unit: 16.0)",
        )
        .expect("a project.ron with visual_style parses");
        assert_eq!(p.name, "Sq");
        assert_eq!(p.world_cell, (16, 16));
        assert_eq!(p.pixels_per_unit, 16.0);
        assert_eq!(p.gameplay_loop, GameplayLoop::TurnBased);
    }
}
