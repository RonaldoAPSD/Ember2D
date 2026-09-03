// mouse.rs — Mouse input tracking, backend-agnostic.

use serde::{Serialize, Deserialize};

/// A backend-agnostic representation of a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

impl MouseButton {
    pub fn from_winit(button: winit::event::MouseButton) -> Self {
        match button {
            winit::event::MouseButton::Left => MouseButton::Left,
            winit::event::MouseButton::Right => MouseButton::Right,
            winit::event::MouseButton::Middle => MouseButton::Middle,
            winit::event::MouseButton::Other(id) => MouseButton::Other(id),
            _ => MouseButton::Other(999),
        }
    }
}

/// Width of one character cell in pixels — must match renderer's CELL_W.
const CELL_W: f32 = 8.0;

/// Height of one character cell in pixels — must match renderer's CELL_H.
const CELL_H: f32 = 16.0;

/// Tracks the mouse position and button state across frames.
pub struct MouseState {
    /// Character cell column under the cursor.
    pub cell_x: usize,

    /// Character cell row under the cursor.
    pub cell_y: usize,

    /// Internal pixel X position.
    pub pixel_x: f32,

    /// Internal pixel Y position.
    pub pixel_y: f32,

    /// True if the cursor is inside the window.
    pub in_bounds: bool,

    /// Horizontal scroll wheel delta this frame.
    pub wheel_x: f32,
    /// Vertical scroll wheel delta this frame.
    pub wheel_y: f32,

    /// Buttons currently held down.
    held: Vec<MouseButton>,

    /// Buttons pressed this frame.
    just_pressed: Vec<MouseButton>,

    /// Buttons released this frame.
    just_released: Vec<MouseButton>,
}

impl MouseState {
    pub fn new() -> Self {
        MouseState {
            cell_x:        0,
            cell_y:        0,
            pixel_x:       0.0,
            pixel_y:       0.0,
            in_bounds:      false,
            wheel_x:        0.0,
            wheel_y:        0.0,
            held:          Vec::new(),
            just_pressed:  Vec::new(),
            just_released: Vec::new(),
        }
    }

    /// Clear transient state (just_pressed, just_released, scroll).
    pub fn clear(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
        self.wheel_x = 0.0;
        self.wheel_y = 0.0;
    }

    /// Update mouse position.
    pub fn handle_move(&mut self, px: f32, py: f32) {
        self.pixel_x = px;
        self.pixel_y = py;
        self.cell_x  = (px / CELL_W) as usize;
        self.cell_y  = (py / CELL_H) as usize;
        self.in_bounds = true;
    }

    pub fn handle_pressed(&mut self, button: MouseButton) {
        if !self.held.contains(&button) {
            self.held.push(button);
            self.just_pressed.push(button);
        }
    }

    pub fn handle_released(&mut self, button: MouseButton) {
        if let Some(pos) = self.held.iter().position(|&b| b == button) {
            self.held.remove(pos);
            self.just_released.push(button);
        }
    }

    pub fn handle_scroll(&mut self, dx: f32, dy: f32) {
        self.wheel_x += dx;
        self.wheel_y += dy;
    }

    pub fn set_in_bounds(&mut self, in_bounds: bool) {
        self.in_bounds = in_bounds;
    }

    // ── Button query methods ─────────────────────────────────────────────────

    pub fn is_held(&self, button: MouseButton) -> bool { self.held.contains(&button) }
    pub fn just_pressed(&self, button: MouseButton) -> bool { self.just_pressed.contains(&button) }
    pub fn just_released(&self, button: MouseButton) -> bool { self.just_released.contains(&button) }

    // Shorthands for common buttons to avoid breaking too much code
    pub fn left_held(&self) -> bool { self.is_held(MouseButton::Left) }
    pub fn right_held(&self) -> bool { self.is_held(MouseButton::Right) }
    pub fn middle_held(&self) -> bool { self.is_held(MouseButton::Middle) }

    pub fn left_just_pressed(&self) -> bool { self.just_pressed(MouseButton::Left) }
    pub fn right_just_pressed(&self) -> bool { self.just_pressed(MouseButton::Right) }
    pub fn middle_just_pressed(&self) -> bool { self.just_pressed(MouseButton::Middle) }

    pub fn left_just_released(&self) -> bool { self.just_released(MouseButton::Left) }
    pub fn right_just_released(&self) -> bool { self.just_released(MouseButton::Right) }
    pub fn middle_just_released(&self) -> bool { self.just_released(MouseButton::Middle) }
}
