// renderer/ui_space.rs — UiSpace: the points<->logical<->physical
// coordinate-space conversion (7D-3, docs/ember2d-master-plan.md §5.4).
//
// Three nested spaces, physical pixels being the ground truth the GPU
// actually rasterizes to:
//   physical px  --(/ render_scale, R)-->  logical px  (unchanged since 7B-2:
//     `Renderer.scale`, DPI-derived, floored at `MIN_UI_SCALE`)
//   logical px   --(/ (ui_scale / render_scale), i.e. * R/S)-->  UI points
//     (NEW: 1 point = `ui_scale` (S) physical pixels — an integer editor
//     chrome preference, independent of R)
// A glyph cell (`CELL_W`/`CELL_H`) is a FOURTH, unrelated unit — the
// viewport/canvas/graph-mode/play-mode grid, which this type never touches;
// see `ember2d::renderer::CELL_W`/`CELL_H` for that one.
//
// `UiSpace` is the one place that arithmetic lives. Editor chrome code never
// multiplies by `ui_scale`/`render_scale` itself — it calls `UiPainter`
// (`ui_painter.rs`), which holds a `UiSpace` and does the conversion once,
// consistently, for every draw call. `logical_to_pt` is the mirror-image
// INPUT choke point: `MouseState::pixel_x`/`pixel_y` (logical) come in, a
// chrome hit-test compares against a points-space `UiRect` — this is the one
// function that bridges them.

use super::Font;

/// `S/R` and the two components it's built from — see this module's own
/// header comment for what each space means. `screen_logical` is the whole
/// screen's own size, in logical pixels, needed by `screen_pt`/`screen_cells`
/// (a chrome caller that wants "how big is the window" in its own unit,
/// without re-deriving it from a `DrawSurface` itself).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiSpace {
    ui_scale: u32,
    render_scale: u32,
    screen_logical: (f32, f32),
}

impl UiSpace {
    /// Both scales are clamped to at least 1 — a `0` in either would make
    /// every conversion below divide by zero or collapse the whole screen
    /// to a point, and neither should ever reach here from a real caller
    /// (`ui_scale` is validated at `UiScaleChoice::resolve`,
    /// `ember2d-editor/src/editor/prefs.rs`; `render_scale` is
    /// `Renderer.scale`, already floored at `MIN_UI_SCALE`), but a
    /// defensive floor here means a bad value degrades to "no scaling"
    /// instead of a panic or a blank screen.
    pub fn new(ui_scale: u32, render_scale: u32, screen_logical: (f32, f32)) -> Self {
        UiSpace { ui_scale: ui_scale.max(1), render_scale: render_scale.max(1), screen_logical }
    }

    /// Builds a `UiSpace` from a live `DrawSurface`'s own reported
    /// `display_scale()` (the render scale) and `pixel_width`/`pixel_height`
    /// (the physical screen size — divided back down to logical here, since
    /// `screen_logical` is this type's own unit). `ui_scale` is the
    /// caller's resolved preference (`EditorState::effective_ui_scale`) —
    /// this function has no opinion on where that number comes from.
    pub fn from_surface(surface: &dyn super::DrawSurface, ui_scale: u32) -> Self {
        let display = surface.display_scale();
        let screen_logical = (
            surface.pixel_width() as f32 / display.render_scale as f32,
            surface.pixel_height() as f32 / display.render_scale as f32,
        );
        UiSpace::new(ui_scale, display.render_scale, screen_logical)
    }

    /// `S = R = 1` — points equal logical pixels exactly. Used by the
    /// headless test harness's default construction and by every
    /// checkpoint commit of 7D-3 before the live UI-scale menu lands (the
    /// step's own "S pinned" phase, master plan §5.4) — pinning `ui_scale`
    /// to `render_scale` (not necessarily `1`) is `EditorState`'s own job
    /// (`theme_loader::effective_ui_scale`); this constructor is only for
    /// tests and defaults that want the trivial 1:1 case specifically.
    pub fn identity(screen_logical: (f32, f32)) -> Self {
        UiSpace { ui_scale: 1, render_scale: 1, screen_logical }
    }

    pub fn ui_scale(self) -> u32 {
        self.ui_scale
    }

    pub fn render_scale(self) -> u32 {
        self.render_scale
    }

