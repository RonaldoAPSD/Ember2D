// editor/input/shortcuts.rs — Keyboard shortcuts for level editor.

use super::super::commands::Command;
use super::super::panel::PanelId;
use super::super::ui::ToolKind;
use super::super::EditorMode;
use super::super::EditorState;
use super::super::TextInputPurpose;
use ember2d::input::Key;

impl EditorState {
    pub(super) fn handle_shortcuts(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        // R14 (7A-2, docs/ember2d-master-plan.md): global shortcuts only
        // apply when nothing else owns the keyboard — see `EditorFocus`'s
        // own doc comment. Without this, the docked script panel having
        // focus never stopped this function from running at all.
        if self.focus() != super::super::EditorFocus::Canvas {
            return;
        }

        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let ctrl = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);

        // ── Ctrl+Z / Ctrl+Y ──────────────────────────────────────────────────
        if ctrl {
            if input.just_pressed(Key::Z) {
                if let Some(cmd) = self.undo.pop_undo() {
                    self.reverse_command(&cmd);
                    self.unsaved = true;
                }
                return;
            }
            if input.just_pressed(Key::Y) {
                if let Some(cmd) = self.undo.pop_redo() {
                    self.apply_command(&cmd);
                    self.unsaved = true;
                }
                return;
            }
        }

        // ── Layer switching (1, 2, 3) ──────────────────────────────────────────
        if input.just_pressed(Key::Key1) {
            self.active_layer = 0;
            self.ignore_drag = true;
            self.save_message = Some("LAYER: Background".to_string());
            self.save_message_timer = 0;
        }
        if input.just_pressed(Key::Key2) {
            self.active_layer = 1;
            self.ignore_drag = true;
            self.save_message = Some("LAYER: Main".to_string());
            self.save_message_timer = 0;
        }
        if input.just_pressed(Key::Key3) {
            self.active_layer = 2;
            self.ignore_drag = true;
            self.save_message = Some("LAYER: Foreground".to_string());
            self.save_message_timer = 0;
        }

        // ── Palette keys 4–0 ─────────────────────────────────────────────────
        let palette_keys = [
            (Key::Key4, 1),
            (Key::Key5, 2),
            (Key::Key6, 3),
            (Key::Key7, 4),
            (Key::Key8, 5),
            (Key::Key9, 6),
            (Key::Key0, 7),
        ];
        for (key, num) in &palette_keys {
            if input.just_pressed(*key) {
                self.palette.select_by_key(*num);
            }
        }

        // ── Keyboard shortcuts ────────────────────────────────────────────────

        if input.just_pressed(Key::Escape) {
            if self.show_help {
                self.show_help = false;
                return;
            }
            if self.rect_anchor.is_some() {
                self.rect_anchor = None;
                self.mode = EditorMode::Paint(ToolKind::Paint);
            } else if self.line_anchor.is_some() {
                self.line_anchor = None;
                self.mode = EditorMode::Paint(ToolKind::Paint);
            }
            return;
        }

