// renderer/text.rs — `Renderer::draw_text_px`/`draw_str`, the two methods
// that actually draw through the `Font` trait. Split out of `mod.rs` (7B-5,
// docs/ember2d-master-plan.md §5.2) once `draw_str`'s rewrite (routing
// `EMBER_UI_FONT=ttf` through `draw_text_px`/`ui_font`) pushed that file
// over the project's 750-line hard limit (CLAUDE.md) — these two are the
// most cohesive unit to pull out together: `draw_str` is built entirely on
// top of `draw_text_px`, and neither has much to do with the pixel-blit
// primitives (`draw_texture_px`, `draw_nine_slice`, `fill_rect_px`) left
// behind in `mod.rs`.

use super::{BitmapFont, Color, Font, GlyphInfo, Renderer, Texture, TextRun, UiFontKind, CELL_H, CELL_W};
use ember2d_sim::math::{Rect, Vec2};

impl Renderer {
    /// Draw one `TextRun` (7D-3, docs/ember2d-master-plan.md §5.4 — replaces
    /// the old, single-purpose `draw_text_px` as the real implementation;
    /// `draw_text_px` below is now a thin wrapper). `run.origin` is where
    /// the FIRST glyph's baseline-left sits (Phase 7 Part 2d — "text draws
    /// from a baseline, not a top-left corner"), in LOGICAL pixels. Built
    /// entirely out of `draw_texture_px` plus one glyph lookup per
    /// character, so it works for any `Font` impl without knowing which one
    /// it got. Returns the total horizontal advance, in logical pixels.
    ///
    /// `run.raster_px` is the size actually asked of `font.glyph` — for a
    /// legacy (`texel_scale: 1.0`) call this is the same value `pos`/the
    /// glyph quads are drawn at, exactly like the pre-7D-3 `draw_text_px`;
    /// for UI-points chrome text (`UiPainter::text`) it's the real PHYSICAL
    /// pixel size (`points * ui_scale`), and `run.texel_scale`
    /// (`ui_scale / render_scale`) converts each glyph's own raster-space
    /// offset/size back down into the logical pixels this method actually
    /// draws at — R50 (§3 in the master plan) is why the drawn SIZE comes
    /// from `GlyphInfo::size`, not `atlas_rect`'s size: those agree for
    /// `TtfFont` (which rasterizes each glyph at exactly its atlas rect) but
    /// not for `BitmapFont`, whose atlas rect stays a fixed native 8×8 cell
    /// regardless of the requested size — drawing `atlas_rect`'s own size
    /// there would always render a `BitmapFont` glyph at its native size,
    /// never the requested one. `run.pitch`, if set, overrides the font's
    /// own per-glyph advance with a fixed logical-pixel step — the script
    /// editor's monospace grid (`ui/script_layout.rs`), where every
    /// character must advance by exactly the same amount for its own
    /// column math to stay correct.
    ///
    /// Two different atlas lifetimes to handle, via `Font::atlas_texture`/
    /// `take_dirty`: `BitmapFont`'s is baked once into the GPU at startup
    /// and never changes (`atlas_texture` returns `None` — nothing to
    /// upload, ever, so a `pixels`-less placeholder `Texture` is fine,
    /// `upload_texture`'s cache check short-circuits before reading it).
    /// `TtfFont`'s `GlyphAtlas` texture is real and grows in place as new
    /// glyphs get rasterized — resolving every glyph in `text` FIRST
    /// (rather than drawing as each is resolved) means any newly-packed
    /// glyphs are already in `texture.pixels` before this decides whether
    /// to invalidate the stale GPU copy and force a fresh upload.
    ///
    /// R27 (7B-3, docs/ember2d-master-plan.md §5.2): used to clone
    /// `atlas_texture()`'s real `Texture` unconditionally on every call —
    /// wasteful for a large atlas redrawn every frame, since
    /// `draw_texture_px`'s own `upload_texture` call only ever reads
    /// `.pixels` when this id isn't already GPU-resident (or was just
    /// invalidated below). A lightweight id/width/height-only placeholder
    /// (same shape `BitmapFont`'s `None` case already used) is a valid
    /// stand-in on every other call — the overwhelming majority of frames,
    /// once an atlas has been uploaded at least once.
    pub fn draw_text_run(&mut self, font: &mut dyn Font, run: &TextRun) -> f32 {
        let glyphs: Vec<GlyphInfo> = run.text.chars().filter_map(|ch| font.glyph(ch, run.raster_px)).collect();

        let dirty = font.take_dirty();
        let tex_id = font.texture_id();
        let (tex_w, tex_h) = font.texture_size();

        if dirty {
            self.backend.invalidate_texture(tex_id.0);
        }
        let needs_real_pixels = dirty || !self.backend.has_texture(tex_id.0);
        let atlas = if needs_real_pixels {
            font.atlas_texture().cloned().unwrap_or_else(|| Texture {
                id: tex_id.0,
                width: tex_w,
                height: tex_h,
                pixels: Vec::new(),
            })
        } else {
            Texture { id: tex_id.0, width: tex_w, height: tex_h, pixels: Vec::new() }
        };

        let mut pen_x = run.origin.x;
        for g in glyphs {
            if g.size.x > 0.0 && g.size.y > 0.0 {
                let dest = Rect::new(
                    pen_x + g.offset.x * run.texel_scale,
                    run.origin.y + g.offset.y * run.texel_scale,
                    g.size.x * run.texel_scale,
                    g.size.y * run.texel_scale,
                );
                self.draw_texture_px(dest, &atlas, Some(g.atlas_rect), run.color);
            }
            pen_x += run.pitch.unwrap_or(g.advance * run.texel_scale);
        }
        pen_x - run.origin.x
    }

