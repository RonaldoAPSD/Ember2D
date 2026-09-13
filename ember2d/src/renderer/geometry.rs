// renderer/geometry.rs — pure coordinate-space math: screen<->cell mapping,
// pixel/UV conversions, and nine-slice quad layout. Extracted from
// renderer/mod.rs (7D-3, docs/ember2d-master-plan.md §5.4) once that file
// needed room for the new UI-points machinery (`ui_space.rs`/`ui_painter.rs`)
// — this is the same kind of "no live GPU device needed" free-function group
// `mod.rs`'s own header comments already called out as deliberately testable
// in isolation; splitting it into its own file just gives it a home that
// matches that intent instead of sharing space with `Renderer`'s wgpu setup.

use ember2d_sim::math::Rect;

/// Physical-pixel <-> cell-space mapping (7B-2, docs/ember2d-master-plan.md
/// §5.2, R21) — recomputed by `Renderer` on `new`/resize/DPI change,
/// exposed for `Engine`/`MouseState` to convert a raw physical cursor
/// position into the cell coordinates `mouse.cell_x`/`cell_y` (and the
/// pixel-space `mouse.pixel_x`/`pixel_y` the editor's `UiRect`/`UiFrame`
/// hit-testing already expects) without hardcoding `CELL_W`/`CELL_H`
/// themselves. Replaces the old single-axis, DPI-blind `scale_factor()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenMapping {
    /// Top-left of the letterboxed drawable area, in physical pixels. Zero
    /// unless the window's physical size isn't an exact multiple of
    /// `cell_px` — the remainder is split evenly on both sides rather than
    /// stretching every cell to fill it (R21's actual bug). Always a whole
    /// physical pixel (R71, 7D-3, docs/ember2d-master-plan.md §5.4) —
    /// `compute_layout` floors it, since an x.5 origin sampled every quad
    /// half a texel off the physical grid regardless of anything drawn on
    /// top of it being snapped correctly itself.
    pub origin_px: (f32, f32),
    /// Physical pixels per cell, per axis: `(CELL_W * scale, CELL_H *
    /// scale)`. Per-axis (not one shared scalar) because a non-square
    /// letterboxed remainder can round differently per axis even though
    /// `scale` itself is uniform.
    pub cell_px: (f32, f32),
}

impl ScreenMapping {
    /// A raw physical cursor position -> the letterbox-origin-relative,
    /// scale-descaled logical pixel position `MouseState::pixel_x`/`pixel_y`
    /// store (1 unit = 1 un-scaled `CELL_W`/`CELL_H` pixel, matching the
    /// editor's own pixel-space UI convention).
    pub fn physical_to_logical(&self, physical: (f32, f32)) -> (f32, f32) {
        let scale_x = self.cell_px.0 / super::CELL_W as f32;
        let scale_y = self.cell_px.1 / super::CELL_H as f32;
        ((physical.0 - self.origin_px.0) / scale_x, (physical.1 - self.origin_px.1) / scale_y)
    }
}

/// The cell grid and `ScreenMapping` a window of `physical_w`×`physical_h`
/// pixels produces at `scale` physical pixels per un-scaled `CELL_W`/
/// `CELL_H` pixel (7B-2, docs/ember2d-master-plan.md §5.2, R21). Floor
/// division (not the old ceiling division) plus letterboxing the remainder
/// is what stops individual cells from stretching to fill whatever's left
/// over — the actual R21 bug. `.max(20)`/`.max(6)` are the same minimum
/// grid floor `try_handle_resize` always enforced; if the window is
/// smaller than that minimum's own physical footprint, the drawable rect
/// is clamped to the real physical size in `WgpuBackend::render` (same
/// defensive clamp its scissor-rect handling already does) rather than
/// asking wgpu for an oversized viewport. A free function, not a method,
/// so it's testable without a live GPU-backed `Renderer` — same reasoning
/// as `screen_cell_to_pixel` below.
pub(super) fn compute_layout(
    physical_w: u32,
    physical_h: u32,
    scale: f32,
) -> (usize, usize, ScreenMapping) {
    let cell_px_w = super::CELL_W as f32 * scale;
    let cell_px_h = super::CELL_H as f32 * scale;
    let cells_w = ((physical_w as f32 / cell_px_w).floor() as usize).max(20);
    let cells_h = ((physical_h as f32 / cell_px_h).floor() as usize).max(6);
    let drawable_w = cells_w as f32 * cell_px_w;
    let drawable_h = cells_h as f32 * cell_px_h;
    // R71 (7D-3, docs/ember2d-master-plan.md §5.4): `.floor()` here, not
    // just `.max(0.0)` — an odd leftover remainder (e.g. a 1291px-wide
    // window at scale 2.0 leaves 11px, split as 5.5px each side) used to
    // put the origin at a half physical pixel, so every quad the renderer
    // drew sampled its texture half a texel off the physical grid no
    // matter how carefully anything on top of it snapped its own edges —
    // found investigating 7D-3's UI-points snapping design, not by any
    // visible symptom before now (a 1px-wide letterbox border is easy to
    // miss). Flooring instead of centering exactly means the leftover
    // border can be 1px wider on the right/bottom than the left/top when
    // the remainder is odd — an invisible tradeoff next to a blurry whole
    // window.
    let origin_x = ((physical_w as f32 - drawable_w) / 2.0).max(0.0).floor();
    let origin_y = ((physical_h as f32 - drawable_h) / 2.0).max(0.0).floor();
    (
        cells_w,
        cells_h,
        ScreenMapping { origin_px: (origin_x, origin_y), cell_px: (cell_px_w, cell_px_h) },
    )
}

