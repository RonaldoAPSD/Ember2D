// gamepad.rs — Gamepad input support via gilrs.

use crate::input::INPUT_BUFFER_WINDOW;
use crate::press_buffer::PressBuffer;
use gilrs::{Axis, Button, Event, EventType, Gilrs};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// A backend-agnostic representation of a gamepad button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadButton {
    South,
    East,
    North,
    West,
    LeftTrigger,
    RightTrigger,
    LeftTrigger2,
    RightTrigger2,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Unknown,
}

impl GamepadButton {
    pub fn from_gilrs(button: Button) -> Self {
        match button {
            Button::South => GamepadButton::South,
            Button::East => GamepadButton::East,
            Button::North => GamepadButton::North,
            Button::West => GamepadButton::West,
            Button::LeftTrigger => GamepadButton::LeftTrigger,
            Button::RightTrigger => GamepadButton::RightTrigger,
            Button::LeftTrigger2 => GamepadButton::LeftTrigger2,
            Button::RightTrigger2 => GamepadButton::RightTrigger2,
            Button::Select => GamepadButton::Select,
            Button::Start => GamepadButton::Start,
            Button::Mode => GamepadButton::Mode,
            Button::LeftThumb => GamepadButton::LeftThumb,
            Button::RightThumb => GamepadButton::RightThumb,
            Button::DPadUp => GamepadButton::DPadUp,
            Button::DPadDown => GamepadButton::DPadDown,
            Button::DPadLeft => GamepadButton::DPadLeft,
            Button::DPadRight => GamepadButton::DPadRight,
            _ => GamepadButton::Unknown,
        }
    }

    pub fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

/// A backend-agnostic representation of a gamepad axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
    Unknown,
}

impl GamepadAxis {
    pub fn from_gilrs(axis: Axis) -> Self {
        match axis {
            Axis::LeftStickX => GamepadAxis::LeftStickX,
            Axis::LeftStickY => GamepadAxis::LeftStickY,
            Axis::RightStickX => GamepadAxis::RightStickX,
            Axis::RightStickY => GamepadAxis::RightStickY,
            Axis::LeftZ => GamepadAxis::LeftTrigger,
            Axis::RightZ => GamepadAxis::RightTrigger,
            _ => GamepadAxis::Unknown,
        }
    }

    pub fn to_string(&self) -> String {
        format!("{:?}", self)
    }
}

pub struct GamepadState {
    /// Held/pending/consumed/just-released bookkeeping, keyed by
    /// `(gamepad_id, button)` — see
    /// `crate::press_buffer::PressBuffer`'s module doc (7B-4) for why this
    /// is shared machinery rather than fields of its own.
    buffer: PressBuffer<(usize, GamepadButton)>,
    /// Current axis values.
    pub(crate) axes: HashMap<(usize, GamepadAxis), f32>,

    gilrs: Option<Gilrs>,
}

impl GamepadState {
    pub fn new() -> Self {
        // Safe initialization: if gilrs fails (e.g. no display server),
        // we just run without gamepad support instead of panicking.
        let gilrs = Gilrs::new().ok();
        if gilrs.is_none() {
            eprintln!("WARN: Gamepad support initialization failed (Gilrs error). Running without controllers.");
        }

        GamepadState { buffer: PressBuffer::new(), axes: HashMap::new(), gilrs }
    }

    /// Clear transient per-frame state (just_released). Deliberately does
    /// NOT touch the buffered-press map — see `InputManager::clear`.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Pull buffered presses into this simulation step's just-pressed set.
    /// Call once per simulation step, before running game/script update code.
    pub fn consume_step(&mut self) {
        self.buffer.consume_step();
    }

    /// Age out buffered presses no simulation step claimed in time.
    /// Call once per frame (real delta time) after the frame's simulation
    /// steps have had their chance to consume them.
    pub fn decay(&mut self, dt: f32) {
        self.buffer.decay(dt);
    }

    pub fn poll(&mut self) {
        let Some(ref mut gilrs) = self.gilrs else { return };

        while let Some(Event { id, event, .. }) = gilrs.next_event() {
            let gamepad_id: usize = id.into();
            match event {
                EventType::ButtonPressed(button, ..) => {
                    let btn = GamepadButton::from_gilrs(button);
                    if btn != GamepadButton::Unknown {
                        self.buffer.handle_pressed((gamepad_id, btn), INPUT_BUFFER_WINDOW);
                    }
                }
                EventType::ButtonReleased(button, ..) => {
                    let btn = GamepadButton::from_gilrs(button);
                    if btn != GamepadButton::Unknown {
                        self.buffer.handle_released((gamepad_id, btn));
                    }
                }
                EventType::AxisChanged(axis, value, ..) => {
                    let ax = GamepadAxis::from_gilrs(axis);
                    if ax != GamepadAxis::Unknown {
                        self.axes.insert((gamepad_id, ax), value);
                    }
                }
                // R25 (7B-4, docs/ember2d-master-plan.md §5.2, §3): a
                // gamepad unplugged mid-hold used to leave its buttons
                // stuck `held` forever — gilrs never sends a
                // `ButtonReleased` for a device that's gone, and nothing
                // else here ever cleared entries for a disconnected
                // `gamepad_id`, so a script gating on e.g. `is_held(0,
                // South)` after an unplug during a held press would see it
                // as still down indefinitely. Fix: drop every held/pending
                // button and axis entry for this device on disconnect.
                EventType::Disconnected => {
                    self.buffer.retain(|&(id, _)| id != gamepad_id);
                    self.axes.retain(|&(id, _), _| id != gamepad_id);
                }
                _ => {}
            }
        }
    }

    pub fn is_held(&self, gamepad_id: usize, button: GamepadButton) -> bool {
        self.buffer.is_held((gamepad_id, button))
    }

    pub fn just_pressed(&self, gamepad_id: usize, button: GamepadButton) -> bool {
        self.buffer.just_pressed((gamepad_id, button))
    }

    pub fn just_released(&self, gamepad_id: usize, button: GamepadButton) -> bool {
        self.buffer.just_released((gamepad_id, button))
    }

    pub fn get_axis(&self, gamepad_id: usize, axis: GamepadAxis) -> f32 {
        let v = self.axes.get(&(gamepad_id, axis)).copied().unwrap_or(0.0);
        // Apply deadzone to prevent input drift at rest
        if v.abs() < 0.1 {
            0.0
        } else {
            v
        }
    }

    /// Returns the ID of the first connected gamepad, or None.
    /// The sim-safe half of this state — see `GamepadSnapshot`'s own doc
    /// comment (command.rs, Step 5i) for why scripting reads this instead
    /// of `&GamepadState` directly (which owns a live `gilrs::Gilrs` handle).
    pub fn snapshot(&self) -> ember2d_sim::command::GamepadSnapshot {
        let mut held = HashSet::new();
        let mut pressed = HashSet::new();
        let mut axes = HashMap::new();
        for &(id, btn) in self.buffer.iter_held() {
            held.insert((id, btn.to_string()));
        }
        for &(id, btn) in self.buffer.iter_consumed() {
            pressed.insert((id, btn.to_string()));
        }
        for (&(id, ax), &val) in &self.axes {
            axes.insert((id, ax.to_string()), val);
        }
        ember2d_sim::command::GamepadSnapshot { held, pressed, axes }
    }
}
