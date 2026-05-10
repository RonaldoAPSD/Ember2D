// editor/input/mod.rs — Input orchestration for EditorState.

use crate::engine::UpdateContext;
use crate::editor::commands::Command;
use super::{EditorState, PaletteField};

mod canvas;
mod graph;
mod panels;
mod shortcuts;
mod text;

impl EditorState {
    pub(super) fn handle_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { input, mouse, .. } = ctx;

        // ── Modal: Palette Editor ─────────────────────────────────────────────
        if self.palette_editor_open {
            use crate::input::Key;
            use crate::renderer::color::Color;
            use super::{TEXT_INPUT_KEYS, key_to_char};

            let mw = 36usize;
            let mh = 18usize;
            let mx = (self.layout.screen_w.saturating_sub(mw)) / 2;
            let my = (self.layout.screen_h.saturating_sub(mh)) / 2;
            let cx = mx + 2;
            let sel = self.palette_editing_idx;

            // 1. Inline Typing Logic
            if let Some(focus) = self.palette_editor_focus {
                let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
                for &key in TEXT_INPUT_KEYS {
                    if input.just_pressed(key) {
                        if let Some(ch) = key_to_char(key, shift) {
                            match focus {
                                PaletteField::Name  => { self.palette.tiles[sel].name.push(ch); self.unsaved = true; }
                                PaletteField::Tag   => { self.palette.tiles[sel].tag.push(ch);  self.unsaved = true; }
                                PaletteField::Glyph => { self.palette.tiles[sel].glyph = ch;    self.unsaved = true; }
                            }
                        }
                    }
                }
                if input.just_pressed(Key::Backspace) {
                    match focus {
                        PaletteField::Name => { self.palette.tiles[sel].name.pop(); self.unsaved = true; }
                        PaletteField::Tag  => { self.palette.tiles[sel].tag.pop();  self.unsaved = true; }
                        _ => {}
                    }
                }
                if input.just_pressed(Key::Enter) { self.palette_editor_focus = None; }
            }

            // 2. Mouse Interaction
            if mouse.left_just_pressed() && mouse.in_bounds {
                // 1. [X] button in title bar (Cancel)
                if mouse.cell_y == my && mouse.cell_x >= mx + mw - 4 && mouse.cell_x < mx + mw - 1 {
                    self.palette_editor_open = false;
                    self.palette_editor_focus = None;
                    return;
                }

                // 2. Bottom row buttons
                let btn_y = my + mh - 2;
                if mouse.cell_y == btn_y {
                    // [ Save & Close ] (left)
                    if mouse.cell_x >= mx + 2 && mouse.cell_x < mx + 20 {
                        self.palette_editor_open = false;
                        self.palette_editor_focus = None;
                        self.save_palette();
                        return;
                    }
                    // [ Delete ] (right)
                    if mouse.cell_x >= mx + 22 && mouse.cell_x < mx + 34 {
                        if self.palette.tiles.len() > 1 {
                            self.palette.tiles.remove(sel);
                            self.palette.selected = self.palette.selected.min(self.palette.tiles.len() - 1);
                            self.palette_editor_open = false;
                            self.palette_editor_focus = None;
                            self.unsaved = true;
                            self.save_message = Some("Asset deleted.".to_string());
                            self.save_message_timer = 0;
                            self.save_palette();
                        } else {
                            self.save_message = Some("Cannot delete last item!".to_string());
                            self.save_message_timer = 0;
                        }
                        return;
                    }
                }

                // 3. Field Clicks
                match mouse.cell_y {
                    r if r == my + 2 => { self.palette_editor_focus = Some(PaletteField::Name); }
                    r if r == my + 3 => { self.palette_editor_focus = Some(PaletteField::Glyph); }
                    r if r == my + 4 => {
                        self.palette_editor_focus = None;
                        if mouse.cell_x >= cx && mouse.cell_x < cx + 10 { self.palette.tiles[sel].solid = !self.palette.tiles[sel].solid; self.unsaved = true; }
                        else if mouse.cell_x >= cx + 13 && mouse.cell_x < cx + 25 { self.palette.tiles[sel].trigger = !self.palette.tiles[sel].trigger; self.unsaved = true; }
                    }
                    r if r == my + 5 => { self.palette_editor_focus = Some(PaletteField::Tag); }
                    r if r == my + 8 || r == my + 9 => { // FG Color Grid
                        self.palette_editor_focus = None;
                        let gx_start = cx;
                        if mouse.cell_x >= gx_start && mouse.cell_x < gx_start + 24 {
                            let col_idx = (mouse.cell_x - gx_start) / 3;
                            let row_idx = mouse.cell_y - (my + 8);
                            let color_idx = row_idx * 8 + col_idx;
                            let colors = [
                                Color::Black, Color::White, Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Cyan, Color::Magenta,
                                Color::DarkGrey, Color::Grey, Color::DarkRed, Color::DarkGreen, Color::DarkBlue, Color::DarkYellow, Color::DarkCyan, Color::DarkMagenta,
                            ];
                            if color_idx < colors.len() { self.palette.tiles[sel].fg = colors[color_idx]; self.unsaved = true; }
                        }
                    }
                    r if r == my + 12 || r == my + 13 => { // BG Color Grid
                        self.palette_editor_focus = None;
                        let gx_start = cx;
                        if mouse.cell_x >= gx_start && mouse.cell_x < gx_start + 24 {
                            let col_idx = (mouse.cell_x - gx_start) / 3;
                            let row_idx = mouse.cell_y - (my + 12);
                            let color_idx = row_idx * 8 + col_idx;
                            let colors = [
                                Color::Black, Color::White, Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Cyan, Color::Magenta,
                                Color::DarkGrey, Color::Grey, Color::DarkRed, Color::DarkGreen, Color::DarkBlue, Color::DarkYellow, Color::DarkCyan, Color::DarkMagenta,
                            ];
                            if color_idx < colors.len() { self.palette.tiles[sel].bg = colors[color_idx]; self.unsaved = true; }
                        }
                    }
                    _ => { self.palette_editor_focus = None; }
                }
            }
            if input.just_pressed(Key::Escape) {
                if self.palette_editor_focus.is_some() { self.palette_editor_focus = None; }
                else { self.palette_editor_open = false; self.save_palette(); }
            }
            return;
        }

