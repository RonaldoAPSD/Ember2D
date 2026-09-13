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
