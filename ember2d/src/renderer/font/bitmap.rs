// renderer/font/bitmap.rs — `BitmapFont`, wrapping the existing font8x8
// atlas. Split out of the original flat `font.rs` when `ttf.rs`/`atlas.rs`
// were added — see `mod.rs`'s header comment.

use ember2d_sim::math::{Rect, Vec2};
use super::super::texture::TextureId;
use super::{Font, GlyphInfo};

/// Wraps the existing font8x8 atlas (`WgpuBackend::new`'s "Create Font
/// Atlas" section, `backend.rs`) — reserved texture id 0, glyphs stored
/// one 8×8-pixel cell per ASCII code point, stacked vertically (`ch_idx *
/// 8 .. ch_idx * 8 + 8` within a 1024-pixel-tall atlas, `ch_idx = ch as
/// usize % 128`). This struct doesn't own or rebuild that atlas — it just
/// knows its fixed addressing scheme, so `glyph()` is a pure computation,
/// not a cache lookup. **The two places must stay in sync**: if
/// `backend.rs`'s atlas layout ever changes, `NATIVE_PX`/`GLYPH_COUNT`/the
/// indexing below have to change with it.
///
/// Only honors integer multiples of its native 8px size — a request for
/// 12px snaps to the nearest multiple (8 or 16) rather than blurring an
/// upscaled bitmap. `advance`/`line_height`/`ascent` are all that same
/// snapped size: a bitmap font glyph is monospace and square (no
/// descender modeled), so every one of those metrics collapses to "the
/// effective pixel size" for this particular implementation.
pub struct BitmapFont {
    texture_id: TextureId,
}

impl BitmapFont {
    /// `font8x8::legacy::BASIC_LEGACY` covers exactly the ASCII range —
    /// `backend.rs` builds its atlas from the same 128 entries.
    const GLYPH_COUNT: usize = 128;
    const NATIVE_PX: f32 = 8.0;

    /// The one true instance — every process has exactly one font8x8
    /// atlas, permanently pinned at texture id 0 by `WgpuBackend::new`.
    pub fn new() -> Self {
        BitmapFont { texture_id: TextureId(0) }
    }

    /// Nearest whole multiple of `NATIVE_PX`, never less than 1x —
    /// "snaps to 8 or 16 rather than blurring" (docs/ember2d-phase7-plan.md
    /// §2a).
    fn scale_for(px: f32) -> f32 {
        (px / Self::NATIVE_PX).round().max(1.0)
    }
}

impl Default for BitmapFont {
    fn default() -> Self { Self::new() }
}

impl Font for BitmapFont {
    fn texture_id(&self) -> TextureId { self.texture_id }

    fn texture_size(&self) -> (u32, u32) {
        // Mirrors `backend.rs`'s "Create Font Atlas" section exactly —
        // 8px wide, 128 stacked 8px-tall glyph cells.
        (8, 128 * 8)
    }

    fn glyph(&mut self, ch: char, px: f32) -> Option<GlyphInfo> {
        // `draw_char`'s legacy `ch as usize % 128` silently wraps an
        // out-of-range char onto some unrelated glyph rather than failing
        // — harmless there since script/editor text has always been
        // ASCII in practice, but this new trait can afford to be honest
        // about what it doesn't have instead of aliasing.
        if !ch.is_ascii() { return None; }
        let scale = Self::scale_for(px);
        let effective = scale * Self::NATIVE_PX;
        let idx = (ch as usize) % Self::GLYPH_COUNT;
        Some(GlyphInfo {
            atlas_rect: Rect::new(0.0, idx as f32 * Self::NATIVE_PX, Self::NATIVE_PX, Self::NATIVE_PX),
            // Phase 7 Part 2d (docs/ember2d-phase7-plan.md): the pen sits
            // ON THE BASELINE (`Font`'s documented convention), and this
            // font models no descender — the baseline IS the glyph's
            // bottom edge, `effective` pixels below its top. `offset` is
            // "pen to top-left," so it's `effective` pixels UP (negative
            // in this engine's y-down convention), matching `ascent`
            // below exactly. An earlier draft of this left `offset` at
            // `Vec2::ZERO` (pen = top-left) — self-consistent in isolation,
            // but disagreed with `ascent()` already claiming the baseline
            // sat `effective` px below the top, and would have placed
            // `BitmapFont` text `effective` px too low wherever a caller
            // trusted `ascent()` to convert a box's top edge into a
            // baseline (`draw_text_px`, added this same step).
            offset: Vec2::new(0.0, -effective),
            advance: effective,
        })
    }

