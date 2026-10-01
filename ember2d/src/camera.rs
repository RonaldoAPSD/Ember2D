// camera.rs — World-space camera (Phase 2 of docs/ember2d-refactor-plan.md).
//
// Converts between WORLD space (float units — entities, tiles, colliders
// live here) and SCREEN space (cells, before the renderer's own pixel
// scaling). Everything else — PlayState's old hand-rolled `cam_x`/`cam_y`
// integer offset, the editor's separate `scroll`/`zoom` pair — either routes
// through this or (for the editor, untouched until Phase 7) stays as-is.
//
// COORDINATE MATH:
//   The camera is centered on `position` and shows a `viewport_width` ×
//   `viewport_height` (in screen cells) window onto the world, scaled by
//   `zoom` (screen cells per world unit). At zoom 1.0, one world unit is one
//   screen cell — the same mapping PlayState used before this existed.
//   In an ASCII project `zoom` should stay an integer so glyphs stay crisp;
//   that's a project-setting decision made by the caller, not enforced here.
//
//   Step 9-5 (docs/ember2d-master-plan.md §5.8): `cell_scale` is how many
//   screen cells (8×16 glyph cells) one world unit spans at zoom 1, per
//   axis — (1, 1) for the classic grid, (2, 1) for a project whose world
//   cell is 16×16 px, so square sprites draw square. Screen space stays in
//   glyph cells (the HUD, widgets and `viewport_*` are untouched); only the
//   world side of the conversion stretches.

use ember2d_sim::math::Vec2;

#[derive(Debug, Clone, Copy)]
pub struct Camera {
    /// World-space point the camera is centered on.
    pub position: Vec2,

    /// Screen cells per world unit. 1.0 = one world unit is one screen cell.
    pub zoom: f32,

    /// Visible viewport size, in screen cells.
    pub viewport_width: f32,
    pub viewport_height: f32,

    /// Where screen (0, 0) of this camera's *own* coordinate space lands on
    /// the actual renderer surface. Lets a camera render into a sub-region
    /// of the screen — e.g. PlayState reserves row 0 for a HUD bar, so its
    /// camera's `viewport_origin` is `(0, 1)`: content row 0 becomes actual
    /// screen row 1. This is Step 2e's replacement for the old hardcoded
    /// `+1`/`-1` HUD-row fudge that used to be hand-duplicated between the
    /// render loop and `get_mouse_world_y` — one number, one place, used by
    /// both directions of the world<->screen conversion below.
    pub viewport_origin: Vec2,

    /// Screen cells per world unit at zoom 1, per axis — see the header.
    /// Set once from the project's `world_cell` (`ProjectData::cell_scale`).
    pub cell_scale: Vec2,
}

impl Camera {
    pub fn new(viewport_width: f32, viewport_height: f32) -> Self {
        Camera {
            position: Vec2::ZERO,
            zoom: 1.0,
            viewport_width,
            viewport_height,
            viewport_origin: Vec2::ZERO,
            cell_scale: Vec2::new(1.0, 1.0),
        }
    }

    /// Screen cells one world unit spans right now, per axis: `zoom ×
    /// cell_scale`. What every world-to-screen conversion multiplies by.
    pub fn cells_per_unit(&self) -> Vec2 {
        Vec2::new(self.zoom * self.cell_scale.x, self.zoom * self.cell_scale.y)
    }

    /// Half the visible world extent along each axis, in world units.
    fn half_extent(&self) -> Vec2 {
        let s = self.cells_per_unit();
        Vec2::new(self.viewport_width / 2.0 / s.x, self.viewport_height / 2.0 / s.y)
    }

    /// World-space position of the viewport's top-left corner.
    pub fn top_left(&self) -> Vec2 {
        self.position - self.half_extent()
    }

    /// Convert a world-space point to a screen-space cell position.
    pub fn world_to_screen(&self, world: Vec2) -> Vec2 {
        let d = world - self.top_left();
        let s = self.cells_per_unit();
        Vec2::new(d.x * s.x, d.y * s.y) + self.viewport_origin
    }

