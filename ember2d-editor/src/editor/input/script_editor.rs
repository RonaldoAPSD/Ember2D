// editor/input/script_editor.rs — Typing logic for the in-engine script editor.

use ember2d::input::Key;
use super::super::EditorState;
use super::super::panel::PanelId;

/// R11 (7A-2, docs/ember2d-master-plan.md): `script_cursor.0` is a CHARACTER
/// index — arrow-key movement increments/decrements it by one character, and
/// a mouse click derives it from a column count, not a byte count. Every
/// mutation below (`insert`/`insert_str`/`remove`/`split_at`) instead wants a
/// BYTE offset into the line's `String`. Using the character index directly
/// as that byte offset used to panic ("byte index N is not a char boundary")
/// the moment a line contained any multi-byte UTF-8 character before the
/// cursor. This converts once, at the point of mutation; `unwrap_or(s.len())`
/// matches a char index equal to the line's char count (cursor at the end).
fn char_byte_offset(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(s.len())
}

impl EditorState {
    // `pub(crate)`, not `pub(super)`: the R11 regression test
    // (impl_state/tests.rs, a sibling module of `editor::input`) drives this
    // directly rather than through a full `InputManager`/event-loop harness
    // — the headless input harness 7C-5 adds is the real long-term answer.
    pub(crate) fn handle_script_mode_input(&mut self, input: &mut ember2d::input::InputManager, mouse: &ember2d::mouse::MouseState) {
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
                        // R11: clamp against the CHARACTER count, not the
                        // byte length — a line with any multi-byte character
                        // has fewer chars than bytes, so `.len()` here let
                        // the cursor land past the last real character,
                        // producing an out-of-range char index for every
                        // mutation site below.
                        self.script_cursor.0 = (col.max(0) as usize).min(self.script_buffer[target_row].chars().count());
                        return;
                    }
                }
            }
        }

        let ctrl = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);

        let old_cursor = self.script_cursor;

        // ── Navigation ────────────────────────────────────────────────────────
        if input.just_pressed(Key::Escape) {
            self.script_mode = false;
            return;
        }

        if input.just_pressed(Key::Up) {
            if self.script_cursor.1 > 0 {
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_cursor.0.min(self.script_buffer[self.script_cursor.1].chars().count());
            }
        }
        if input.just_pressed(Key::Down) {
            if self.script_cursor.1 + 1 < self.script_buffer.len() {
                self.script_cursor.1 += 1;
                self.script_cursor.0 = self.script_cursor.0.min(self.script_buffer[self.script_cursor.1].chars().count());
            }
        }
        if input.just_pressed(Key::Left) {
            if self.script_cursor.0 > 0 {
                self.script_cursor.0 -= 1;
            } else if self.script_cursor.1 > 0 {
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_buffer[self.script_cursor.1].chars().count();
            }
        }
        if input.just_pressed(Key::Right) {
            if self.script_cursor.0 < self.script_buffer[self.script_cursor.1].chars().count() {
                self.script_cursor.0 += 1;
            } else if self.script_cursor.1 + 1 < self.script_buffer.len() {
                self.script_cursor.1 += 1;
                self.script_cursor.0 = 0;
            }
        }
        if input.just_pressed(Key::Home) { self.script_cursor.0 = 0; }
        if input.just_pressed(Key::End)  { self.script_cursor.0 = self.script_buffer[self.script_cursor.1].chars().count(); }

        // ── Shortcuts ─────────────────────────────────────────────────────────
        if ctrl && input.just_pressed(Key::S) {
            self.save_script();
            return;
        }

        // ── Typing ────────────────────────────────────────────────────────────
        // R12 (7A-2, docs/ember2d-master-plan.md): reads the engine's
        // captured text characters (handles Shift, AltGr, dead keys, and
        // non-ASCII input correctly) instead of the old physical-key +
        // US-QWERTY `key_to_char` lookup, which silently mistyped every
        // other keyboard layout. `begin_text_capture` must be renewed every
        // frame this panel stays focused (see its own doc comment,
        // ember2d/src/input.rs) — without it, `Engine::poll_events` clears
        // `text_buffer` before this line ever sees it.
        input.begin_text_capture();
        let typed = input.take_text();
        for ch in typed.chars() {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let byte_col = char_byte_offset(&self.script_buffer[row], col);
            self.script_buffer[row].insert(byte_col, ch);
            self.script_cursor.0 += 1;
            self.script_unsaved = true;
        }

        if input.just_pressed(Key::Tab) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let byte_col = char_byte_offset(&self.script_buffer[row], col);
            self.script_buffer[row].insert_str(byte_col, "  ");
            self.script_cursor.0 += 2;
            self.script_unsaved = true;
        }

        if input.just_pressed(Key::Enter) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let byte_col = char_byte_offset(&self.script_buffer[row], col);
            let current_line = self.script_buffer[row].clone();
            let (left, right) = current_line.split_at(byte_col);
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
                let byte_col = char_byte_offset(&self.script_buffer[row], col - 1);
                self.script_buffer[row].remove(byte_col);
                self.script_cursor.0 -= 1;
                self.script_unsaved = true;
            } else if row > 0 {
                let current_line = self.script_buffer.remove(row);
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_buffer[self.script_cursor.1].chars().count();
                self.script_buffer[self.script_cursor.1].push_str(&current_line);
                self.script_unsaved = true;
            }
        }

        if input.just_pressed(Key::Delete) {
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let char_count = self.script_buffer[row].chars().count();
            if col < char_count {
                let byte_col = char_byte_offset(&self.script_buffer[row], col);
                self.script_buffer[row].remove(byte_col);
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
