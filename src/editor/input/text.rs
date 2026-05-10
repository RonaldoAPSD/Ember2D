// editor/input/text.rs — Text input handling for level editor.

use crate::input::Key;
use super::super::EditorState;
use super::super::{key_to_char, TEXT_INPUT_KEYS, DEFAULT_LEVEL_W, DEFAULT_LEVEL_H};
use super::super::TextInputPurpose;
use super::super::commands::Command;
use super::super::commands::UndoStack;

impl EditorState {
    pub(super) fn handle_text_input(&mut self, input: &crate::input::InputManager) {
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        
        if let Some(ref mut ti) = self.text_input {
            for &key in TEXT_INPUT_KEYS {
                if input.just_pressed(key) {
                    if let Some(ch) = key_to_char(key, shift) { ti.buffer.push(ch); }
                }
            }
            if input.just_pressed(Key::Backspace) { ti.buffer.pop(); }

            if input.just_pressed(Key::Enter) {
                let ti = self.text_input.take().unwrap();
                match ti.purpose {
                    TextInputPurpose::LevelName => {
                        if !ti.buffer.is_empty() { self.grid.name = ti.buffer; self.unsaved = true; }
                    }
                    TextInputPurpose::SaveAs => {
                        if !ti.buffer.is_empty() { self.save_path = ti.buffer; self.save(); }
                    }
                    TextInputPurpose::ScriptPath { gx, gy } => {
                        let lyr = self.active_layer;
                        if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                            let mut new_tile = tile.clone();
                            new_tile.script = if ti.buffer.is_empty() { None } else { Some(ti.buffer) };
                            self.undo.push(Command::Batch { cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))] });
                            self.grid.place(gx, gy, lyr, new_tile);
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::TileNextLevel { gx, gy } => {
                        let lyr = self.active_layer;
                        if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                            let mut new_tile = tile.clone();
                            new_tile.next_level = if ti.buffer.is_empty() { None } else { Some(ti.buffer) };
                            self.undo.push(Command::Batch { cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))] });
                            self.grid.place(gx, gy, lyr, new_tile);
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::TileTag { gx, gy } => {
                        if gx == -1 {
                            let sel = self.palette.selected;
                            self.palette.tiles[sel].tag = ti.buffer;
                            self.unsaved = true;
                        } else {
                            let lyr = self.active_layer;
                            if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                                let mut new_tile = tile.clone();
                                new_tile.tag = ti.buffer;
                                self.undo.push(Command::Batch { cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))] });
                                self.grid.place(gx, gy, lyr, new_tile);
                                self.unsaved = true;
                            }
                        }
                    }
                    TextInputPurpose::PaletteName => {
                        let sel = self.palette.selected;
                        self.palette.tiles[sel].name = ti.buffer;
                        self.unsaved = true;
                    }
                    TextInputPurpose::NamedSpawn => {
                        if !ti.buffer.is_empty() {
                            self.placing_named_spawn = Some(ti.buffer);
                            self.save_message = Some("Click on grid to place named spawn. Esc to cancel.".to_string());
                            self.save_message_timer = 0;
                        }
                    }
                    TextInputPurpose::ResizeLevel => {
                        let s = ti.buffer.replace('x', " ").replace('X', " ").replace(',', " ");
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
                                self.save_message = Some("Invalid format — enter WxH e.g. 40x20".to_string());
                                self.save_message_timer = 0;
                            }
                        }
                    }
                    TextInputPurpose::PlayerTag => {
                        let before = self.grid.player.clone();
                        let mut after = before.clone();
                        after.tag = ti.buffer;
                        self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                        self.grid.player = after;
                        self.unsaved = true;
                    }
                    TextInputPurpose::PlayerScript => {
                        let before = self.grid.player.clone();
                        let mut after = before.clone();
                        after.script = if ti.buffer.is_empty() { None } else { Some(ti.buffer) };
                        self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                        self.grid.player = after;
                        self.unsaved = true;
                    }
                    TextInputPurpose::TileGlyph { gx, gy } => {
                        if let Some(ch) = ti.buffer.chars().next() {
                            if gx == -1 {
                                let sel = self.palette.selected;
                                self.palette.tiles[sel].glyph = ch;
                                self.unsaved = true;
                            } else {
                                let lyr = self.active_layer;
                                if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.glyph = ch;
                                    self.undo.push(Command::Batch { cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))] });
                                    self.grid.place(gx, gy, lyr, new_tile);
                                    self.unsaved = true;
                                }
                            }
                        }
                    }
                    TextInputPurpose::PlayerGlyph => {
                        if let Some(ch) = ti.buffer.chars().next() {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.glyph = ch;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::NewLevelName => {
                        let name = ti.buffer.trim().to_string();
                        if !name.is_empty() {
                            let file_name = format!("{}.level", name);
                            let path = if let Some(ref folder) = self.project_folder {
                                format!("{}/{}", folder, file_name)
                            } else {
                                file_name
                            };
                            self.save();
                            let mut new_grid = crate::editor::grid::LevelGrid::new(
                                DEFAULT_LEVEL_W, DEFAULT_LEVEL_H,
                            );
                            new_grid.name = name.clone();
                            match new_grid.to_level_data().save(&path) {
                                Ok(()) => {
                                    self.grid      = new_grid;
                                    self.undo      = UndoStack::new();
                                    self.save_path = path.clone();
                                    self.unsaved   = false;
                                    self.save_message = Some(format!("New level: {}", path));
                                    self.save_message_timer = 0;
                                }
                                Err(e) => {
                                    self.save_message = Some(format!("Error: {}", e));
                                    self.save_message_timer = 0;
                                }
                            }
                        }
                    }
                    TextInputPurpose::NewFolderName => {}
                }
                return;
            }

            if input.just_pressed(Key::Escape) { self.text_input = None; }
        }
    }
}
