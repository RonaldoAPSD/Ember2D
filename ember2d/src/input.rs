// input.rs — Keyboard input system, backend-agnostic.

use std::collections::{BTreeSet, HashMap, HashSet};
use serde::{Serialize, Deserialize};
use winit::keyboard::{KeyCode, PhysicalKey};

use ember2d_sim::command::InputSnapshot;

/// How long a press waits in the buffer for a simulation step to consume it.
///
/// This exists because `just_pressed` is produced once per *frame* (in
/// `poll_events`) but consumed once per *simulation step*, and those two
/// cadences don't match under a fixed-timestep accumulator: a heavy frame
/// runs several steps, a light frame can run zero. Without buffering, a
/// press either fires on every step in a heavy frame (duplicates) or gets
/// silently cleared before any step observes it (drops) — defect D1 in
/// docs/ember2d-refactor-plan.md §3/§4.1.
///
/// The chosen fix: a press enters this buffer and lives here until the
/// first simulation step consumes it, surviving frames that run zero steps.
/// 100–150ms also happens to double as jump-buffering/coyote-time forgiveness.
pub const INPUT_BUFFER_WINDOW: f32 = 0.12;

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

    /// Keys pressed but not yet consumed by a simulation step, each with its
    /// remaining lifetime in the buffer (seconds). Populated by `handle_pressed`,
    /// drained by `consume_step`, decayed by `decay`.
    pending: HashMap<Key, f32>,

    /// The set of keys the current simulation step sees as just-pressed —
    /// i.e. whatever `consume_step` last pulled out of `pending`. This is
    /// what `just_pressed` reads; it does not change again until the next
    /// `consume_step` call.
    consumed: HashSet<Key>,

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
            pending:       HashMap::new(),
            consumed:      HashSet::new(),
            just_released: Vec::new(),
            text_buffer:   String::new(),
            quit_requested: false,
        }
    }

    /// Clear the just_released list. Should be called at the start of every
    /// frame before processing new events.
    ///
    /// Deliberately does NOT touch `pending` — a buffered press must survive
    /// across frames until a simulation step consumes it or it decays away.
    pub fn clear(&mut self) {
        self.just_released.clear();
    }

    /// Pull the current buffered presses into this simulation step's
    /// just-pressed set and remove them from the buffer, so no later step
    /// (this frame or a future one) observes the same press again.
    ///
    /// Call once per simulation step, before running game/script update code.
    pub fn consume_step(&mut self) {
        self.consumed = self.pending.keys().copied().collect();
        self.pending.clear();
    }

    /// Age out buffered presses that no simulation step claimed in time.
    /// Call once per frame (real delta time, not sim dt) after the frame's
    /// simulation steps have had their chance to consume them.
    pub fn decay(&mut self, dt: f32) {
        self.pending.retain(|_, remaining| { *remaining -= dt; *remaining > 0.0 });
    }

    /// Returns the contents of the text buffer and clears it.
    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text_buffer)
    }

    /// Process a key press event.
    pub fn handle_pressed(&mut self, key: Key) {
        if !self.held.contains(&key) {
            self.held.push(key);
            self.pending.insert(key, INPUT_BUFFER_WINDOW);
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

    /// True in exactly one simulation step per physical press — see
    /// `INPUT_BUFFER_WINDOW` for why this is buffered rather than frame-scoped.
    pub fn just_pressed(&self, key: Key) -> bool {
        self.consumed.contains(&key)
    }

    /// True ONLY on the single frame this key was released.
    pub fn just_released(&self, key: Key) -> bool {
        self.just_released.contains(&key)
    }

    /// Build the sim-safe, winit-free snapshot of which key names are
    /// held/just-pressed this step — what scripts' `is_held`/`just_pressed`
    /// and the `on_input` lifecycle actually see. Moved here from
    /// `scripting/types.rs`'s `snapshot_keys` free function in Step 5e
    /// (docs/ember2d-phase5-plan.md): the winit-to-string conversion
    /// (`KEY_MAP` below) is engine-side by nature, so it belongs next to
    /// `Key` itself — after this move, the scripting module no longer needs
    /// to know `InputManager` (or winit) exists at all, which is what lets
    /// `ScriptState`/`ScriptEngine` eventually live in the sim-only crate
    /// the Phase 5 plan's workspace split (§5.5) calls for.
    pub fn snapshot(&self) -> InputSnapshot {
        // Lowercase to match the documented script API contract
        // (docs/ember2d-scripting-api.md §3: `"w"`, `"space"`, `"escape"`, `"left"`, …).
        const KEY_MAP: &[(Key, &str)] = &[
            (Key::W, "w"), (Key::A, "a"), (Key::S, "s"), (Key::D, "d"),
            (Key::Q, "q"), (Key::E, "e"), (Key::R, "r"), (Key::F, "f"),
            (Key::Z, "z"), (Key::X, "x"), (Key::C, "c"), (Key::V, "v"),
            (Key::Up, "up"), (Key::Down, "down"), (Key::Left, "left"), (Key::Right, "right"),
            (Key::Space, "space"), (Key::Enter, "enter"), (Key::Escape, "escape"),
            (Key::LeftShift, "shift"), (Key::RightShift, "shift"),
            (Key::LeftCtrl, "ctrl"),  (Key::RightCtrl, "ctrl"),
            (Key::Key1, "1"), (Key::Key2, "2"), (Key::Key3, "3"),
            (Key::Key4, "4"), (Key::Key5, "5"), (Key::Key6, "6"),
            (Key::Key7, "7"), (Key::Key8, "8"), (Key::Key9, "9"), (Key::Key0, "0"),
            (Key::Tab, "tab"),
            (Key::Backspace, "backspace"),
            (Key::F1, "f1"), (Key::F2, "f2"), (Key::F3, "f3"), (Key::F4, "f4"),
            (Key::F5, "f5"), (Key::F6, "f6"), (Key::F7, "f7"), (Key::F8, "f8"),
            (Key::F9, "f9"), (Key::F10, "f10"), (Key::F11, "f11"), (Key::F12, "f12"),
        ];

        let mut held = BTreeSet::new();
        let mut pressed = BTreeSet::new();
        for (key, name) in KEY_MAP {
            if self.is_held(*key)      { held.insert(name.to_string()); }
            if self.just_pressed(*key) { pressed.insert(name.to_string()); }
        }
        InputSnapshot { held, pressed }
    }
}

// ── Tests: D1 input buffering (docs/ember2d-refactor-plan.md §3/§4.1) ──────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumed_once_even_across_multiple_steps_in_one_frame() {
        let mut input = InputManager::new();
        input.handle_pressed(Key::Space);

        input.consume_step(); // first sim step this (heavy) frame
        assert!(input.just_pressed(Key::Space));

        input.consume_step(); // second sim step, same frame
        assert!(!input.just_pressed(Key::Space), "a second step must not see the same press again");
    }

    #[test]
    fn survives_a_frame_that_runs_zero_steps() {
        let mut input = InputManager::new();
        input.handle_pressed(Key::Space);

        // Light frame: no simulation step runs, so nothing consumes it —
        // only the per-frame decay ticks the buffer down.
        input.decay(1.0 / 240.0);
        assert!(!input.just_pressed(Key::Space), "no step ran yet, so nothing should be marked just-pressed");

        // Next frame, a step finally runs and should still see the press.
        input.consume_step();
        assert!(input.just_pressed(Key::Space), "a press must survive a frame that ran zero steps");
    }

    #[test]
    fn expires_if_unclaimed_past_the_buffer_window() {
        let mut input = InputManager::new();
        input.handle_pressed(Key::Space);

        // Starve it past INPUT_BUFFER_WINDOW without any step consuming it.
        input.decay(INPUT_BUFFER_WINDOW + 0.01);

        input.consume_step();
        assert!(!input.just_pressed(Key::Space), "an unclaimed press should eventually expire, not buffer forever");
    }

    #[test]
    fn press_and_release_within_one_frame_still_registers() {
        let mut input = InputManager::new();
        input.handle_pressed(Key::Space);
        input.handle_released(Key::Space);

        assert!(!input.is_held(Key::Space));
        assert!(input.just_released(Key::Space));

        input.consume_step();
        assert!(input.just_pressed(Key::Space), "a tap shorter than one frame must still register as a press");
    }

    #[test]
    fn is_held_is_unbuffered_continuous_state() {
        let mut input = InputManager::new();
        assert!(!input.is_held(Key::W));
        input.handle_pressed(Key::W);
        assert!(input.is_held(Key::W));
        input.consume_step();
        assert!(input.is_held(Key::W), "is_held must stay true regardless of buffer consumption");
        input.handle_released(Key::W);
        assert!(!input.is_held(Key::W));
    }

    // ── Test: Step 5e snapshot() (docs/ember2d-phase5-plan.md) — moved from
    // scripting/types.rs's snapshot_keys, which this replaces. ────────────

    #[test]
    fn snapshot_uses_lowercase_names() {
        // Regression test: the old KEY_MAP this moved from previously
        // emitted "W"/"Up"/"Enter" while both the documented API
        // (ember2d-scripting-api.md §3) and every demo script call
        // ctx.is_held("w") / ctx.just_pressed("enter") in lowercase. The
        // mismatch meant scripts gating on movement/menu keys silently
        // never matched — e.g. the original demo's player script
        // (docs/archive/demo/scripts/player.rhai) had a tutorial gate that
        // never dismissed, which zeroed player velocity every frame.
        let mut input = InputManager::new();
        input.handle_pressed(Key::W);
        input.handle_pressed(Key::Enter);
        input.consume_step();

        let snap = input.snapshot();
        assert!(snap.is_held("w"), "held set should use lowercase key names");
        assert!(snap.just_pressed("enter"), "just_pressed set should use lowercase key names");
        assert!(!snap.is_held("W") && !snap.just_pressed("Enter"), "no capitalized names should leak through");
    }
}
