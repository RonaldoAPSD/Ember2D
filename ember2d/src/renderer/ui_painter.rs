// renderer/ui_painter.rs — UiPainter: the one drawing choke point every
// editor chrome draw call goes through (7D-3, docs/ember2d-master-plan.md
// §5.4). Theme-agnostic and crate-public so Phase 9-3's planned
// script-facing pixel HUD (master plan §5.8, "through the theme font on the
// pixel path") can reuse it unchanged with its own `UiSpace` — it never
// reads a `Theme` or an `EditorState` field, only the `UiSpace` it's built
// with and whatever `Font`/`Texture`/`Color` a caller hands it per call.
//
// Every method takes and returns POINTS — see `ui_space.rs`'s own header
// comment for the three nested spaces this bridges. Nothing here computes
// with `render_scale`/`ui_scale` directly except through `self.space`;
// that's the point of having one choke point instead of every chrome draw
// function repeating the same multiply.

use super::{Color, DrawSurface, Font, TextRun, Texture, UiSpace};
use ember2d_sim::math::{Rect, Vec2};

pub struct UiPainter<'a> {
    surface: &'a mut dyn DrawSurface,
    space: UiSpace,
}

impl<'a> UiPainter<'a> {
    pub fn new(surface: &'a mut dyn DrawSurface, space: UiSpace) -> Self {
        UiPainter { surface, space }
    }

    pub fn space(&self) -> UiSpace {
        self.space
    }

    /// The whole screen, in points — `(0, 0, screen_pt.0, screen_pt.1)`.
    pub fn screen(&self) -> Rect {
        let (w, h) = self.space.screen_pt();
        Rect::new(0.0, 0.0, w, h)
    }

    /// Escape hatch to the underlying `DrawSurface`, in its own native
    /// (logical-pixel) space — for the level canvas ONLY (7D-3, master plan
    /// §5.4: the canvas/viewport is deliberately untouched by UI points).
    /// No chrome draw function should ever call this; if one needs a
    /// `DrawSurface` primitive this type doesn't wrap, that primitive
    /// belongs on `UiPainter` instead, not bypassed.
    pub fn surface(&mut self) -> &mut dyn DrawSurface {
        self.surface
    }

    /// A solid-filled rect, both edges snapped to the physical pixel grid
    /// (`UiSpace::snap_rect`) before converting to logical pixels — see
    /// `geometry::snap_rect_to_scale`'s own doc comment for why both edges,
    /// not origin-then-size.
    pub fn fill(&mut self, r: Rect, color: Color) {
        let r = self.space.rect_to_logical(self.space.snap_rect(r));
        self.surface.fill_rect_px(r, color);
    }

    /// `border_texels` is the 9-slice's border in the ATLAS's own,
    /// unscaled texels (`NineSlice::border` — never itself multiplied by
    /// `ui_scale`); this method supplies `border_scale = S/R` to
    /// `DrawSurface::draw_nine_slice_px` so each source texel occupies `S`
    /// physical pixels on screen, the same "1 unscaled unit -> `ui_scale`
    /// physical pixels" contract text and fills already follow (see
    /// `geometry::nine_slice_quads`'s own doc comment for the full
    /// dest/source split).
    pub fn nine_slice(
        &mut self,
        r: Rect,
        tex: &Texture,
        src: Rect,
        border_texels: (f32, f32, f32, f32),
        tint: Color,
    ) {
        let r = self.space.rect_to_logical(self.space.snap_rect(r));
        self.surface.draw_nine_slice_px(
            r,
            tex,
            src,
            border_texels,
            self.space.pt_to_logical(),
            tint,
        );
    }

    /// Draw `text` at `pt` points, baseline-left at `baseline_left`
    /// (points). Rasterizes at the real physical size (`UiSpace::raster_px`)
    /// and draws each glyph at `texel_scale = raster_to_logical()` (`1/R`)
    /// logical pixels per atlas texel — NOT `pt_to_logical()` (`S/R`): the
    /// glyph's own raster is already `S` times the plain point size
    /// (`raster_px = pt * S`), so drawing it back down at `1/R` logical
    /// pixels per texel is what lands it at its real physical size — crisp,
    /// not nearest-upscaled the way pre-7D-3 chrome text always was (R85,
    /// §3 in the master plan; see this crate's `renderer/text.rs` header
    /// comment on `draw_text_run`). Returns the advance, in points.
    pub fn text(
        &mut self,
        font: &mut dyn Font,
        text: &str,
        baseline_left: Vec2,
        pt: f32,
        color: Color,
    ) -> f32 {
        let origin = {
            let (x, y) = self.space.to_logical(baseline_left.x, baseline_left.y);
            Vec2::new(x, y)
        };
        let run = TextRun {
            text,
            origin,
            raster_px: self.space.raster_px(pt),
            texel_scale: self.space.raster_to_logical(),
            pitch: None,
            color,
        };
        let advance_logical = self.surface.draw_text_run(font, &run);
        advance_logical / self.space.pt_to_logical()
    }

