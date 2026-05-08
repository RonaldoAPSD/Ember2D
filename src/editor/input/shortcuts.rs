// editor/input/shortcuts.rs — Keyboard shortcuts for level editor.

use crate::input::Key;
use super::super::EditorState;
use super::super::{TextInput, TextInputPurpose};
use super::super::ui::ToolKind;
use super::super::panel::PanelId;
use super::super::commands::Command;

impl EditorState {
    pub(super) fn handle_shortcuts(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState) {
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let ctrl  = input.is_held(Key::LeftCtrl)  || input.is_held(Key::RightCtrl);

        // ── Ctrl+Z / Ctrl+Y ──────────────────────────────────────────────────
        if ctrl {
            if input.just_pressed(Key::Z) {
                if let Some(cmd) = self.undo.pop_undo() { self.reverse_command(&cmd); self.unsaved = true; }
                return;
            }
            if input.just_pressed(Key::Y) {
                if let Some(cmd) = self.undo.pop_redo() { self.apply_command(&cmd); self.unsaved = true; }
                return;
            }
        }

        // ── Palette keys 1–9 ─────────────────────────────────────────────────
        let number_keys = [
            (Key::Key1,1),(Key::Key2,2),(Key::Key3,3),(Key::Key4,4),(Key::Key5,5),
            (Key::Key6,6),(Key::Key7,7),(Key::Key8,8),(Key::Key9,9),
        ];
        for (key, num) in &number_keys {
            if input.just_pressed(*key) { self.palette.select_by_key(*num); }
        }

        // ── Keyboard shortcuts ────────────────────────────────────────────────

        if input.just_pressed(Key::Escape) {
            if self.show_help {
                self.show_help = false;
                return;
            }
            if self.rect_anchor.is_some() {
                self.rect_anchor = None;
                self.active_tool = ToolKind::Paint;
            } else if self.line_anchor.is_some() {
                self.line_anchor = None;
                self.active_tool = ToolKind::Paint;
            }
            return;
        }

        // F5 / Shift+F5 — Preview / Preview from cursor.
        if input.just_pressed(Key::F5) {
            let mut data = self.grid.to_level_data();
            data.path = self.save_path.clone();
            if shift {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    data.spawn_point = (gx as f32, gy as f32);
                }
            }
            self.pending_transition = Some(crate::engine::Transition::ToPlay(data));
            return;
        }

        // S — save / Shift+S — save-as.
        if input.just_pressed(Key::S) {
            if shift {
                self.text_input = Some(TextInput {
                    buffer:  self.save_path.clone(),
                    purpose: TextInputPurpose::SaveAs,
                });
                return;
            }
            self.save();
        }

        if input.just_pressed(Key::Tab) { self.show_grid = !self.show_grid; }
        if input.just_pressed(Key::B)   { self.panels.toggle(PanelId::Palette); }
        if input.just_pressed(Key::G)   { self.show_physics = !self.show_physics; }

        // Home — center view on player spawn point.
        if input.just_pressed(Key::Home) {
            let (sx, sy) = self.grid.spawn_point;
            self.center_on(sx as i32, sy as i32);
        }

        // ? (Shift+Slash) — toggle help screen.
        if input.just_pressed(Key::Slash) && shift {
            self.show_help = !self.show_help;
            return;
        }

