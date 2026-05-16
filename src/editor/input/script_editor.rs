// editor/input/script_editor.rs — Typing logic for the in-engine script editor.

use crate::input::Key;
use super::super::EditorState;
use super::super::helpers::{key_to_char, TEXT_INPUT_KEYS};
use super::super::panel::PanelId;

impl EditorState {
    pub(super) fn handle_script_mode_input(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState) {
        if self.focused_panel != Some(PanelId::ScriptEditor) && !self.script_mode { return; }
        if self.script_path.is_none() { return; }

        let p = self.panels.get(PanelId::ScriptEditor);
        
        // 1. Resolve exact text area bounds dynamically
        let sw = self.layout.screen_w;
        let sh = self.layout.screen_h;

        let (cx, cy, cw, ch) = if self.script_mode {
            (0usize, 1usize, sw, sh.saturating_sub(2))
        } else {
            (p.content_x(), p.content_y(), p.content_w(), p.content_h())
        };

        // Header row inside the panel is cy, text starts at cy + 1
        let text_y = cy + 1;
        let text_h = ch.saturating_sub(1);
        let gutter_w = 4;

        // ── Mouse Interaction (Click & Scroll) ────────────────────────────────
        if mouse.in_bounds {
            // 1. Mouse Wheel Scroll
            if mouse.wheel_y != 0.0 {
                let delta = if mouse.wheel_y > 0.0 { -2i32 } else { 2i32 };
                let max_scroll = self.script_buffer.len().saturating_sub(text_h);
                self.script_scroll = (self.script_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                return;
            }

            // 2. Click to place cursor
            if mouse.left_just_pressed() {
                if mouse.cell_x >= cx && mouse.cell_x < cx + cw && mouse.cell_y >= text_y && mouse.cell_y < text_y + text_h {
                    let row_in_view = mouse.cell_y - text_y;
                    let target_row = self.script_scroll + row_in_view;
                    
                    if target_row < self.script_buffer.len() {
                        self.script_cursor.1 = target_row;
                        let line_start_x = cx + gutter_w;
                        let col = mouse.cell_x as i32 - line_start_x as i32;
                        self.script_cursor.0 = (col.max(0) as usize).min(self.script_buffer[target_row].len());
                        return;
                    }
                }
            }
        }

        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let ctrl  = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);

        let old_cursor = self.script_cursor;

        // ── Navigation ────────────────────────────────────────────────────────
        if input.just_pressed(Key::Escape) {
            self.script_mode = false;
            return;
        }

        if input.just_pressed(Key::Up) {
            if self.script_cursor.1 > 0 {
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_cursor.0.min(self.script_buffer[self.script_cursor.1].len());
            }
        }
        if input.just_pressed(Key::Down) {
            if self.script_cursor.1 + 1 < self.script_buffer.len() {
                self.script_cursor.1 += 1;
                self.script_cursor.0 = self.script_cursor.0.min(self.script_buffer[self.script_cursor.1].len());
            }
        }
        if input.just_pressed(Key::Left) {
            if self.script_cursor.0 > 0 {
                self.script_cursor.0 -= 1;
            } else if self.script_cursor.1 > 0 {
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_buffer[self.script_cursor.1].len();
            }
        }
        if input.just_pressed(Key::Right) {
            if self.script_cursor.0 < self.script_buffer[self.script_cursor.1].len() {
                self.script_cursor.0 += 1;
            } else if self.script_cursor.1 + 1 < self.script_buffer.len() {
                self.script_cursor.1 += 1;
                self.script_cursor.0 = 0;
            }
        }
        if input.just_pressed(Key::Home) { self.script_cursor.0 = 0; }
        if input.just_pressed(Key::End)  { self.script_cursor.0 = self.script_buffer[self.script_cursor.1].len(); }

        // ── Shortcuts ─────────────────────────────────────────────────────────
        if ctrl && input.just_pressed(Key::S) {
            self.save_script();
            return;
        }

        // ── Typing ────────────────────────────────────────────────────────────
        for &key in TEXT_INPUT_KEYS {
            if input.just_pressed(key) {
                if let Some(ch) = key_to_char(key, shift) {
                    let row = self.script_cursor.1;
                    let col = self.script_cursor.0;
                    self.script_buffer[row].insert(col, ch);
                    self.script_cursor.0 += 1;
                    self.script_unsaved = true;
                }
            }
        }

        if input.just_pressed(Key::Space) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            self.script_buffer[row].insert(col, ' ');
            self.script_cursor.0 += 1;
            self.script_unsaved = true;
        }

        if input.just_pressed(Key::Tab) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            self.script_buffer[row].insert_str(col, "  ");
            self.script_cursor.0 += 2;
            self.script_unsaved = true;
        }

        if input.just_pressed(Key::Enter) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let current_line = self.script_buffer[row].clone();
            let (left, right) = current_line.split_at(col);
            self.script_buffer[row] = left.to_string();
            self.script_buffer.insert(row + 1, right.to_string());
            self.script_cursor.1 += 1;
            self.script_cursor.0 = 0;
            self.script_unsaved = true;
        }

        if input.just_pressed(Key::Backspace) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            if col > 0 {
                self.script_buffer[row].remove(col - 1);
                self.script_cursor.0 -= 1;
                self.script_unsaved = true;
            } else if row > 0 {
                let current_line = self.script_buffer.remove(row);
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_buffer[self.script_cursor.1].len();
                self.script_buffer[self.script_cursor.1].push_str(&current_line);
                self.script_unsaved = true;
            }
        }

        if input.just_pressed(Key::Delete) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            if col < self.script_buffer[row].len() {
                self.script_buffer[row].remove(col);
                self.script_unsaved = true;
            } else if row + 1 < self.script_buffer.len() {
                let next_line = self.script_buffer.remove(row + 1);
                self.script_buffer[row].push_str(&next_line);
                self.script_unsaved = true;
            }
        }

        // ── Scroll Auto-tracking ──────────────────────────────────────────────
        if self.script_cursor != old_cursor {
            if self.script_cursor.1 < self.script_scroll {
                self.script_scroll = self.script_cursor.1;
            } else if self.script_cursor.1 >= self.script_scroll + text_h {
                self.script_scroll = self.script_cursor.1 - text_h + 1;
            }
        }
    }
}