    /// Logical pixels per point (`S / R`) — the one ratio every other
    /// conversion in this type is built from. Can be non-integer (e.g.
    /// `1.5` at `S=3, R=2`), which is exactly why chrome must never assume
    /// a point is a whole number of logical pixels.
    pub fn pt_to_logical(self) -> f32 {
        self.ui_scale as f32 / self.render_scale as f32
    }

    pub fn to_logical(self, x: f32, y: f32) -> (f32, f32) {
        let k = self.pt_to_logical();
        (x * k, y * k)
    }

    /// A points-space rect (already `snap`-ped by the caller, typically)
    /// converted to logical pixels — `UiPainter`'s own internal use, and
    /// anything bridging a points rect into a `DrawSurface` call that still
    /// takes logical pixels (`set_scissor`).
    pub fn rect_to_logical(self, r: ember2d_sim::math::Rect) -> ember2d_sim::math::Rect {
        let k = self.pt_to_logical();
        ember2d_sim::math::Rect::new(r.x * k, r.y * k, r.w * k, r.h * k)
    }

    /// The INPUT choke point (see this module's header comment): a logical
    /// pixel position (`MouseState::pixel_x`/`pixel_y`) converted to points,
    /// for a chrome hit-test to compare against a points-space `UiRect`.
    pub fn logical_to_pt(self, x: f32, y: f32) -> (f32, f32) {
        let k = self.pt_to_logical();
        (x / k, y / k)
    }

    pub fn logical_rect_to_pt(self, r: ember2d_sim::math::Rect) -> ember2d_sim::math::Rect {
        let k = self.pt_to_logical();
        ember2d_sim::math::Rect::new(r.x / k, r.y / k, r.w / k, r.h / k)
    }

    /// The whole screen's size, in points.
    pub fn screen_pt(self) -> (f32, f32) {
        self.logical_to_pt(self.screen_logical.0, self.screen_logical.1)
    }

    /// The whole screen's size, in `CELL_W`/`CELL_H` cells — for graph mode
    /// only (7D-3, master plan §5.4: graph mode stays a cell-grid surface,
    /// out of this step's scope; `input/graph.rs`'s own palette-height
    /// lookup uses this instead of the stale `PanelManager::screen_size_cells`
    /// it used to read, R69).
    pub fn screen_cells(self) -> (usize, usize) {
        (
            (self.screen_logical.0 / super::CELL_W as f32) as usize,
            (self.screen_logical.1 / super::CELL_H as f32) as usize,
        )
    }

    /// Round `v_pt` to the nearest whole POINT — a point is already
    /// `ui_scale` physical pixels wide by definition, so "snap to a whole
    /// point" and "snap to the physical pixel grid" are the same operation
    /// here (contrast `Renderer::draw_texture_px`'s `snap_rect_to_scale`,
    /// which snaps a LOGICAL rect against `render_scale` instead).
    pub fn snap(self, v_pt: f32) -> f32 {
        v_pt.round()
    }

    /// Snap both edges of a points-space rect independently — see
    /// `geometry::snap_rect_to_scale`'s own doc comment for why both edges,
    /// not origin-then-size (the same reasoning applies here at `scale =
    /// 1.0`, since a point IS the target unit already).
    pub fn snap_rect(self, r: ember2d_sim::math::Rect) -> ember2d_sim::math::Rect {
        super::geometry::snap_rect_to_scale(r, 1.0)
    }

    /// The physical pixel size a chrome font request of `pt` points should
    /// actually rasterize at — `pt * ui_scale`. Text measured or drawn at
    /// anything other than this exact value would either blur (a size
    /// smaller than the glyph's real physical footprint, then upscaled) or
    /// pollute the glyph atlas with a second, unused set of cached sizes
    /// (measuring at the raw point size instead of this one) — see this
    /// module's own header comment and `measure`'s doc comment below.
    pub fn raster_px(self, pt: f32) -> f32 {
        pt * self.ui_scale as f32
    }

    /// Measure `text` at `pt` points — rasterizes (or looks up) at the real
    /// PHYSICAL size (`raster_px`) and divides the result back down to
    /// points, so a caller measuring a title never mints a second cache
    /// entry at the un-scaled point size that nothing is ever drawn at
    /// (7D-3, docs/ember2d-master-plan.md §5.4 — found designing this step:
    /// `TtfFont::measure` rasterizes on a cache miss exactly like `glyph`
    /// does, so measuring at the wrong size is a real, not just
    /// theoretical, atlas-pollution bug).
    pub fn measure(self, font: &mut dyn Font, text: &str, pt: f32) -> (f32, f32) {
        let (w, h) = font.measure(text, self.raster_px(pt));
        let k = self.ui_scale as f32;
        (w / k, h / k)
    }

