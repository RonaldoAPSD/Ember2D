// input.rs — Keyboard input system, now driven by minifb instead of crossterm.
//
// WHAT CHANGED:
//   Before: we used crossterm to read raw terminal key events from stdin.
//   After:  minifb gives us the set of held keys after each window update.
//           We compute just_pressed / just_released ourselves by diffing
//           the current held set against the previous frame's held set.
//
// KEY DESIGN: "DIFF THE HELD SET"
//
//   Each frame, minifb tells us ALL keys that are currently held down.
//   We compare this against the set that was held LAST frame:
//
//     just_pressed  = current_held  MINUS  last_held   (newly appeared)
//     just_released = last_held     MINUS  current_held (newly disappeared)
//     held          = current_held  (replace entirely)
//
//   This is cleaner than processing individual press/release events and
//   works correctly even if the window loses focus mid-game.
//
// KEY TYPE:
//   We re-export minifb::Key as our public `Key` type.
//   Game code uses `Key::W`, `Key::Up`, `Key::Escape`, etc.
//   (Previously it was crossterm's KeyCode with `Key::Char('w')` — now it's just `Key::W`.)

// Re-export minifb::Key so game code can write `Key::W` without importing minifb.
pub use minifb::Key;

/// Tracks keyboard state across frames: held, just-pressed, and just-released.
pub struct InputManager {
    /// All keys that are currently held down (still pressed this frame).
    held: Vec<Key>,

    /// Keys that transitioned from UP → DOWN this frame only.
    /// This Vec is cleared at the start of every frame.
    just_pressed: Vec<Key>,

    /// Keys that transitioned from DOWN → UP this frame only.
    /// This Vec is cleared at the start of every frame.
    just_released: Vec<Key>,

    /// Set to true when the window is closed or Ctrl+C-equivalent is detected.
    pub quit_requested: bool,
}

impl InputManager {
    /// Create a fresh InputManager with no keys pressed.
    pub fn new() -> Self {
        InputManager {
            held:          Vec::new(),
            just_pressed:  Vec::new(),
            just_released: Vec::new(),
            quit_requested: false,
        }
    }

    /// Update input state for this frame.
    ///
    /// Call this ONCE per frame, passing:
    ///   `current_keys` — all keys currently held, from `renderer.current_keys()`
    ///   `window_open`  — false when the user clicks the window's X button
    ///
    /// This computes just_pressed and just_released by comparing `current_keys`
    /// against what was held last frame.
    pub fn poll(&mut self, current_keys: Vec<Key>, window_open: bool) {
        // Keys that appeared since last frame → just_pressed.
        // "Was NOT in held last frame, but IS in current_keys now."
        self.just_pressed = current_keys
            .iter()
            .filter(|k| !self.held.contains(k))
            .copied()
            .collect();

        // Keys that disappeared since last frame → just_released.
        // "Was in held last frame, but is NOT in current_keys now."
        self.just_released = self
            .held
            .iter()
            .filter(|k| !current_keys.contains(k))
            .copied()
            .collect();

        // The new held set is exactly what minifb reports right now.
        self.held = current_keys;

        // Close button or programmatic quit signal.
        if !window_open {
            self.quit_requested = true;
        }
    }

    /// True if `key` is currently held down (pressed one or more frames ago).
    ///
    /// Use for continuous actions: moving while an arrow key is held.
    pub fn is_held(&self, key: Key) -> bool {
        self.held.contains(&key)
    }

    /// True ONLY on the single frame this key first went down.
    ///
    /// Use for one-shot actions: fire, jump, open a menu.
    pub fn just_pressed(&self, key: Key) -> bool {
        self.just_pressed.contains(&key)
    }

    /// True ONLY on the single frame this key was released.
    ///
    /// Use for charge-and-release mechanics.
    pub fn just_released(&self, key: Key) -> bool {
        self.just_released.contains(&key)
    }
}
