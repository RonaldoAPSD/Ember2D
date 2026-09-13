// renderer/tests.rs — split out of renderer/mod.rs (7B-2,
// docs/ember2d-master-plan.md §5.2) once that file crossed the project's
// 750-line hard limit (CLAUDE.md) adding ScreenMapping/compute_layout and
// their own test coverage. Same `#[path = "..."] mod tests;` pattern
// scripting/engine.rs already established for the same reason — see that
// file's own header comment. `use super::*` reaches renderer/mod.rs's own
// items exactly as it did as a nested `mod tests`.

use super::*;
use crate::camera::Camera;
use ember2d_sim::math::{Rect, Vec2};

#[test]
fn screen_cell_to_pixel_scales_by_cell_size_and_rounds() {
    assert_eq!(screen_cell_to_pixel(Vec2::new(0.0, 0.0)), (0, 0));
    assert_eq!(screen_cell_to_pixel(Vec2::new(1.0, 1.0)), (CELL_W as i32, CELL_H as i32));
    assert_eq!(screen_cell_to_pixel(Vec2::new(2.5, 3.0)), (20, 48)); // 2.5*8=20, 3.0*16=48
}

#[test]
fn camera_position_lands_at_the_viewport_center_in_pixels() {
    let mut cam = Camera::new(80.0, 24.0);
    cam.position = Vec2::new(10.0, 5.0);
    cam.zoom = 1.0;

    let (px, py) = screen_cell_to_pixel(cam.world_to_screen(cam.position));
    assert_eq!((px, py), ((40 * CELL_W) as i32, (12 * CELL_H) as i32));
}

// ── Tests: 7B-2 (docs/ember2d-master-plan.md §5.2, R21) — letterboxed
// integer cell projection and per-axis/HiDPI screen mapping ─────────────

#[test]
fn compute_layout_has_no_letterbox_at_an_exact_multiple() {
    // 80x24 cells at scale 2.0: exactly 1280x768 physical pixels.
    let (cells_w, cells_h, mapping) = compute_layout(1280, 768, 2.0);
    assert_eq!((cells_w, cells_h), (80, 24));
    assert_eq!(mapping.origin_px, (0.0, 0.0));
    assert_eq!(mapping.cell_px, (16.0, 32.0));
}

#[test]
fn compute_layout_letterboxes_instead_of_stretching_a_non_exact_size() {
    // 10px wider, 2px taller than the exact 1280x768 multiple — R21's
    // actual bug was every cell stretching to fill this remainder;
    // the fix floors the cell count (unchanged from the exact-multiple
    // case) and centers the leftover as a letterbox border instead.
    let (cells_w, cells_h, mapping) = compute_layout(1290, 770, 2.0);
    assert_eq!((cells_w, cells_h), (80, 24), "the remainder must not add a partial cell");
    assert_eq!(mapping.cell_px, (16.0, 32.0), "cell size in physical pixels must not change");
    assert_eq!(mapping.origin_px, (5.0, 1.0), "the remainder splits evenly on both sides");
}

#[test]
fn compute_layout_cell_px_scales_with_dpi() {
    let (_, _, at_1x) = compute_layout(2000, 2000, 1.0);
    let (_, _, at_2x) = compute_layout(2000, 2000, 2.0);
    assert_eq!(at_1x.cell_px, (CELL_W as f32, CELL_H as f32));
    assert_eq!(at_2x.cell_px, (CELL_W as f32 * 2.0, CELL_H as f32 * 2.0));
}

#[test]
fn compute_layout_enforces_the_minimum_grid_floor_below_a_tiny_window() {
    let (cells_w, cells_h, _) = compute_layout(100, 50, 1.0);
    assert_eq!((cells_w, cells_h), (20, 6), "same minimum floor try_handle_resize always enforced");
}

#[test]
fn screen_mapping_physical_to_logical_subtracts_origin_and_descales_per_axis() {
    let mapping = ScreenMapping { origin_px: (5.0, 1.0), cell_px: (16.0, 32.0) };
    // scale_x = 16/8 = 2.0, scale_y = 32/16 = 2.0
    assert_eq!(mapping.physical_to_logical((5.0 + 32.0, 1.0 + 64.0)), (16.0, 32.0));
    assert_eq!(
        mapping.physical_to_logical((5.0, 1.0)),
        (0.0, 0.0),
        "the origin itself maps to (0, 0)"
    );
}

