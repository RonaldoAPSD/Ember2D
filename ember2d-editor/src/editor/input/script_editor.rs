// editor/input/script_editor.rs — Typing logic for the in-engine script editor.

use super::super::panel::PanelId;
use super::super::ui::ToolKind;
use super::super::{EditorMode, EditorState, ScriptEditGroup};
use ember2d::input::Key;

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

/// ~500ms at the editor's fixed 60Hz step (7C-7, master plan §5.3, R18) —
/// see `EditorState::script_idle_timer`'s own doc comment.
const SCRIPT_IDLE_CHECK_FRAMES: u32 = 30;

/// Oldest checkpoint is dropped once the stack exceeds this (7C-8, master
/// plan §5.3) — scripts are small text files, so this is a generous cap
/// against unbounded growth over a very long editing session, not a real
/// memory concern.
const SCRIPT_UNDO_LIMIT: usize = 200;

impl EditorState {
    /// Every script-buffer mutation site below calls this instead of
    /// setting `script_unsaved` directly (7C-7, master plan §5.3, R18) —
    /// the one place that also resets the idle timer, so a live syntax
    /// check fires ~500ms after the LATEST edit, not the first one in a
    /// typing burst.
    fn note_script_edit(&mut self) {
        self.script_unsaved = true;
        self.script_idle_timer = 0;
    }

    // ── Undo/redo (7C-8, master plan §5.3) ──────────────────────────────────
    //
    // A completely separate stack from `commands::UndoStack` (the level
    // grid's own undo, 7C-6) — see `EditorState::script_undo`'s own doc
    // comment for why nothing there is reusable for text.

    /// Always pushes a fresh checkpoint, clears redo, and resets the
    /// coalescing group to `None` — used by every edit that must never
    /// coalesce with whatever came before or comes after it (Enter, cut,
    /// paste, replacing a selection, and undo/redo's own bookkeeping).
    fn push_undo_checkpoint(&mut self) {
        self.script_undo.push((self.script_buffer.clone(), self.script_cursor));
        self.script_redo.clear();
        if self.script_undo.len() > SCRIPT_UNDO_LIMIT {
            self.script_undo.remove(0);
        }
        self.script_undo_group = ScriptEditGroup::None;
    }

    /// Checkpoints only when the last checkpoint wasn't already `group` —
    /// lets a run of plain typed characters (or a run of Backspace
    /// presses) undo as one step instead of one per character.
    fn checkpoint_script_edit(&mut self, group: ScriptEditGroup) {
        if self.script_undo_group != group {
            self.push_undo_checkpoint();
            self.script_undo_group = group;
        }
    }

    /// Checkpoints for an edit about to insert new content, replacing any
    /// active selection first. Replacing a selection always forces a
    /// FRESH checkpoint (never a continuation of prior typing); with no
    /// selection, `group` decides whether this coalesces with the
    /// previous edit. Shared by plain typing and Tab.
    fn begin_replacing_edit(&mut self, group: ScriptEditGroup) {
        if self.script_selection_anchor.is_some() {
            self.push_undo_checkpoint();
            self.delete_selection();
            self.script_undo_group = group;
        } else {
            self.checkpoint_script_edit(group);
        }
    }

    fn script_undo_action(&mut self) {
        if let Some((buf, cur)) = self.script_undo.pop() {
            self.script_redo.push((self.script_buffer.clone(), self.script_cursor));
            self.script_buffer = buf;
            self.script_cursor = cur;
            self.script_selection_anchor = None;
            self.script_undo_group = ScriptEditGroup::None;
            self.note_script_edit();
            self.check_script_syntax();
        }
    }

    fn script_redo_action(&mut self) {
        if let Some((buf, cur)) = self.script_redo.pop() {
            self.script_undo.push((self.script_buffer.clone(), self.script_cursor));
            self.script_buffer = buf;
            self.script_cursor = cur;
            self.script_selection_anchor = None;
            self.script_undo_group = ScriptEditGroup::None;
            self.note_script_edit();
            self.check_script_syntax();
        }
    }

