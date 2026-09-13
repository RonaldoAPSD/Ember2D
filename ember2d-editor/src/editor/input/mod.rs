// editor/input/mod.rs — Input orchestration for EditorState.

use super::commands::Command;
use super::panel::PanelId;
use super::ui::{ToolKind, WidgetId, PALETTE_COLORS};
use super::{EditorMode, EditorState, PaletteField};
use ember2d::engine::UpdateContext;

mod canvas;
mod context_menu;
mod graph;
mod modal;
mod panels;
mod script_editor;
mod shortcuts;
mod text;

impl EditorState {
    pub(super) fn handle_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { input, mouse, .. } = ctx;

        // ── Ignore drag state ─────────────────────────────────────────────────
        if self.ignore_drag && !mouse.left_held() {
            self.ignore_drag = false;
        }

        // Tick save message.
        if self.save_message.is_some() {
            self.save_message_timer += 1;
            if self.save_message_timer > 90 {
                self.save_message = None;
                self.save_message_timer = 0;
            }
        }

        // R14 (7A-2, docs/ember2d-master-plan.md): the DOCKED script panel
        // having focus swallows keyboard input the same way fullscreen
        // `EditorMode::Script` does — but unlike fullscreen, other panels
        // are still visible and clickable, so a click OUTSIDE the script
        // panel's own bounds must still fall through to
        // `handle_panel_input`, which is what reassigns `focused_panel` to
        // whatever was actually clicked. Checked before the `mode` match
        // below since it's an orthogonal FOCUS concern, not part of `mode`
        // itself — see `EditorMode::Script`'s own doc comment.
        if self.focused_panel == Some(PanelId::ScriptEditor) && !matches!(self.mode, EditorMode::Script)
        {
            let p = self.panels.get(PanelId::ScriptEditor);
            let click_outside = mouse.left_just_pressed()
                && !(mouse.in_bounds && p.contains(mouse.pixel_x, mouse.pixel_y));
            if !click_outside {
                self.handle_script_mode_input(input, mouse);
                return;
            }
        }

        // 7C-4 (master plan §5.3): one match on `mode` replaces the old
        // if-chain (modal → context menu → color picker → palette editor →
        // graph → script → spawn placement → palette search → text input),
        // whose correctness used to depend on checking those in exactly
        // this order. Every "hard exclusive" arm handles its own input and
        // returns; `Paint`/`Inspect`/`Select`/`Paste` fall through to the
        // shared panels/canvas/shortcuts dispatch below, since none of them
        // stop the rest of the editor (other panels, global shortcuts)
        // from working normally alongside them. `mem::take` moves `mode`
        // out by value rather than matching a borrow of it, since several
        // arms below need to call other `&mut self` methods (including,
        // for `Modal`, `*self = ns` on a successful level switch) that
        // can't run while `self.mode` is still borrowed.
        match std::mem::take(&mut self.mode) {
            EditorMode::Modal(modal) => {
                self.handle_modal_input(modal, input, mouse);
                return;
            }
            EditorMode::ContextMenu(menu) => {
                self.handle_context_menu_input(menu, input, mouse);
                return;
            }
            EditorMode::ColorPicker { is_fg } => {
                self.handle_color_picker_input(is_fg, input, mouse);
                return;
            }
            EditorMode::PaletteEditor => {
                self.handle_palette_editor_input(input, mouse);
                return;
            }
            EditorMode::Graph { gx, gy } => {
                self.update_graph_mode(gx, gy, input, mouse);
                return;
            }
            EditorMode::Script => {
                // Found live by the user (2026-09-12): every other
                // hard-exclusive arm's own handler restores `self.mode`
                // as its first action (see `handle_color_picker_input`/
                // `handle_palette_editor_input`/`handle_place_spawn_input`/
                // `handle_palette_search_input`'s own first lines) since
                // `std::mem::take` above already emptied it to
                // `EditorMode::default()`. This arm never did — a unit
                // variant has no payload to reconstruct the handler side,
                // so it's restored here instead, before the call.
                // Without it, `handle_script_mode_input`'s own
                // `fullscreen = matches!(self.mode, EditorMode::Script)`
                // always read `false` (mode was already `Paint` by the
                // time it ran), and `self.mode` stayed `Paint` for
                // `draw()` to read — the fullscreen editor rendered for
                // the one frame `load_script` set it, then silently
                // reverted on every frame after, invisible at 60fps.
                self.mode = EditorMode::Script;
                self.handle_script_mode_input(input, mouse);
                return;
            }
            EditorMode::PlaceSpawn(named) => {
                self.handle_place_spawn_input(named, input, mouse);
                return;
            }
            EditorMode::PaletteSearch => {
                self.handle_palette_search_input(input);
                return;
            }
            EditorMode::Prompt(purpose) => {
                self.handle_text_input(purpose, input);
                return;
            }
            mode @ (EditorMode::Paint(_)
            | EditorMode::Inspect
            | EditorMode::Select { .. }
            | EditorMode::Paste) => {
                self.mode = mode;
            }
        }