    /// Convert a screen-space cell position back to world space — the
    /// inverse of `world_to_screen`. This is what replaces the ad hoc
    /// `get_mouse_world_x`/`get_mouse_world_y` math (and its hardcoded
    /// HUD-row fudge) once `PlayState` owns a real `Camera` (Step 2e).
    pub fn screen_to_world(&self, screen: Vec2) -> Vec2 {
        let d = screen - self.viewport_origin;
        let s = self.cells_per_unit();
        Vec2::new(d.x / s.x, d.y / s.y) + self.top_left()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: Vec2, b: Vec2) {
        assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4, "{:?} != {:?}", a, b);
    }

    #[test]
    fn round_trips_through_world_and_screen_space() {
        let mut cam = Camera::new(80.0, 24.0);
        cam.position = Vec2::new(37.5, 12.25);
        cam.zoom = 2.0;

        for p in [Vec2::new(0.0, 0.0), Vec2::new(100.0, 50.0), Vec2::new(-10.0, -3.0), cam.position]
        {
            approx_eq(cam.screen_to_world(cam.world_to_screen(p)), p);
        }
    }

    #[test]
    fn camera_position_maps_to_viewport_center() {
        for zoom in [0.5, 1.0, 2.0, 4.0] {
            let mut cam = Camera::new(80.0, 24.0);
            cam.position = Vec2::new(12.0, 8.0);
            cam.zoom = zoom;
            approx_eq(cam.world_to_screen(cam.position), Vec2::new(40.0, 12.0));
        }
    }

    #[test]
    fn zoom_one_matches_the_old_direct_cell_mapping() {
        // At zoom 1.0 this must behave exactly like PlayState's old
        // `world_pos - Vec2::new(cam_x, cam_y)` subtraction.
        let mut cam = Camera::new(80.0, 24.0);
        cam.position = Vec2::new(40.0, 12.0); // centered exactly on the viewport
        approx_eq(cam.world_to_screen(Vec2::new(45.0, 15.0)), Vec2::new(45.0, 15.0));
    }

    #[test]
    fn zooming_in_halves_the_visible_world_span() {
        let mut cam = Camera::new(80.0, 24.0);
        cam.zoom = 2.0;
        let left = cam.screen_to_world(Vec2::new(0.0, 0.0));
        let right = cam.screen_to_world(Vec2::new(80.0, 0.0));
        assert!(
            (right.x - left.x - 40.0).abs() < 1e-4,
            "zoom 2.0 over an 80-cell viewport should show 40 world units"
        );
    }

    /// Step 9-5: a 16×16 world cell is two glyph cells wide and one tall,
    /// so one world unit spans (2, 1) screen cells at zoom 1, and the
    /// visible world area halves horizontally.
    #[test]
    fn a_square_world_cell_stretches_only_the_world_side() {
        let mut cam = Camera::new(80.0, 24.0);
        cam.cell_scale = Vec2::new(2.0, 1.0);
        cam.position = Vec2::new(20.0, 12.0);
        approx_eq(cam.world_to_screen(cam.position), Vec2::new(40.0, 12.0));
        approx_eq(cam.world_to_screen(Vec2::new(21.0, 13.0)), Vec2::new(42.0, 13.0));
        approx_eq(cam.top_left(), Vec2::new(0.0, 0.0));
        cam.zoom = 2.0;
        for p in [Vec2::new(3.0, 4.0), Vec2::new(-7.5, 30.25)] {
            approx_eq(cam.screen_to_world(cam.world_to_screen(p)), p);
        }
        assert_eq!(cam.cells_per_unit(), Vec2::new(4.0, 2.0));
    }

    #[test]
    fn viewport_origin_shifts_where_content_lands_without_changing_the_math() {
        // PlayState's HUD-row use case: a camera whose own space starts at
        // content row 0, but that content should render at actual screen
        // row 1 (row 0 reserved for a HUD bar).
        let mut cam = Camera::new(80.0, 22.0); // 22 content rows, not the full 24
        cam.viewport_origin = Vec2::new(0.0, 1.0);

        // The camera's own center still lands at its own (40, 11) — origin
        // is a post-hoc shift, it doesn't change the camera's internal math.
        approx_eq(cam.world_to_screen(cam.position) - cam.viewport_origin, Vec2::new(40.0, 11.0));

        // Round-trips exactly like the zero-origin case.
        let p = Vec2::new(12.0, 34.0);
        approx_eq(cam.screen_to_world(cam.world_to_screen(p)), p);

        // A mouse click at actual screen row 1 (top of the content area)
        // must map back to the camera's own top-left, not one row off.
        approx_eq(cam.screen_to_world(Vec2::new(0.0, 1.0)), cam.top_left());
    }
}
