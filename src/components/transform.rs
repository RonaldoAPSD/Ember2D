// components/transform.rs — Position and velocity component.
//
// Transform answers the question: "WHERE is this entity and HOW is it moving?"
//
// WHY SEPARATE position AND velocity?
//   If we only stored position and moved it directly ("add 1 each frame"),
//   the speed would be tied to the frame rate. On a 120 Hz monitor the entity
//   moves twice as fast as on a 60 Hz monitor.
//
//   With velocity, we store INTENT ("move at 8 cells per second") and let
//   physics apply it scaled by delta_time:
//
//     new_position = old_position + velocity * delta_time
//
//   At 120 Hz: 8 * (1/120) = 0.067 per frame
//   At  60 Hz: 8 * (1/60)  = 0.133 per frame
//   Both reach the same destination in the same real-world time. ✓
//
// COORDINATE SYSTEM REMINDER:
//   +X → right    +Y → down (screen space, matches terminal rows)

use crate::math::Vec2;

/// Stores an entity's world-space position and movement velocity.
///
/// Every entity that exists "somewhere in the game world" should have one.
/// Entities without a Transform are considered to have no location.
#[derive(Debug, Clone)]
pub struct Transform {
    /// Current world-space position.
    /// (0.0, 0.0) is the top-left of the viewport.
    pub position: Vec2,

    /// Speed and direction of movement, in world-units per second.
    ///
    /// A velocity of Vec2::new(8.0, 0.0) means "move 8 columns per second rightward."
    /// The physics step multiplies this by delta_time each frame to advance position.
    pub velocity: Vec2,
}

impl Transform {
    /// Create a Transform at position (x, y) with zero velocity.
    pub fn new(x: f32, y: f32) -> Self {
        Transform {
            position: Vec2::new(x, y),
            velocity: Vec2::ZERO,
        }
    }

    /// Create a Transform with both an initial position and velocity.
    ///
    /// Useful for projectiles, particles, or anything that starts moving immediately.
    pub fn with_velocity(x: f32, y: f32, vx: f32, vy: f32) -> Self {
        Transform {
            position: Vec2::new(x, y),
            velocity: Vec2::new(vx, vy),
        }
    }

    /// Apply velocity to position, scaled by delta_time.
    ///
    /// This is called "Euler integration" — the simplest form of physics.
    /// It's not perfectly accurate for large timesteps or curved paths, but
    /// for a frame-rate-capped ASCII game it's indistinguishable from perfect.
    ///
    /// The engine calls this automatically each frame. You don't normally
    /// call it from game code — just set `velocity` and the engine handles the rest.
    pub fn integrate(&mut self, delta_time: f32) {
        // position += velocity * delta_time
        self.position += self.velocity * delta_time;
    }
}