// ── Tests: Phase 7 Part 1a pixel-space primitives
// (docs/ember2d-phase7-plan.md) ─────────────────────────────────────────

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
    let quads = nine_slice_quads(dest, Rect::new(0.0, 0.0, 30.0, 30.0), (5.0, 5.0, 5.0, 5.0));
    assert_eq!(quads.len(), 9);

    // Corners keep the exact border size on both source and dest sides
    // — that's what "1:1, not stretched" means.
    let (top_left_d, top_left_s) = quads[0];
    assert_eq!(top_left_d, Rect::new(10.0, 20.0, 5.0, 5.0));
    assert_eq!(top_left_s, Rect::new(0.0, 0.0, 5.0, 5.0));

    let (bottom_right_d, bottom_right_s) = quads[8];
    assert_eq!(bottom_right_d, Rect::new(105.0, 75.0, 5.0, 5.0));
    assert_eq!(bottom_right_s, Rect::new(25.0, 25.0, 5.0, 5.0));

    // The center (index 4) stretches: dest grows to fill the remaining
    // area, but its source rect stays the texture's own unstretched
    // middle — that's the actual point of a nine-slice.
    let (center_d, center_s) = quads[4];
    assert_eq!(center_d, Rect::new(15.0, 25.0, 90.0, 50.0));
    assert_eq!(center_s, Rect::new(5.0, 5.0, 20.0, 20.0));
}

#[test]
fn nine_slice_quads_with_zero_border_degenerates_to_one_stretched_center() {
    let dest = Rect::new(0.0, 0.0, 40.0, 20.0);
    let quads = nine_slice_quads(dest, Rect::new(0.0, 0.0, 8.0, 8.0), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(quads.len(), 9);
    // The four true corners (0, 2, 6, 8) collapse to zero in BOTH
    // dimensions — there's no border pixel to draw. The four edges
    // (1, 3, 5, 7) collapse only along the axis their border would
    // have occupied; the other axis still spans the whole dest, since
    // that's the axis the (now-zero) corners would otherwise have
    // shared width/height with.
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
        // top edge, bottom edge: zero height, full width
        let (d, _) = quads[i];
        assert_eq!(d.h, 0.0, "edge quad {i} should be degenerate along its border axis");
        assert_eq!(d.w, dest.w);
    }
    for &i in &[3, 5] {
        // left edge, right edge: zero width, full height
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
    // dest (30px) is smaller than the combined left+right border (40px)
    // — the middle column must clamp to zero width, not go negative.
    let dest = Rect::new(0.0, 0.0, 30.0, 30.0);
    let quads = nine_slice_quads(dest, Rect::new(0.0, 0.0, 100.0, 100.0), (20.0, 20.0, 20.0, 20.0));
    let (center_d, _) = quads[4];
    assert_eq!(center_d.w, 0.0);
    assert_eq!(center_d.h, 0.0);
}

/// Regression test (7D-2, master plan §5.4): found live running the
/// editor after wiring `SliceRole`'s own `NineSlice.src` through a shared
/// theme atlas — `draw_nine_slice`/`nine_slice_quads` originally hardcoded
/// `src_x`/`src_y` starting at `(0.0, 0.0)` (`texture.width`/`.height`
/// AS the whole source region), which was fine for a texture dedicated to
/// exactly one 9-slice but silently sampled from the WRONG region — often
/// spilling into several OTHER slices packed into the same atlas — the
/// instant `src` named a sub-rect anywhere but the atlas's own origin.
/// Visually this showed up as small chrome elements (the close button,
/// resize grip) rendering as a tiled mosaic of several unrelated slice
/// colors instead of their own single intended one.
#[test]
fn nine_slice_quads_offsets_every_src_rect_by_a_non_zero_atlas_origin() {
    let dest = Rect::new(0.0, 0.0, 100.0, 100.0);
    // A slice living at (64.0, 32.0) in a larger shared atlas, not at the
    // atlas's own origin.
    let src = Rect::new(64.0, 32.0, 30.0, 30.0);
    let quads = nine_slice_quads(dest, src, (5.0, 5.0, 5.0, 5.0));

    let (_, top_left_s) = quads[0];
    assert_eq!(
        top_left_s,
        Rect::new(64.0, 32.0, 5.0, 5.0),
        "the top-left corner's source rect must start at the atlas sub-rect's own origin, not (0,0)"
    );

    let (_, bottom_right_s) = quads[8];
    assert_eq!(
        bottom_right_s,
        Rect::new(89.0, 57.0, 5.0, 5.0),
        "the bottom-right corner must stay within this slice's own 30x30 region, offset by src's origin"
    );

    let (_, center_s) = quads[4];
    assert_eq!(
        center_s,
        Rect::new(69.0, 37.0, 20.0, 20.0),
        "the stretched center must sample this slice's own middle, not the atlas's"
    );
}
