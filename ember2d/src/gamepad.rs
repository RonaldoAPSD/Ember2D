// gamepad.rs — Gamepad input support via gilrs.

use serde::{Serialize, Deserialize};
use std::collections::{HashMap, HashSet};
use gilrs::{Gilrs, Button, Axis, Event, EventType};
use crate::input::INPUT_BUFFER_WINDOW;

/// A backend-agnostic representation of a gamepad button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GamepadButton {
    South, East, North, West,
    LeftTrigger, RightTrigger, LeftTrigger2, RightTrigger2,
    Select, Start, Mode,
    LeftThumb, RightThumb,
    DPadUp, DPadDown, DPadLeft, DPadRight,
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
    LeftStickX, LeftStickY,
    RightStickX, RightStickY,
    LeftTrigger, RightTrigger,
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
    /// Buttons currently held down. (gamepad_id, button)
    pub(crate) held: HashSet<(usize, GamepadButton)>,
    /// Buttons pressed but not yet consumed by a simulation step, each with
    /// its remaining buffer lifetime (seconds). See `input::INPUT_BUFFER_WINDOW`
    /// for why button presses are buffered rather than frame-scoped (D1).
    pub(crate) pending: HashMap<(usize, GamepadButton), f32>,
    /// The set of buttons the current simulation step sees as just-pressed.
    pub(crate) consumed: HashSet<(usize, GamepadButton)>,
    /// Buttons released this frame.
    pub(crate) just_released: HashSet<(usize, GamepadButton)>,
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

        GamepadState {
            held: HashSet::new(),
            pending: HashMap::new(),
            consumed: HashSet::new(),
            just_released: HashSet::new(),
            axes: HashMap::new(),
            gilrs,
        }
    }

    /// Clear transient per-frame state (just_released). Deliberately does
    /// NOT touch `pending` — see `InputManager::clear`.
    pub fn clear(&mut self) {
        self.just_released.clear();
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

    pub fn poll(&mut self) {
        let Some(ref mut gilrs) = self.gilrs else { return };

        while let Some(Event { id, event, .. }) = gilrs.next_event() {
            let gamepad_id: usize = id.into();
            match event {
                EventType::ButtonPressed(button, ..) => {
                    let btn = GamepadButton::from_gilrs(button);
                    if btn != GamepadButton::Unknown && self.held.insert((gamepad_id, btn)) {
                        self.pending.insert((gamepad_id, btn), INPUT_BUFFER_WINDOW);
                    }
                }
                EventType::ButtonReleased(button, ..) => {
                    let btn = GamepadButton::from_gilrs(button);
                    if btn != GamepadButton::Unknown {
                        self.held.remove(&(gamepad_id, btn));
                        self.just_released.insert((gamepad_id, btn));
                    }
                }
                EventType::AxisChanged(axis, value, ..) => {
                    let ax = GamepadAxis::from_gilrs(axis);
                    if ax != GamepadAxis::Unknown {
                        self.axes.insert((gamepad_id, ax), value);
                    }
                }
                _ => {}
            }
        }
    }

    pub fn is_held(&self, gamepad_id: usize, button: GamepadButton) -> bool {
        self.held.contains(&(gamepad_id, button))
    }

    pub fn just_pressed(&self, gamepad_id: usize, button: GamepadButton) -> bool {
        self.consumed.contains(&(gamepad_id, button))
    }

    pub fn just_released(&self, gamepad_id: usize, button: GamepadButton) -> bool {
        self.just_released.contains(&(gamepad_id, button))
    }

    pub fn get_axis(&self, gamepad_id: usize, axis: GamepadAxis) -> f32 {
        let v = self.axes.get(&(gamepad_id, axis)).copied().unwrap_or(0.0);
        // Apply deadzone to prevent input drift at rest
        if v.abs() < 0.1 { 0.0 } else { v }
    }

    /// Returns the ID of the first connected gamepad, or None.
    pub fn first_gamepad(&self) -> Option<usize> {
        self.gilrs.as_ref()?.gamepads().next().map(|(id, _)| id.into())
    }

    /// The sim-safe half of this state — see `GamepadSnapshot`'s own doc
    /// comment (command.rs, Step 5i) for why scripting reads this instead
    /// of `&GamepadState` directly (which owns a live `gilrs::Gilrs` handle).
    pub fn snapshot(&self) -> ember2d_sim::command::GamepadSnapshot {
        let mut held = HashSet::new();
        let mut pressed = HashSet::new();
        let mut axes = HashMap::new();
        for &(id, btn) in &self.held { held.insert((id, btn.to_string())); }
        for &(id, btn) in &self.consumed { pressed.insert((id, btn.to_string())); }
        for (&(id, ax), &val) in &self.axes { axes.insert((id, ax.to_string()), val); }
        ember2d_sim::command::GamepadSnapshot { held, pressed, axes }
    }
}