        // F5 / Shift+F5 — Preview / Preview from cursor.
        if input.just_pressed(Key::F5) {
            let mut data = self.grid.to_level_data();
            data.path = self.save_path.clone();
            if shift {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                    data.spawn_point = (gx as f32, gy as f32);
                }
            }
            self.pending_transition = Some(ember2d::engine::Transition::ToPlay(data));
            return;
        }

        // S — save / Shift+S — save-as.
        if input.just_pressed(Key::S) {
            if shift {
                self.prompt_buffer = self.save_path.clone();
                self.mode = EditorMode::Prompt(TextInputPurpose::SaveAs);
                return;
            }
            self.save();
        }

        if input.just_pressed(Key::G) {
            self.show_physics = !self.show_physics;
        }
        if input.just_pressed(Key::B) {
            self.panels.toggle(PanelId::Palette);
        }
        if input.just_pressed(Key::H) {
            self.panels.toggle(PanelId::Hierarchy);
        }
        if input.just_pressed(Key::Tab) {
            self.show_grid = !self.show_grid;
        }

        // Home — center view on player spawn point and reset zoom.
        if input.just_pressed(Key::Home) {
            let (sx, sy) = self.grid.spawn_point;
            self.center_on(sx as i32, sy as i32);
            self.zoom = 1.0;
        }

        // ? (Shift+Slash) — toggle help screen.
        if input.just_pressed(Key::Slash) && shift {
            self.show_help = !self.show_help;
            return;
        }

        // Delete — erase tile under cursor.
        if input.just_pressed(Key::Delete) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                self.erase_brush(gx, gy);
            }
            return;
        }

        // Q — toggle select mode (click to inspect instead of paint).
        if input.just_pressed(Key::Q) {
            if matches!(self.mode, EditorMode::Inspect) {
                self.selected_pos = None;
                self.mode = EditorMode::Paint(ToolKind::Paint);
            } else {
                self.mode = EditorMode::Inspect;
            }
        }

        if input.just_pressed(Key::Backquote) {
            self.panels.toggle(PanelId::Stats);
        }

        // U — undo, R — redo.
        if input.just_pressed(Key::U) {
            if let Some(cmd) = self.undo.pop_undo() {
                self.reverse_command(&cmd);
                self.unsaved = true;
            }
        }
        if input.just_pressed(Key::R) {
            if let Some(cmd) = self.undo.pop_redo() {
                self.apply_command(&cmd);
                self.unsaved = true;
            }
        }

        // E — cycle eraser size.
        if input.just_pressed(Key::E) {
            self.erase_size = match self.erase_size {
                1 => 3,
                3 => 5,
                _ => 1,
            };
        }

        // P / Shift+P — spawn points.
        if input.just_pressed(Key::P) {
            if shift {
                self.prompt_buffer.clear();
                self.mode = EditorMode::Prompt(TextInputPurpose::NamedSpawn);
            } else {
                self.mode = EditorMode::PlaceSpawn(None);
                self.save_message =
                    Some("Click on grid to place spawn. Esc to cancel.".to_string());
                self.save_message_timer = 0;
            }
            return;
        }

        // N — rename.
        if input.just_pressed(Key::N) {
            self.prompt_buffer = self.grid.name.clone();
            self.mode = EditorMode::Prompt(TextInputPurpose::LevelName);
            return;
        }

        // T — script attachment.
        if input.just_pressed(Key::T) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                if let Some(tile) = self.grid.get(gx, gy, self.active_layer) {
                    self.prompt_buffer = tile.script.clone().unwrap_or_default();
                    self.mode = EditorMode::Prompt(TextInputPurpose::ScriptPath { gx, gy });
                    return;
                }
            }
        }

        // D — set exit destination (next_level path) on tile under cursor.
        if input.just_pressed(Key::D) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                if let Some(tile) = self.grid.get(gx, gy, self.active_layer) {
                    self.prompt_buffer = tile.next_level.clone().unwrap_or_default();
                    self.mode = EditorMode::Prompt(TextInputPurpose::TileNextLevel { gx, gy });
                    return;
                }
            }
        }

        // I — edit tile tag.
        if input.just_pressed(Key::I) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                if let Some(tile) = self.grid.get(gx, gy, self.active_layer) {
                    self.prompt_buffer = tile.tag.clone();
                    self.mode = EditorMode::Prompt(TextInputPurpose::TileTag { gx, gy });
                    return;
                }
            }
        }

        // ; — toggle solid, ' — toggle trigger on tile under cursor.
        if input.just_pressed(Key::Semicolon) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                let lyr = self.active_layer;
                if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                    let mut new_tile = tile.clone();
                    new_tile.solid = !new_tile.solid;
                    self.undo.push(Command::Batch {
                        cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                    });
                    self.grid.place(gx, gy, lyr, new_tile);
                    self.unsaved = true;
                }
            }
        }
        if input.just_pressed(Key::Apostrophe) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                let lyr = self.active_layer;
                if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
                    let mut new_tile = tile.clone();
                    new_tile.trigger = !new_tile.trigger;
                    self.undo.push(Command::Batch {
                        cells: vec![(gx, gy, lyr, Some(tile), Some(new_tile.clone()))],
                    });
                    self.grid.place(gx, gy, lyr, new_tile);
                    self.unsaved = true;
                }
            }
        }

        // F — flood fill.
        if input.just_pressed(Key::F) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                self.flood_fill(gx, gy);
            }
        }

        // L — line tool (anchor on first press, stamp on second).
        if input.just_pressed(Key::L) {
            if self.line_anchor.is_none() {
                if let Some(pos) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                    self.line_anchor = Some(pos);
                    self.mode = EditorMode::Paint(ToolKind::Line);
                }
            } else {
                if let Some(end) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                    self.stamp_line(self.line_anchor.unwrap(), end);
                }
                self.line_anchor = None;
                self.mode = EditorMode::Paint(ToolKind::Paint);
            }
            return;
        }

        // C — copy-select, X — cut-select.
        if input.just_pressed(Key::C) {
            self.mode = EditorMode::Select { start: None, cutting: false };
            return;
        }
        if input.just_pressed(Key::X) {
            self.mode = EditorMode::Select { start: None, cutting: true };
            return;
        }

        // V — paste.
        if input.just_pressed(Key::V) && !self.clipboard.is_empty() {
            self.mode = EditorMode::Paste;
            return;
        }

        // O — open file browser.
        // Redirect 'O' to toggle Content Browser
        if input.just_pressed(Key::O) {
            self.dispatch_toolbar_action(crate::editor::ui::ToolbarAction::ToggleFileBrowser);
            return;
        }

        // Z — resize level.
        if input.just_pressed(Key::Z) {
            self.prompt_buffer.clear();
            self.mode = EditorMode::Prompt(TextInputPurpose::ResizeLevel);
        }
    }
}