/// Convert a screen-space cell position (as `Camera::world_to_screen`
/// returns it) into the pixel-snapped convention `draw_char_scaled_pixels`
/// and the backend's `draw_texture` expect — multiply by the cell size in
/// pixels, then round to a whole pixel. Pulled out as a free function so the
/// coordinate math (Step 2c) is testable without a live GPU-backed `Renderer`.
pub(super) fn screen_cell_to_pixel(screen: ember2d_sim::math::Vec2) -> (i32, i32) {
    (
        (screen.x * super::CELL_W as f32).round() as i32,
        (screen.y * super::CELL_H as f32).round() as i32,
    )
}

/// A pixel size converted to the "cell units" convention
/// `SpriteInstance::size`/`WgpuBackend::draw_texture`'s own `size`
/// parameter already use (see `Renderer::draw_texture`'s `scale`
/// computation for the same divide) — pulled out so `fill_rect_px`/
/// `draw_texture_px`'s coordinate math is testable without a live
/// GPU-backed `Renderer` (Phase 7 Part 1a).
pub(super) fn pixel_size_to_cells(w: f32, h: f32) -> [f32; 2] {
    [w / super::CELL_W as f32, h / super::CELL_H as f32]
}

/// A pixel-space sub-rect of a texture, normalized to the `[x, y, w, h]`
/// (0..1) convention `WgpuBackend::draw_texture`'s `uv_rect` expects — the
/// same computation `draw_texture_world` does inline, pulled out here
/// (Phase 7 Part 1a) so it's independently testable.
pub(super) fn uv_rect_for(texture_w: u32, texture_h: u32, src: Rect) -> [f32; 4] {
    [
        src.x / texture_w as f32,
        src.y / texture_h as f32,
        src.w / texture_w as f32,
        src.h / texture_h as f32,
    ]
}

/// The nine (dest, src) rect pairs `draw_nine_slice` draws, in row-major
/// order — index 4 is always the stretched center. `src` is the 9-slice's
/// own region of the texture (7D-2, master plan §5.4 — an atlas sub-rect,
/// not necessarily the whole texture); `border` is `(left, top, right,
/// bottom)` in SOURCE pixels, relative to `src`'s own origin, not the
/// texture's — always in the atlas's own, unscaled texel units, regardless
/// of `border_scale`. `border_scale` (7D-3, docs/ember2d-master-plan.md
/// §5.4) is the DESTINATION-side border multiplier: `1.0` reproduces every
/// caller's behavior before this parameter existed (a source texel drawn
/// 1:1); the UI-points painter passes `ui_scale / render_scale` so each
/// corner's SOURCE texel occupies `ui_scale` physical pixels on screen —
/// the same "1 unscaled pixel -> N physical pixels" contract chrome text
/// and fills already follow, just applied to a 9-slice corner instead of a
/// glyph. Corners keep their (possibly rescaled) size on the dest side
/// while staying 1:1 on the source side, which is what makes them crisp
/// rather than stretched, as long as `dest` is at least as large as the
/// combined left+right / top+bottom DESTINATION borders — a smaller `dest`
/// clamps the middle column/row to zero width/height rather than going
/// negative.
pub(super) fn nine_slice_quads(
    dest: Rect,
    src: Rect,
    border: (f32, f32, f32, f32),
    border_scale: f32,
) -> Vec<(Rect, Rect)> {
    let (bl, bt, br, bb) = border;
    let src_x = [src.x, src.x + bl, src.x + src.w - br];
    let src_w = [bl, (src.w - bl - br).max(0.0), br];
    let src_y = [src.y, src.y + bt, src.y + src.h - bb];
    let src_h = [bt, (src.h - bt - bb).max(0.0), bb];

    let (dbl, dbt, dbr, dbb) =
        (bl * border_scale, bt * border_scale, br * border_scale, bb * border_scale);
    let dst_x = [dest.x, dest.x + dbl, dest.x + dest.w - dbr];
    let dst_w = [dbl, (dest.w - dbl - dbr).max(0.0), dbr];
    let dst_y = [dest.y, dest.y + dbt, dest.y + dest.h - dbb];
    let dst_h = [dbt, (dest.h - dbt - dbb).max(0.0), dbb];

    let mut quads = Vec::with_capacity(9);
    for row in 0..3 {
        for col in 0..3 {
            let d = Rect::new(dst_x[col], dst_y[row], dst_w[col], dst_h[row]);
            let s = Rect::new(src_x[col], src_y[row], src_w[col], src_h[row]);
            quads.push((d, s));
        }
    }
    quads
}

