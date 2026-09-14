// editor/impl_state/viewport.rs — the canvas viewport seam: mapping a mouse
// position to a grid cell, and keeping `scroll`/`target_scroll` inside the
// level's own bounds. Split out of `impl_state/mod.rs` (7D-3 checkpoint 7,
// docs/ember2d-master-plan.md §5.4) once that file's own R70 fix and this
// checkpoint's points<->logical conversion together pushed it back over
// CLAUDE.md's 750-line hard limit — the same "own file, no room left" reason
// `export.rs`/`graph_sidecars.rs` (`mod.rs`'s own header comment) already
// split out of it. This is also exactly the "one viewport seam module" this
// step's own plan called for: `mouse_to_grid`/`center_on`/`clamp_scroll`
// (`viewport_tiles`, its one private helper) are the sole place canvas code
// converts a points-space `Panel::content_rect` down to the LOGICAL pixels
// the 7C-9 decision gate (master plan §7.1) keeps the level canvas on
// forever, independent of `ui_scale`.

use super::super::panel::PanelId;
use super::super::EditorState;

impl EditorState {
    /// 7C-3 (master plan §5.3, E4): takes the mouse's true pixel position
    /// now, not `mouse.cell_x`/`cell_y` — reads the Viewport panel's own
    /// content rect (`PanelManager::viewport().content_rect()`) directly
    /// instead of the deleted `Layout.canvas_x`/`canvas_y`/`canvas_w`/
    /// `canvas_h`, which used to rebuild an independent cell-quantized copy
    /// of the same rect every frame. Using the real sub-cell pixel position
    /// (rather than pre-floored cell coordinates) also makes this agree
    /// with `ui/canvas.rs::draw_cursor_highlight`'s identical formula at
    /// every zoom level, not just whole-number ones — see that function's
    /// own comment on E1 for why the two must never independently drift.
    pub(in crate::editor) fn mouse_to_grid(
        &self,
        mouse_px: f32,
        mouse_py: f32,
    ) -> Option<(i32, i32)> {
        // 7D-3 checkpoint 7 (master plan §5.4): every caller passes
        // `mouse.pixel_x/y` (LOGICAL, per the 7C-9 decision gate — canvas
        // math stays logical forever) but `PanelManager`/`Panel::rect` are
        // POINTS-space now (draw side went through `UiPainter` this
        // checkpoint) — `panel_at`/`contains` below need the mouse
        // converted to points to compare against them. The viewport rect
        // used for the actual grid-cell division is converted back down
        // to LOGICAL (`rect_to_logical`) and divided against the original
        // logical mouse position instead — mixing a points-space delta
        // with `CELL_W`/`CELL_H` (logical-pixel constants) would silently
        // scale the cursor position by `ui_scale`.
        let (px, py) = self.ui_space.logical_to_pt(mouse_px, mouse_py);
        // 1. Block input if mouse is over any OTHER panel (except Viewport).
        if let Some(pid) = self.panels.panel_at(px, py) {
            if pid != PanelId::Viewport {
                return None;
            }
        }

        // 2. Localize to viewport content space (logical pixels).
        let metrics = super::super::ui::ChromeMetrics::from_theme(&self.theme);
        let viewport_pt = self.panels.viewport().content_rect(&metrics);
        if !viewport_pt.contains(px, py) {
            return None;
        }
        let viewport = self.ui_space.rect_to_logical(viewport_pt.into());
        let local_x = (mouse_px - viewport.x) / ember2d::renderer::CELL_W as f32;
        let local_y = (mouse_py - viewport.y) / ember2d::renderer::CELL_H as f32;

        // 3. Project to grid coordinates
        let gx = (local_x / self.zoom + self.scroll.0).floor() as i32;
        let gy = (local_y / self.zoom + self.scroll.1).floor() as i32;

        Some((gx, gy))
    }

    /// The viewport's own content area, in `CELL_W`/`CELL_H` TILES — R66-C
    /// (§3 in the master plan): `center_on`/`clamp_scroll` below used to
    /// read `Panel::content_w()`/`content_h()` (the CELL-ROUNDED bridge) as
    /// a tile count directly, which silently changed meaning the moment a
    /// panel's real pixel size stopped being a whole-cell multiple (a
    /// theme's `row_h`/`border` rarely divide `CELL_H`/`CELL_W` evenly).
    /// Computed from the EXACT `content_rect` instead, matching
    /// `mouse_to_grid`'s own formula above.
    fn viewport_tiles(&self) -> (f32, f32) {
        // 7D-3 checkpoint 7: `content_rect` is points-space now — converted
        // to logical before dividing by the logical-pixel `CELL_W`/`CELL_H`
        // constants, same reasoning as `mouse_to_grid` above.
        let metrics = super::super::ui::ChromeMetrics::from_theme(&self.theme);
        let viewport_pt = self.panels.viewport().content_rect(&metrics);
        let viewport = self.ui_space.rect_to_logical(viewport_pt.into());
        (
            viewport.w / ember2d::renderer::CELL_W as f32,
            viewport.h / ember2d::renderer::CELL_H as f32,
        )
    }

    pub(in crate::editor) fn center_on(&mut self, gx: i32, gy: i32) {
        let (canvas_w, canvas_h) = self.viewport_tiles();
        self.target_scroll.0 = (gx as f32 - canvas_w / 2.0 / self.zoom).max(0.0);
        self.target_scroll.1 = (gy as f32 - canvas_h / 2.0 / self.zoom).max(0.0);
        self.clamp_scroll();
    }

    pub(in crate::editor) fn clamp_scroll(&mut self) {
        let (canvas_w, canvas_h) = self.viewport_tiles();
        let max_x = (self.grid.width as f32 - canvas_w / self.zoom).max(0.0);
        let max_y = (self.grid.height as f32 - canvas_h / self.zoom).max(0.0);
        self.target_scroll.0 = self.target_scroll.0.clamp(0.0, max_x);
        self.target_scroll.1 = self.target_scroll.1.clamp(0.0, max_y);
    }
}