    pub fn advance(self, font: &mut dyn Font, ch: char, pt: f32) -> f32 {
        font.glyph(ch, self.raster_px(pt)).map(|g| g.advance).unwrap_or(0.0) / self.ui_scale as f32
    }

    pub fn ascent(self, font: &dyn Font, pt: f32) -> f32 {
        font.ascent(self.raster_px(pt)) / self.ui_scale as f32
    }

    pub fn line_height(self, font: &dyn Font, pt: f32) -> f32 {
        font.line_height(self.raster_px(pt)) / self.ui_scale as f32
    }

    pub fn truncate_to_width(
        self,
        font: &mut dyn Font,
        text: &str,
        pt: f32,
        max_w_pt: f32,
    ) -> String {
        font.truncate_to_width(text, self.raster_px(pt), max_w_pt * self.ui_scale as f32)
    }

    /// The whole-POINT advance a monospace font run should use per
    /// character (`ui/script_layout.rs`'s `pitch`), derived from `'M'`'s
    /// real advance at the physical raster size and rounded to a whole
    /// point — a script editor's column math (click -> character, cursor ->
    /// x position) only holds together if every character advances by
    /// exactly the same, whole amount; a font's own possibly-fractional
    /// per-glyph advance would drift the two apart over a long line.
    pub fn mono_pitch(self, font: &mut dyn Font, pt: f32) -> f32 {
        self.advance(font, 'M', pt).round().max(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember2d_sim::math::Rect;

    #[test]
    fn points_equal_logical_px_when_ui_scale_equals_render_scale() {
        let ui = UiSpace::new(2, 2, (100.0, 100.0));
        assert_eq!(ui.pt_to_logical(), 1.0);
        assert_eq!(ui.to_logical(10.0, 20.0), (10.0, 20.0));
        assert_eq!(ui.logical_to_pt(10.0, 20.0), (10.0, 20.0));
    }

    #[test]
    fn logical_to_points_inverts_points_to_logical_at_s3_r2() {
        let ui = UiSpace::new(3, 2, (100.0, 100.0));
        assert_eq!(ui.pt_to_logical(), 1.5);
        let (lx, ly) = ui.to_logical(10.0, 4.0);
        assert_eq!((lx, ly), (15.0, 6.0));
        assert_eq!(ui.logical_to_pt(lx, ly), (10.0, 4.0));
    }

    #[test]
    fn snap_rounds_to_the_physical_pixel_grid_at_s3() {
        let ui = UiSpace::new(3, 1, (100.0, 100.0));
        assert_eq!(ui.snap(10.4), 10.0);
        assert_eq!(ui.snap(10.6), 11.0);
        let r = ui.snap_rect(Rect::new(0.3, 0.3, 4.4, 4.4));
        assert_eq!((r.x, r.y), (0.0, 0.0));
        assert_eq!((r.x + r.w, r.y + r.h), (5.0, 5.0));
    }

    #[test]
    fn measure_in_points_is_the_physical_measure_divided_by_s() {
        use super::super::{BitmapFont, Font};
        let mut font = BitmapFont::new();
        let ui = UiSpace::new(2, 1, (100.0, 100.0));
        // BitmapFont's native size is 8px; at ui_scale 2 a request of "8
        // points" rasterizes at 16 physical px (one whole multiple above
        // native), so the returned point measurement must be half that.
        let (w, _) = ui.measure(&mut font, "ab", 8.0);
        let (raw_w, _) = font.measure("ab", 16.0);
        assert_eq!(w, raw_w / 2.0);
    }

    #[test]
    fn screen_pt_divides_the_logical_screen_by_pt_to_logical() {
        let ui = UiSpace::new(3, 2, (150.0, 60.0));
        assert_eq!(ui.screen_pt(), (100.0, 40.0));
    }

    #[test]
    fn mono_pitch_is_a_whole_point() {
        use super::super::BitmapFont;
        let mut font = BitmapFont::new();
        let ui = UiSpace::new(3, 2, (100.0, 100.0));
        let pitch = ui.mono_pitch(&mut font, 8.0);
        assert_eq!(pitch.fract(), 0.0, "pitch must be a whole point for stable column math");
    }
}