        // Delete — erase tile under cursor.
        if input.just_pressed(Key::Delete) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.erase_brush(gx, gy);
            }
            return;
        }

        // Q — toggle select mode (click to inspect instead of paint).
        if input.just_pressed(Key::Q) {
            self.select_mode = !self.select_mode;
            if !self.select_mode {
                self.selected_pos = None;
                self.active_tool  = ToolKind::Paint;
            } else {
                self.active_tool = ToolKind::Select;
            }
        }

        if input.just_pressed(Key::Backquote)  { self.panels.toggle(PanelId::Stats); }

        // U — undo, R — redo.
        if input.just_pressed(Key::U) {
            if let Some(cmd) = self.undo.pop_undo() { self.reverse_command(&cmd); self.unsaved = true; }
        }
        if input.just_pressed(Key::R) {
            if let Some(cmd) = self.undo.pop_redo() { self.apply_command(&cmd); self.unsaved = true; }
        }

        // E — cycle eraser size.
        if input.just_pressed(Key::E) {
            self.erase_size = match self.erase_size { 1 => 3, 3 => 5, _ => 1 };
        }

        // P / Shift+P — spawn points.
        if input.just_pressed(Key::P) {
            if shift {
                self.text_input = Some(TextInput {
                    buffer:  String::new(),
                    purpose: TextInputPurpose::NamedSpawn,
                });
            } else {
                self.placing_spawn = true;
                self.save_message = Some("Click on grid to place spawn. Esc to cancel.".to_string());
                self.save_message_timer = 0;
            }
            return;
        }

        // N — rename.
        if input.just_pressed(Key::N) {
            self.text_input = Some(TextInput { buffer: self.grid.name.clone(), purpose: TextInputPurpose::LevelName });
            return;
        }

        // T — script attachment.
        if input.just_pressed(Key::T) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy) {
                    let existing = tile.script.clone().unwrap_or_default();
                    self.text_input = Some(TextInput { buffer: existing, purpose: TextInputPurpose::ScriptPath { gx, gy } });
                    return;
                }
            }
        }

        // D — set exit destination (next_level path) on tile under cursor.
        if input.just_pressed(Key::D) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy) {
                    let existing = tile.next_level.clone().unwrap_or_default();
                    self.text_input = Some(TextInput { buffer: existing, purpose: TextInputPurpose::TileNextLevel { gx, gy } });
                    return;
                }
            }
        }

        // I — edit tile tag.
        if input.just_pressed(Key::I) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy) {
                    self.text_input = Some(TextInput { buffer: tile.tag.clone(), purpose: TextInputPurpose::TileTag { gx, gy } });
                    return;
                }
            }
        }

        // ; — toggle solid, ' — toggle trigger on tile under cursor.
        if input.just_pressed(Key::Semicolon) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    new_tile.solid = !new_tile.solid;
                    self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
        }
        if input.just_pressed(Key::Apostrophe) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    new_tile.trigger = !new_tile.trigger;
                    self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
        }

        // F — flood fill.
        if input.just_pressed(Key::F) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.flood_fill(gx, gy);
            }
        }

        // L — line tool (anchor on first press, stamp on second).
        if input.just_pressed(Key::L) {
            if self.line_anchor.is_none() {
                if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.line_anchor = Some(pos);
                    self.active_tool = ToolKind::Line;
                }
            } else {
                if let Some(end) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.stamp_line(self.line_anchor.unwrap(), end);
                }
                self.line_anchor = None;
                self.active_tool = ToolKind::Paint;
            }
            return;
        }

        // C — copy-select, X — cut-select.
        if input.just_pressed(Key::C) {
            self.selecting = true; self.sel_anchor = None;
            self.active_tool = ToolKind::Copy;
            return;
        }
        if input.just_pressed(Key::X) {
            self.cutting = true; self.sel_anchor = None;
            self.active_tool = ToolKind::Cut;
            return;
        }

        // V — paste.
        if input.just_pressed(Key::V) && !self.clipboard.is_empty() {
            self.pasting = true;
            self.active_tool = ToolKind::Paste;
            return;
        }

        // O — open file browser.
        if input.just_pressed(Key::O) {
            self.refresh_file_list();
            self.browsing = true;
            return;
        }

        // Z — resize level.
        if input.just_pressed(Key::Z) {
            self.text_input = Some(TextInput {
                buffer:  String::new(),
                purpose: TextInputPurpose::ResizeLevel,
            });
            return;
        }
    }
}
