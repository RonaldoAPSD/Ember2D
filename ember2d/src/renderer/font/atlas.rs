// renderer/font/atlas.rs — GlyphAtlas: the dynamic, shelf-packed texture
// `TtfFont` rasterizes into on a cache miss (Phase 7 Part 2b,
// docs/ember2d-phase7-plan.md).
//
// Cache keyed `(font_id, char, quantized px)` — quantized to 0.5px
// increments (`quantize_px`) so continuous zoom doesn't mint a new atlas
// entry every frame, per the plan's own wording. No eviction: an editor
// uses a few hundred distinct glyphs across a handful of sizes and won't
// fill even a modest atlas; a full atlas logs one warning (not per-miss —
// see `full`) and every further miss at that size just returns `None`.
// Solve real eviction if this is ever actually observed, not preemptively.
//
// NOT YET WIRED to the GPU: `texture` is a CPU-side pixel buffer a caller
// re-uploads (whole or dirty-rect) once actual backend integration lands
// (Part 4) — this module only owns rasterizing and packing glyph bytes
// correctly, not getting them onto a GPU surface.

use std::collections::HashMap;
use super::super::texture::{Texture, TextureId};
use super::GlyphInfo;
use ember2d_sim::math::{Rect, Vec2};

pub struct GlyphAtlas {
    pub texture: Texture,
    cache: HashMap<(usize, char, u32), GlyphInfo>,
    shelf_x: u32,
    shelf_y: u32,
    shelf_h: u32,
    /// Set once the atlas has failed to pack a glyph — suppresses every
    /// further "atlas full" warning at the same size/font so a genuinely
    /// exhausted atlas doesn't spam `eprintln!` once per frame.
    full: bool,
    /// Set whenever a newly-packed glyph mutates `texture.pixels` in
    /// place, cleared by `take_dirty` (Phase 7 Part 2d,
    /// docs/ember2d-phase7-plan.md) — a caller that actually uploads this
    /// texture to a GPU needs to know when to re-upload, since a texture
    /// once uploaded is normally assumed immutable (see
    /// `RenderBackend::invalidate_texture`'s own doc comment).
    dirty: bool,
}

impl GlyphAtlas {
    /// `fill = 0x0000_0000` (fully transparent) is the right default for
    /// callers wiring this into real rendering — an unpacked-into pixel
    /// should composite as nothing, not stray opaque white.
    pub fn new(width: u32, height: u32, fill: u32) -> Self {
        GlyphAtlas {
            texture: Texture::blank(width, height, fill),
            cache: HashMap::new(),
            shelf_x: 0,
            shelf_y: 0,
            shelf_h: 0,
            full: false,
            dirty: false,
        }
    }

    pub fn texture_id(&self) -> TextureId { TextureId(self.texture.id) }

    /// Returns whether `texture.pixels` has changed since the last call,
    /// clearing the flag either way (a caller checks this once per draw,
    /// not once per glyph).
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// 0.5px increments, represented as a count of half-pixels — "px_size
    /// must be quantized... before it becomes a cache key" (2b).
    fn quantize_px(px: f32) -> u32 {
        (px * 2.0).round().max(0.0) as u32
    }