    /// `text` filled into `row` (a background fill the row's full width,
    /// `bg`) with the glyphs baseline-positioned from the row's own top
    /// edge plus `font`'s ascent at `pt` — the shape every panel-content
    /// text row (console, inspector, hierarchy, file browser, ...) draws.
    pub fn text_in_row(
        &mut self,
        font: &mut dyn Font,
        text: &str,
        row: Rect,
        pt: f32,
        fg: Color,
        bg: Option<Color>,
    ) -> f32 {
        if let Some(bg) = bg {
            self.fill(row, bg);
        }
        let ascent = self.space.ascent(font, pt);
        self.text(font, text, Vec2::new(row.x, row.y + ascent), pt, fg)
    }

    /// A monospace text run at a fixed per-character advance (`pitch_pt`,
    /// from `UiSpace::mono_pitch`) rather than the font's own (possibly
    /// fractional, possibly non-uniform for a proportional face) glyph
    /// advance — the script editor's own draw path (`ui/script.rs`), so a
    /// character's drawn position always matches the same column math its
    /// input handling (`ui/script_layout.rs`) uses.
    pub fn text_mono(
        &mut self,
        font: &mut dyn Font,
        text: &str,
        baseline_left: Vec2,
        pt: f32,
        pitch_pt: f32,
        color: Color,
    ) -> f32 {
        let origin = {
            let (x, y) = self.space.to_logical(baseline_left.x, baseline_left.y);
            Vec2::new(x, y)
        };
        let run = TextRun {
            text,
            origin,
            raster_px: self.space.raster_px(pt),
            texel_scale: self.space.raster_to_logical(),
            pitch: Some(pitch_pt * self.space.pt_to_logical()),
            color,
        };
        let advance_logical = self.surface.draw_text_run(font, &run);
        advance_logical / self.space.pt_to_logical()
    }

    /// A literal, unthemed font8x8 glyph preview (a tile/color swatch's own
    /// glyph, e.g. the palette panel row or the palette editor's preview) —
    /// deliberately NOT drawn through `text`/`Font`: this previews an
    /// actual GAME tile glyph on the engine's dedicated bitmap pipeline, the
    /// same pixels the canvas itself would draw, which must never change
    /// meaning just because the surrounding chrome scaled (7D-2's own
    /// "never themed" precedent for this exact preview, master plan §5.4).
    /// `height_pt` is the drawn size, points; `top_left` likewise.
    pub fn tile_glyph(&mut self, top_left: Vec2, height_pt: f32, ch: char, fg: Color, bg: Color) {
        let (lx, ly) = self.space.to_logical(top_left.x, top_left.y);
        let scale = (height_pt * self.space.pt_to_logical()) / super::CELL_H as f32;
        self.surface.draw_char_px(Vec2::new(lx, ly), ch, fg, bg, scale);
    }

    /// `r` is `None` for "no clip"; `Some` is snapped and converted the same
    /// way `fill`'s rect is.
    pub fn clip(&mut self, r: Option<Rect>) {
        let r = r.map(|r| self.space.rect_to_logical(self.space.snap_rect(r)));
        self.surface.set_scissor(r);
    }

    pub fn measure(&self, font: &mut dyn Font, text: &str, pt: f32) -> f32 {
        self.space.measure(font, text, pt).0
    }

