// editor/input/clip_editor.rs — keyboard and mouse for the clip editor
// dialog (`EditorMode::ClipEditor`).
//
// Step 8-3 (docs/ember2d-master-plan.md §5.7). Same discipline as the
// importer's input (input/importer.rs): typed text through `take_text`
// while a field has focus, every click resolved against the rects
// `draw_clip_editor_modal` registered this frame — no layout recomputed
// here. With no field focused the keyboard drives playback: Space plays/
// pauses, Left/Right scrub frame by frame, Delete removes the selected
// frame.

use ember2d::input::Key;

use super::super::ui::WidgetId;
use super::super::{EditorMode, EditorState};

impl EditorState {
    pub(super) fn handle_clip_editor_input(
        &mut self,
        input: &mut ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::ClipEditor;
        if input.just_pressed(Key::Escape) {
            self.close_clip_editor();
            return;
        }
        let Some(ce) = self.clip_editor.as_mut() else {
            self.close_clip_editor();
            return;
        };

        if ce.focus.is_some() {
            input.begin_text_capture();
            for ch in input.take_text().chars() {
                ce.type_char(ch);
            }
            if input.just_pressed(Key::Backspace) {
                ce.backspace();
            }
            if input.just_pressed(Key::Enter) || input.just_pressed(Key::Tab) {
                ce.focus = None;
            }
        } else {
            if input.just_pressed(Key::Space) {
                ce.toggle_play();
            }
            if input.just_pressed(Key::Left) {
                ce.step(-1);
            }
            if input.just_pressed(Key::Right) {
                ce.step(1);
            }
            if input.just_pressed(Key::Delete) {
                ce.remove_selected();
            }
        }

        if !(mouse.left_just_pressed() && mouse.in_bounds) {
            return;
        }
        let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
        let hit = self.ui_frame.hit(px, py);
        let ce = self.clip_editor.as_mut().expect("checked above");
        match hit {
            Some(WidgetId::ClipField(f)) => ce.focus = Some(f),
            Some(WidgetId::ClipLoop) => ce.looping = !ce.looping,
            Some(WidgetId::ClipPlay) => ce.toggle_play(),
            Some(WidgetId::ClipFrame(i)) => ce.select(i),
            Some(WidgetId::ClipMoveLeft) => ce.move_selected(-1),
            Some(WidgetId::ClipMoveRight) => ce.move_selected(1),
            Some(WidgetId::ClipDeleteFrame) => ce.remove_selected(),
            Some(WidgetId::ClipSheet) => {
                ce.focus = None;
                let sheet = self.ui_frame.rect_of(WidgetId::ClipSheet);
                let tileset = ce.tileset.as_ref().and_then(|t| self.sprites.tilesets.get(t));
                if let (Some(r), Some(t)) = (sheet, tileset) {
                    if let Some(tex) = t.texture.as_ref() {
                        // Points on the preview -> sheet pixels -> the named
                        // region covering that pixel, if any.
                        let ix = (px - r.x) * tex.width as f32 / r.w;
                        let iy = (py - r.y) * tex.height as f32 / r.h;
                        let region = t.data.regions.iter().find(|reg| {
                            let rr = t.data.cell_rect(reg.col, reg.row, reg.w, reg.h);
                            ix >= rr.x && ix < rr.x + rr.w && iy >= rr.y && iy < rr.y + rr.h
                        });
                        match region {
                            Some(reg) => ce.add_frame(&reg.name.clone()),
                            None => {
                                ce.status = Some((
                                    false,
                                    "that cell has no region name - name it in File > Import Tileset..."
                                        .to_string(),
                                ))
                            }
                        }
                    }
                }
            }
            Some(WidgetId::ClipListRow(i)) => {
                if let Some(name) = self.sprites.clips.keys().nth(i).cloned() {
                    self.load_clip_into_editor(&name);
                }
            }
            Some(WidgetId::ClipNew) => {
                let first = self.sprites.tilesets.keys().next().cloned();
                self.clip_editor = Some(super::super::clip_editor::ClipEditor::new(first));
            }
            Some(WidgetId::ClipTilesetCycle) => self.cycle_clip_tileset(),
            Some(WidgetId::ClipSave) => {
                self.save_clip();
            }
            Some(WidgetId::ClipAddToPalette) => self.add_clip_to_palette(),
            Some(WidgetId::ClipClose) => self.close_clip_editor(),
            _ => ce.focus = None,
        }
    }
}
