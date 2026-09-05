// editor/input/canvas.rs — Canvas interaction, painting, tools, and scrolling.

use ember2d::input::Key;
use super::super::EditorState;
use super::super::ui::ToolKind;
use super::super::commands::Command;

impl EditorState {
    pub(super) fn handle_canvas_input(&mut self, input: &ember2d::input::InputManager, mouse: &ember2d::mouse::MouseState) {
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let alt   = input.is_held(Key::LeftAlt)   || input.is_held(Key::RightAlt);

        // ── Middle-mouse drag pan ─────────────────────────────────────────────
        if mouse.middle_just_pressed() {
            self.pan_anchor = Some((mouse.cell_x, mouse.cell_y, self.target_scroll.0.round() as i32, self.target_scroll.1.round() as i32));
        }
        if mouse.middle_held() {
            if let Some((ax, ay, sx, sy)) = self.pan_anchor {
                let dx = (mouse.cell_x as i32 - ax as i32) as f32 / self.zoom;
                let dy = (mouse.cell_y as i32 - ay as i32) as f32 / self.zoom;
                self.target_scroll.0 = sx as f32 - dx;
                self.target_scroll.1 = sy as f32 - dy;
                self.clamp_scroll();
            }
        }
        if mouse.middle_just_released() { self.pan_anchor = None; }

        // ── Paste mode ────────────────────────────────────────────────────────
        if self.pasting && !self.ignore_drag {
            if input.just_pressed(Key::Escape) {
                self.pasting = false;
                self.active_tool = ToolKind::Paint;
                return;
            }
            if input.just_pressed(Key::H) { self.paste_flip_x = !self.paste_flip_x; }
            if input.just_pressed(Key::LeftBracket)  { self.paste_rotate = (self.paste_rotate + 3) % 4; }
            if input.just_pressed(Key::RightBracket) { self.paste_rotate = (self.paste_rotate + 1) % 4; }
            if input.just_pressed(Key::J) { self.paste_flip_y = !self.paste_flip_y; }
            if mouse.left_just_pressed() {
                if let Some(cursor) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.stamp_paste(cursor);
                    self.pasting     = false;
                    self.active_tool = ToolKind::Paint;
                    self.ignore_drag = true;
                }
            }
            return;
        }