    pub fn ascent(&self, font: &dyn Font, pt: f32) -> f32 {
        self.space.ascent(font, pt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{draw_log::DrawOp, BitmapFont, NullRenderer, UiSpace};

    fn painter_at(s: u32, r: u32) -> (NullRenderer, UiSpace) {
        let mut surface = NullRenderer::new(800, 600);
        surface.start_recording();
        let space = UiSpace::new(s, r, (800.0 / r as f32, 600.0 / r as f32));
        (surface, space)
    }

    #[test]
    fn painter_fill_edges_land_on_whole_physical_pixels_at_s3_r2() {
        let (mut surface, space) = painter_at(3, 2);
        {
            let mut p = UiPainter::new(&mut surface, space);
            // 1.3 points is not a whole point; both edges must still snap.
            p.fill(Rect::new(1.3, 1.3, 4.4, 4.4), Color::White);
        }
        let ops = surface.ops();
        let DrawOp::Fill(r) = ops[0] else { panic!("expected a Fill op") };
        // At S=3,R=2: pt_to_logical = 1.5. Snapped points rect: x0=1,y0=1
        // (1.3 rounds to 1), x1=y1=round(5.7)=6 -> logical = 1*1.5=1.5,
        // (6-1)*1.5=7.5 wide.
        assert_eq!((r.x, r.y), (1.5, 1.5));
        assert_eq!((r.w, r.h), (7.5, 7.5));
        // Whichever the exact numbers, every edge in PHYSICAL pixels
        // (logical * render_scale) must be a whole number.
        for v in [r.x, r.y, r.x + r.w, r.y + r.h] {
            let physical = v * 2.0;
            assert!((physical - physical.round()).abs() < 1e-4, "{v} is not on the physical grid");
        }
    }

    #[test]
    fn painter_text_rasterizes_at_points_times_ui_scale() {
        let (mut surface, space) = painter_at(3, 1);
        let mut font = BitmapFont::new();
        {
            let mut p = UiPainter::new(&mut surface, space);
            p.text(&mut font, "A", Vec2::new(0.0, 0.0), 8.0, Color::White);
        }
        let ops = surface.ops();
        let DrawOp::Text { raster_px, texel_scale, .. } = &ops[0] else {
            panic!("expected a Text op")
        };
        assert_eq!(*raster_px, 24.0, "8pt at ui_scale 3 must rasterize at 24 physical px");
        // R85 (§3 in the master plan): `texel_scale` is `1/render_scale`,
        // NOT `ui_scale/render_scale` — the glyph's own raster is already
        // `ui_scale` times the plain point size (`raster_px` above), so
        // drawing it back down at `ui_scale/render_scale` logical pixels
        // per texel double-counted `ui_scale`, rendering every glyph
        // `ui_scale`× too large the instant anything actually consumed
        // `texel_scale` in a live draw (checkpoint 7, not before — this
        // test asserted the wrong value from checkpoint 1 onward and
        // nothing caught it until then).
        assert_eq!(*texel_scale, 1.0, "at render_scale 1, one atlas texel is exactly one logical px");
    }

    /// R85: at a render_scale that ACTUALLY differs from 1 (unlike the test
    /// above), `texel_scale` and `pt_to_logical` are genuinely different
    /// numbers (`1/2` vs `3/2`) — this is the case that would have failed
    /// loudly, not just numerically, had the bug still been in place.
    #[test]
    fn painter_text_texel_scale_is_one_over_render_scale_not_s_over_r() {
        let (mut surface, space) = painter_at(3, 2);
        let mut font = BitmapFont::new();
        {
            let mut p = UiPainter::new(&mut surface, space);
            p.text(&mut font, "A", Vec2::new(0.0, 0.0), 8.0, Color::White);
        }
        let ops = surface.ops();
        let DrawOp::Text { texel_scale, .. } = &ops[0] else { panic!("expected a Text op") };
        assert_eq!(*texel_scale, 0.5);
        assert_ne!(*texel_scale, space.pt_to_logical(), "texel_scale must not be pt_to_logical (S/R)");
    }

    #[test]
    fn painter_nine_slice_border_scale_is_s_over_r() {
        let (mut surface, space) = painter_at(3, 2);
        let tex = Texture::solid(0xFFFFFFFF);
        {
            let mut p = UiPainter::new(&mut surface, space);
            p.nine_slice(
                Rect::new(0.0, 0.0, 20.0, 20.0),
                &tex,
                Rect::new(0.0, 0.0, 8.0, 8.0),
                (2.0, 2.0, 2.0, 2.0),
                Color::White,
            );
        }
        let ops = surface.ops();
        let DrawOp::NineSlice { border_scale, .. } = &ops[0] else {
            panic!("expected a NineSlice op")
        };
        assert_eq!(*border_scale, 1.5);
    }

    #[test]
    fn painter_clip_converts_points_to_logical() {
        let (mut surface, space) = painter_at(2, 2);
        {
            let mut p = UiPainter::new(&mut surface, space);
            p.clip(Some(Rect::new(10.0, 10.0, 5.0, 5.0)));
        }
        let ops = surface.ops();
        let DrawOp::Scissor(Some(r)) = ops[0] else { panic!("expected a Scissor op") };
        // S=R=2 -> pt_to_logical = 1.0, so points == logical here.
        assert_eq!((r.x, r.y, r.w, r.h), (10.0, 10.0, 5.0, 5.0));
    }

    #[test]
    fn painter_tile_glyph_scale_is_s_over_r() {
        let (mut surface, space) = painter_at(2, 1);
        {
            let mut p = UiPainter::new(&mut surface, space);
            // height_pt = CELL_H points -> scale should be exactly ui_scale/render_scale.
            p.tile_glyph(Vec2::ZERO, super::super::CELL_H as f32, 'A', Color::White, Color::Black);
        }
        let ops = surface.ops();
        let DrawOp::Char { scale, .. } = ops[0] else { panic!("expected a Char op") };
        assert_eq!(scale, 2.0);
    }
}
