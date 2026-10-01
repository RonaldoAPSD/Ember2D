// input.rs — Keyboard input system, backend-agnostic.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::press_buffer::PressBuffer;
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
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Left,
    Right,
    Up,
    Down,
    Escape,
    Space,
    Enter,
    Backspace,
    Tab,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    LeftShift,
    RightShift,
    LeftCtrl,
    RightCtrl,
    LeftAlt,
    RightAlt,
    Semicolon,
    Apostrophe,
    Comma,
    Period,
    Slash,
    Backslash,
    LeftBracket,
    RightBracket,
    Minus,
    Equals,
    Backquote,
}

impl Key {
    pub fn from_winit(wkey: PhysicalKey) -> Option<Self> {
        let code = match wkey {
            PhysicalKey::Code(c) => c,
            _ => return None,
        };

        Some(match code {
            KeyCode::KeyA => Key::A,
            KeyCode::KeyB => Key::B,
            KeyCode::KeyC => Key::C,
            KeyCode::KeyD => Key::D,
            KeyCode::KeyE => Key::E,
            KeyCode::KeyF => Key::F,
            KeyCode::KeyG => Key::G,
            KeyCode::KeyH => Key::H,
            KeyCode::KeyI => Key::I,
            KeyCode::KeyJ => Key::J,
            KeyCode::KeyK => Key::K,
            KeyCode::KeyL => Key::L,
            KeyCode::KeyM => Key::M,
            KeyCode::KeyN => Key::N,
            KeyCode::KeyO => Key::O,
            KeyCode::KeyP => Key::P,
            KeyCode::KeyQ => Key::Q,
            KeyCode::KeyR => Key::R,
            KeyCode::KeyS => Key::S,
            KeyCode::KeyT => Key::T,
            KeyCode::KeyU => Key::U,
            KeyCode::KeyV => Key::V,
            KeyCode::KeyW => Key::W,
            KeyCode::KeyX => Key::X,
            KeyCode::KeyY => Key::Y,
            KeyCode::KeyZ => Key::Z,

            KeyCode::Digit0 => Key::Key0,
            KeyCode::Digit1 => Key::Key1,
            KeyCode::Digit2 => Key::Key2,
            KeyCode::Digit3 => Key::Key3,
            KeyCode::Digit4 => Key::Key4,
            KeyCode::Digit5 => Key::Key5,
            KeyCode::Digit6 => Key::Key6,
            KeyCode::Digit7 => Key::Key7,
            KeyCode::Digit8 => Key::Key8,
            KeyCode::Digit9 => Key::Key9,

            KeyCode::F1 => Key::F1,
            KeyCode::F2 => Key::F2,
            KeyCode::F3 => Key::F3,
            KeyCode::F4 => Key::F4,
            KeyCode::F5 => Key::F5,
            KeyCode::F6 => Key::F6,
            KeyCode::F7 => Key::F7,
            KeyCode::F8 => Key::F8,
            KeyCode::F9 => Key::F9,
            KeyCode::F10 => Key::F10,
            KeyCode::F11 => Key::F11,
            KeyCode::F12 => Key::F12,

            KeyCode::ArrowLeft => Key::Left,
            KeyCode::ArrowRight => Key::Right,
            KeyCode::ArrowUp => Key::Up,
            KeyCode::ArrowDown => Key::Down,
            KeyCode::Escape => Key::Escape,
            KeyCode::Space => Key::Space,
            KeyCode::Enter => Key::Enter,
            KeyCode::Backspace => Key::Backspace,
            KeyCode::Tab => Key::Tab,
            KeyCode::Delete => Key::Delete,
            KeyCode::Insert => Key::Insert,
            KeyCode::Home => Key::Home,
            KeyCode::End => Key::End,
            KeyCode::PageUp => Key::PageUp,
            KeyCode::PageDown => Key::PageDown,

            KeyCode::ShiftLeft => Key::LeftShift,
            KeyCode::ShiftRight => Key::RightShift,
            KeyCode::ControlLeft => Key::LeftCtrl,
            KeyCode::ControlRight => Key::RightCtrl,
            KeyCode::AltLeft => Key::LeftAlt,
            KeyCode::AltRight => Key::RightAlt,

            KeyCode::Semicolon => Key::Semicolon,
            KeyCode::Quote => Key::Apostrophe,
            KeyCode::Comma => Key::Comma,
            KeyCode::Period => Key::Period,
            KeyCode::Slash => Key::Slash,
            KeyCode::Backslash => Key::Backslash,
            KeyCode::BracketLeft => Key::LeftBracket,
            KeyCode::BracketRight => Key::RightBracket,
            KeyCode::Minus => Key::Minus,
            KeyCode::Equal => Key::Equals,
            KeyCode::Backquote => Key::Backquote,

            _ => return None,
        })
    }