        // ── Copy/Cut select mode ──────────────────────────────────────────────
        if self.selecting || self.cutting {
            if input.just_pressed(Key::Escape) {
                self.selecting   = false;
                self.cutting     = false;
                self.sel_anchor  = None;
                self.active_tool = ToolKind::Paint;
                return;
            }

            if !self.ignore_drag {
                if mouse.left_just_pressed() {
                    if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                        self.sel_anchor = Some(pos);
                    }
                }
                if mouse.left_just_released() {
                    if let (Some(anchor), Some(current)) = (
                        self.sel_anchor, self.mouse_to_grid(mouse.cell_x, mouse.cell_y),
                    ) {
                        if self.cutting { self.cut_selection(anchor, current); }
                        else            { self.copy_selection(anchor, current); }
                    }

                    // Only finish/reset if we actually started a selection or if it was a deliberate click
                    if self.sel_anchor.is_some() {
                        self.selecting   = false;
                        self.cutting     = false;
                        self.sel_anchor  = None;
                        self.active_tool = ToolKind::Paint;
                    }
                }
            }
            return;
        }

        // ── Track inspected tile (last canvas cell the mouse was over) ────────
        let click = mouse.left_just_pressed();
        let l = &self.layout;
        let on_canvas = mouse.in_bounds
            && mouse.cell_x >= l.canvas_x
            && mouse.cell_x < l.canvas_x + l.canvas_w
            && mouse.cell_y >= (l.canvas_y - 1) // Allow interaction on Viewport title bar (row 2)
            && mouse.cell_y < l.canvas_y + l.canvas_h
            && !self.panels.is_point_on_panel(mouse.cell_x, mouse.cell_y);

        if on_canvas {
            self.inspected_pos = self.mouse_to_grid(mouse.cell_x, mouse.cell_y);
            if click || mouse.right_just_pressed() {
                self.hierarchy_sel = None;
            }
        }

        if self.select_mode && click && on_canvas {
            self.selected_pos = self.inspected_pos;
            return;
        }

        // ── Canvas scrolling (Arrow keys + Wheel) ──────────────────────────────
        
        // Arrow keys — scroll canvas (smooth: fire on frame 1, then every 2 frames after a 12-frame delay).
        let any_arrow = input.is_held(Key::Left) || input.is_held(Key::Right)
            || input.is_held(Key::Up) || input.is_held(Key::Down);
        if any_arrow { self.scroll_repeat = self.scroll_repeat.saturating_add(1); }
        else { self.scroll_repeat = 0; }
        let do_scroll = self.scroll_repeat == 1
            || (self.scroll_repeat > 12 && self.scroll_repeat % 2 == 0);
        if do_scroll {
            let scroll_speed = if shift { 5.0f32 } else { 1.0f32 };
            if input.is_held(Key::Left)  { self.target_scroll.0 -= scroll_speed; }
            if input.is_held(Key::Right) { self.target_scroll.0 += scroll_speed; }
            if input.is_held(Key::Up)    { self.target_scroll.1 -= scroll_speed; }
            if input.is_held(Key::Down)  { self.target_scroll.1 += scroll_speed; }
            self.clamp_scroll();
        }

        // Mouse wheel — zoom canvas (ctrl+wheel for faster zoom).
        if on_canvas && mouse.wheel_y != 0.0 {
            let ctrl = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);
            
            // 1. Capture grid position under mouse before zoom
            let mx = mouse.cell_x as f32;
            let my = mouse.cell_y as f32;
            let cx = self.layout.canvas_x as f32;
            let cy = self.layout.canvas_y as f32;
            
            let gx_before = (mx - cx) / self.zoom + self.target_scroll.0;
            let gy_before = (my - cy) / self.zoom + self.target_scroll.1;

            // 2. Apply multiplicative zoom
            let factor = if ctrl { 1.5f32 } else { 1.1f32 };
            if mouse.wheel_y > 0.0 {
                self.zoom *= factor;
            } else {
                self.zoom /= factor;
            }
            self.zoom = self.zoom.clamp(0.25, 4.0);

            // 3. Adjust scroll to keep the same grid point under the mouse
            self.target_scroll.0 = gx_before - (mx - cx) / self.zoom;
            self.target_scroll.1 = gy_before - (my - cy) / self.zoom;
            
            self.clamp_scroll();
        }
        if mouse.wheel_x != 0.0 {
            let dx = if mouse.wheel_x > 0.0 { 3.0f32 } else { -3.0f32 };
            self.target_scroll.0 += dx;
            self.clamp_scroll();
        }

        // In select mode, no painting or erasing — only selection.
        if self.select_mode { return; }

        // ── Toolbar sticky tools (no modifier needed) ─────────────────────────
        if !shift && !alt {
            match self.active_tool {
                ToolKind::Rect => {
                    if mouse.left_just_pressed() {
                        if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.rect_anchor = Some(pos);
                        }
                    }
                    if mouse.left_just_released() {
                        if let (Some(anchor), Some(current)) = (
                            self.rect_anchor.take(),
                            self.mouse_to_grid(mouse.cell_x, mouse.cell_y),
                        ) {
                            self.stamp_rect(anchor, current);
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.erase_brush(gx, gy);
                        }
                    } else if mouse.right_held() && self.erase_size == 1 {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            let lyr = self.active_layer;
                            if let Some(removed) = self.grid.erase(gx, gy, lyr) {
                                self.undo.push(Command::EraseTile { before: removed });
                                self.unsaved = true;
                            }
                        }
                    }
                    return;
                }
                ToolKind::Line => {
                    if mouse.left_just_pressed() {
                        if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            if self.line_anchor.is_none() {
                                self.line_anchor = Some(pos);
                            } else {
                                self.stamp_line(self.line_anchor.unwrap(), pos);
                                self.line_anchor = None;
                                self.active_tool = ToolKind::Paint;
                                self.ignore_drag = true;
                            }
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.erase_brush(gx, gy);
                        }
                    }
                    return;
                }
                ToolKind::Fill => {
                    if mouse.left_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.flood_fill(gx, gy);
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.erase_brush(gx, gy);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // ── Mouse: rectangle tool (Shift+drag) ────────────────────────────────
        if shift {
            if mouse.left_just_pressed() {
                if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.rect_anchor = Some(pos);
                }
            }
            if mouse.left_just_released() {
                if let (Some(anchor), Some(current)) = (
                    self.rect_anchor.take(),
                    self.mouse_to_grid(mouse.cell_x, mouse.cell_y),
                ) {
                    self.stamp_rect(anchor, current);
                }
            }
            return;
        } else {
            self.rect_anchor = None;
        }

        // ── Mouse: alt+drag = scatter paint ──────────────────────────────────
        if alt && mouse.left_held() && !self.ignore_drag {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if !self.grid.in_bounds(gx, gy) { return; }
                let lyr = self.active_layer;
                if (gx * 1234 + gy * 5678 + self.undo.len() as i32) % 2 == 0 {
                    let mut new_tile = self.palette.current().to_tile_record(gx, gy);
                    new_tile.layer = lyr;
                    let existing = self.grid.get(gx, gy, lyr).cloned();
                    self.undo.push(Command::PlaceTile { before: existing, after: new_tile.clone() });
                    self.grid.place(gx, gy, lyr, new_tile);
                    self.unsaved = true;
                }
            }
            return;
        }

        // ── Normal left-click paint ───────────────────────────────────────────
        if mouse.left_held() && !self.ignore_drag {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if !self.grid.in_bounds(gx, gy) { return; }
                let lyr = self.active_layer;
                let mut new_tile = self.palette.current().to_tile_record(gx, gy);
                new_tile.layer = lyr;
                let existing = self.grid.get(gx, gy, lyr).cloned();
                let same = existing.as_ref().map(|t| {
                    t.glyph == new_tile.glyph && t.solid == new_tile.solid
                        && t.trigger == new_tile.trigger && t.tag == new_tile.tag
                }).unwrap_or(false);
                if !same {
                    self.undo.push(Command::PlaceTile { before: existing, after: new_tile.clone() });
                    self.grid.place(gx, gy, lyr, new_tile);
                    self.unsaved = true;
                }
            }
        }

        // ── Right-click erase (brush size) ───────────────────────────────────
        if mouse.right_just_pressed() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.erase_brush(gx, gy);
            }
        } else if mouse.right_held() && self.erase_size == 1 {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                let lyr = self.active_layer;
                if let Some(removed) = self.grid.erase(gx, gy, lyr) {
                    self.undo.push(Command::EraseTile { before: removed });
                    self.unsaved = true;
                }
            }
        }
    }
}
