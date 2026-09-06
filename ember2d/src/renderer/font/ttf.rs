// renderer/font/ttf.rs — TtfFont: rasterizes a real TTF/OTF via `fontdue`
// on demand into a `GlyphAtlas` (Phase 7 Part 2, docs/ember2d-phase7-plan.md
// §2a/§2b — the plan describes these as one feature, since `TtfFont`
// literally cannot produce a usable `GlyphInfo` without somewhere real to
// pack a rasterized glyph into).

use super::super::texture::{Texture, TextureId};
use super::{Font, GlyphAtlas, GlyphInfo};

/// A default atlas size the plan itself argues won't be reached: "an
/// editor uses a few hundred distinct glyphs across a handful of sizes and
/// will never fill a 1024×1024 sheet" (§2b).
const DEFAULT_ATLAS_SIZE: u32 = 1024;

pub struct TtfFont {
    font: fontdue::Font,
    /// This instance's own key into whatever `GlyphAtlas` it owns —
    /// distinguishes glyphs from different `TtfFont`s if a future caller
    /// ever shares one atlas across several fonts. Not exposed; there's
    /// nothing outside this struct that needs to know it.
    font_id: usize,
    atlas: GlyphAtlas,
}

impl TtfFont {
    /// Parse `bytes` as a TTF/OTF and build a fresh, empty `GlyphAtlas`
    /// sized `DEFAULT_ATLAS_SIZE`² for it. Use `with_atlas_size` instead
    /// to pick a smaller atlas (tests exercising the "atlas full" path
    /// want a tiny one, not 1024²).
    pub fn from_bytes(bytes: &[u8], font_id: usize) -> Result<Self, String> {
        Self::with_atlas_size(bytes, font_id, DEFAULT_ATLAS_SIZE, DEFAULT_ATLAS_SIZE)
    }

    pub fn with_atlas_size(bytes: &[u8], font_id: usize, atlas_w: u32, atlas_h: u32) -> Result<Self, String> {
        let font = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())?;
        Ok(TtfFont { font, font_id, atlas: GlyphAtlas::new(atlas_w, atlas_h, 0x0000_0000) })
    }
}

impl Font for TtfFont {
    fn texture_id(&self) -> TextureId { self.atlas.texture_id() }

    fn texture_size(&self) -> (u32, u32) { (self.atlas.texture.width, self.atlas.texture.height) }

    fn atlas_texture(&self) -> Option<&Texture> { Some(&self.atlas.texture) }

    fn take_dirty(&mut self) -> bool { self.atlas.take_dirty() }

    fn glyph(&mut self, ch: char, px: f32) -> Option<GlyphInfo> {
        self.atlas.get_or_rasterize(&self.font, self.font_id, ch, px)
    }

    /// Routed through the same cached `glyph()` every draw call uses,
    /// rather than `fontdue::Font::metrics` directly — guarantees
    /// `measure`'s width can never disagree with what actually gets drawn
    /// later for the same text, and warms the atlas cache for it as a
    /// side effect (text that gets measured is, in practice, text that's
    /// about to be drawn).
    fn measure(&mut self, text: &str, px: f32) -> (f32, f32) {
        let mut width = 0.0;
        for ch in text.chars() {
            if let Some(g) = self.glyph(ch, px) { width += g.advance; }
        }
        (width, self.line_height(px))
    }

    fn line_height(&self, px: f32) -> f32 {
        self.font.horizontal_line_metrics(px).map(|m| m.new_line_size).unwrap_or(px)
    }

    fn ascent(&self, px: f32) -> f32 {
        self.font.horizontal_line_metrics(px).map(|m| m.ascent).unwrap_or(px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cascadia Mono (SIL OFL 1.1) — see `ember2d/assets/fonts/ATTRIBUTION.md`.
    /// Test-only for now; not yet referenced by any theme (Part 3).
    const TEST_FONT: &[u8] = include_bytes!("../../../assets/fonts/CascadiaMono.ttf");

    fn test_font() -> TtfFont {
        TtfFont::from_bytes(TEST_FONT, 0).expect("bundled test font must parse")
    }

    #[test]
    fn glyph_is_cached_and_returns_identical_info_on_repeat_calls() {
        let mut font = test_font();
        let first = font.glyph('A', 16.0).unwrap();
        let second = font.glyph('A', 16.0).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn glyph_at_different_sizes_produces_distinct_cache_entries() {
        let mut font = test_font();
        let small = font.glyph('A', 16.0).unwrap();
        let large = font.glyph('A', 32.0).unwrap();
        assert_ne!(small, large, "a 2x size request must not reuse the smaller glyph's cached metrics");
        assert!(large.advance > small.advance);
    }

    #[test]
    fn whitespace_glyph_has_zero_atlas_size_but_a_real_advance() {
        let mut font = test_font();
        let space = font.glyph(' ', 16.0).unwrap();
        assert_eq!((space.atlas_rect.w, space.atlas_rect.h), (0.0, 0.0));
        assert!(space.advance > 0.0, "a space still moves the pen forward");
    }

    #[test]
    fn measure_matches_the_sum_of_each_glyphs_own_advance() {
        let mut font = test_font();
        let a = font.glyph('A', 16.0).unwrap().advance;
        let b = font.glyph('B', 16.0).unwrap().advance;
        let (w, h) = font.measure("AB", 16.0);
        assert_eq!(w, a + b);
        assert_eq!(h, font.line_height(16.0));
    }

    #[test]
    fn measure_of_empty_text_is_zero_width_with_a_real_line_height() {
        let mut font = test_font();
        let (w, h) = font.measure("", 16.0);
        assert_eq!(w, 0.0);
        assert!(h > 0.0);
    }

    #[test]
    fn line_height_and_ascent_grow_with_requested_size() {
        let font = test_font();
        assert!(font.line_height(32.0) > font.line_height(16.0));
        assert!(font.ascent(32.0) > font.ascent(16.0));
    }

    #[test]
    fn texture_id_is_stable_across_repeated_glyph_requests() {
        let mut font = test_font();
        let id_before = font.texture_id();
        font.glyph('A', 16.0);
        font.glyph('Z', 24.0);
        assert_eq!(font.texture_id(), id_before, "the atlas is one texture for this TtfFont's whole lifetime");
    }

    #[test]
    fn texture_size_matches_the_atlas_it_was_constructed_with() {
        let font = TtfFont::with_atlas_size(TEST_FONT, 0, 256, 128).expect("bundled test font must parse");
        assert_eq!(font.texture_size(), (256, 128));
    }

    #[test]
    fn glyph_offset_places_the_pen_on_the_baseline() {
        // Phase 7 Part 2d (docs/ember2d-phase7-plan.md): a letter with no
        // descender (like 'A') has its bottom edge AT the baseline, so
        // moving from the pen up by the glyph's own bitmap height must
        // land exactly on its top edge — i.e. `offset.y` is the negated
        // bitmap height for a glyph with no descender.
        let mut font = test_font();
        let g = font.glyph('A', 32.0).unwrap();
        assert_eq!(g.offset.y, -g.atlas_rect.h);
    }

    #[test]
    fn a_tiny_atlas_eventually_refuses_further_glyphs_instead_of_corrupting_earlier_ones() {
        let mut font = TtfFont::with_atlas_size(TEST_FONT, 0, 8, 8).expect("bundled test font must parse");
        // A single glyph at a normal reading size is already bigger than
        // this atlas — the very first request must come back empty
        // (`None`), not panic or silently write out of bounds.
        assert_eq!(font.glyph('A', 32.0), None);
    }
}