    // ── Selection (7C-8, master plan §5.3) ──────────────────────────────────

    /// Called before every cursor-moving action (arrows/Home/End/click).
    /// Holding Shift starts a selection at the cursor's PRE-move position
    /// if one isn't already open; anything else collapses it. A plain
    /// (non-Shift) move while a selection is open therefore always drops
    /// the selection and moves the cursor as if nothing were selected —
    /// simpler than real editors' "collapse to the near edge" convention,
    /// and a deliberate scope cut for this step.
    fn update_selection_anchor(&mut self, shift: bool) {
        if shift {
            if self.script_selection_anchor.is_none() {
                self.script_selection_anchor = Some(self.script_cursor);
            }
        } else {
            self.script_selection_anchor = None;
        }
    }

    /// Removes the current selection's text, places the cursor at its
    /// start, and clears the selection. Returns `false` (no-op) when
    /// nothing is selected. Deliberately does NOT checkpoint undo itself
    /// — every call site decides that, since "replace selection with
    /// typed text" and "Backspace/Delete a selection" checkpoint
    /// differently (see `begin_replacing_edit`).
    fn delete_selection(&mut self) -> bool {
        let Some((start, end)) = self.script_selection() else {
            return false;
        };
        let ((sc, sl), (ec, el)) = (start, end);
        if sl == el {
            let byte_s = char_byte_offset(&self.script_buffer[sl], sc);
            let byte_e = char_byte_offset(&self.script_buffer[sl], ec);
            self.script_buffer[sl].replace_range(byte_s..byte_e, "");
        } else {
            let byte_s = char_byte_offset(&self.script_buffer[sl], sc);
            let byte_e = char_byte_offset(&self.script_buffer[el], ec);
            let tail = self.script_buffer[el][byte_e..].to_string();
            self.script_buffer[sl].truncate(byte_s);
            self.script_buffer[sl].push_str(&tail);
            self.script_buffer.drain(sl + 1..=el);
        }
        self.script_cursor = (sc, sl);
        self.script_selection_anchor = None;
        true
    }

    /// The current selection's text, joined with `\n` across lines —
    /// `None` when nothing is selected.
    fn selected_text(&self) -> Option<String> {
        let (start, end) = self.script_selection()?;
        let ((sc, sl), (ec, el)) = (start, end);
        if sl == el {
            let line = &self.script_buffer[sl];
            let bs = char_byte_offset(line, sc);
            let be = char_byte_offset(line, ec);
            Some(line[bs..be].to_string())
        } else {
            let mut result = String::new();
            let first = &self.script_buffer[sl];
            result.push_str(&first[char_byte_offset(first, sc)..]);
            for line in &self.script_buffer[sl + 1..el] {
                result.push('\n');
                result.push_str(line);
            }
            result.push('\n');
            let last = &self.script_buffer[el];
            result.push_str(&last[..char_byte_offset(last, ec)]);
            Some(result)
        }
    }

    // ── Clipboard (7C-8, master plan §5.3) ──────────────────────────────────
    //
    // `script_clipboard` (a plain `String`) is the real source of truth —
    // see its own doc comment (editor/mod.rs) for why. These two helpers
    // are the ONLY place `arboard` is touched, and both treat any failure
    // (no display, no OS clipboard support, anything) as "silently skip
    // the OS sync" — never a reason to fail the actual cut/copy/paste the
    // user asked for.