    /// Text a focused widget's `InputManager::text_buffer` should receive
    /// for this *logical* key press (`winit::keyboard::Key` — a different
    /// type from this enum's own physical-key `from_winit` above) — called
    /// from `Engine::poll_events`.
    ///
    /// R44 (docs/ember2d-master-plan.md §5.1, 7A-11): winit classifies
    /// Space as `Key::Named(NamedKey::Space)`, not `Key::Character(" ")`
    /// the way every other printable key comes through — `poll_events`
    /// used to match only `Key::Character`, so a Space press was silently
    /// dropped before it ever reached `text_buffer`. Every `take_text()`
    /// consumer (the script editor, palette editor, palette search, graph
    /// param field) inherited the same gap: none of them could type a
    /// space, which makes writing actual Rhai source in the built-in
    /// script editor impossible (found live during the Phase 7A gate's
    /// manual regression pass, not caught by R11/R12's own 7A-2 tests
    /// since neither exercised Space specifically). Unit-testable by
    /// constructing a `winit::keyboard::Key` directly, no live event loop
    /// needed — same reasoning as `from_winit` above.
    pub fn logical_key_text(key: &winit::keyboard::Key) -> String {
        match key {
            winit::keyboard::Key::Character(text) => {
                text.chars().filter(|ch| !ch.is_control()).collect()
            }
            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space) => " ".to_string(),
            _ => String::new(),
        }
    }
}

/// Tracks keyboard state across frames: held, just-pressed, and just-released.
pub struct InputManager {
    /// Held/pending/consumed/just-released/repeat bookkeeping — extracted
    /// (7B-4, docs/ember2d-master-plan.md §5.2) into `PressBuffer` since
    /// `MouseState` and `GamepadState` carried byte-for-byte copies of the
    /// same five fields and methods; see that type's module doc for why.
    buffer: PressBuffer<Key>,

    /// Captured text characters from this frame.
    pub text_buffer: String,

    /// R12 (7A-2, docs/ember2d-master-plan.md): set by `begin_text_capture`
    /// when some focused widget wants this step's `text_buffer` — checked
    /// (and reset) by `finish_frame_text_capture`. Without this,
    /// `text_buffer` had exactly one consumer (`take_text`, called only
    /// from the level editor's modal text prompt) while several OTHER
    /// editor surfaces (script editor, palette editor, palette search,
    /// graph param fields) typed via the older `key_to_char`/`just_pressed`
    /// path and never touched `text_buffer` at all — so every keystroke
    /// typed anywhere else silently piled up here, unconsumed, until
    /// whenever a prompt next opened, which then received the entire
    /// backlog in one `take_text()` call. Now: no consumer since the last
    /// check means the buffer is wiped before it can carry over to a
    /// later, unrelated one.
    ///
    /// R55 (7C-5 follow-up, docs/ember2d-master-plan.md §5.1): despite the
    /// name, `finish_frame_text_capture` is called once per completed
    /// SIMULATION STEP now, not once per real frame — `Engine::run` calls
    /// it right after a step actually ran (`sim::step`, the only place
    /// `begin_text_capture` can be called from), not from
    /// `Engine::poll_events` unconditionally. A real frame under
    /// `GameplayLoop::RealTime`'s fixed-timestep accumulator can complete
    /// with zero steps (any display faster than 60Hz produces these
    /// constantly) — checking on one of those wiped `text_buffer` out from
    /// under keystrokes `poll_events` had just captured that same frame,
    /// since nothing could have renewed the request yet.
    text_capture_requested: bool,

