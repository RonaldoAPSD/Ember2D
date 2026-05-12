// input.rs — Keyboard input system, backend-agnostic.

use serde::{Serialize, Deserialize};
use winit::keyboard::{KeyCode, PhysicalKey};

/// A backend-agnostic representation of a keyboard key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Key0, Key1, Key2, Key3, Key4, Key5, Key6, Key7, Key8, Key9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Left, Right, Up, Down,
    Escape, Space, Enter, Backspace, Tab, Delete, Insert, Home, End, PageUp, PageDown,
    LeftShift, RightShift, LeftCtrl, RightCtrl, LeftAlt, RightAlt,
    Semicolon, Apostrophe, Comma, Period, Slash, Backslash, LeftBracket, RightBracket, Minus, Equals, Backquote,
}

impl Key {
    pub fn from_winit(wkey: PhysicalKey) -> Option<Self> {
        let code = match wkey {
            PhysicalKey::Code(c) => c,
            _ => return None,
        };

        Some(match code {
            KeyCode::KeyA => Key::A, KeyCode::KeyB => Key::B, KeyCode::KeyC => Key::C,
            KeyCode::KeyD => Key::D, KeyCode::KeyE => Key::E, KeyCode::KeyF => Key::F,
            KeyCode::KeyG => Key::G, KeyCode::KeyH => Key::H, KeyCode::KeyI => Key::I,
            KeyCode::KeyJ => Key::J, KeyCode::KeyK => Key::K, KeyCode::KeyL => Key::L,
            KeyCode::KeyM => Key::M, KeyCode::KeyN => Key::N, KeyCode::KeyO => Key::O,
            KeyCode::KeyP => Key::P, KeyCode::KeyQ => Key::Q, KeyCode::KeyR => Key::R,
            KeyCode::KeyS => Key::S, KeyCode::KeyT => Key::T, KeyCode::KeyU => Key::U,
            KeyCode::KeyV => Key::V, KeyCode::KeyW => Key::W, KeyCode::KeyX => Key::X,
            KeyCode::KeyY => Key::Y, KeyCode::KeyZ => Key::Z,

            KeyCode::Digit0 => Key::Key0, KeyCode::Digit1 => Key::Key1, KeyCode::Digit2 => Key::Key2,
            KeyCode::Digit3 => Key::Key3, KeyCode::Digit4 => Key::Key4, KeyCode::Digit5 => Key::Key5,
            KeyCode::Digit6 => Key::Key6, KeyCode::Digit7 => Key::Key7, KeyCode::Digit8 => Key::Key8,
            KeyCode::Digit9 => Key::Key9,

            KeyCode::F1 => Key::F1, KeyCode::F2 => Key::F2, KeyCode::F3 => Key::F3,
            KeyCode::F4 => Key::F4, KeyCode::F5 => Key::F5, KeyCode::F6 => Key::F6,
            KeyCode::F7 => Key::F7, KeyCode::F8 => Key::F8, KeyCode::F9 => Key::F9,
            KeyCode::F10 => Key::F10, KeyCode::F11 => Key::F11, KeyCode::F12 => Key::F12,

            KeyCode::ArrowLeft  => Key::Left,  KeyCode::ArrowRight => Key::Right,
            KeyCode::ArrowUp    => Key::Up,    KeyCode::ArrowDown  => Key::Down,
            KeyCode::Escape     => Key::Escape, KeyCode::Space      => Key::Space,
            KeyCode::Enter      => Key::Enter,  KeyCode::Backspace  => Key::Backspace,
            KeyCode::Tab        => Key::Tab,    KeyCode::Delete     => Key::Delete,
            KeyCode::Insert     => Key::Insert, KeyCode::Home       => Key::Home,
            KeyCode::End        => Key::End,    KeyCode::PageUp     => Key::PageUp,
            KeyCode::PageDown   => Key::PageDown,

            KeyCode::ShiftLeft    => Key::LeftShift,   KeyCode::ShiftRight    => Key::RightShift,
            KeyCode::ControlLeft  => Key::LeftCtrl,    KeyCode::ControlRight  => Key::RightCtrl,
            KeyCode::AltLeft      => Key::LeftAlt,     KeyCode::AltRight      => Key::RightAlt,

            KeyCode::Semicolon    => Key::Semicolon,   KeyCode::Quote         => Key::Apostrophe,
            KeyCode::Comma        => Key::Comma,       KeyCode::Period        => Key::Period,
            KeyCode::Slash        => Key::Slash,       KeyCode::Backslash     => Key::Backslash,
            KeyCode::BracketLeft  => Key::LeftBracket, KeyCode::BracketRight  => Key::RightBracket,
            KeyCode::Minus        => Key::Minus,       KeyCode::Equal         => Key::Equals,
            KeyCode::Backquote    => Key::Backquote,

            _ => return None,
        })
    }
}

/// Tracks keyboard state across frames: held, just-pressed, and just-released.
pub struct InputManager {
    /// All keys that are currently held down.
    held: Vec<Key>,

    /// Keys that transitioned from UP → DOWN this frame only.
    just_pressed: Vec<Key>,

    /// Keys that transitioned from DOWN → UP this frame only.
    just_released: Vec<Key>,

    /// Captured text characters from this frame.
    pub text_buffer: String,

    /// Set to true when the window is closed or a quit signal is received.
    pub quit_requested: bool,
}

impl InputManager {
    /// Create a fresh InputManager with no keys pressed.
    pub fn new() -> Self {
        InputManager {
            held:          Vec::new(),
            just_pressed:  Vec::new(),
            just_released: Vec::new(),
            text_buffer:   String::new(),
            quit_requested: false,
        }
    }

    /// Clear the just_pressed and just_released lists.
    /// Should be called at the start of every frame before processing new events.
    pub fn clear(&mut self) {
        self.just_pressed.clear();
        self.just_released.clear();
    }

    /// Returns the contents of the text buffer and clears it.
    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text_buffer)
    }

    /// Process a key press event.
    pub fn handle_pressed(&mut self, key: Key) {
        if !self.held.contains(&key) {
            self.held.push(key);
            self.just_pressed.push(key);
        }
    }

    /// Process a key release event.
    pub fn handle_released(&mut self, key: Key) {
        if let Some(pos) = self.held.iter().position(|&k| k == key) {
            self.held.remove(pos);
            self.just_released.push(key);
        }
    }

    /// True if `key` is currently held down.
    pub fn is_held(&self, key: Key) -> bool {
        self.held.contains(&key)
    }

    /// True ONLY on the single frame this key first went down.
    pub fn just_pressed(&self, key: Key) -> bool {
        self.just_pressed.contains(&key)
    }

    /// True ONLY on the single frame this key was released.
    pub fn just_released(&self, key: Key) -> bool {
        self.just_released.contains(&key)
    }
}