    fn sync_os_clipboard_copy(text: &str) {
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.set_text(text.to_string());
        }
    }

    /// Inserts `text` at the cursor, replacing any active selection first,
    /// splitting on `\n` for a multi-line paste. Does not checkpoint undo
    /// itself (the caller already did, same convention as `delete_selection`).
    fn insert_text_at_cursor(&mut self, text: &str) {
        self.delete_selection();
        let lines: Vec<&str> = text.split('\n').collect();
        let (row, col) = (self.script_cursor.1, self.script_cursor.0);
        let byte_col = char_byte_offset(&self.script_buffer[row], col);
        let tail = self.script_buffer[row][byte_col..].to_string();
        self.script_buffer[row].truncate(byte_col);
        self.script_buffer[row].push_str(lines[0]);
        if lines.len() == 1 {
            self.script_cursor.0 = col + lines[0].chars().count();
            self.script_buffer[row].push_str(&tail);
        } else {
            for (i, l) in lines[1..].iter().enumerate() {
                self.script_buffer.insert(row + 1 + i, l.to_string());
            }
            let last_idx = row + lines.len() - 1;
            self.script_cursor = (lines[lines.len() - 1].chars().count(), last_idx);
            self.script_buffer[last_idx].push_str(&tail);
        }
        self.note_script_edit();
    }

    // ── Find (7C-8, master plan §5.3) ───────────────────────────────────────

    /// Case-insensitive substring search starting at `start`, wrapping to
    /// the top of the buffer if nothing is found before the end — ASCII
    /// case-folding only (see `to_lowercase`'s use below): good enough for
    /// Rhai source, not general Unicode-correct search, matching the file's
    /// existing "good enough, not a real parser" syntax highlighting.
    fn script_find_search(&mut self, start: (usize, usize)) {
        if self.script_find_query.is_empty() {
            self.script_selection_anchor = None;
            return;
        }
        let query = self.script_find_query.to_lowercase();
        let line_count = self.script_buffer.len();
        let (start_char, start_line) = start;
        for offset in 0..line_count {
            let line_idx = (start_line + offset) % line_count;
            let line = &self.script_buffer[line_idx];
            let haystack = line.to_lowercase();
            let search_from_char = if offset == 0 { start_char } else { 0 };
            let byte_from = char_byte_offset(line, search_from_char);
            if let Some(byte_pos) = haystack.get(byte_from..).and_then(|s| s.find(&query)) {
                let char_start = line[..byte_from + byte_pos].chars().count();
                let char_end = char_start + query.chars().count();
                self.script_selection_anchor = Some((char_start, line_idx));
                self.script_cursor = (char_end, line_idx);
                return;
            }
        }
        self.script_selection_anchor = None;
    }

    fn handle_script_find_input(&mut self, input: &mut ember2d::input::InputManager) {
        if input.just_pressed(Key::Escape) {
            self.script_find_active = false;
            return;
        }
        if input.just_pressed(Key::Enter) || input.is_repeating(Key::Enter) {
            // Advance from the end of the current match (or wherever the
            // cursor is, if nothing matched yet) — searching forward
            // through the buffer on each press.
            let from = self.script_cursor;
            self.script_find_search(from);
            return;
        }
        if input.just_pressed(Key::Backspace) || input.is_repeating(Key::Backspace) {
            self.script_find_query.pop();
            let from = self.script_find_origin;
            self.script_find_search(from);
            return;
        }
        input.begin_text_capture();
        let typed = input.take_text();
        if !typed.is_empty() {
            self.script_find_query.push_str(&typed);
            let from = self.script_find_origin;
            self.script_find_search(from);
        }
    }

    // `pub(crate)`, not `pub(super)`: the R11 regression test
    // (impl_state/tests.rs, a sibling module of `editor::input`) drives this
    // directly rather than through a full `InputManager`/event-loop harness
    // — the headless input harness 7C-5 adds is the real long-term answer.
    pub(crate) fn handle_script_mode_input(
        &mut self,
        input: &mut ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        let fullscreen = matches!(self.mode, EditorMode::Script);
        if self.focused_panel != Some(PanelId::ScriptEditor) && !fullscreen {
            return;
        }
        if self.script_path.is_none() {
            return;
        }

        // 7C-8 (master plan §5.3): the find bar owns all input while open
        // — typed characters build the query instead of editing the
        // buffer. Checked before the idle-timer/syntax-check below since
        // neither the buffer nor the timer changes while finding.
        if self.script_find_active {
            self.handle_script_find_input(input);
            return;
        }

        // 7C-7 (master plan §5.3, R18): idle-triggered live syntax check —
        // counts frames since the last edit (`note_script_edit` resets it
        // to 0), so a fast typist doesn't recompile on every keystroke.
        // `==`, not `>=`: fires exactly once per idle stretch, not every
        // frame after the threshold, without needing a separate "already
        // checked this stretch" flag.
        self.script_idle_timer = self.script_idle_timer.saturating_add(1);
        if self.script_idle_timer == SCRIPT_IDLE_CHECK_FRAMES {
            self.check_script_syntax();
        }

        let p = self.panels.get(PanelId::ScriptEditor);

        // 1. Resolve exact text area bounds dynamically
        let (sw, sh) = self.panels.screen_size_cells();

        let (cx, cy, cw, ch) = if fullscreen {
            (0usize, 1usize, sw, sh.saturating_sub(2))
        } else {
            (p.content_x(), p.content_y(), p.content_w(), p.content_h())
        };

        // Header row inside the panel is cy, text starts at cy + 1
        let text_y = cy + 1;
        let text_h = ch.saturating_sub(1);
        let gutter_w = 4;

        let ctrl = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);

        // ── Mouse Interaction (Click & Scroll) ────────────────────────────────
        if mouse.in_bounds {
            // 1. Mouse Wheel Scroll
            if mouse.wheel_y != 0.0 {
                let delta = if mouse.wheel_y > 0.0 { -2i32 } else { 2i32 };
                let max_scroll = self.script_buffer.len().saturating_sub(text_h);
                self.script_scroll =
                    (self.script_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                return;
            }

            // 2. Click to place cursor
            if mouse.left_just_pressed()
                && mouse.cell_x >= cx
                && mouse.cell_x < cx + cw
                && mouse.cell_y >= text_y
                && mouse.cell_y < text_y + text_h
            {
                let row_in_view = mouse.cell_y - text_y;
                let target_row = self.script_scroll + row_in_view;

                if target_row < self.script_buffer.len() {
                    // 7C-8 (master plan §5.3): Shift+click extends a
                    // selection instead of just moving the cursor — same
                    // anchor mechanism as Shift+arrows.
                    self.update_selection_anchor(shift);
                    self.script_cursor.1 = target_row;
                    let line_start_x = cx + gutter_w;
                    let col = mouse.cell_x as i32 - line_start_x as i32 + self.script_hscroll as i32;
                    // R11: clamp against the CHARACTER count, not the
                    // byte length — a line with any multi-byte character
                    // has fewer chars than bytes, so `.len()` here let
                    // the cursor land past the last real character,
                    // producing an out-of-range char index for every
                    // mutation site below.
                    self.script_cursor.0 =
                        (col.max(0) as usize).min(self.script_buffer[target_row].chars().count());
                    return;
                }
            }
        }

        let old_cursor = self.script_cursor;

        // ── Navigation ────────────────────────────────────────────────────────
        if input.just_pressed(Key::Escape) {
            // Only leave `Script` mode if that's actually what's active —
            // the docked (non-fullscreen) panel merely having keyboard
            // focus doesn't touch `mode` at all (see `EditorMode::Script`'s
            // own doc comment), so Escape there is a no-op here, matching
            // the original's own harmless `script_mode = false` when it
            // was already `false`.
            if fullscreen {
                self.mode = EditorMode::Paint(ToolKind::Paint);
            }
            return;
        }

        // ── Shortcuts that don't touch the buffer's own text ────────────────────
        if ctrl && input.just_pressed(Key::S) {
            self.save_script();
            return;
        }
        if ctrl && input.just_pressed(Key::A) {
            // Select all (7C-8, master plan §5.3).
            self.script_selection_anchor = Some((0, 0));
            let last = self.script_buffer.len() - 1;
            self.script_cursor = (self.script_buffer[last].chars().count(), last);
            return;
        }
        if ctrl && input.just_pressed(Key::F) {
            self.script_find_active = true;
            self.script_find_query.clear();
            self.script_find_origin = self.script_cursor;
            return;
        }
        if ctrl && input.just_pressed(Key::Z) {
            self.script_undo_action();
            return;
        }
        if ctrl && input.just_pressed(Key::Y) {
            self.script_redo_action();
            return;
        }
        if ctrl && input.just_pressed(Key::C) {
            if let Some(text) = self.selected_text() {
                Self::sync_os_clipboard_copy(&text);
                self.script_clipboard = text;
            }
            return;
        }
        if ctrl && input.just_pressed(Key::X) {
            if let Some(text) = self.selected_text() {
                Self::sync_os_clipboard_copy(&text);
                self.script_clipboard = text;
                self.push_undo_checkpoint();
                self.delete_selection();
                self.note_script_edit();
                self.check_script_syntax();
            }
            return;
        }
        if ctrl && input.just_pressed(Key::V) {
            if !self.script_clipboard.is_empty() {
                let text = self.script_clipboard.clone();
                self.push_undo_checkpoint();
                self.insert_text_at_cursor(&text);
                self.check_script_syntax();
            }
            return;
        }

        // R24 (7B-4, docs/ember2d-master-plan.md §5.2/§3): every navigation
        // and editing key below used to check `just_pressed` alone, which
        // fires exactly once per physical press — holding, say, Down never
        // moved the cursor past one line no matter how long the key stayed
        // down, since winit's OS-level key-repeat was never read anywhere
        // in the engine. `is_repeating` (fed from `KeyEvent::repeat` via
        // `InputManager::handle_repeat`, engine.rs) now supplies that
        // per-frame "still repeating" signal alongside the original
        // per-press one.
        if (input.just_pressed(Key::Up) || input.is_repeating(Key::Up)) && self.script_cursor.1 > 0
        {
            self.update_selection_anchor(shift);
            self.script_cursor.1 -= 1;
            self.script_cursor.0 =
                self.script_cursor.0.min(self.script_buffer[self.script_cursor.1].chars().count());
        }
        if (input.just_pressed(Key::Down) || input.is_repeating(Key::Down))
            && self.script_cursor.1 + 1 < self.script_buffer.len()
        {
            self.update_selection_anchor(shift);
            self.script_cursor.1 += 1;
            self.script_cursor.0 =
                self.script_cursor.0.min(self.script_buffer[self.script_cursor.1].chars().count());
        }
        if input.just_pressed(Key::Left) || input.is_repeating(Key::Left) {
            self.update_selection_anchor(shift);
            if self.script_cursor.0 > 0 {
                self.script_cursor.0 -= 1;
            } else if self.script_cursor.1 > 0 {
                self.script_cursor.1 -= 1;
                self.script_cursor.0 = self.script_buffer[self.script_cursor.1].chars().count();
            }
        }
        if input.just_pressed(Key::Right) || input.is_repeating(Key::Right) {
            self.update_selection_anchor(shift);
            if self.script_cursor.0 < self.script_buffer[self.script_cursor.1].chars().count() {
                self.script_cursor.0 += 1;
            } else if self.script_cursor.1 + 1 < self.script_buffer.len() {
                self.script_cursor.1 += 1;
                self.script_cursor.0 = 0;
            }
        }
        if input.just_pressed(Key::Home) {
            self.update_selection_anchor(shift);
            self.script_cursor.0 = 0;
        }
        if input.just_pressed(Key::End) {
            self.update_selection_anchor(shift);
            self.script_cursor.0 = self.script_buffer[self.script_cursor.1].chars().count();
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
        if !typed.is_empty() {
            // 7C-8 (master plan §5.3): typed text replaces an active
            // selection instead of being inserted alongside it.
            self.begin_replacing_edit(ScriptEditGroup::Insert);
            for ch in typed.chars() {
                let row = self.script_cursor.1;
                let col = self.script_cursor.0;
                let byte_col = char_byte_offset(&self.script_buffer[row], col);
                self.script_buffer[row].insert(byte_col, ch);
                self.script_cursor.0 += 1;
            }
            self.note_script_edit();
        }

        if input.just_pressed(Key::Tab) || input.is_repeating(Key::Tab) {
            self.begin_replacing_edit(ScriptEditGroup::Insert);
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let byte_col = char_byte_offset(&self.script_buffer[row], col);
            self.script_buffer[row].insert_str(byte_col, "  ");
            self.script_cursor.0 += 2;
            self.note_script_edit();
        }

        if input.just_pressed(Key::Enter) || input.is_repeating(Key::Enter) {
            // Enter never coalesces with anything before or after it.
            self.push_undo_checkpoint();
            self.delete_selection();
            let row = self.script_cursor.1;
            let col = self.script_cursor.0;
            let byte_col = char_byte_offset(&self.script_buffer[row], col);
            let current_line = self.script_buffer[row].clone();
            let (left, right) = current_line.split_at(byte_col);
            self.script_buffer[row] = left.to_string();
            self.script_buffer.insert(row + 1, right.to_string());
            self.script_cursor.1 += 1;
            self.script_cursor.0 = 0;
            self.note_script_edit();
        }

        if input.just_pressed(Key::Backspace) || input.is_repeating(Key::Backspace) {
            if self.script_selection_anchor.is_some() {
                self.push_undo_checkpoint();
                self.delete_selection();
                self.note_script_edit();
            } else {
                self.checkpoint_script_edit(ScriptEditGroup::Delete);
                let row = self.script_cursor.1;
                let col = self.script_cursor.0;
                if col > 0 {
                    let byte_col = char_byte_offset(&self.script_buffer[row], col - 1);
                    self.script_buffer[row].remove(byte_col);
                    self.script_cursor.0 -= 1;
                    self.note_script_edit();
                } else if row > 0 {
                    let current_line = self.script_buffer.remove(row);
                    self.script_cursor.1 -= 1;
                    self.script_cursor.0 = self.script_buffer[self.script_cursor.1].chars().count();
                    self.script_buffer[self.script_cursor.1].push_str(&current_line);
                    self.note_script_edit();
                }
            }
        }

        // R24: Delete never got the same key-repeat treatment as every
        // other editing key above when 7B-4 added `is_repeating` — found
        // in this step's own investigation, unrelated to anything it set
        // out to change; a one-line fix, fixed alongside rather than
        // logged and deferred.
        if input.just_pressed(Key::Delete) || input.is_repeating(Key::Delete) {
            if self.script_selection_anchor.is_some() {
                self.push_undo_checkpoint();
                self.delete_selection();
                self.note_script_edit();
            } else {
                self.checkpoint_script_edit(ScriptEditGroup::Delete);
                let row = self.script_cursor.1;
                let col = self.script_cursor.0;
                let char_count = self.script_buffer[row].chars().count();
                if col < char_count {
                    let byte_col = char_byte_offset(&self.script_buffer[row], col);
                    self.script_buffer[row].remove(byte_col);
                    self.note_script_edit();
                } else if row + 1 < self.script_buffer.len() {
                    let next_line = self.script_buffer.remove(row + 1);
                    self.script_buffer[row].push_str(&next_line);
                    self.note_script_edit();
                }
            }
        }

        // ── Scroll Auto-tracking ──────────────────────────────────────────────
        if self.script_cursor != old_cursor {
            if self.script_cursor.1 < self.script_scroll {
                self.script_scroll = self.script_cursor.1;
            } else if self.script_cursor.1 >= self.script_scroll + text_h {
                self.script_scroll = self.script_cursor.1 - text_h + 1;
            }
            // 7C-8 (master plan §5.3): horizontal twin of the above.
            let visible_w = cw.saturating_sub(gutter_w);
            if self.script_cursor.0 < self.script_hscroll {
                self.script_hscroll = self.script_cursor.0;
            } else if self.script_cursor.0 >= self.script_hscroll + visible_w {
                self.script_hscroll = self.script_cursor.0 - visible_w + 1;
            }
        }
    }
}
