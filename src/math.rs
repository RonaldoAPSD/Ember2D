// math.rs — Core mathematical primitives used throughout the engine.
//
// Almost every other module depends on this one, so it lives at the bottom
// of the dependency graph (nothing in math.rs imports other engine modules).
//
// ┌─────────────────────────────────────────────────────────────┐
// │  Coordinate system used by ember2d                          │
// │                                                             │
// │  (0,0) ──────────────── +X ──▶                             │
// │    │   ┌───────────────────┐                               │
// │   +Y   │  game viewport    │                               │
// │    │   │                   │                               │
// │    ▼   └───────────────────┘                               │
// │                                                             │
// │  X grows to the RIGHT, Y grows DOWNWARD.                   │
// │  This matches how terminals work: top-left is the origin.   │
// └─────────────────────────────────────────────────────────────┘
//
// WHY DO WE HAVE BOTH Vec2 (f32) AND IVec2 (i32)?
//   Physics and smooth movement need fractional values — a player moving
//   at 7.5 cells/sec for 0.016 seconds moves 0.12 cells this frame.
//   Screen cells are always integers — you can't draw at column 3.7.
//   We use Vec2 for world-space calculations and IVec2 for screen positions.

use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

// ─────────────────────────── Vec2 ────────────────────────────────────────────

/// A 2-dimensional vector with 32-bit floating-point components.
///
/// Used for world-space positions, velocities, directions, and forces.
/// The `Copy` derive means this type is cheap to copy — passing it to a
/// function does not move it out of the caller (unlike heap-allocated types).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    // Associated constants — zero-cost, live in read-only memory.

    /// The zero vector: no position, no direction, no movement.
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
    /// One unit to the right (positive X).
    pub const RIGHT: Vec2 = Vec2 { x: 1.0, y: 0.0 };
    /// One unit to the left (negative X).
    pub const LEFT: Vec2 = Vec2 { x: -1.0, y: 0.0 };
    /// One unit downward (positive Y in screen space).
    pub const DOWN: Vec2 = Vec2 { x: 0.0, y: 1.0 };
    /// One unit upward (negative Y in screen space).
    pub const UP: Vec2 = Vec2 { x: 0.0, y: -1.0 };

    /// Construct a Vec2 from x and y components.
    pub fn new(x: f32, y: f32) -> Self {
        Vec2 { x, y }
    }

    /// The Euclidean length (magnitude) of this vector.
    ///
    /// Uses the Pythagorean theorem: |v| = √(x² + y²)
    /// This is the "distance from the origin to the tip of the vector."
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Returns a vector pointing the same direction but with length exactly 1.0.
    ///
    /// Useful when you care about DIRECTION but not DISTANCE.
    /// Example: `velocity = direction.normalized() * speed`
    ///
    /// Returns Vec2::ZERO if the vector has zero length (can't normalize nothing).
    pub fn normalized(self) -> Vec2 {
        let len = self.length();
        if len == 0.0 {
            Vec2::ZERO
        } else {
            Vec2::new(self.x / len, self.y / len)
        }
    }

    /// The dot product of two vectors.
    ///
    /// Geometrically: `a · b = |a| * |b| * cos(θ)` where θ is the angle between them.
    /// For unit vectors specifically:
    ///   dot = 1.0  → same direction (parallel)
    ///   dot = 0.0  → perpendicular
    ///   dot = -1.0 → opposite directions
    ///
    /// Useful for: checking if two entities face each other, projecting one vector onto another.
    pub fn dot(self, other: Vec2) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Convert this floating-point vector to an integer grid position by rounding.
    ///
    /// Use this when you need to snap a physics position to a screen column/row.
    /// Example: Vec2::new(3.7, 5.2).to_ivec2() == IVec2::new(4, 5)
    pub fn to_ivec2(self) -> IVec2 {
        IVec2::new(self.x.round() as i32, self.y.round() as i32)
    }
}

// Operator overloading — lets us write `a + b` instead of `Vec2::add(a, b)`.
// Each `impl` block teaches Rust what `+`, `-`, `*`, etc. mean for Vec2.

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl AddAssign for Vec2 {
    // Enables `a += b`
    fn add_assign(&mut self, rhs: Vec2) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Vec2) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

/// Scalar multiplication: `Vec2 * f32` scales the vector.
/// Example: `Vec2::RIGHT * 5.0` == `Vec2::new(5.0, 0.0)`
impl Mul<f32> for Vec2 {
    type Output = Vec2;
    fn mul(self, scalar: f32) -> Vec2 {
        Vec2::new(self.x * scalar, self.y * scalar)
    }
}

/// Negation: `-Vec2` flips both components.
/// Example: `-Vec2::RIGHT` == `Vec2::LEFT`
impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}

// ─────────────────────────── IVec2 ───────────────────────────────────────────

/// A 2-dimensional vector with 32-bit integer components.
///
/// Used for screen/grid positions where fractional values are meaningless.
/// `Eq` and `Hash` are derived so IVec2 can be used as a HashMap key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IVec2 {
    pub x: i32,
    pub y: i32,
}

impl IVec2 {
    pub const ZERO: IVec2 = IVec2 { x: 0, y: 0 };

    pub fn new(x: i32, y: i32) -> Self {
        IVec2 { x, y }
    }

    /// Convert to a floating-point Vec2.
    /// Useful when you need to start physics calculations from a grid position.
    pub fn to_vec2(self) -> Vec2 {
        Vec2::new(self.x as f32, self.y as f32)
    }
}

impl Add for IVec2 {
    type Output = IVec2;
    fn add(self, rhs: IVec2) -> IVec2 {
        IVec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for IVec2 {
    type Output = IVec2;
    fn sub(self, rhs: IVec2) -> IVec2 {
        IVec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

// ─────────────────────────── Rect ────────────────────────────────────────────

/// An axis-aligned bounding rectangle.
///
/// "Axis-aligned" means the sides are always parallel to the X and Y axes —
/// the rect never rotates. This keeps collision math simple and fast.
///
/// Defined by its top-left corner (x, y) and its size (w, h).
/// The rect occupies columns x..x+w and rows y..y+h.
///
///   (x, y) ─── w ───▶
///     │  ┌───────────┐
///     h  │           │
///     │  └───────────┘
///     ▼
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    /// X coordinate of the left edge.
    pub x: f32,
    /// Y coordinate of the top edge.
    pub y: f32,
    /// Width: how many units this rect spans horizontally.
    pub w: f32,
    /// Height: how many units this rect spans vertically.
    pub h: f32,
}

impl Rect {
    /// Create a new Rect at position (x, y) with the given width and height.
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Rect { x, y, w, h }
    }

    /// X coordinate of the right edge.
    pub fn right(self) -> f32 {
        self.x + self.w
    }

    /// Y coordinate of the bottom edge.
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// The center point of this rect.
    pub fn center(self) -> Vec2 {
        Vec2::new(self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    /// AABB (Axis-Aligned Bounding Box) overlap test.
    ///
    /// Two rects overlap if and only if NONE of these separation conditions hold:
    ///   - self is entirely to the right of other  (self.x >= other.right())
    ///   - self is entirely to the left of other   (self.right() <= other.x)
    ///   - self is entirely below other             (self.y >= other.bottom())
    ///   - self is entirely above other             (self.bottom() <= other.y)
    ///
    /// We negate that: they overlap when ALL gaps are absent.
    pub fn intersects(self, other: Rect) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Returns true if the point (px, py) falls inside this rect.
    pub fn contains_point(self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}
