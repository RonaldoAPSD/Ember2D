// editor/input/canvas.rs — Canvas interaction, painting, tools, and scrolling.

use super::super::commands::Command;
use super::super::ui::ToolKind;
use super::super::{EditorMode, EditorState};
use ember2d::input::Key;

impl EditorState {
    /// Only ever called while `mode` is `Paint`/`Inspect`/`Select`/`Paste`
    /// (`handle_update`'s dispatch handles every other mode itself) — see
    /// `EditorMode`'s own doc comment (7C-4, master plan §5.3). `Paste` and
    /// `Select` are fully self-contained (own Escape/click handling,
    /// `return` before ever reaching canvas scrolling — matching the
    /// original code's own early-return shape); `Paint` and `Inspect`
    /// share the "which tile is the mouse over, and scroll/zoom" tracking
    /// below, then diverge (`Inspect` stops there; `Paint` goes on to its
    /// own tool-specific dispatch).
    pub(super) fn handle_canvas_input(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let alt = input.is_held(Key::LeftAlt) || input.is_held(Key::RightAlt);

        // ── Middle-mouse drag pan ─────────────────────────────────────────────
        if mouse.middle_just_pressed() {
            self.pan_anchor = Some((
                mouse.cell_x,
                mouse.cell_y,
                self.target_scroll.0.round() as i32,
                self.target_scroll.1.round() as i32,
            ));
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
        if mouse.middle_just_released() {
            self.pan_anchor = None;
        }

        match std::mem::take(&mut self.mode) {
            EditorMode::Paste => {
                self.handle_paste_input(input, mouse);
                return;
            }
            EditorMode::Select { start, cutting } => {
                self.handle_select_input(start, cutting, input, mouse);
                return;
            }
            mode @ (EditorMode::Paint(_) | EditorMode::Inspect) => {
                self.mode = mode;
            }
            // Defensive: `handle_update` only reaches `handle_canvas_input`
            // for the four variants above.
            other => {
                self.mode = other;
                return;
            }
        }

        // ── Track inspected tile (last canvas cell the mouse was over) ────────
        // 7C-3 (master plan §5.3, E4): `canvas_x`/`canvas_y`/`canvas_w`
        // /`canvas_h` now read from the Viewport panel's own content rect
        // instead of the deleted `Layout`'s independent cell-based copy.
        let click = mouse.left_just_pressed();
        let vp = self.panels.viewport();
        let (canvas_x, canvas_y, canvas_w, canvas_h) =
            (vp.content_x(), vp.content_y(), vp.content_w(), vp.content_h());
        let on_canvas = mouse.in_bounds
            && mouse.cell_x >= canvas_x
            && mouse.cell_x < canvas_x + canvas_w
            && mouse.cell_y >= (canvas_y - 1) // Allow interaction on Viewport title bar (row 2)
            && mouse.cell_y < canvas_y + canvas_h
            // Pixel-space hit test (Phase 7 Part 1c, docs/ember2d-phase7-plan.md).
            && !self.panels.is_point_on_panel(mouse.pixel_x, mouse.pixel_y);

        if on_canvas {
            self.inspected_pos = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y);
            if click || mouse.right_just_pressed() {
                self.hierarchy_sel = None;
            }
        }

        let inspecting = matches!(self.mode, EditorMode::Inspect);
        if inspecting && click && on_canvas {
            self.selected_pos = self.inspected_pos;
            return;
        }

        // ── Canvas scrolling (Arrow keys + Wheel) ──────────────────────────────

        // Arrow keys — scroll canvas (smooth: fire on frame 1, then every 2 frames after a 12-frame delay).
        let any_arrow = input.is_held(Key::Left)
            || input.is_held(Key::Right)
            || input.is_held(Key::Up)
            || input.is_held(Key::Down);
        if any_arrow {
            self.scroll_repeat = self.scroll_repeat.saturating_add(1);
        } else {
            self.scroll_repeat = 0;
        }
        let do_scroll = self.scroll_repeat == 1
            || (self.scroll_repeat > 12 && self.scroll_repeat.is_multiple_of(2));
        if do_scroll {
            let scroll_speed = if shift { 5.0f32 } else { 1.0f32 };
            if input.is_held(Key::Left) {
                self.target_scroll.0 -= scroll_speed;
            }
            if input.is_held(Key::Right) {
                self.target_scroll.0 += scroll_speed;
            }
            if input.is_held(Key::Up) {
                self.target_scroll.1 -= scroll_speed;
            }
            if input.is_held(Key::Down) {
                self.target_scroll.1 += scroll_speed;
            }
            self.clamp_scroll();
        }

        // Mouse wheel — zoom canvas (ctrl+wheel for faster zoom).
        if on_canvas && mouse.wheel_y != 0.0 {
            let ctrl = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);

            // 1. Capture grid position under mouse before zoom — 7C-3
            // (master plan §5.3, E4): the pivot now uses the mouse's true
            // pixel position against the Viewport panel's own content rect,
            // not `mouse.cell_x`/`cell_y` against the deleted `Layout`'s
            // cell-quantized origin.
            let viewport = self.panels.viewport().content_rect();
            let mx = (mouse.pixel_x - viewport.x) / ember2d::renderer::CELL_W as f32;
            let my = (mouse.pixel_y - viewport.y) / ember2d::renderer::CELL_H as f32;

            let gx_before = mx / self.zoom + self.target_scroll.0;
            let gy_before = my / self.zoom + self.target_scroll.1;

            // 2. Apply multiplicative zoom
            let factor = if ctrl { 1.5f32 } else { 1.1f32 };
            if mouse.wheel_y > 0.0 {
                self.zoom *= factor;
            } else {
                self.zoom /= factor;
            }
            self.zoom = self.zoom.clamp(0.25, 4.0);

            // 3. Adjust scroll to keep the same grid point under the mouse
            self.target_scroll.0 = gx_before - mx / self.zoom;
            self.target_scroll.1 = gy_before - my / self.zoom;

            self.clamp_scroll();
        }
        if mouse.wheel_x != 0.0 {
            let dx = if mouse.wheel_x > 0.0 { 3.0f32 } else { -3.0f32 };
            self.target_scroll.0 += dx;
            self.clamp_scroll();
        }