        // ── Ignore drag state ─────────────────────────────────────────────────
        if self.ignore_drag {
            if !mouse.left_held() {
                self.ignore_drag = false;
            }
        }

        // Graph editor mode swallows all input.
        if self.graph_mode.is_some() {
            self.update_graph_mode(input, mouse);
            return;
        }

        // Tick save message.
        if self.save_message.is_some() {
            self.save_message_timer += 1;
            if self.save_message_timer > 90 {
                self.save_message       = None;
                self.save_message_timer = 0;
            }
        }

        // ── Spawn placement modes ─────────────────────────────────────────────
        if self.placing_spawn {
            if input.just_pressed(crate::input::Key::Escape) || mouse.right_just_pressed() {
                self.placing_spawn = false;
                self.save_message = Some("Spawn placement cancelled.".to_string());
                self.save_message_timer = 0;
                return;
            }
            if mouse.left_just_pressed() {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    let before = self.grid.spawn_point;
                    let after  = (gx as f32, gy as f32);
                    self.undo.push(Command::MoveSpawn { before, after });
                    self.grid.spawn_point = after;
                    self.unsaved = true;
                    self.placing_spawn = false;
                    self.ignore_drag = true;
                    self.save_message = Some("Spawn placed.".to_string());
                    self.save_message_timer = 0;
                }
            }
            return;
        }
        if let Some(buf) = self.placing_named_spawn.clone() {
            if input.just_pressed(crate::input::Key::Escape) || mouse.right_just_pressed() {
                self.placing_named_spawn = None;
                self.save_message = Some("Spawn placement cancelled.".to_string());
                self.save_message_timer = 0;
                return;
            }
            if mouse.left_just_pressed() {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    let before = self.grid.extra_spawns.clone();
                    let mut after = before.clone();
                    after.push((buf, gx as f32, gy as f32));
                    self.undo.push(Command::UpdateExtraSpawns { before, after: after.clone() });
                    self.grid.extra_spawns = after;
                    self.unsaved = true;
                    self.placing_named_spawn = None;
                    self.ignore_drag = true;
                    self.save_message = Some("Named spawn placed.".to_string());
                    self.save_message_timer = 0;
                }
            }
            return;
        }

        // ── Text input: Palette Search ───────────────────────────────────────
        if self.palette_search_focused {
            use crate::input::Key;
            use super::{TEXT_INPUT_KEYS, key_to_char};
            let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
            for &key in TEXT_INPUT_KEYS {
                if input.just_pressed(key) {
                    if let Some(ch) = key_to_char(key, shift) {
                        self.palette.search.push(ch);
                    }
                }
            }
            if input.just_pressed(Key::Backspace) { self.palette.search.pop(); }
            if input.just_pressed(Key::Enter) || input.just_pressed(Key::Escape) { self.palette_search_focused = false; }
            if input.just_pressed(Key::Escape) { self.palette.search.clear(); }
            return;
        }

        // ── Text input ────────────────────────────────────────────────────────
        if self.text_input.is_some() {
            self.handle_text_input(input);
            return;
        }

        // ── Specialized interaction modes (panels, canvas, shortcuts) ──────────
        
        self.handle_panel_input(input, mouse);
        self.handle_canvas_input(input, mouse);
        self.handle_shortcuts(input, mouse);
    }
}