    /// Thin wrapper over `draw_text_run` at `texel_scale: 1.0, pitch: None`
    /// (7D-3, docs/ember2d-master-plan.md §5.4) — reproduces this method's
    /// own pre-7D-3 behavior exactly for every existing caller (`draw_str`'s
    /// TTF branch, below).
    pub fn draw_text_px(
        &mut self,
        font: &mut dyn Font,
        text: &str,
        pos: Vec2,
        px: f32,
        color: Color,
    ) -> f32 {
        self.draw_text_run(font, &TextRun { text, origin: pos, raster_px: px, texel_scale: 1.0, pitch: None, color })
    }

    /// 7B-5 (docs/ember2d-master-plan.md §5.2): the `EMBER_UI_FONT=ttf`
    /// case now routes through `draw_text_px`/`ui_font` instead of always
    /// hitting the dedicated font8x8 GPU path (`draw_char`) — Part 2's own
    /// unmet done-criterion was that measuring already went through `Font`
    /// while drawing didn't, so the two could silently disagree once a
    /// theme other than the bitmap font existed.
    ///
    /// The DEFAULT (`UiFontKind::Bitmap`) case deliberately still calls
    /// `draw_char` per character, byte-for-byte the original
    /// implementation — found live while building this step (before/after
    /// screenshot comparison, per this step's own Test line): the
    /// dedicated font8x8 path has always stretched its native 8×8 bitmap
    /// 2x vertically to fill the 8×16 `CELL_W`×`CELL_H` cell, but
    /// `BitmapFont`'s `Font` implementation models a native glyph as a
    /// literal, unstretched 8×8 square (see `UiFontKind`'s own doc
    /// comment) — routing the default case through `draw_text_px` too
    /// would have shrunk every character in the editor to roughly half
    /// its familiar height, a real visual regression rather than the
    /// refactor this step asks for. Confirmed with the user rather than
    /// silently choosing either direction; unifying `BitmapFont`'s glyph
    /// model with the historical stretch (or vice versa) is unscoped here.
    pub fn draw_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color) {
        if self.ui_font_kind == UiFontKind::Bitmap {
            for (i, ch) in s.chars().enumerate() {
                self.draw_char(x.saturating_add(i), y, ch, fg, bg);
            }
            return;
        }

        let px = self.ui_font_px;
        let cell_x = x as f32 * CELL_W as f32;
        let cell_y = y as f32 * CELL_H as f32;

        // `draw_text_px` takes its font as a caller-supplied `&mut dyn
        // Font` parameter (deliberately — Part 4's themed panels may one
        // day hand it a font they own themselves), so it can't also read
        // `self.ui_font` while `self` is borrowed for the call below;
        // swap it out via `mem::replace` instead of borrowing it
        // directly. The placeholder `BitmapFont` in the meantime is free
        // to construct (one `TextureId` field, no allocation) and is
        // never actually drawn with — this branch only runs when
        // `ui_font_kind` is `Ttf`.
        let mut font: Box<dyn Font> =
            std::mem::replace(&mut self.ui_font, Box::new(BitmapFont::new()));

        // One background rect sized by the WHOLE run's measured
        // width/height, not one `fill_rect_px` per character — `bg` is a
        // single uniform color per call, so this is one draw call instead
        // of `s.len()` of them.
        let (w, h) = font.measure(s, px);
        self.fill_rect_px(Rect::new(cell_x, cell_y, w, h), bg);

        // "Cell-snapped baseline": `draw_text_px`'s `pos` is where the pen
        // SITS, on the baseline — `y`'s cell top is `y * CELL_H` pixels
        // down, so the baseline is that plus this font's own `ascent` at
        // `px`, rounded to a whole physical pixel so a non-integer ascent
        // doesn't blur text half a pixel off the grid — the same rounding
        // `compute_layout`/`ScreenMapping` already apply elsewhere in this
        // file for the identical reason.
        let baseline_y = (cell_y + font.ascent(px)).round();
        self.draw_text_px(font.as_mut(), s, Vec2::new(cell_x, baseline_y), px, fg);

        self.ui_font = font;
    }
}
