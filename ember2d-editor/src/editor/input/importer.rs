// editor/input/importer.rs — keyboard and mouse for the tileset importer
// dialog (`EditorMode::TilesetImport`).
//
// Step 8-2 (docs/ember2d-master-plan.md §5.7). Same shape as
// palette_editor.rs: typed text arrives through `InputManager::take_text`
// while a field has focus (R12's captured-text path, never a physical-key
// lookup), and every click is resolved against the rects
// `draw_tileset_import_modal` registered in `UiFrame` this frame, in points
// — nothing here recomputes the dialog's layout.

use ember2d::input::Key;

use super::super::importer::ImportField;
use super::super::ui::WidgetId;
use super::super::{EditorMode, EditorState};

/// Tab order through the dialog's fields.
const ORDER: [ImportField; 6] = [
    ImportField::Name,
    ImportField::CellW,
    ImportField::CellH,
    ImportField::Margin,
    ImportField::Spacing,
    ImportField::Region,
];

impl EditorState {
    pub(super) fn handle_tileset_import_input(
        &mut self,
        input: &mut ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::TilesetImport;
        if input.just_pressed(Key::Escape) {
            self.cancel_tileset_import();
            return;
        }
        if input.just_pressed(Key::Enter) {
            self.finish_tileset_import();
            return;
        }
        let Some(imp) = self.tileset_import.as_mut() else {
            self.cancel_tileset_import();
            return;
        };

        if imp.focus.is_some() {
            input.begin_text_capture();
            for ch in input.take_text().chars() {
                imp.type_char(ch);
            }
            if input.just_pressed(Key::Backspace) {
                imp.backspace();
            }
        }
        if input.just_pressed(Key::Tab) {
            let i = imp.focus.and_then(|f| ORDER.iter().position(|o| *o == f));
            let mut next = ORDER[i.map(|i| (i + 1) % ORDER.len()).unwrap_or(0)];
            // The region field only exists while a cell is selected.
            if next == ImportField::Region && imp.selected.is_none() {
                next = ImportField::Name;
            }
            imp.focus = Some(next);
        }

        if !(mouse.left_just_pressed() && mouse.in_bounds) {
            return;
        }
        let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
        match self.ui_frame.hit(px, py) {
            Some(WidgetId::ImporterField(f)) => imp.focus = Some(f),
            Some(WidgetId::ImporterSheet) => {
                if let Some(r) = self.ui_frame.rect_of(WidgetId::ImporterSheet) {
                    // Points on the preview -> pixels of the sheet image.
                    let ix = (px - r.x) * imp.texture.width as f32 / r.w;
                    let iy = (py - r.y) * imp.texture.height as f32 / r.h;
                    if let Some(cell) = imp.cell_at_pixel(ix, iy) {
                        imp.select(cell);
                    }
                }
            }
            Some(WidgetId::ImporterImport) => self.finish_tileset_import(),
            Some(WidgetId::ImporterCancel) => self.cancel_tileset_import(),
            _ => imp.focus = None,
        }
    }
}
