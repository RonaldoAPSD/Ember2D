// renderer/tests.rs — split out of renderer/mod.rs (7B-2,
// docs/ember2d-master-plan.md §5.2) once that file crossed the project's
// 750-line hard limit (CLAUDE.md) adding ScreenMapping/compute_layout and
// their own test coverage. Same `#[path = "..."] mod tests;` pattern
// scripting/engine.rs already established for the same reason — see that
// file's own header comment. `use super::*` reaches renderer/mod.rs's own
// items exactly as it did as a nested `mod tests`.
//
// The pure coordinate-space free functions this file used to test
// (`compute_layout`, `screen_cell_to_pixel`, `pixel_size_to_cells`,
// `uv_rect_for`, `nine_slice_quads`) moved to `geometry.rs` at 7D-3
// (docs/ember2d-master-plan.md §5.4), along with their own tests — see that
// file's `mod tests` for them. What's left here is `Renderer`/`Camera`
// integration coverage that doesn't belong in a pure-geometry file.

use super::*;
use crate::camera::Camera;
use ember2d_sim::math::Vec2;

#[test]
fn camera_position_lands_at_the_viewport_center_in_pixels() {
    let mut cam = Camera::new(80.0, 24.0);
    cam.position = Vec2::new(10.0, 5.0);
    cam.zoom = 1.0;

    let (px, py) = screen_cell_to_pixel(cam.world_to_screen(cam.position));
    assert_eq!((px, py), ((40 * CELL_W) as i32, (12 * CELL_H) as i32));
}

/// Step 9-5 (docs/ember2d-master-plan.md §5.8): with a 16×16 world cell
/// (`cell_scale` (2, 1)) one world unit is 16 logical pixels both ways.
#[test]
fn a_square_world_cell_is_square_in_pixels() {
    let mut cam = Camera::new(80.0, 24.0);
    cam.cell_scale = Vec2::new(2.0, 1.0);
    cam.position = Vec2::new(10.0, 5.0);
    let (x0, y0) = screen_cell_to_pixel(cam.world_to_screen(Vec2::new(10.0, 5.0)));
    let (x1, y1) = screen_cell_to_pixel(cam.world_to_screen(Vec2::new(11.0, 6.0)));
    assert_eq!((x1 - x0, y1 - y0), (16, 16));
}