    fn measure(&mut self, text: &str, px: f32) -> (f32, f32) {
        let effective = Self::scale_for(px) * Self::NATIVE_PX;
        (text.chars().count() as f32 * effective, effective)
    }

    fn line_height(&self, px: f32) -> f32 {
        Self::scale_for(px) * Self::NATIVE_PX
    }

    fn ascent(&self, px: f32) -> f32 {
        Self::scale_for(px) * Self::NATIVE_PX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_at_native_size_returns_the_exact_8x8_atlas_cell() {
        let mut font = BitmapFont::new();
        let g = font.glyph('A', 8.0).unwrap();
        assert_eq!(g.atlas_rect.w, 8.0);
        assert_eq!(g.atlas_rect.h, 8.0);
        assert_eq!(g.advance, 8.0);
    }

    #[test]
    fn glyph_offset_places_the_pen_on_the_baseline_matching_ascent() {
        // Phase 7 Part 2d: the pen sits ON the baseline, `ascent(px)`
        // pixels below the glyph's top — `offset` (pen to top-left) must
        // move up by exactly that much, for every requested size, not
        // just the native one.
        let mut font = BitmapFont::new();
        for &px in &[8.0f32, 16.0, 24.0] {
            let g = font.glyph('A', px).unwrap();
            assert_eq!(g.offset, Vec2::new(0.0, -font.ascent(px)));
        }
    }

    #[test]
    fn glyph_index_addresses_the_correct_row_in_the_shared_atlas() {
        let mut font = BitmapFont::new();
        // 'B' is ASCII 66 — its 8x8 cell sits 66 rows down the atlas.
        let g = font.glyph('B', 8.0).unwrap();
        assert_eq!(g.atlas_rect.y, 66.0 * 8.0);
    }

    #[test]
    fn glyph_snaps_a_non_multiple_size_to_the_nearest_whole_multiple() {
        let mut font = BitmapFont::new();
        // 12px is halfway between 8 and 16 — either snap target would
        // satisfy "snaps to 8 or 16" (docs/ember2d-phase7-plan.md §2a);
        // `f32::round`'s round-half-away-from-zero picks 16 here.
        let g = font.glyph('A', 12.0).unwrap();
        assert_eq!(g.advance, 16.0);
        // Still the native-size atlas cell — scaling changes how large the
        // glyph is DRAWN, never which pixels of the atlas it samples.
        assert_eq!((g.atlas_rect.w, g.atlas_rect.h), (8.0, 8.0));
    }

    #[test]
    fn glyph_below_the_native_size_still_clamps_to_a_whole_1x_scale() {
        let mut font = BitmapFont::new();
        let g = font.glyph('A', 3.0).unwrap();
        assert_eq!(g.advance, 8.0, "a bitmap font can't render smaller than its native pixel size");
    }

    #[test]
    fn glyph_returns_none_for_non_ascii_input() {
        let mut font = BitmapFont::new();
        assert_eq!(font.glyph('日', 8.0), None);
    }

    #[test]
    fn measure_multiplies_char_count_by_the_effective_advance() {
        let mut font = BitmapFont::new();
        assert_eq!(font.measure("abc", 8.0), (24.0, 8.0));
        assert_eq!(font.measure("abc", 16.0), (48.0, 16.0));
        assert_eq!(font.measure("", 8.0), (0.0, 8.0), "empty text has zero width but still a real line height");
    }

    #[test]
    fn line_height_and_ascent_both_equal_the_effective_size() {
        // No descender is modeled for this simple bitmap font — the full
        // glyph cell sits above the baseline.
        let font = BitmapFont::new();
        assert_eq!(font.line_height(8.0), 8.0);
        assert_eq!(font.ascent(8.0), 8.0);
        assert_eq!(font.line_height(12.0), font.ascent(12.0));
    }

    #[test]
    fn texture_id_is_the_reserved_font_atlas_slot() {
        let font = BitmapFont::new();
        assert_eq!(font.texture_id(), TextureId(0));
    }

    #[test]
    fn texture_size_matches_backends_own_font_atlas_dimensions() {
        let font = BitmapFont::new();
        assert_eq!(font.texture_size(), (8, 1024));
    }
}
