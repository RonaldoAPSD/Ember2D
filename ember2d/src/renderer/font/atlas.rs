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

use super::super::texture::{Texture, TextureId};
use super::GlyphInfo;
use ember2d_sim::math::{Rect, Vec2};
use std::collections::HashMap;

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
    /// `WgpuBackend::invalidate_texture`'s own doc comment).
    dirty: bool,
}

/// A `GlyphAtlas` side length (a square power-of-two texture) big enough to
/// hold the editor's ASCII prewarm set (`theme_loader::ASCII_PREWARM`, the
/// printable ASCII range) at up to three font sizes, at `max_raster_px`
/// (7D-3, docs/ember2d-master-plan.md §5.4 — R75: sized once per UI-scale
/// change, since a higher `ui_scale` rasterizes every glyph physically
/// larger and the pre-7D-3 fixed `DEFAULT_ATLAS_SIZE` would otherwise start
/// silently dropping glyphs at high scale, per this module's own "no
/// eviction" doc comment above). Rather than computing each glyph's exact
/// `fontdue` metrics (which vary per font and aren't known until rasterized
/// — a chicken-and-egg problem for sizing the atlas BEFORE rasterizing into
/// it), this uses a deliberately generous per-glyph bounding box
/// (`max_raster_px` square plus 2px of shelf-packing slack, larger than any
/// real glyph at that size) and rounds the total area up to the next power
/// of two, clamped to `512` (never smaller than the pre-7D-3
/// `DEFAULT_ATLAS_SIZE`) and `4096` (`wgpu::Limits::default()`'s texture
/// dimension ceiling is 8192 — this leaves headroom rather than pushing
/// against it).
pub fn glyph_atlas_side_for(max_raster_px: f32) -> u32 {
    const GLYPHS_PER_SIZE: f32 = 96.0; // printable ASCII, 0x20..=0x7E plus one
    const SIZES_TO_PREWARM: f32 = 3.0; // a theme's small/body/heading
    let glyph_area = (max_raster_px.max(1.0) + 2.0).powi(2);
    let total_area = glyph_area * GLYPHS_PER_SIZE * SIZES_TO_PREWARM;
    let side = total_area.sqrt().ceil() as u32;
    side.next_power_of_two().clamp(512, 4096)
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

    pub fn texture_id(&self) -> TextureId {
        TextureId(self.texture.id)
    }

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
        if w > self.texture.width || h > self.texture.height {
            return None;
        }
        if self.shelf_x + w > self.texture.width {
            self.shelf_y += self.shelf_h;
            self.shelf_x = 0;
            self.shelf_h = 0;
        }
        if self.shelf_y + h > self.texture.height {
            return None;
        }
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
    pub fn get_or_rasterize(
        &mut self,
        font: &fontdue::Font,
        font_id: usize,
        ch: char,
        px: f32,
    ) -> Option<GlyphInfo> {
        let quantized = Self::quantize_px(px);
        let key = (font_id, ch, quantized);
        if let Some(info) = self.cache.get(&key) {
            return Some(*info);
        }

        // 7B-5 (docs/ember2d-master-plan.md §5.2): rasterize at the SAME
        // quantized size the cache is keyed on, not the raw `px` this call
        // happened to be asked for — two requests that quantize to the
        // same key (within `quantize_px`'s 0.5px bucket) must be
        // guaranteed to return the exact same bitmap either way. Using raw
        // `px` here made that only true by accident: whichever raw value
        // reached this line FIRST for a given bucket silently became the
        // one every later, slightly-different raw request for that same
        // bucket got back from cache — deterministic given a fixed call
        // sequence, but not given the same cache KEY, which is the actual
        // contract `quantize_px`'s own doc comment promises.
        let quantized_px = quantized as f32 / 2.0;
        let (metrics, bitmap) = font.rasterize(ch, quantized_px);

        // Whitespace (and any glyph with no visible ink) has a real
        // advance but nothing to pack — a zero-size `atlas_rect` is a
        // valid, cacheable answer, not a miss.
        if metrics.width == 0 || metrics.height == 0 {
            let info = GlyphInfo {
                atlas_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
                offset: Vec2::ZERO,
                advance: metrics.advance_width,
                size: Vec2::ZERO,
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
            // R50 (§3 in the master plan, fixed 7D-3): `TtfFont` always
            // rasterizes a glyph at exactly the requested size, so its
            // drawn size and its atlas rect's size are the same value —
            // unlike `BitmapFont` (see that impl's own `GlyphInfo::size`
            // comment), there's no separate "native vs requested" gap here.
            size: Vec2::new(metrics.width as f32, metrics.height as f32),
        };
        self.cache.insert(key, info);
        Some(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_atlas_side_grows_with_the_largest_raster_size() {
        let small = glyph_atlas_side_for(11.0);
        let large = glyph_atlas_side_for(64.0); // e.g. a 16pt heading at ui_scale 4
        assert!(large > small, "a bigger requested raster size must never produce a smaller atlas");
        assert!(small.is_power_of_two() && large.is_power_of_two());
    }

    #[test]
    fn glyph_atlas_side_never_shrinks_below_the_pre_7d_3_default_or_past_the_wgpu_headroom_cap() {
        assert_eq!(glyph_atlas_side_for(0.5), 512, "clamped up to the old fixed default");
        assert_eq!(glyph_atlas_side_for(10_000.0), 4096, "clamped down, leaving headroom under wgpu's 8192 limit");
    }

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
