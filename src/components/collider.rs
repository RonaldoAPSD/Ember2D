// components/collider.rs — Collision detection component.
//
// A Collider defines the physical "hitbox" of an entity — the region of space
// it occupies for the purpose of collision detection.
//
// WE USE AABB (Axis-Aligned Bounding Box) COLLISION:
//   An AABB is a rectangle whose sides are always parallel to the axes.
//   It cannot rotate. In an ASCII grid this is a perfect fit because
//   characters are always grid-aligned — there's no concept of a
//   "rotated character."
//
//   AABB overlap test: two rects overlap if and only if they overlap
//   on BOTH the X axis AND the Y axis simultaneously. See Rect::intersects.
//
// SOLID vs TRIGGER colliders:
//   SOLID   — physical object; the collision system should prevent overlap.
//             Examples: walls, floor edges, doors.
//   TRIGGER — ghost zone; detects overlap but doesn't block movement.
//             Examples: pickup items, damage zones, room transitions.
//
//   The Collider itself doesn't enforce this distinction — the game's
//   late_update() responds to collision events and decides what to do
//   based on whether the other entity is solid.
//
// POSITION:
//   The Collider doesn't store a position. It's always the size of the box,
//   and the position comes from the entity's Transform component.
//   Use `world_rect()` to compute the actual world-space bounding box.

use crate::math::Rect;

/// Defines the bounding box used for collision detection.
#[derive(Debug, Clone)]
pub struct Collider {
    /// Width of the hitbox in world units (characters wide).
    pub width: f32,

    /// Height of the hitbox in world units (character rows tall).
    pub height: f32,

    /// If true: this is a physical obstacle. The game should prevent other solid
    /// entities from overlapping it (via position rollback in late_update).
    ///
    /// If false: this is a "trigger zone." It detects overlap and fires a
    /// Collision event, but movement is not blocked.
    pub solid: bool,

    /// Optional layer name for fine-grained collision filtering in scripts.
    pub layer: String,

    /// Optional collision mask: a list of layer names this collider should
    /// interact with. If empty, it interacts with ALL layers (default).
    pub mask: Vec<String>,
}

impl Collider {
    /// Create a solid collider with the given dimensions.
    pub fn new(width: f32, height: f32) -> Self {
        Collider { width, height, solid: true, layer: String::new(), mask: Vec::new() }
    }

    /// A 1×1 solid collider — the standard size for a single-character entity.
    /// Most game objects (player, enemies, items) are 1×1.
    pub fn unit() -> Self {
        Collider::new(1.0, 1.0)
    }

    /// A non-solid trigger zone of the given size.
    ///
    /// Trigger colliders fire Collision events but don't block movement.
    /// Use them for: pickups, damage areas, door triggers, room boundaries.
    pub fn trigger(width: f32, height: f32) -> Self {
        Collider { width, height, solid: false, layer: String::new(), mask: Vec::new() }
    }

    /// Compute the world-space bounding Rect for this collider given the
    /// entity's current position (from its Transform).
    ///
    /// The position is the TOP-LEFT corner of the bounding box.
    /// For a 1×1 collider at position (5, 3), the rect is (5, 3, 1, 1).
    pub fn world_rect(&self, pos_x: f32, pos_y: f32) -> Rect {
        Rect::new(pos_x, pos_y, self.width, self.height)
    }
}
