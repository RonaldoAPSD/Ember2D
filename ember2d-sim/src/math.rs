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

use serde::{Serialize, Deserialize};

// ─────────────────────────── Vec2 ────────────────────────────────────────────

/// A 2-dimensional vector with 32-bit floating-point components.
///
/// Used for world-space positions, velocities, directions, and forces.
/// The `Copy` derive means this type is cheap to copy — passing it to a
/// function does not move it out of the caller (unlike heap-allocated types).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
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

    /// Slab method for Ray-AABB intersection.
    /// Returns the distance `t` to the first intersection point along the ray
    /// starting at (ox, oy) with direction (dx, dy).
    /// Returns None if no intersection occurs.
    pub fn ray_intersects(&self, ox: f32, oy: f32, dx: f32, dy: f32) -> Option<f32> {
        let mut tmin = 0.0f32;
        let mut tmax = f32::INFINITY;

        // X slab
        if dx.abs() < f32::EPSILON {
            if ox < self.x || ox >= self.right() { return None; }
        } else {
            let inv_d = 1.0 / dx;
            let mut t1 = (self.x - ox) * inv_d;
            let mut t2 = (self.right() - ox) * inv_d;
            if t1 > t2 { std::mem::swap(&mut t1, &mut t2); }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax { return None; }
        }

        // Y slab
        if dy.abs() < f32::EPSILON {
            if oy < self.y || oy >= self.bottom() { return None; }
        } else {
            let inv_d = 1.0 / dy;
            let mut t1 = (self.y - oy) * inv_d;
            let mut t2 = (self.bottom() - oy) * inv_d;
            if t1 > t2 { std::mem::swap(&mut t1, &mut t2); }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax { return None; }
        }

        if tmax < 0.0 { return None; }
        Some(tmin)
    }
}

// ─────────────────────────── atan2_approx ────────────────────────────────────