    /// Set to true when the window is closed or a quit signal is received.
    pub quit_requested: bool,
}

impl InputManager {
    /// Create a fresh InputManager with no keys pressed.
    pub fn new() -> Self {
        InputManager {
            buffer: PressBuffer::new(),
            text_buffer: String::new(),
            text_capture_requested: false,
            quit_requested: false,
        }
    }

    /// Clear the just_released/repeating sets. Should be called at the start
    /// of every frame before processing new events.
    ///
    /// Deliberately does NOT touch the buffered-press map — a buffered
    /// press must survive across frames until a simulation step consumes it
    /// or it decays away.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    /// Pull the current buffered presses into this simulation step's
    /// just-pressed set and remove them from the buffer, so no later step
    /// (this frame or a future one) observes the same press again.
    ///
    /// Call once per simulation step, before running game/script update code.
    pub fn consume_step(&mut self) {
        self.buffer.consume_step();
    }

    /// Age out buffered presses that no simulation step claimed in time.
    /// Call once per frame (real delta time, not sim dt) after the frame's
    /// simulation steps have had their chance to consume them.
    pub fn decay(&mut self, dt: f32) {
        self.buffer.decay(dt);
    }

    /// Returns the contents of the text buffer and clears it.
    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text_buffer)
    }

    /// Called by whichever widget currently wants this step's captured
    /// text (a text prompt, the palette editor/search, a graph param field,
    /// the script editor) — see `text_capture_requested`'s own doc comment.
    /// Must be called every simulation step the widget stays focused (not
    /// every real frame — see R55 on `text_capture_requested`); it only
    /// protects the check that runs right after the step it's called in.
    pub fn begin_text_capture(&mut self) {
        self.text_capture_requested = true;
    }

    /// `Engine::run`'s post-step half of the mechanism described on
    /// `text_capture_requested`: if nothing asked to keep this step's
    /// text, wipe it now so it can never carry over into a future,
    /// unrelated consumer. Either way, the request is one-shot — it must
    /// be renewed every step via `begin_text_capture`. R55
    /// (docs/ember2d-master-plan.md §5.1): must be called only when a
    /// step actually ran (never unconditionally per real frame) — see
    /// `Engine::run`'s own call sites for why.
    pub fn finish_frame_text_capture(&mut self) {
        if !self.text_capture_requested {
            self.text_buffer.clear();
        }
        self.text_capture_requested = false;
    }

    /// Process a key press event.
    pub fn handle_pressed(&mut self, key: Key) {
        self.buffer.handle_pressed(key, INPUT_BUFFER_WINDOW);
    }

    /// Process a key release event.
    pub fn handle_released(&mut self, key: Key) {
        self.buffer.handle_released(key);
    }

    /// Record an OS-level key-repeat event (winit's `KeyEvent::repeat`) for
    /// an already-held key — 7B-4, R24 (docs/ember2d-master-plan.md §5.2).
    /// See `PressBuffer::repeating`'s own doc comment for why this is a
    /// separate signal from `handle_pressed`.
    pub fn handle_repeat(&mut self, key: Key) {
        self.buffer.handle_repeat(key);
    }

    /// True if `key` is currently held down.
    pub fn is_held(&self, key: Key) -> bool {
        self.buffer.is_held(key)
    }

    /// True in exactly one simulation step per physical press — see
    /// `INPUT_BUFFER_WINDOW` for why this is buffered rather than frame-scoped.
    pub fn just_pressed(&self, key: Key) -> bool {
        self.buffer.just_pressed(key)
    }

    /// True ONLY on the single frame this key was released.
    pub fn just_released(&self, key: Key) -> bool {
        self.buffer.just_released(key)
    }

    /// True on a frame winit reported an OS-level repeat for `key` — see
    /// `handle_repeat`. Held-key text widgets (the script editor's
    /// Backspace-to-delete, arrow navigation, Tab/Enter) check this
    /// alongside `just_pressed` so holding the key repeats at the OS's own
    /// cadence instead of only firing once per physical press.
    pub fn is_repeating(&self, key: Key) -> bool {
        self.buffer.is_repeating(key)
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
        //
        // R108 (§3 in the master plan): every letter — this map used to
        // carry only the twelve the demos happened to use (WASD, Q/E/R/F,
        // Z/X/C/V), so `just_pressed("t")`/`("m")`/`("i")` and the other
        // fourteen silently never fired. Found building Step 9-3's
        // dialogue test bed.
        const KEY_MAP: &[(Key, &str)] = &[
            (Key::A, "a"),
            (Key::B, "b"),
            (Key::C, "c"),
            (Key::D, "d"),
            (Key::E, "e"),
            (Key::F, "f"),
            (Key::G, "g"),
            (Key::H, "h"),
            (Key::I, "i"),
            (Key::J, "j"),
            (Key::K, "k"),
            (Key::L, "l"),
            (Key::M, "m"),
            (Key::N, "n"),
            (Key::O, "o"),
            (Key::P, "p"),
            (Key::Q, "q"),
            (Key::R, "r"),
            (Key::S, "s"),
            (Key::T, "t"),
            (Key::U, "u"),
            (Key::V, "v"),
            (Key::W, "w"),
            (Key::X, "x"),
            (Key::Y, "y"),
            (Key::Z, "z"),
            (Key::Up, "up"),
            (Key::Down, "down"),
            (Key::Left, "left"),
            (Key::Right, "right"),
            (Key::Space, "space"),
            (Key::Enter, "enter"),
            (Key::Escape, "escape"),
            (Key::LeftShift, "shift"),
            (Key::RightShift, "shift"),
            (Key::LeftCtrl, "ctrl"),
            (Key::RightCtrl, "ctrl"),
            (Key::Key1, "1"),
            (Key::Key2, "2"),
            (Key::Key3, "3"),
            (Key::Key4, "4"),
            (Key::Key5, "5"),
            (Key::Key6, "6"),
            (Key::Key7, "7"),
            (Key::Key8, "8"),
            (Key::Key9, "9"),
            (Key::Key0, "0"),
            (Key::Tab, "tab"),
            (Key::Backspace, "backspace"),
            (Key::F1, "f1"),
            (Key::F2, "f2"),
            (Key::F3, "f3"),
            (Key::F4, "f4"),
            (Key::F5, "f5"),
            (Key::F6, "f6"),
            (Key::F7, "f7"),
            (Key::F8, "f8"),
            (Key::F9, "f9"),
            (Key::F10, "f10"),
            (Key::F11, "f11"),
            (Key::F12, "f12"),
        ];

        let mut held = BTreeSet::new();
        let mut pressed = BTreeSet::new();
        for (key, name) in KEY_MAP {
            if self.is_held(*key) {
                held.insert(name.to_string());
            }
            if self.just_pressed(*key) {
                pressed.insert(name.to_string());
            }
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
        assert!(
            !input.just_pressed(Key::Space),
            "no step ran yet, so nothing should be marked just-pressed"
        );

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
        assert!(
            !input.just_pressed(Key::Space),
            "an unclaimed press should eventually expire, not buffer forever"
        );
    }

    #[test]
    fn press_and_release_within_one_frame_still_registers() {
        let mut input = InputManager::new();
        input.handle_pressed(Key::Space);
        input.handle_released(Key::Space);

        assert!(!input.is_held(Key::Space));
        assert!(input.just_released(Key::Space));

        input.consume_step();
        assert!(
            input.just_pressed(Key::Space),
            "a tap shorter than one frame must still register as a press"
        );
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
        assert!(
            !snap.is_held("W") && !snap.just_pressed("Enter"),
            "no capitalized names should leak through"
        );
    }

    #[test]
    fn r108_every_letter_key_reaches_scripts() {
        let letters = [
            Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I, Key::J,
            Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R, Key::S, Key::T,
            Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
        ];
        for (key, name) in letters.into_iter().zip('a'..='z') {
            let mut input = InputManager::new();
            input.handle_pressed(key);
            input.consume_step();
            let snap = input.snapshot();
            assert!(snap.just_pressed(&name.to_string()), "{name} never reached scripts");
            assert!(snap.is_held(&name.to_string()));
        }
    }

    // ── Tests: R12 (7A-2, docs/ember2d-master-plan.md) — text_buffer must
    // not survive a frame nothing asked to capture it ──────────────────────

    #[test]
    fn text_buffer_is_cleared_after_a_frame_with_no_capture_request() {
        let mut input = InputManager::new();
        input.text_buffer.push_str("stray keystrokes");
        input.finish_frame_text_capture();
        assert!(input.text_buffer.is_empty(), "with no begin_text_capture call this frame, leftover text must not survive into a later, unrelated consumer");
    }

    #[test]
    fn text_buffer_survives_a_frame_that_requested_capture() {
        let mut input = InputManager::new();
        input.text_buffer.push_str("hello");
        input.begin_text_capture();
        input.finish_frame_text_capture();
        assert_eq!(
            input.text_buffer, "hello",
            "a widget that called begin_text_capture this frame must still see its text"
        );
    }

    #[test]
    fn a_capture_request_does_not_carry_over_to_the_next_frame() {
        let mut input = InputManager::new();
        input.begin_text_capture();
        input.finish_frame_text_capture(); // consumes this frame's request
        input.text_buffer.push_str("typed after focus moved away");
        input.finish_frame_text_capture(); // no request renewed this frame
        assert!(input.text_buffer.is_empty(), "begin_text_capture must be renewed every frame — a stale request from an earlier frame must not protect a later one");
    }

    // ── Tests: R44 (7A-11, docs/ember2d-master-plan.md) — Space must reach
    // text_buffer even though winit reports it as a Named key, not a
    // Character ──────────────────────────────────────────────────────────

    #[test]
    fn logical_key_text_produces_a_space_for_the_named_space_key() {
        let key = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Space);
        assert_eq!(Key::logical_key_text(&key), " ");
    }

    #[test]
    fn logical_key_text_passes_through_an_ordinary_character_key() {
        let key = winit::keyboard::Key::Character("a".into());
        assert_eq!(Key::logical_key_text(&key), "a");
    }

    #[test]
    fn logical_key_text_strips_control_characters_from_a_character_key() {
        // Some IME/compose sequences can hand back a control character
        // inside `Key::Character` — filtered here the same way the
        // original inline match in `Engine::poll_events` always did.
        let key = winit::keyboard::Key::Character("\u{7}".into());
        assert_eq!(Key::logical_key_text(&key), "");
    }

    #[test]
    fn logical_key_text_is_empty_for_other_named_keys() {
        // Enter/Backspace/Tab/arrows etc. are handled through their own
        // physical-key `just_pressed` paths elsewhere (script_editor.rs) —
        // this function must not also inject them as text.
        let key = winit::keyboard::Key::Named(winit::keyboard::NamedKey::Enter);
        assert_eq!(Key::logical_key_text(&key), "");
    }
}
