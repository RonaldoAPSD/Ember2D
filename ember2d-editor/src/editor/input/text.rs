// editor/input/text.rs — Text input handling for level editor.

use super::super::commands::Command;
use super::super::commands::UndoStack;
use super::super::ui::ToolKind;
use super::super::EditorMode;
use super::super::EditorState;
use super::super::TextInputPurpose;
use super::super::{DEFAULT_LEVEL_H, DEFAULT_LEVEL_W};
use ember2d::input::Key;
use ember2d_sim::scripting::LogEntry;

impl EditorState {
    /// 7C-4 (master plan §5.3): `purpose` comes in owned from `mode` (moved
    /// out by `handle_update`'s `mem::take`) instead of being read from the
    /// deleted `text_input: Option<TextInput>` — the buffer it goes with
    /// stays its own field, `prompt_buffer` (see that field's own doc
    /// comment for why). Every arm below is unchanged from the removed
    /// `TextInput`'s own `ti.purpose`/`ti.buffer`, except `NamedSpawn`,
    /// which used to write `self.placing_named_spawn` directly and now
    /// transitions `mode` to `PlaceSpawn` instead.
    pub(super) fn handle_text_input(
        &mut self,
        purpose: TextInputPurpose,
        input: &mut ember2d::input::InputManager,
    ) {
        self.mode = EditorMode::Prompt(purpose);
        // R12 (7A-2, docs/ember2d-master-plan.md): must be renewed every
        // frame this prompt stays focused — see `begin_text_capture`'s
        // own doc comment (ember2d/src/input.rs).
        input.begin_text_capture();
        // Use the engine's captured text characters first (handles Shift, AltGr, Symbols correctly)
        let captured = input.take_text();
        for ch in captured.chars() {
            self.prompt_buffer.push(ch);
        }

        if input.just_pressed(Key::Backspace) {
            self.prompt_buffer.pop();
        }

        if input.just_pressed(Key::Enter) {
            let buffer = std::mem::take(&mut self.prompt_buffer);
            // Default outcome for every purpose below except `NamedSpawn`,
            // which overrides this with a transition into `PlaceSpawn`.
            self.mode = EditorMode::Paint(ToolKind::Paint);
            match purpose {
                TextInputPurpose::LevelName => {
                    if !buffer.is_empty() {
                        self.grid.name = buffer.clone();
                        self.unsaved = true;

                        // Found live by the user (2026-09-12): renaming a
                        // level only ever updated `grid.name` (the
                        // in-memory/title-bar display name) — the file on
                        // disk, and `save_path`, kept the old name
                        // forever, so the File Browser and the level's
                        // own displayed name silently drifted apart. If a
                        // file already exists at the old `save_path`,
                        // rename it alongside; either way, `save_path`
                        // itself always moves to match so the next save
                        // lands at the new name instead of the old one.
                        if let Some(parent) = std::path::Path::new(&self.save_path).parent() {
                            let new_path = parent
                                .join(format!("{}.level", buffer))
                                .to_string_lossy()
                                .into_owned();
                            if new_path != self.save_path {
                                if std::path::Path::new(&self.save_path).exists() {
                                    if let Err(e) = std::fs::rename(&self.save_path, &new_path) {
                                        self.console_log.push(LogEntry::error(format!(
                                            "Failed to rename level file: {}",
                                            e
                                        )));
                                    }
                                }
                                self.save_path = new_path;
                                self.refresh_project_files();
                            }
                        }
                    }
                }
                TextInputPurpose::SaveAs => {
                    if !buffer.is_empty() {
                        self.save_path = buffer;
                        self.save();
                    }
                }
                TextInputPurpose::ScriptPath { gx, gy } => {
                    let lyr = self.active_layer;
                    if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                        let mut new_tile = tile.clone();
                        new_tile.script = if buffer.is_empty() { None } else { Some(buffer) };
                        self.undo.push(Command::Batch {
                            cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                        });
                        self.grid.place(gx, gy, lyr, new_tile);
                        self.unsaved = true;
                    }
                }
                TextInputPurpose::TileNextLevel { gx, gy } => {
                    let lyr = self.active_layer;
                    if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                        let mut new_tile = tile.clone();
                        new_tile.next_level = if buffer.is_empty() { None } else { Some(buffer) };
                        self.undo.push(Command::Batch {
                            cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                        });
                        self.grid.place(gx, gy, lyr, new_tile);
                        self.unsaved = true;
                    }
                }
                TextInputPurpose::ProjectSetting(field) => {
                    self.commit_project_setting(field, &buffer);
                }
                TextInputPurpose::Inspector { target, field } => {
                    self.commit_inspector_edit(target, field, buffer);
                }
                TextInputPurpose::TileTag { gx, gy } => {
                    if gx == -1 {
                        let sel = self.palette.selected;
                        self.palette.tiles[sel].tag = buffer;
                        self.unsaved = true;
                    } else {
                        let lyr = self.active_layer;
                        if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                            let mut new_tile = tile.clone();
                            new_tile.tag = buffer;
                            self.undo.push(Command::Batch {
                                cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                            });
                            self.grid.place(gx, gy, lyr, new_tile);
                            self.unsaved = true;
                        }
                    }
                }
                TextInputPurpose::NewScriptName => {
                    let mut name = buffer.trim().to_string();
                    if !name.is_empty() {
                        if !name.ends_with(".rhai") {
                            name.push_str(".rhai");
                        }
                        // Step 9-6: through `create_project_file`, which
                        // makes the folders a name like `scripts/ai.rhai`
                        // needs (this used to fail silently without them).
                        self.create_project_file(&name, "");
                    }
                }
                // Step 9-6 (docs/ember2d-master-plan.md §5.8): File > New
                // Scene — `scenes/<name>.rhai`, from a working template.
                TextInputPurpose::NewSceneName => {
                    let name = buffer.trim().trim_end_matches(".rhai").to_string();
                    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        self.console_log.push(LogEntry::warn(format!(
                            "New Scene: '{name}' — use letters, digits and _ (it's the name push_scene takes)"
                        )));
                    } else {
                        let text = super::super::project_settings::scene_template(&name);
                        self.create_project_file(&format!("scenes/{name}.rhai"), &text);
                    }
                }
                TextInputPurpose::PaletteName => {
                    let sel = self.palette.selected;
                    self.palette.tiles[sel].name = buffer;
                    self.unsaved = true;
                }
                TextInputPurpose::NamedSpawn => {
                    if !buffer.is_empty() {
                        self.mode = EditorMode::PlaceSpawn(Some(buffer));
                        self.save_message =
                            Some("Click on grid to place named spawn. Esc to cancel.".to_string());
                        self.save_message_timer = 0;
                    }
                }
                // Step 9.5-5: the level's collision layers. A collider's
                // layer or mask name that isn't in this list resolves to no
                // bit at all, so masks can't filter by it — a level whose
                // scripts spawn "enemy"/"bullet" colliders lists them here.
                TextInputPurpose::CollisionLayers => {
                    let mut names: Vec<String> = Vec::new();
                    // Commas or spaces between names (a layer name never
                    // has a space in it).
                    for name in buffer.split([',', ' ']).filter(|n| !n.is_empty()) {
                        if !names.iter().any(|n| n == name) && names.len() < 32 {
                            names.push(name.to_string());
                        }
                    }
                    if names != self.grid.collision_layers {
                        self.grid.collision_layers = names;
                        self.unsaved = true;
                    }
                }
                TextInputPurpose::ResizeLevel => {
                    let s = buffer.replace(['x', 'X', ','], " ");
                    let parts: Vec<&str> = s.split_whitespace().collect();
                    let parsed = if parts.len() == 2 {
                        parts[0].parse::<usize>().ok().zip(parts[1].parse::<usize>().ok())
                    } else {
                        None
                    };
                    match parsed {
                        Some((w, h)) if w >= 4 && h >= 3 => {
                            let old_w = self.grid.width;
                            let old_h = self.grid.height;
                            // Capture all tiles that might be lost
                            let mut lost_tiles = Vec::new();
                            for (&(gx, gy, _lyr), t) in &self.grid.tiles {
                                if gx < 0 || gy < 0 || gx as usize >= w || gy as usize >= h {
                                    lost_tiles.push(t.clone());
                                }
                            }
                            self.undo.push(Command::ResizeLevel {
                                before_w: old_w,
                                before_h: old_h,
                                before_tiles: lost_tiles,
                                after_w: w,
                                after_h: h,
                            });

                            self.grid.resize(w, h);
                            self.clamp_scroll();
                            self.unsaved = true;
                            self.save_message = Some(format!("Resized to {}×{}", w, h));
                            self.save_message_timer = 0;
                        }
                        Some((w, h)) => {
                            self.save_message = Some(format!("Too small: {}×{} (min 4×3)", w, h));
                            self.save_message_timer = 0;
                        }
                        None => {
                            self.save_message =
                                Some("Invalid format — enter WxH e.g. 40x20".to_string());
                            self.save_message_timer = 0;
                        }
                    }
                }
                TextInputPurpose::PlayerTag => {
                    let before = self.grid.player.clone();
                    let mut after = before.clone();
                    after.tag = buffer;
                    self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                    self.grid.player = after;
                    self.unsaved = true;
                }
                TextInputPurpose::PlayerScript => {
                    let before = self.grid.player.clone();
                    let mut after = before.clone();
                    after.script = if buffer.is_empty() { None } else { Some(buffer) };
                    self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                    self.grid.player = after;
                    self.unsaved = true;
                }
                TextInputPurpose::TileGlyph { gx, gy } => {
                    if let Some(ch) = buffer.chars().next() {
                        if gx == -1 {
                            let sel = self.palette.selected;
                            self.palette.tiles[sel].glyph = ch;
                            self.unsaved = true;
                        } else {
                            let lyr = self.active_layer;
                            if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                                let mut new_tile = tile.clone();
                                new_tile.glyph = ch;
                                self.undo.push(Command::Batch {
                                    cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                                });
                                self.grid.place(gx, gy, lyr, new_tile);
                                self.unsaved = true;
                            }
                        }
                    }
                }
                TextInputPurpose::PlayerGlyph => {
                    if let Some(ch) = buffer.chars().next() {
                        let before = self.grid.player.clone();
                        let mut after = before.clone();
                        after.glyph = ch;
                        self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                        self.grid.player = after;
                        self.unsaved = true;
                    }
                }
                TextInputPurpose::NewLevelName => {
                    let name = buffer.trim().to_string();
                    if !name.is_empty() {
                        let file_name = format!("{}.level", name);
                        let path = if let Some(ref folder) = self.project_folder {
                            format!("{}/{}", folder, file_name)
                        } else {
                            file_name
                        };
                        self.save();
                        let mut new_grid =
                            crate::editor::grid::LevelGrid::new(DEFAULT_LEVEL_W, DEFAULT_LEVEL_H);
                        new_grid.name = name.clone();
                        match new_grid.to_level_data().save(&path) {
                            Ok(()) => {
                                self.grid = new_grid;
                                self.undo = UndoStack::new();
                                self.save_path = path.clone();
                                self.unsaved = false;
                                self.save_message = Some(format!("New level: {}", path));
                                self.save_message_timer = 0;
                                // Found live by the user (2026-09-12):
                                // the new level was written to disk
                                // but never appeared in the File
                                // Browser until something else (a
                                // folder navigation) happened to
                                // refresh it — every other file-
                                // creating action (New Script,
                                // confirming a level switch, this
                                // same prompt's context-menu twin)
                                // already calls this.
                                self.refresh_project_files();
                            }
                            Err(e) => {
                                self.save_message = Some(format!("Error: {}", e));
                                self.save_message_timer = 0;
                            }
                        }
                    }
                }
                TextInputPurpose::TileColliderLayer { gx, gy } => {
                    let lyr = self.active_layer;
                    if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                        let mut new_tile = tile.clone();
                        new_tile.collider_layer = buffer;
                        self.undo.push(Command::Batch {
                            cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                        });
                        self.grid.place(gx, gy, lyr, new_tile);
                        self.unsaved = true;
                    }
                }
                TextInputPurpose::TileColliderMask { gx, gy } => {
                    let lyr = self.active_layer;
                    if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                        let mut new_tile = tile.clone();
                        new_tile.collider_mask = buffer
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        self.undo.push(Command::Batch {
                            cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                        });
                        self.grid.place(gx, gy, lyr, new_tile);
                        self.unsaved = true;
                    }
                }
                TextInputPurpose::PlayerColliderLayer => {
                    let before = self.grid.player.clone();
                    let mut after = before.clone();
                    after.collider_layer = buffer;
                    self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                    self.grid.player = after;
                    self.unsaved = true;
                }
                TextInputPurpose::PlayerColliderMask => {
                    let before = self.grid.player.clone();
                    let mut after = before.clone();
                    after.collider_mask = buffer
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                    self.grid.player = after;
                    self.unsaved = true;
                }
                TextInputPurpose::PaletteFgCustom => {
                    let hex = buffer.trim().trim_start_matches('#');
                    if hex.len() == 6 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            u8::from_str_radix(&hex[0..2], 16),
                            u8::from_str_radix(&hex[2..4], 16),
                            u8::from_str_radix(&hex[4..6], 16),
                        ) {
                            let sel = self.palette.selected;
                            self.palette.tiles[sel].fg =
                                ember2d::renderer::color::Color::Rgb(r, g, b);
                            self.unsaved = true;
                        }
                    }
                }
                TextInputPurpose::PaletteBgCustom => {
                    let hex = buffer.trim().trim_start_matches('#');
                    if hex.len() == 6 {
                        if let (Ok(r), Ok(g), Ok(b)) = (
                            u8::from_str_radix(&hex[0..2], 16),
                            u8::from_str_radix(&hex[2..4], 16),
                            u8::from_str_radix(&hex[4..6], 16),
                        ) {
                            let sel = self.palette.selected;
                            self.palette.tiles[sel].bg =
                                ember2d::renderer::color::Color::Rgb(r, g, b);
                            self.unsaved = true;
                        }
                    }
                }
            }
            return;
        }

        if input.just_pressed(Key::Escape) {
            self.prompt_buffer.clear();
            self.mode = EditorMode::Paint(ToolKind::Paint);
        }
    }
}