/// Snap both edges of `rect` (in some source unit) to the nearest whole
/// multiple of `1.0 / scale` — i.e. to the grid `scale` units-per-target-unit
/// produces (7D-3, docs/ember2d-master-plan.md §5.4). Used for two distinct
/// grids: `Renderer::draw_texture_px` snaps a logical-pixel rect to the
/// PHYSICAL pixel grid (`scale` = `Renderer.scale`, R), and `UiSpace::snap`/
/// `snap_rect` snap a points rect to the same physical grid (`scale` =
/// `ui_scale`, S — points are already physical-pixel-sized by definition, so
/// snapping there means "round to the nearest whole point"). Snapping BOTH
/// edges independently (not origin-then-size) is what keeps two adjacent
/// quads sharing an edge value snapping to the same grid line instead of
/// drifting apart by rounding error and leaving a gap or overlap.
pub(super) fn snap_rect_to_scale(rect: Rect, scale: f32) -> Rect {
    let snap = |v: f32| (v * scale).round() / scale;
    let x0 = snap(rect.x);
    let y0 = snap(rect.y);
    let x1 = snap(rect.x + rect.w);
    let y1 = snap(rect.y + rect.h);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_cell_to_pixel_scales_by_cell_size_and_rounds() {
        assert_eq!(screen_cell_to_pixel(ember2d_sim::math::Vec2::new(0.0, 0.0)), (0, 0));
        assert_eq!(
            screen_cell_to_pixel(ember2d_sim::math::Vec2::new(1.0, 1.0)),
            (super::super::CELL_W as i32, super::super::CELL_H as i32)
        );
        assert_eq!(screen_cell_to_pixel(ember2d_sim::math::Vec2::new(2.5, 3.0)), (20, 48));
    }

    #[test]
    fn compute_layout_has_no_letterbox_at_an_exact_multiple() {
        let (cells_w, cells_h, mapping) = compute_layout(1280, 768, 2.0);
        assert_eq!((cells_w, cells_h), (80, 24));
        assert_eq!(mapping.origin_px, (0.0, 0.0));
        assert_eq!(mapping.cell_px, (16.0, 32.0));
    }

    #[test]
    fn compute_layout_letterboxes_instead_of_stretching_a_non_exact_size() {
        let (cells_w, cells_h, mapping) = compute_layout(1290, 770, 2.0);
        assert_eq!((cells_w, cells_h), (80, 24), "the remainder must not add a partial cell");
        assert_eq!(mapping.cell_px, (16.0, 32.0), "cell size in physical pixels must not change");
        assert_eq!(mapping.origin_px, (5.0, 1.0), "the remainder splits evenly on both sides");
    }

    /// R71 (7D-3, docs/ember2d-master-plan.md §5.4): an ODD leftover
    /// remainder (11 physical px wide, 3 tall) used to center the origin at
    /// a half physical pixel (5.5, 1.5) — every quad drawn then sampled its
    /// texture off the physical grid regardless of its own snapping.
    #[test]
    fn r71_letterbox_origin_is_always_a_whole_physical_pixel() {
        let (cells_w, cells_h, mapping) = compute_layout(1291, 771, 2.0);
        assert_eq!((cells_w, cells_h), (80, 24));
        assert_eq!(
            mapping.origin_px,
            (5.0, 1.0),
            "floors the centered remainder, never a half pixel"
        );
    }

    #[test]
    fn compute_layout_cell_px_scales_with_dpi() {
        let (_, _, at_1x) = compute_layout(2000, 2000, 1.0);
        let (_, _, at_2x) = compute_layout(2000, 2000, 2.0);
        assert_eq!(at_1x.cell_px, (super::super::CELL_W as f32, super::super::CELL_H as f32));
        assert_eq!(
            at_2x.cell_px,
            (super::super::CELL_W as f32 * 2.0, super::super::CELL_H as f32 * 2.0)
        );
    }

    #[test]
    fn compute_layout_enforces_the_minimum_grid_floor_below_a_tiny_window() {
        let (cells_w, cells_h, _) = compute_layout(100, 50, 1.0);
        assert_eq!(
            (cells_w, cells_h),
            (20, 6),
            "same minimum floor try_handle_resize always enforced"
        );
    }

    #[test]
    fn screen_mapping_physical_to_logical_subtracts_origin_and_descales_per_axis() {
        let mapping = ScreenMapping { origin_px: (5.0, 1.0), cell_px: (16.0, 32.0) };
        assert_eq!(mapping.physical_to_logical((5.0 + 32.0, 1.0 + 64.0)), (16.0, 32.0));
        assert_eq!(
            mapping.physical_to_logical((5.0, 1.0)),
            (0.0, 0.0),
            "the origin itself maps to (0, 0)"
        );
    }

    #[test]
    fn pixel_size_to_cells_divides_by_cell_dimensions() {
        assert_eq!(pixel_size_to_cells(8.0, 16.0), [1.0, 1.0]);
        assert_eq!(pixel_size_to_cells(16.0, 32.0), [2.0, 2.0]);
        assert_eq!(pixel_size_to_cells(4.0, 8.0), [0.5, 0.5]);
    }

    #[test]
    fn uv_rect_for_normalizes_a_pixel_sub_rect_to_0_1() {
        assert_eq!(uv_rect_for(64, 64, Rect::new(0.0, 0.0, 64.0, 64.0)), [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(uv_rect_for(64, 64, Rect::new(0.0, 0.0, 32.0, 32.0)), [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(uv_rect_for(100, 50, Rect::new(50.0, 25.0, 50.0, 25.0)), [0.5, 0.5, 0.5, 0.5]);
    }

    #[test]
    fn nine_slice_quads_produces_nine_quads_with_the_center_at_index_4() {
        let dest = Rect::new(10.0, 20.0, 100.0, 60.0);
        let quads =
            nine_slice_quads(dest, Rect::new(0.0, 0.0, 30.0, 30.0), (5.0, 5.0, 5.0, 5.0), 1.0);
        assert_eq!(quads.len(), 9);

        let (top_left_d, top_left_s) = quads[0];
        assert_eq!(top_left_d, Rect::new(10.0, 20.0, 5.0, 5.0));
        assert_eq!(top_left_s, Rect::new(0.0, 0.0, 5.0, 5.0));

        let (bottom_right_d, bottom_right_s) = quads[8];
        assert_eq!(bottom_right_d, Rect::new(105.0, 75.0, 5.0, 5.0));
        assert_eq!(bottom_right_s, Rect::new(25.0, 25.0, 5.0, 5.0));

        let (center_d, center_s) = quads[4];
        assert_eq!(center_d, Rect::new(15.0, 25.0, 90.0, 50.0));
        assert_eq!(center_s, Rect::new(5.0, 5.0, 20.0, 20.0));
    }

    #[test]
    fn nine_slice_quads_with_zero_border_degenerates_to_one_stretched_center() {
        let dest = Rect::new(0.0, 0.0, 40.0, 20.0);
        let quads =
            nine_slice_quads(dest, Rect::new(0.0, 0.0, 8.0, 8.0), (0.0, 0.0, 0.0, 0.0), 1.0);
        assert_eq!(quads.len(), 9);
        for &i in &[0, 2, 6, 8] {
            let (d, s) = quads[i];
            assert_eq!(
                (d.w, d.h),
                (0.0, 0.0),
                "corner quad {i} should be degenerate in both axes with a zero border"
            );
            assert_eq!(
                (s.w, s.h),
                (0.0, 0.0),
                "corner quad {i} should be degenerate in both axes with a zero border"
            );
        }
        for &i in &[1, 7] {
            let (d, _) = quads[i];
            assert_eq!(d.h, 0.0, "edge quad {i} should be degenerate along its border axis");
            assert_eq!(d.w, dest.w);
        }
        for &i in &[3, 5] {
            let (d, _) = quads[i];
            assert_eq!(d.w, 0.0, "edge quad {i} should be degenerate along its border axis");
            assert_eq!(d.h, dest.h);
        }
        let (center_d, center_s) = quads[4];
        assert_eq!(center_d, dest, "with a zero border the center dest must cover the whole rect");
        assert_eq!(
            center_s,
            Rect::new(0.0, 0.0, 8.0, 8.0),
            "with a zero border the center src must cover the whole texture"
        );
    }

    #[test]
    fn nine_slice_quads_clamps_a_dest_smaller_than_the_combined_borders() {
        let dest = Rect::new(0.0, 0.0, 30.0, 30.0);
        let quads = nine_slice_quads(
            dest,
            Rect::new(0.0, 0.0, 100.0, 100.0),
            (20.0, 20.0, 20.0, 20.0),
            1.0,
        );
        let (center_d, _) = quads[4];
        assert_eq!(center_d.w, 0.0);
        assert_eq!(center_d.h, 0.0);
    }

    /// Regression test (7D-2, master plan §5.4): found live running the
    /// editor after wiring `SliceRole`'s own `NineSlice.src` through a shared
    /// theme atlas — `draw_nine_slice`/`nine_slice_quads` originally hardcoded
    /// `src_x`/`src_y` starting at `(0.0, 0.0)`, which silently sampled the
    /// wrong region the instant `src` named a sub-rect anywhere but the
    /// atlas's own origin.
    #[test]
    fn nine_slice_quads_offsets_every_src_rect_by_a_non_zero_atlas_origin() {
        let dest = Rect::new(0.0, 0.0, 100.0, 100.0);
        let src = Rect::new(64.0, 32.0, 30.0, 30.0);
        let quads = nine_slice_quads(dest, src, (5.0, 5.0, 5.0, 5.0), 1.0);

        let (_, top_left_s) = quads[0];
        assert_eq!(top_left_s, Rect::new(64.0, 32.0, 5.0, 5.0), "the top-left corner's source rect must start at the atlas sub-rect's own origin, not (0,0)");

        let (_, bottom_right_s) = quads[8];
        assert_eq!(bottom_right_s, Rect::new(89.0, 57.0, 5.0, 5.0), "the bottom-right corner must stay within this slice's own 30x30 region, offset by src's origin");

        let (_, center_s) = quads[4];
        assert_eq!(
            center_s,
            Rect::new(69.0, 37.0, 20.0, 20.0),
            "the stretched center must sample this slice's own middle, not the atlas's"
        );
    }

    /// 7D-3 (docs/ember2d-master-plan.md §5.4): `border_scale` multiplies
    /// only the DESTINATION-side border, never the source region.
    #[test]
    fn nine_slice_quads_scales_dest_borders_by_the_border_scale() {
        let dest = Rect::new(0.0, 0.0, 100.0, 100.0);
        let src = Rect::new(0.0, 0.0, 30.0, 30.0);
        let quads = nine_slice_quads(dest, src, (5.0, 5.0, 5.0, 5.0), 1.5);

        let (top_left_d, top_left_s) = quads[0];
        assert_eq!(top_left_d, Rect::new(0.0, 0.0, 7.5, 7.5), "dest border scales by border_scale");
        assert_eq!(
            top_left_s,
            Rect::new(0.0, 0.0, 5.0, 5.0),
            "source border is unaffected by border_scale"
        );

        let (center_d, _) = quads[4];
        assert_eq!(center_d, Rect::new(7.5, 7.5, 85.0, 85.0));
    }

    #[test]
    fn snap_rect_to_scale_rounds_both_edges_to_the_same_grid() {
        // At scale 2.0 the grid is half-unit steps.
        let r = Rect::new(0.3, 0.3, 4.4, 4.4); // spans 0.3..4.7
        let snapped = snap_rect_to_scale(r, 2.0);
        assert_eq!(snapped.x, 0.5);
        assert_eq!(snapped.y, 0.5);
        assert_eq!(snapped.x + snapped.w, 4.5);
        assert_eq!(snapped.y + snapped.h, 4.5);
    }

    #[test]
    fn snap_rect_to_scale_leaves_an_already_aligned_rect_unchanged() {
        let r = Rect::new(2.0, 4.0, 10.0, 6.0);
        assert_eq!(snap_rect_to_scale(r, 3.0), r);
    }
}