        // ── Specialized interaction modes (panels, canvas, shortcuts) ──────────

        self.handle_panel_input(input, mouse);
        self.handle_canvas_input(input, mouse);
        self.handle_shortcuts(input, mouse);

        // ── Smooth camera lerp ────────────────────────────────────────────────
        let lerp_factor = 0.2; // Snappiness
        self.scroll.0 += (self.target_scroll.0 - self.scroll.0) * lerp_factor;
        self.scroll.1 += (self.target_scroll.1 - self.scroll.1) * lerp_factor;

        // Snap if close enough to prevent infinite micro-movement
        if (self.scroll.0 - self.target_scroll.0).abs() < 0.001 {
            self.scroll.0 = self.target_scroll.0;
        }
        if (self.scroll.1 - self.target_scroll.1).abs() < 0.001 {
            self.scroll.1 = self.target_scroll.1;
        }
    }

    // 7C-1 (master plan §5.3): the hue bar and SV map read their own
    // registered rect back via `UiFrame::rect_of` (populated by
    // `draw_color_picker_modal`) instead of recomputing `mx`/`cx`/
    // `hbar_x`/`map_x` independently here (E5); the title-close/Apply/
    // Cancel buttons read `UiFrame::hit` the same way every other migrated
    // button in this step does. Always returns to `PaletteEditor` — the
    // only mode that ever opens this one (7C-4, master plan §5.3).
    fn handle_color_picker_input(
        &mut self,
        is_fg: bool,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::ColorPicker { is_fg };

        if input.just_pressed(ember2d::input::Key::Escape) {
            self.mode = EditorMode::PaletteEditor;
            return;
        }

        if mouse.left_held() && mouse.in_bounds {
            // 1. Hue Bar interaction
            if let Some(rect) = self.ui_frame.rect_of(WidgetId::ColorPickerHueBar) {
                if rect.contains(mouse.pixel_x, mouse.pixel_y) {
                    let cells_w = (rect.w / ember2d::renderer::CELL_W as f32).round();
                    let col = ((mouse.pixel_x - rect.x) / ember2d::renderer::CELL_W as f32).floor();
                    let pct = col / (cells_w - 1.0);
                    self.color_picker_hsv.0 = pct.clamp(0.0, 1.0) * 360.0;
                }
            }

            // 2. SV Map interaction
            if let Some(rect) = self.ui_frame.rect_of(WidgetId::ColorPickerSvMap) {
                if rect.contains(mouse.pixel_x, mouse.pixel_y) {
                    let cells_w = (rect.w / ember2d::renderer::CELL_W as f32).round();
                    let cells_h = (rect.h / ember2d::renderer::CELL_H as f32).round();
                    let col = ((mouse.pixel_x - rect.x) / ember2d::renderer::CELL_W as f32).floor();
                    let row = ((mouse.pixel_y - rect.y) / ember2d::renderer::CELL_H as f32).floor();
                    self.color_picker_hsv.1 = (col / (cells_w - 1.0)).clamp(0.0, 1.0);
                    self.color_picker_hsv.2 = 1.0 - (row / (cells_h - 1.0)).clamp(0.0, 1.0);
                }
            }

            if mouse.left_just_pressed() {
                match self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                    Some(WidgetId::ColorPickerClose) => {
                        self.mode = EditorMode::PaletteEditor;
                    }
                    Some(WidgetId::ColorPickerApply) => {
                        let final_col = ember2d::renderer::color::Color::from_hsv(
                            self.color_picker_hsv.0,
                            self.color_picker_hsv.1,
                            self.color_picker_hsv.2,
                        );
                        let sel = self.palette.selected;
                        if is_fg {
                            self.palette.tiles[sel].fg = final_col;
                        } else {
                            self.palette.tiles[sel].bg = final_col;
                        }
                        self.unsaved = true;
                        self.mode = EditorMode::PaletteEditor;
                    }
                    Some(WidgetId::ColorPickerCancel) => {
                        self.mode = EditorMode::PaletteEditor;
                    }
                    _ => {}
                }
            }
        }
    }

    /// Close the palette editor, pushing the whole open-edit-close session
    /// as one `Command::UpdatePalette` if a session was actually open
    /// (7C-6, master plan §5.3, D18) — see `palette_edit_before`'s own doc
    /// comment. Called from every exit path (`[X]`, Save & Close,
    /// item-delete, Escape) so none of them can forget it.
    fn close_palette_editor(&mut self) {
        if let Some(before) = self.palette_edit_before.take() {
            self.undo.push(Command::UpdatePalette { before, after: self.palette.clone() });
        }
        self.mode = EditorMode::Paint(ToolKind::Paint);
    }

    fn handle_palette_editor_input(
        &mut self,
        input: &mut ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::PaletteEditor;
        use ember2d::input::Key;

        let mw = 36usize;
        let mh = 18usize;
        // `self.ui_space.screen_cells()` (7D-3, docs/ember2d-master-plan.md
        // §5.4) replaces the removed `PanelManager::screen_size_cells` with
        // the identical numeric value (both ultimately divide the same
        // logical screen size by `CELL_W`/`CELL_H`) — this modal's own
        // R65 fix (drawing centers in real px while this input handler
        // centers in cells, landing clicks on the wrong field when the
        // cell-count remainder is odd) is a separate, later checkpoint of
        // this same step; this is only the type-signature update needed to
        // keep building in the meantime.
        let (screen_w, screen_h) = self.ui_space.screen_cells();
        let mx = (screen_w.saturating_sub(mw)) / 2;
        let my = (screen_h.saturating_sub(mh)) / 2;
        let cx = mx + 2;
        let sel = self.palette_editing_idx;

        // 1. Inline Typing Logic
        // R12 (7A-2, docs/ember2d-master-plan.md): reads the engine's
        // captured text (`take_text`) instead of the old physical-key +
        // US-QWERTY `key_to_char` lookup — see script_editor.rs's own
        // R12 comment for why. `begin_text_capture` only while a field
        // actually has focus, renewed every such frame.
        if let Some(focus) = self.palette_editor_focus {
            input.begin_text_capture();
            for ch in input.take_text().chars() {
                match focus {
                    PaletteField::Name => {
                        self.palette.tiles[sel].name.push(ch);
                        self.unsaved = true;
                    }
                    PaletteField::Tag => {
                        self.palette.tiles[sel].tag.push(ch);
                        self.unsaved = true;
                    }
                    PaletteField::Glyph => {
                        self.palette.tiles[sel].glyph = ch;
                        self.unsaved = true;
                    }
                }
            }
            if input.just_pressed(Key::Backspace) {
                match focus {
                    PaletteField::Name => {
                        self.palette.tiles[sel].name.pop();
                        self.unsaved = true;
                    }
                    PaletteField::Tag => {
                        self.palette.tiles[sel].tag.pop();
                        self.unsaved = true;
                    }
                    _ => {}
                }
            }
            if input.just_pressed(Key::Enter) {
                self.palette_editor_focus = None;
            }
        }

        // 2. Mouse Interaction
        if mouse.left_just_pressed() && mouse.in_bounds {
            // 1. [X] button in title bar (Cancel)
            if mouse.cell_y == my && mouse.cell_x >= mx + mw - 4 && mouse.cell_x < mx + mw - 1 {
                self.palette_editor_focus = None;
                self.close_palette_editor();
                return;
            }

            // 2. Bottom row buttons
            let btn_y = my + mh - 2;
            if mouse.cell_y == btn_y {
                // [ Save & Close ] (left)
                if mouse.cell_x >= mx + 2 && mouse.cell_x < mx + 20 {
                    self.palette_editor_focus = None;
                    self.save_palette();
                    self.close_palette_editor();
                    return;
                }
                // [ Delete ] (right)
                if mouse.cell_x >= mx + 22 && mouse.cell_x < mx + 34 {
                    if self.palette.tiles.len() > 1 {
                        self.palette.tiles.remove(sel);
                        self.palette.selected =
                            self.palette.selected.min(self.palette.tiles.len() - 1);
                        self.palette_editor_focus = None;
                        self.unsaved = true;
                        self.save_message = Some("Asset deleted.".to_string());
                        self.save_message_timer = 0;
                        self.save_palette();
                        self.close_palette_editor();
                    } else {
                        self.save_message = Some("Cannot delete last item!".to_string());
                        self.save_message_timer = 0;
                    }
                    return;
                }
            }

            // 3. Field Clicks
            match mouse.cell_y {
                r if r == my + 2 => {
                    self.palette_editor_focus = Some(PaletteField::Name);
                }
                r if r == my + 3 => {
                    self.palette_editor_focus = Some(PaletteField::Glyph);
                }
                r if r == my + 4 => {
                    self.palette_editor_focus = None;
                    if mouse.cell_x >= cx && mouse.cell_x < cx + 10 {
                        self.palette.tiles[sel].solid = !self.palette.tiles[sel].solid;
                        self.unsaved = true;
                    } else if mouse.cell_x >= cx + 13 && mouse.cell_x < cx + 25 {
                        self.palette.tiles[sel].trigger = !self.palette.tiles[sel].trigger;
                        self.unsaved = true;
                    }
                }
                r if r == my + 5 => {
                    self.palette_editor_focus = Some(PaletteField::Tag);
                }
                // 7C-1 (master plan §5.3): reads the swatch's own
                // `WidgetId::PaletteEditorSwatch` back from `UiFrame`
                // (populated by `draw_palette_editor_modal` via
                // `draw_swatch`) instead of recomputing `gx_start`/
                // `col_idx`/`row_idx` from `cx` independently here (E5).
                r if r == my + 8 || r == my + 9 => {
                    self.palette_editor_focus = None;
                    if let Some(WidgetId::PaletteEditorSwatch { is_fg: true, index }) =
                        self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y)
                    {
                        if let Some(&col) = PALETTE_COLORS.get(index) {
                            self.palette.tiles[sel].fg = col;
                            self.unsaved = true;
                        }
                    }
                }
                r if r == my + 12 || r == my + 13 => {
                    self.palette_editor_focus = None;
                    if let Some(WidgetId::PaletteEditorSwatch { is_fg: false, index }) =
                        self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y)
                    {
                        if let Some(&col) = PALETTE_COLORS.get(index) {
                            self.palette.tiles[sel].bg = col;
                            self.unsaved = true;
                        }
                    }
                }
                r if r == my + 10 => {
                    self.palette_editor_focus = None;
                    if mouse.cell_x >= cx && mouse.cell_x < cx + 30 {
                        let col = self.palette.tiles[sel].fg;
                        self.color_picker_hsv = col.to_hsv(ember2d::renderer::color::DEFAULT_FG);
                        self.mode = EditorMode::ColorPicker { is_fg: true };
                        return;
                    }
                }
                r if r == my + 14 => {
                    self.palette_editor_focus = None;
                    if mouse.cell_x >= cx && mouse.cell_x < cx + 30 {
                        let col = self.palette.tiles[sel].bg;
                        self.color_picker_hsv = col.to_hsv(ember2d::renderer::color::DEFAULT_BG);
                        self.mode = EditorMode::ColorPicker { is_fg: false };
                        return;
                    }
                }
                _ => {
                    self.palette_editor_focus = None;
                }
            }
        }
        if input.just_pressed(Key::Escape) {
            // Escape dismisses the palette editor without writing
            // anything (7A-8, master plan §5.1 "Hygiene") — only the
            // explicit "Save & Close" button and item-delete above
            // persist a customized palette to disk.
            if self.palette_editor_focus.is_some() {
                self.palette_editor_focus = None;
            } else {
                self.close_palette_editor();
            }
        }
    }

    fn handle_place_spawn_input(
        &mut self,
        named: Option<String>,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::PlaceSpawn(named.clone());

        if input.just_pressed(ember2d::input::Key::Escape) || mouse.right_just_pressed() {
            self.save_message = Some("Spawn placement cancelled.".to_string());
            self.save_message_timer = 0;
            self.mode = EditorMode::Paint(ToolKind::Paint);
            return;
        }
        if mouse.left_just_pressed() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                match named {
                    None => {
                        let before = self.grid.spawn_point;
                        let after = (gx as f32, gy as f32);
                        self.undo.push(Command::MoveSpawn { before, after });
                        self.grid.spawn_point = after;
                        self.save_message = Some("Spawn placed.".to_string());
                    }
                    Some(name) => {
                        let before = self.grid.extra_spawns.clone();
                        let mut after = before.clone();
                        after.push((name, gx as f32, gy as f32));
                        self.undo.push(Command::UpdateExtraSpawns { before, after: after.clone() });
                        self.grid.extra_spawns = after;
                        self.save_message = Some("Named spawn placed.".to_string());
                    }
                }
                self.unsaved = true;
                self.ignore_drag = true;
                self.save_message_timer = 0;
                self.mode = EditorMode::Paint(ToolKind::Paint);
            }
        }
    }

    fn handle_palette_search_input(&mut self, input: &mut ember2d::input::InputManager) {
        self.mode = EditorMode::PaletteSearch;
        use ember2d::input::Key;
        // R12 (7A-2, docs/ember2d-master-plan.md): see script_editor.rs's
        // own R12 comment for why take_text() replaces key_to_char here.
        input.begin_text_capture();
        for ch in input.take_text().chars() {
            self.palette.search.push(ch);
        }
        if input.just_pressed(Key::Backspace) {
            self.palette.search.pop();
        }
        if input.just_pressed(Key::Enter) || input.just_pressed(Key::Escape) {
            self.mode = EditorMode::Paint(ToolKind::Paint);
        }
        if input.just_pressed(Key::Escape) {
            self.palette.search.clear();
        }
    }
}