/// A deterministic, cross-platform replacement for `f64::atan2` — Phase 6
/// Step 12 (docs/ember2d-phase6-plan.md, §5.2 H2 in the refactor plan).
///
/// WHY THIS EXISTS: `+ - * /` and `sqrt` are IEEE-754-exact, so they produce
/// identical bits on every platform this engine runs on — `atan2`, `atan`,
/// `sin`, `cos`, and every other transcendental function are NOT: they come
/// from the platform's own libm, and different libm implementations (or
/// even different versions of the same one) are free to round the last bit
/// differently. Two machines given the same inputs and running the real
/// `f64::atan2` are not guaranteed to compute the same `f64` — a real
/// cross-platform desync hazard for lockstep netcode (Phase 9) or a replay
/// recorded on one machine and checked on another. `ctx.get_angle_to`
/// (`scripting/api_spatial.rs`) is the one place in this crate that ever
/// called `atan2` — exhaustive grep at the time this was written found no
/// other transcendental-math call in `ember2d-sim`, and no shipped script
/// calls it either, so this single function closes out §5.2 H2 in full.
///
/// HOW: a minimax rational approximation of `atan` — a well-known one (Jim
/// Shima, "A Fast, Accurate Approximation to atan()", 1999), built entirely
/// from `+ - * /` on `z = y / x` (and its reciprocal-shaped twin for
/// `|z| >= 1`, to keep the polynomial's argument in the range it was fitted
/// for) — composed with the same sign/quadrant case analysis the real
/// `atan2` uses to turn a single `atan` into a full-circle angle. Since
/// every operation here is `+ - * /`, the result is bit-identical on any
/// IEEE-754-compliant platform, the same property `sqrt` already has.
///
/// ACCURACY: maximum absolute error is documented (and independently
/// verified by this module's own test, checked against the real
/// `f64::atan2` at many angles and radii) at approximately 0.01 radians
/// (~0.6°) — invisible for `get_angle_to`'s actual use (AI chase/aim
/// direction), and irrelevant to gameplay correctness the way an exact
/// value would only matter for, say, precision physics this engine doesn't
/// have.
///
/// NOT `mul_add` ANYWHERE below: `f64::mul_add` (fused multiply-add) uses a
/// real hardware FMA instruction where one exists, computing `a*b+c` with a
/// single rounding step instead of two separate ones — a different (and,
/// once again, platform/hardware-dependent) result from writing the
/// multiply and the add out as separate operations the way this function
/// does throughout. Rust does not contract `a*b+c` into an FMA on its own
/// (unlike a C compiler under `-ffast-math`), so simply never calling
/// `.mul_add()` is sufficient here.
///
/// See `docs/ember2d-scripting-api.md`'s "Spatial queries" note for the
/// hazard this fix does NOT close: a script that takes this deterministic
/// angle and calls Rhai's own `.cos()`/`.sin()` on it reintroduces the exact
/// same platform-libm nondeterminism one step later, in script space instead
/// of engine space.
pub fn atan2_approx(y: f64, x: f64) -> f64 {
    use std::f64::consts::{FRAC_PI_2, PI};

    // Checked first, before dividing — 0.0/0.0 would otherwise produce NaN,
    // and this crate's rule against calling into libm doesn't mean "produce
    // garbage instead," it means "compute the same real answer a different
    // way." Matches the real atan2's own documented convention: atan2(0, 0)
    // is conventionally 0, not an error.
    if x == 0.0 {
        if y > 0.0 { return FRAC_PI_2; }
        if y < 0.0 { return -FRAC_PI_2; }
        return 0.0;
    }

    let z = y / x;
    if z.abs() < 1.0 {
        // atan(z) for |z| <= 1, where the approximation below was fitted.
        let atan = z / (1.0 + 0.28 * z * z);
        if x < 0.0 {
            if y < 0.0 { return atan - PI; }
            return atan + PI;
        }
        atan
    } else {
        // |z| >= 1: the reciprocal identity atan(z) = sign(z)*(pi/2) -
        // atan(1/z), algebraically simplified so the division stays on `z`
        // (never `1/z` computed separately) — substituting w = 1/z into the
        // same rational form above and simplifying gives exactly
        // `z / (z*z + 0.28)` for the `atan(1/z)` term. The `sign(z)*(pi/2)`
        // half collapses to a bare `pi/2` here because the y<0 branch below
        // already applies the same correction the full quadrant case
        // analysis needs — verified against the real atan2 at all four
        // quadrants by this module's own test, not just algebra.
        let atan = FRAC_PI_2 - z / (z * z + 0.28);
        if y < 0.0 { atan - PI } else { atan }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atan2_approx_matches_known_exact_values_on_the_axes() {
        assert_eq!(atan2_approx(0.0, 1.0), 0.0);
        assert_eq!(atan2_approx(0.0, 0.0), 0.0);
        assert_eq!(atan2_approx(1.0, 0.0), std::f64::consts::FRAC_PI_2);
        assert_eq!(atan2_approx(-1.0, 0.0), -std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn atan2_approx_stays_within_the_documented_error_bound_of_the_real_atan2() {
        // Comparing against the real (libm) atan2 is fine in a TEST — unlike
        // in sim code, nothing here has to agree bit-for-bit across
        // machines, only fall within the ~0.01 rad error this specific
        // approximation is documented to have. 0.015 leaves a little
        // headroom above that bound rather than testing exactly against it.
        const TOLERANCE: f64 = 0.015;
        for deg in (0..360).step_by(3) {
            let theta = (deg as f64).to_radians();
            for &r in &[0.1_f64, 1.0, 5.0, 100.0] {
                let x = r * theta.cos();
                let y = r * theta.sin();
                if x.abs() < 1e-9 && y.abs() < 1e-9 { continue; }

                let expected = y.atan2(x);
                let got = atan2_approx(y, x);

                // Angles wrap at ±π — the shortest angular distance, not a
                // raw subtraction, which would falsely fail right at the
                // wrap boundary despite the two angles being nearly identical.
                let raw_diff = (got - expected).abs();
                let diff = if raw_diff > std::f64::consts::PI { 2.0 * std::f64::consts::PI - raw_diff } else { raw_diff };

                assert!(
                    diff < TOLERANCE,
                    "theta={:.4} r={}: expected {:.6}, got {:.6}, diff {:.6}",
                    theta, r, expected, got, diff
                );
            }
        }
    }
}