    /// Shelf bin-packing: place `w`x`h` at the current shelf's cursor,
    /// wrapping to a new shelf (below the tallest glyph packed on the
    /// current one) when it doesn't fit the remaining width. Returns
    /// `None` if it doesn't fit anywhere in the atlas at all — either
    /// `w`/`h` alone exceeds the atlas's own dimensions, or every shelf
    /// (current and any future one within `texture.height`) is full.
    fn pack(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w > self.texture.width || h > self.texture.height { return None; }
        if self.shelf_x + w > self.texture.width {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > self.texture.height { return None; }
        let pos = (self.shelf_x, self.shelf_y);
        self.shelf_x += w;
        self.shelf_h = self.shelf_h.max(h);
        Some(pos)
    }

    /// Fetch `ch`'s `GlyphInfo` at `px` for font `font_id`, rasterizing
    /// and packing it into the atlas on a cache miss. `font_id` is the
    /// caller's own identifier (distinct `TtfFont` instances must pass
    /// distinct ids if they ever share one atlas) — this module has no
    /// opinion on how ids are assigned.
    pub fn get_or_rasterize(&mut self, font: &fontdue::Font, font_id: usize, ch: char, px: f32) -> Option<GlyphInfo> {
        let key = (font_id, ch, Self::quantize_px(px));
        if let Some(info) = self.cache.get(&key) { return Some(*info); }

        let (metrics, bitmap) = font.rasterize(ch, px);

        // Whitespace (and any glyph with no visible ink) has a real
        // advance but nothing to pack — a zero-size `atlas_rect` is a
        // valid, cacheable answer, not a miss.
        if metrics.width == 0 || metrics.height == 0 {
            let info = GlyphInfo {
                atlas_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
                offset: Vec2::ZERO,
                advance: metrics.advance_width,
            };
            self.cache.insert(key, info);
            return Some(info);
        }

        let Some((x, y)) = self.pack(metrics.width as u32, metrics.height as u32) else {
            if !self.full {
                eprintln!(
                    "GlyphAtlas: full at {}x{} packing '{}' @ {}px for font {} — further misses at this size will silently return None (docs/ember2d-phase7-plan.md §2b: no eviction yet)",
                    self.texture.width, self.texture.height, ch, px, font_id
                );
                self.full = true;
            }
            return None;
        };

        // fontdue's bitmap is single-channel coverage (0-255); stored here
        // as opaque white with that coverage as alpha, so a future
        // alpha-blending draw path can composite it directly without a
        // separate "is this the font atlas" branch.
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let coverage = bitmap[row * metrics.width + col];
                let idx = (y as usize + row) * self.texture.width as usize + (x as usize + col);
                self.texture.pixels[idx] = ((coverage as u32) << 24) | 0x00FF_FFFF;
            }
        }
        self.dirty = true;

        let info = GlyphInfo {
            atlas_rect: Rect::new(x as f32, y as f32, metrics.width as f32, metrics.height as f32),
            // fontdue's `ymin` is the bitmap's BOTTOM edge, measured
            // upward from the baseline (negative if below it, i.e. a
            // descender) — the bitmap's TOP edge therefore sits at
            // `ymin + height` above the baseline. This engine is y-DOWN,
            // so "top-left corner offset from the pen (on the baseline)"
            // negates that upward distance.
            offset: Vec2::new(metrics.xmin as f32, -(metrics.ymin as f32 + metrics.height as f32)),
            advance: metrics.advance_width,
        };
        self.cache.insert(key, info);
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_places_shapes_left_to_right_on_one_shelf() {
        let mut atlas = GlyphAtlas::new(64, 64, 0);
        assert_eq!(atlas.pack(10, 10), Some((0, 0)));
        assert_eq!(atlas.pack(10, 8), Some((10, 0)));
        assert_eq!(atlas.pack(5, 12), Some((20, 0)));
    }

    #[test]
    fn pack_wraps_to_a_new_shelf_below_the_tallest_glyph_on_the_current_one() {
        let mut atlas = GlyphAtlas::new(20, 64, 0);
        assert_eq!(atlas.pack(12, 10), Some((0, 0)));
        // Doesn't fit next to the first (12+12 > 20) — wraps down past the
        // first shelf's own height (10, the tallest thing packed on it),
        // not down by the new glyph's own height.
        assert_eq!(atlas.pack(12, 6), Some((0, 10)));
    }

    #[test]
    fn pack_returns_none_when_a_shape_cannot_fit_anywhere() {
        let mut atlas = GlyphAtlas::new(16, 16, 0);
        assert_eq!(atlas.pack(20, 4), None, "wider than the whole atlas");
        assert_eq!(atlas.pack(4, 20), None, "taller than the whole atlas");
    }

    #[test]
    fn pack_returns_none_once_every_shelf_is_exhausted() {
        let mut atlas = GlyphAtlas::new(10, 10, 0);
        assert_eq!(atlas.pack(10, 5), Some((0, 0)));
        assert_eq!(atlas.pack(10, 5), Some((0, 5)));
        // The atlas is now fully covered (two 10x5 shelves in a 10x10
        // atlas) — nothing else fits, however small.
        assert_eq!(atlas.pack(1, 1), None);
    }
}
