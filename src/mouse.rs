// mouse.rs — Mouse input tracking.
//
// Works exactly like InputManager (src/input.rs) but for the mouse instead of
// the keyboard. The key design is the same: we compare the current frame's
// state against the previous frame's state to compute "just pressed" and
// "just released" events.
//
// ── COORDINATE CONVERSION ────────────────────────────────────────────────────
//
// minifb's get_mouse_pos() already accounts for the Scale::X2 factor internally.
// It returns coordinates in BUFFER pixel space (0..pixel_width, 0..pixel_height),
// NOT in the larger on-screen window space.
//
// So the only conversion needed is:
//
//   cell_x = buffer_pixel_x / CELL_W    (8 px per cell column)
//   cell_y = buffer_pixel_y / CELL_H    (16 px per cell row)
//
// No division by a scale factor is needed here.

/// Width of one character cell in pixels — must match renderer's CELL_W.
const CELL_W: f32 = 8.0;

/// Height of one character cell in pixels — must match renderer's CELL_H.
const CELL_H: f32 = 16.0;

/// Tracks the mouse position and button state across frames.
///
/// Owned by the Engine. Updated once per frame (after the previous frame's
/// `present()` call, which is when minifb refreshes its internal event state).
/// A reference is passed to game code via `UpdateContext.mouse` and `RenderContext.mouse`.
pub struct MouseState {
    /// Character cell column under the cursor.
    /// Only valid when `in_bounds` is true.
    pub cell_x: usize,

    /// Character cell row under the cursor.
    /// Only valid when `in_bounds` is true.
    pub cell_y: usize,

    /// Internal (pre-scale) pixel X position.
    /// Useful if you need sub-cell precision.
    pub pixel_x: f32,

    /// Internal (pre-scale) pixel Y position.
    pub pixel_y: f32,

    /// True if the cursor is inside the window.
    /// When false, `cell_x`/`cell_y` retain their last known values.
    pub in_bounds: bool,

    /// Horizontal scroll wheel delta this frame (positive = right).
    pub wheel_x: f32,
    /// Vertical scroll wheel delta this frame (positive = down/away from user).
    pub wheel_y: f32,

    // ── Private state: current and previous frame button values ──────────────

    left_down:   bool,
    right_down:  bool,
    middle_down: bool,
    prev_left:   bool,
    prev_right:  bool,
    prev_middle: bool,
}

impl MouseState {
    /// Create a zeroed MouseState (cursor at 0,0, no buttons pressed).
    pub fn new() -> Self {
        MouseState {
            cell_x:     0,
            cell_y:     0,
            pixel_x:    0.0,
            pixel_y:    0.0,
            in_bounds:   false,
            wheel_x:     0.0,
            wheel_y:     0.0,
            left_down:   false,
            right_down:  false,
            middle_down: false,
            prev_left:   false,
            prev_right:  false,
            prev_middle: false,
        }
    }

    /// Update state for this frame.
    ///
    /// Called by the Engine once per frame, passing data read from the Renderer's window.
    ///
    /// `pos`   — raw window-space mouse position from `renderer.current_mouse_pos()`.
    ///           None when the cursor is outside the window.
    /// `left`  — whether the left button is currently down.
    /// `right` — whether the right button is currently down.
    pub fn poll(&mut self, pos: Option<(f32, f32)>, left: bool, right: bool, middle: bool, scroll: (f32, f32)) {
        // Carry current state into "previous" before updating.
        self.prev_left   = self.left_down;
        self.prev_right  = self.right_down;
        self.prev_middle = self.middle_down;

        match pos {
            Some((wx, wy)) => {
                self.in_bounds = true;

                // minifb already returns buffer-space pixel coordinates.
                // Just divide by cell size to get character cell indices.
                self.pixel_x = wx;
                self.pixel_y = wy;
                self.cell_x  = (self.pixel_x / CELL_W) as usize;
                self.cell_y  = (self.pixel_y / CELL_H) as usize;
            }
            None => {
                self.in_bounds = false;
                // Keep last known cell_x / cell_y so code that ignores in_bounds
                // doesn't immediately jump to (0, 0).
            }
        }

        self.left_down   = left;
        self.right_down  = right;
        self.middle_down = middle;
        self.wheel_x     = scroll.0;
        self.wheel_y     = scroll.1;
    }

    // ── Button query methods ─────────────────────────────────────────────────

    /// True while the left mouse button is held down (current frame).
    pub fn left_held(&self) -> bool {
        self.left_down
    }

    /// True while the right mouse button is held down (current frame).
    pub fn right_held(&self) -> bool {
        self.right_down
    }

    /// True ONLY on the frame the left button first went down.
    /// Use for single-click actions: select a palette tile, begin a placement.
    pub fn left_just_pressed(&self) -> bool {
        self.left_down && !self.prev_left
    }

    /// True ONLY on the frame the right button first went down.
    pub fn right_just_pressed(&self) -> bool {
        self.right_down && !self.prev_right
    }

    /// True ONLY on the frame the left button was released.
    /// Use to detect "end of drag" — e.g. commit a paint stroke to the undo stack.
    pub fn left_just_released(&self) -> bool {
        !self.left_down && self.prev_left
    }

    /// True ONLY on the frame the right button was released.
    pub fn right_just_released(&self) -> bool {
        !self.right_down && self.prev_right
    }

    /// True while the middle mouse button is held down.
    pub fn middle_held(&self) -> bool { self.middle_down }

    /// True ONLY on the frame the middle button first went down.
    pub fn middle_just_pressed(&self) -> bool { self.middle_down && !self.prev_middle }

    /// True ONLY on the frame the middle button was released.
    pub fn middle_just_released(&self) -> bool { !self.middle_down && self.prev_middle }
}
