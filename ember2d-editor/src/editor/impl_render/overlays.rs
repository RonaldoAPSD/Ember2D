// editor/impl_render/overlays.rs — what draws over the docked panels: the
// asset drag ghost (Step 8-4) and the asset/palette modals (the palette
// editor, Step 8-2's tileset importer, Step 8-3's clip editor, the color
// picker).
//
// The modal block moved here unchanged from `impl_render/mod.rs` at Step
// 8-4 (docs/ember2d-master-plan.md §5.7), whose own File Browser additions
// would otherwise have pushed that file past CLAUDE.md's 750-line limit.

use ember2d::renderer::UiPainter;
use ember2d_sim::math::Vec2;

use super::super::assets;
use super::super::panel::PanelId;
use super::super::ui;
use super::super::EditorMode;
use super::super::EditorState;

impl EditorState {
    /// Step 8-4: while an asset is being dragged out of the File Browser,
    /// outline the palette if releasing would drop there, and draw the
    /// ghost (picture, name, what a release here would do) at the mouse.
    pub(super) fn draw_asset_drag(
        &mut self,
        painter: &mut UiPainter,
        mouse: &ember2d::mouse::MouseState,
    ) {
        let Some(drag) = self.asset_drag.as_ref().filter(|d| d.active) else { return };
        let target = self.asset_drop_target(mouse);
        let multi_region = match &drag.asset {
            assets::AssetRef::Tileset(n) => {
                self.sprites.tilesets.get(n).is_some_and(|t| t.data.regions.len() != 1)
            }
            _ => false,
        };
        let hint = match target {
            Some(None) => "release: add to the palette".to_string(),
            Some(Some(_)) if multi_region => "release: add its regions to the palette".to_string(),
            Some(Some((x, y))) => format!("release: paint tile ({x},{y})"),
            None => "drop on the palette or a canvas tile".to_string(),
        };
        if target == Some(None) {
            let r = self.panels.get(PanelId::Palette).rect;
            ui::draw_drop_outline(painter, &self.theme, r.into());
        }
        let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
        let image =
            assets::thumbnail(&drag.asset, &self.sprites, &self.file_thumbs, self.anim_time);
        ui::draw_asset_drag_ghost(
            painter,
            self.font.as_mut(),
            &self.theme,
            &drag.asset.label(),
            image,
            &hint,
            Vec2::new(px, py),
        );
    }

    /// The palette editor, the tileset importer, the clip editor and the
    /// color picker, whichever `mode` has open.
    pub(super) fn draw_asset_modals(
        &mut self,
        painter: &mut UiPainter,
        screen_w: f32,
        screen_h: f32,
    ) {
        if matches!(self.mode, EditorMode::PaletteEditor | EditorMode::ColorPicker { .. }) {
            if let Some(pal) = self.palette.tiles.get(self.palette_editing_idx) {
                ui::draw_palette_editor_modal(
                    painter,
                    self.font.as_mut(),
                    &self.theme,
                    &self.theme_chrome_tex,
                    pal,
                    &self.sprites,
                    self.anim_time,
                    self.palette_editor_focus.as_ref(),
                    screen_w,
                    screen_h,
                    &mut self.ui_frame,
                );
            }
        }

        // Step 8-2: the tileset importer — see ui/panels/importer_panel.rs.
        if let (EditorMode::TilesetImport, Some(imp)) = (&self.mode, &self.tileset_import) {
            ui::draw_tileset_import_modal(
                painter,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                imp,
                screen_w,
                screen_h,
                &mut self.ui_frame,
            );
        }

        // Step 9-6: Project Settings — see ui/panels/project_settings_panel.rs.
        if let (EditorMode::ProjectSettings, Some(p)) = (&self.mode, &self.project_settings) {
            ui::draw_project_settings_modal(
                painter,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                p,
                screen_w,
                screen_h,
                &mut self.ui_frame,
            );
        }

        // Step 8-3: the clip editor — see ui/panels/clip_editor_panel.rs.
        if let (EditorMode::ClipEditor, Some(ce)) = (&self.mode, &self.clip_editor) {
            ui::draw_clip_editor_modal(
                painter,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                ce,
                &self.sprites,
                screen_w,
                screen_h,
                &mut self.ui_frame,
            );
        }

        if let EditorMode::ColorPicker { is_fg } = &self.mode {
            let is_fg = *is_fg;
            ui::draw_color_picker_modal(
                painter,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                self.color_picker_hsv,
                is_fg,
                screen_w,
                screen_h,
                &mut self.ui_frame,
            );
        }
    }
}
