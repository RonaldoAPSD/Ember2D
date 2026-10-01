// play/camera_ctl.rs — moving play mode's camera each frame: what it
// follows, where it may go, how fast it gets there, how far it's zoomed.
//
// Step 9-2 (docs/ember2d-master-plan.md §5.8). Scripts decide all of that
// (`set_camera_target`/`set_camera_zoom`/`set_camera_bounds`/
// `set_camera_speed`, stored on `Simulation` as `CameraSettings`); this file
// is the presentation half that acts on it. It used to be a fixed block in
// `PlayState::update` that always followed the camera-follow actor at zoom
// 1, clamped to the level, at a hardcoded speed. The smoothing runs on real
// frame time and its `exp()` never reaches the simulation (refactor plan
// §5.2 H2) — the same rule as before, now just with script-chosen inputs.

use ember2d_sim::math::{Rect, Vec2};
use ember2d_sim::scripting::{CameraSettings, CameraTarget};
use ember2d_sim::world::World;

use super::PlayState;

/// Where the camera's centre may sit so a view of half-size
/// `(half_w, half_h)` stays inside `bounds`. When the bounds are smaller
/// than the view on an axis, the view's top/left edge sits on the bounds'
/// top/left edge — the way play mode has always shown a level smaller than
/// the window (anchored, not centred).
pub(super) fn clamp_center(target: Vec2, half_w: f32, half_h: f32, bounds: Rect) -> Vec2 {
    let min_x = bounds.x + half_w;
    let max_x = (bounds.x + bounds.w - half_w).max(min_x);
    let min_y = bounds.y + half_h;
    let max_y = (bounds.y + bounds.h - half_h).max(min_y);
    Vec2::new(target.x.clamp(min_x, max_x), target.y.clamp(min_y, max_y))
}

/// How far toward its target the camera moves this frame (0..=1).
pub(super) fn follow_fraction(speed: f32, frame_dt: f32) -> f32 {
    if speed <= 0.0 {
        1.0
    } else {
        1.0 - (-speed * frame_dt).exp()
    }
}

impl PlayState {
    /// The world point the camera wants to centre on this frame.
    fn camera_target(&self, world: &World, settings: &CameraSettings) -> Vec2 {
        let follow = || {
            self.sim.camera_entity().map(|id| world.get_global_position(id)).unwrap_or(Vec2::ZERO)
        };
        match settings.target {
            CameraTarget::Default => follow(),
            CameraTarget::Entity(id) if world.transforms.contains_key(&id) => {
                world.get_global_position(id)
            }
            CameraTarget::Entity(_) => follow(),
            CameraTarget::Point(p) => p,
        }
    }

    /// Moves `self.camera` toward this frame's target and sets its viewport
    /// and zoom — what `update` calls before stepping the simulation.
    pub(super) fn update_camera(
        &mut self,
        world: &World,
        frame_dt: f32,
        viewport_width: usize,
        viewport_height: usize,
    ) {
        let settings = self.sim.camera_settings();
        let level = self.sim.level();
        let bounds = settings
            .bounds
            .unwrap_or_else(|| Rect::new(0.0, 0.0, level.width as f32, level.height as f32));
        // Step 4g: the world gets the full viewport — no HUD bars reserve rows.
        let game_h = (viewport_height as i32).max(1) as f32;
        let zoom = settings.zoom;
        let half_w = viewport_width as f32 / 2.0 / zoom;
        let half_h = game_h / 2.0 / zoom;
        let target = clamp_center(self.camera_target(world, &settings), half_w, half_h, bounds);

        if self.camera.position == Vec2::ZERO {
            self.camera.position = target;
        } else {
            let t = follow_fraction(settings.speed, frame_dt);
            self.camera.position = self.camera.position + (target - self.camera.position) * t;
        }
        self.camera.viewport_width = viewport_width as f32;
        self.camera.viewport_height = game_h;
        self.camera.viewport_origin = Vec2::ZERO;
        self.camera.zoom = zoom;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_inside_the_bounds_is_left_alone() {
        let b = Rect::new(0.0, 0.0, 100.0, 50.0);
        assert_eq!(clamp_center(Vec2::new(50.0, 25.0), 10.0, 5.0, b), Vec2::new(50.0, 25.0));
    }

    #[test]
    fn the_view_never_crosses_the_bounds_edges() {
        let b = Rect::new(10.0, 10.0, 40.0, 30.0);
        assert_eq!(clamp_center(Vec2::new(0.0, 0.0), 5.0, 5.0, b), Vec2::new(15.0, 15.0));
        assert_eq!(clamp_center(Vec2::new(99.0, 99.0), 5.0, 5.0, b), Vec2::new(45.0, 35.0));
    }

    #[test]
    fn bounds_smaller_than_the_view_anchor_its_top_left_edge() {
        // A 20x10 level in an 80x24 view: the old play-mode behaviour.
        let b = Rect::new(0.0, 0.0, 20.0, 10.0);
        assert_eq!(clamp_center(Vec2::new(10.0, 5.0), 40.0, 12.0, b), Vec2::new(40.0, 12.0));
    }

    #[test]
    fn zooming_in_lets_the_camera_reach_closer_to_the_edges() {
        let b = Rect::new(0.0, 0.0, 100.0, 100.0);
        // zoom 1: an 80-cell view's centre can't go below x = 40.
        assert_eq!(clamp_center(Vec2::new(0.0, 50.0), 40.0, 12.0, b).x, 40.0);
        // zoom 2: half the world span, so x = 20.
        assert_eq!(clamp_center(Vec2::new(0.0, 50.0), 20.0, 6.0, b).x, 20.0);
    }

    #[test]
    fn speed_zero_jumps_and_any_speed_moves_part_way() {
        assert_eq!(follow_fraction(0.0, 1.0 / 60.0), 1.0);
        let t = follow_fraction(5.0, 1.0 / 60.0);
        assert!(t > 0.0 && t < 1.0);
    }
}
