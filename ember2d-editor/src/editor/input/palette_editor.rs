// editor/input/palette_editor.rs — Palette editor modal and advanced color
// picker input. Extracted from `input/mod.rs` (7D-3, master plan §5.4)
// alongside that step's own R65 fix — see each function's own doc comment.

use super::super::commands::Command;
use super::super::ui::{ToolKind, WidgetId, PALETTE_COLORS};
use super::super::{EditorMode, EditorState, PaletteField};

impl EditorState {
    /// R65 (§3 in the master plan): the hue bar and SV map read their own
    /// registered rect back via `UiFrame::rect_of` (populated by
    /// `draw_color_picker_modal`) instead of recomputing `mx`/`cx`/
    /// `hbar_x`/`map_x` independently here (E5); the title-close/Apply/
    /// Cancel buttons read `UiFrame::hit` the same way every other migrated
    /// button in this step does. Always returns to `PaletteEditor` — the
    /// only mode that ever opens this one (7C-4, master plan §5.3). This
    /// modal's own step-count division (`cells_w`/`cells_h`) was already
    /// exact before this step — it derives its step count from the SAME
    /// rect `draw_color_picker_modal` pushed, not an independent literal —
    /// so it's unchanged here beyond the `PanelId` import move.
    pub(super) fn handle_color_picker_input(
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
            // 7D-3 checkpoint 7 (master plan §5.4): logical -> points, the
            // input choke point every chrome hit-test now goes through —
            // `rect_of`'s own rects are points-space (`draw_color_picker_modal`
            // draws through `UiPainter` now), so the mouse position being
            // compared/subtracted against them must be too. The hue-bar/
            // SV-map step size stays a literal `CELL_W`/`CELL_H` count
            // either way (R65's own "literal content, not chrome"
            // reasoning) — only the SPACE the comparison happens in changed.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            // 1. Hue Bar interaction
            if let Some(rect) = self.ui_frame.rect_of(WidgetId::ColorPickerHueBar) {
                if rect.contains(px, py) {
                    let cells_w = (rect.w / ember2d::renderer::CELL_W as f32).round();
                    let col = ((px - rect.x) / ember2d::renderer::CELL_W as f32).floor();
                    let pct = col / (cells_w - 1.0);
                    self.color_picker_hsv.0 = pct.clamp(0.0, 1.0) * 360.0;
                }
            }

            // 2. SV Map interaction
            if let Some(rect) = self.ui_frame.rect_of(WidgetId::ColorPickerSvMap) {
                if rect.contains(px, py) {
                    let cells_w = (rect.w / ember2d::renderer::CELL_W as f32).round();
                    let cells_h = (rect.h / ember2d::renderer::CELL_H as f32).round();
                    let col = ((px - rect.x) / ember2d::renderer::CELL_W as f32).floor();
                    let row = ((py - rect.y) / ember2d::renderer::CELL_H as f32).floor();
                    self.color_picker_hsv.1 = (col / (cells_w - 1.0)).clamp(0.0, 1.0);
                    self.color_picker_hsv.2 = 1.0 - (row / (cells_h - 1.0)).clamp(0.0, 1.0);
                }
            }

            if mouse.left_just_pressed() {
                match self.ui_frame.hit(px, py) {
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

    /// R65 (§3 in the master plan): every interactive row/button/toggle now
    /// has its own `WidgetId`, pushed by `draw_palette_editor_modal` at the
    /// exact point it's drawn — this handler reads them all back via
    /// `UiFrame::hit` instead of comparing `mouse.cell_x`/`mouse.cell_y`
    /// against `mx`/`my`/`cx` values it used to recompute independently
    /// (in CELL units, from a rounded `screen_cells()`, while the draw side
    /// centered in real px — the actual mismatch: an odd leftover cell
    /// remainder, true at the default 1280×720, put every row half a cell
    /// off from where the input handler expected it). `PanelId` import
    /// dropped — this file never actually needed it once the cell-based
    /// `screen_size_cells`-derived centering above went away.
    pub(super) fn handle_palette_editor_input(
        &mut self,
        input: &mut ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::PaletteEditor;
        use ember2d::input::Key;

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
            // 7D-3 checkpoint 7: logical -> points, same input choke point
            // as `handle_color_picker_input`.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            match self.ui_frame.hit(px, py) {
                Some(WidgetId::PaletteEditorClose) => {
                    self.palette_editor_focus = None;
                    self.close_palette_editor();
                    return;
                }
                Some(WidgetId::PaletteEditorSaveClose) => {
                    self.palette_editor_focus = None;
                    self.save_palette();
                    self.close_palette_editor();
                    return;
                }
                Some(WidgetId::PaletteEditorDelete) => {
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
                Some(WidgetId::PaletteEditorField(field)) => {
                    self.palette_editor_focus = Some(field);
                }
                Some(WidgetId::PaletteEditorToggle { is_solid: true }) => {
                    self.palette_editor_focus = None;
                    self.palette.tiles[sel].solid = !self.palette.tiles[sel].solid;
                    self.unsaved = true;
                }
                Some(WidgetId::PaletteEditorToggle { is_solid: false }) => {
                    self.palette_editor_focus = None;
                    self.palette.tiles[sel].trigger = !self.palette.tiles[sel].trigger;
                    self.unsaved = true;
                }
                Some(WidgetId::PaletteEditorSwatch { is_fg: true, index }) => {
                    self.palette_editor_focus = None;
                    if let Some(&col) = PALETTE_COLORS.get(index) {
                        self.palette.tiles[sel].fg = col;
                        self.unsaved = true;
                    }
                }
                Some(WidgetId::PaletteEditorSwatch { is_fg: false, index }) => {
                    self.palette_editor_focus = None;
                    if let Some(&col) = PALETTE_COLORS.get(index) {
                        self.palette.tiles[sel].bg = col;
                        self.unsaved = true;
                    }
                }
                Some(WidgetId::PaletteEditorCustomColor { is_fg: true }) => {
                    self.palette_editor_focus = None;
                    let col = self.palette.tiles[sel].fg;
                    self.color_picker_hsv = col.to_hsv(ember2d::renderer::color::DEFAULT_FG);
                    self.mode = EditorMode::ColorPicker { is_fg: true };
                    return;
                }
                Some(WidgetId::PaletteEditorCustomColor { is_fg: false }) => {
                    self.palette_editor_focus = None;
                    let col = self.palette.tiles[sel].bg;
                    self.color_picker_hsv = col.to_hsv(ember2d::renderer::color::DEFAULT_BG);
                    self.mode = EditorMode::ColorPicker { is_fg: false };
                    return;
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
}