        // In Inspect mode, no painting or erasing — only selection (above).
        if inspecting {
            return;
        }
        let tool = match self.mode {
            EditorMode::Paint(t) => t,
            _ => return, // unreachable given the match above, but no unwrap needed
        };

        // ── Toolbar sticky tools (no modifier needed) ─────────────────────────
        if !shift && !alt {
            match tool {
                ToolKind::Rect => {
                    // R13 (7A-2, docs/ember2d-master-plan.md): without the
                    // `!self.ignore_drag` guard every other paint path in
                    // this file already uses, a click that dismisses a menu
                    // or closes a dropdown (which sets `ignore_drag` for
                    // exactly this reason) also drops a rect anchor or
                    // stamps one straight onto the canvas underneath it.
                    if mouse.left_just_pressed() && !self.ignore_drag {
                        if let Some(pos) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                            self.rect_anchor = Some(pos);
                        }
                    }
                    if mouse.left_just_released() && !self.ignore_drag {
                        if let (Some(anchor), Some(current)) = (
                            self.rect_anchor.take(),
                            self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y),
                        ) {
                            self.stamp_rect(anchor, current);
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                            self.erase_brush(gx, gy);
                        }
                    } else if mouse.right_held() && self.erase_size == 1 {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
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
                    // R13: see the matching comment on ToolKind::Rect above.
                    if mouse.left_just_pressed() && !self.ignore_drag {
                        if let Some(pos) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                            if self.line_anchor.is_none() {
                                self.line_anchor = Some(pos);
                            } else {
                                self.stamp_line(self.line_anchor.unwrap(), pos);
                                self.line_anchor = None;
                                self.mode = EditorMode::Paint(ToolKind::Paint);
                                self.ignore_drag = true;
                            }
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                            self.erase_brush(gx, gy);
                        }
                    }
                    return;
                }
                ToolKind::Fill => {
                    // R13: see the matching comment on ToolKind::Rect above.
                    if mouse.left_just_pressed() && !self.ignore_drag {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                            self.flood_fill(gx, gy);
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                            self.erase_brush(gx, gy);
                        }
                    }
                    return;
                }
                ToolKind::Paint => {}
            }
        }

        // ── Mouse: rectangle tool (Shift+drag) ────────────────────────────────
        if shift {
            if mouse.left_just_pressed() {
                if let Some(pos) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                    self.rect_anchor = Some(pos);
                }
            }
            if mouse.left_just_released() {
                if let (Some(anchor), Some(current)) =
                    (self.rect_anchor.take(), self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y))
                {
                    self.stamp_rect(anchor, current);
                }
            }
            return;
        } else {
            self.rect_anchor = None;
        }

        // ── Mouse: alt+drag = scatter paint ──────────────────────────────────
        if alt && mouse.left_held() && !self.ignore_drag {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                if !self.grid.in_bounds(gx, gy) {
                    return;
                }
                let lyr = self.active_layer;
                if (gx * 1234 + gy * 5678 + self.undo.len() as i32) % 2 == 0 {
                    let mut new_tile = self.palette.current().to_tile_record(gx, gy);
                    new_tile.layer = lyr;
                    let existing = self.grid.get(gx, gy, lyr).cloned();
                    self.undo
                        .push(Command::PlaceTile { before: existing, after: new_tile.clone() });
                    self.grid.place(gx, gy, lyr, new_tile);
                    self.unsaved = true;
                }
            }
            return;
        }

        // ── Normal left-click paint ───────────────────────────────────────────
        if mouse.left_held() && !self.ignore_drag {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                if !self.grid.in_bounds(gx, gy) {
                    return;
                }
                let lyr = self.active_layer;
                let mut new_tile = self.palette.current().to_tile_record(gx, gy);
                new_tile.layer = lyr;
                let existing = self.grid.get(gx, gy, lyr).cloned();
                let same = existing
                    .as_ref()
                    .map(|t| {
                        t.glyph == new_tile.glyph
                            && t.solid == new_tile.solid
                            && t.trigger == new_tile.trigger
                            && t.tag == new_tile.tag
                    })
                    .unwrap_or(false);
                if !same {
                    self.undo
                        .push(Command::PlaceTile { before: existing, after: new_tile.clone() });
                    self.grid.place(gx, gy, lyr, new_tile);
                    self.unsaved = true;
                }
            }
        }

        // ── Right-click erase (brush size) ───────────────────────────────────
        if mouse.right_just_pressed() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                self.erase_brush(gx, gy);
            }
        } else if mouse.right_held() && self.erase_size == 1 {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                let lyr = self.active_layer;
                if let Some(removed) = self.grid.erase(gx, gy, lyr) {
                    self.undo.push(Command::EraseTile { before: removed });
                    self.unsaved = true;
                }
            }
        }
    }

    /// Was the `self.pasting` branch of `handle_canvas_input` — see that
    /// function's own doc comment (7C-4, master plan §5.3).
    fn handle_paste_input(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::Paste;
        if self.ignore_drag {
            return;
        }
        if input.just_pressed(Key::Escape) {
            self.mode = EditorMode::Paint(ToolKind::Paint);
            return;
        }
        if input.just_pressed(Key::H) {
            self.paste_flip_x = !self.paste_flip_x;
        }
        if input.just_pressed(Key::LeftBracket) {
            self.paste_rotate = (self.paste_rotate + 3) % 4;
        }
        if input.just_pressed(Key::RightBracket) {
            self.paste_rotate = (self.paste_rotate + 1) % 4;
        }
        if input.just_pressed(Key::J) {
            self.paste_flip_y = !self.paste_flip_y;
        }
        if mouse.left_just_pressed() {
            if let Some(cursor) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                self.stamp_paste(cursor);
                self.mode = EditorMode::Paint(ToolKind::Paint);
                self.ignore_drag = true;
            }
        }
    }

    /// Was the `self.selecting || self.cutting` branch of
    /// `handle_canvas_input` — see that function's own doc comment (7C-4,
    /// master plan §5.3). `start`/`cutting` are `EditorMode::Select`'s own
    /// fields, replacing the deleted `sel_anchor`/`selecting`/`cutting`.
    fn handle_select_input(
        &mut self,
        start: Option<(i32, i32)>,
        cutting: bool,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::Select { start, cutting };

        if input.just_pressed(Key::Escape) {
            self.mode = EditorMode::Paint(ToolKind::Paint);
            return;
        }

        if self.ignore_drag {
            return;
        }

        if mouse.left_just_pressed() {
            if let Some(pos) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                self.mode = EditorMode::Select { start: Some(pos), cutting };
            }
        }
        if mouse.left_just_released() {
            if let (Some(anchor), Some(current)) = (start, self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y))
            {
                if cutting {
                    self.cut_selection(anchor, current);
                } else {
                    self.copy_selection(anchor, current);
                }
            }

            // Only finish/reset if we actually started a selection or if it was a deliberate click
            if start.is_some() {
                self.mode = EditorMode::Paint(ToolKind::Paint);
            }
        }
    }
}
