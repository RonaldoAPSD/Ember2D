// mouse.rs — Mouse input tracking, backend-agnostic.

use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use crate::input::INPUT_BUFFER_WINDOW;

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

    /// Buttons pressed but not yet consumed by a simulation step, each with
    /// its remaining buffer lifetime (seconds). See `input::INPUT_BUFFER_WINDOW`
    /// for why button presses are buffered rather than frame-scoped (D1).
    pending: HashMap<MouseButton, f32>,

    /// The set of buttons the current simulation step sees as just-pressed.
    consumed: HashSet<MouseButton>,

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
            pending:       HashMap::new(),
            consumed:      HashSet::new(),
            just_released: Vec::new(),
        }
    }

    /// Clear transient per-frame state (just_released, scroll). Deliberately
    /// does NOT touch `pending` — see `InputManager::clear`.
    pub fn clear(&mut self) {
        self.just_released.clear();
        self.wheel_x = 0.0;
        self.wheel_y = 0.0;
    }

    /// Pull buffered presses into this simulation step's just-pressed set.
    /// Call once per simulation step, before running game/script update code.
    pub fn consume_step(&mut self) {
        self.consumed = self.pending.keys().copied().collect();
        self.pending.clear();
    }

    /// Age out buffered presses no simulation step claimed in time.
    /// Call once per frame (real delta time) after the frame's simulation
    /// steps have had their chance to consume them.
    pub fn decay(&mut self, dt: f32) {
        self.pending.retain(|_, remaining| { *remaining -= dt; *remaining > 0.0 });
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
            self.pending.insert(button, INPUT_BUFFER_WINDOW);
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
    pub fn just_pressed(&self, button: MouseButton) -> bool { self.consumed.contains(&button) }
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

    /// The sim-safe half of this state — see `MouseSnapshot`'s own doc
    /// comment (command.rs, Step 5i) for why scripting reads this instead
    /// of `&MouseState` directly. No middle-button field: nothing in the
    /// scripting API exposes one today (`mouse_left_held`/`mouse_right_held`
    /// only), so there's nothing to carry.
    pub fn snapshot(&self) -> ember2d_sim::command::MouseSnapshot {
        ember2d_sim::command::MouseSnapshot {
            cell: (self.cell_x as f32, self.cell_y as f32),
            held: (self.left_held(), self.right_held()),
            pressed: (self.left_just_pressed(), self.right_just_pressed()),
        }
    }
}
