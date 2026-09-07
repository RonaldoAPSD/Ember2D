// press_buffer.rs — Generic held/pending/consumed/just-released/repeat
// tracking, shared by `InputManager` (input.rs), `MouseState` (mouse.rs),
// and `GamepadState` (gamepad.rs).
//
// 7B-4 (docs/ember2d-master-plan.md §5.2): all three of those types carried
// their own copy of the exact same five fields and the exact same
// `clear`/`consume_step`/`decay`/`handle_pressed`/`handle_released`/
// `is_held`/`just_pressed`/`just_released` bodies, differing only in the
// key type (`Key`, `MouseButton`, `(usize, GamepadButton)`) — this extracts
// that shared machinery once so a future behavioral fix (e.g. this same
// step's repeat signal, below) only has to be written and tested in one
// place instead of three.
//
// `held`/`consumed`/`just_released` are `HashSet`s rather than the `Vec`s
// `InputManager`/`MouseState` used before this extraction: nothing in any
// of the three callers ever iterates them (every read is `.contains`), and
// `GamepadState` already used `HashSet` for the same fields — see
// `GamepadSnapshot`'s own doc comment (ember2d-sim/src/command.rs) for why
// that's fine even for the sim-facing snapshot types built from these.

use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Buffered press/hold/repeat state for one input device's set of
/// buttons/keys. See `crate::input::INPUT_BUFFER_WINDOW` for why presses
/// are buffered rather than frame-scoped (D1).
pub struct PressBuffer<K> {
    /// Keys/buttons currently held down.
    held: HashSet<K>,

    /// Keys pressed but not yet consumed by a simulation step, each with its
    /// remaining lifetime in the buffer (seconds). Populated by
    /// `handle_pressed`, drained by `consume_step`, decayed by `decay`.
    pending: HashMap<K, f32>,

    /// The set of keys the current simulation step sees as just-pressed —
    /// i.e. whatever `consume_step` last pulled out of `pending`. This is
    /// what `just_pressed` reads; it does not change again until the next
    /// `consume_step` call.
    consumed: HashSet<K>,

    /// Keys that transitioned from DOWN → UP this frame only.
    just_released: HashSet<K>,

    /// Keys the backend reported an OS-level key-repeat event for this
    /// frame (winit's `KeyEvent::repeat`) — 7B-4, R24
    /// (docs/ember2d-master-plan.md §5.2). Deliberately separate from
    /// `pending`/`consumed`: a repeat must never look like a fresh press to
    /// `just_pressed` (a script or UI action gated on `just_pressed` firing
    /// again on every OS repeat tick would misfire), but held-key text
    /// widgets (the script editor's Backspace-to-delete-repeatedly) still
    /// need *some* per-frame signal that isn't just `is_held`, since
    /// `is_held` alone would delete every frame at 60fps rather than at the
    /// OS's own repeat cadence. Cleared every frame in `clear()`, same as
    /// `just_released`.
    repeating: HashSet<K>,
}

impl<K: Eq + Hash + Copy> PressBuffer<K> {
    pub fn new() -> Self {
        PressBuffer {
            held: HashSet::new(),
            pending: HashMap::new(),
            consumed: HashSet::new(),
            just_released: HashSet::new(),
            repeating: HashSet::new(),
        }
    }

    /// Clear per-frame transient state (`just_released`, `repeating`).
    /// Should be called once per frame before processing new events.
    ///
    /// Deliberately does NOT touch `pending` — a buffered press must
    /// survive across frames until a simulation step consumes it or it
    /// decays away.
    pub fn clear(&mut self) {
        self.just_released.clear();
        self.repeating.clear();
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
        self.pending.retain(|_, remaining| {
            *remaining -= dt;
            *remaining > 0.0
        });
    }

    /// Process a press event. `buffer_window` is how long (seconds) the
    /// press waits in `pending` for a simulation step to consume it — the
    /// caller passes `crate::input::INPUT_BUFFER_WINDOW`.
    pub fn handle_pressed(&mut self, key: K, buffer_window: f32) {
        if self.held.insert(key) {
            self.pending.insert(key, buffer_window);
        }
    }

    /// Process a release event.
    pub fn handle_released(&mut self, key: K) {
        if self.held.remove(&key) {
            self.just_released.insert(key);
        }
    }

    /// Record an OS-level key-repeat event for an already-held key. A
    /// repeat for a key that isn't held (shouldn't happen, but backends are
    /// never fully trusted) is ignored rather than fabricating a hold.
    pub fn handle_repeat(&mut self, key: K) {
        if self.held.contains(&key) {
            self.repeating.insert(key);
        }
    }

    /// True if `key` is currently held down.
    pub fn is_held(&self, key: K) -> bool {
        self.held.contains(&key)
    }

    /// True in exactly one simulation step per physical press — see
    /// `PressBuffer`'s module doc for why this is buffered rather than
    /// frame-scoped.
    pub fn just_pressed(&self, key: K) -> bool {
        self.consumed.contains(&key)
    }

    /// True ONLY on the single frame this key was released.
    pub fn just_released(&self, key: K) -> bool {
        self.just_released.contains(&key)
    }

    /// True on a frame the backend reported an OS-level repeat for this
    /// key — see `repeating`'s own doc comment.
    pub fn is_repeating(&self, key: K) -> bool {
        self.repeating.contains(&key)
    }

    /// Iterate every key currently held. Used by callers (e.g.
    /// `GamepadState::snapshot`) that need to convert the whole held set,
    /// not just query one key.
    pub fn iter_held(&self) -> impl Iterator<Item = &K> {
        self.held.iter()
    }

    /// Iterate every key `consume_step` marked just-pressed this step. Same
    /// use case as `iter_held`.
    pub fn iter_consumed(&self) -> impl Iterator<Item = &K> {
        self.consumed.iter()
    }

    /// Drop every held/pending/consumed/just-released/repeating entry for a
    /// key `keep` returns `false` for. 7B-4, R25 (docs/ember2d-master-plan.md
    /// §5.2/§3): `GamepadState::poll` uses this on `EventType::Disconnected`
    /// to clear a device's stuck-held buttons — gilrs never sends a
    /// `ButtonReleased` for a device that's gone, so without this a button
    /// held at unplug time would stay `held` forever.
    pub fn retain(&mut self, keep: impl Fn(&K) -> bool) {
        self.held.retain(|k| keep(k));
        self.pending.retain(|k, _| keep(k));
        self.consumed.retain(|k| keep(k));
        self.just_released.retain(|k| keep(k));
        self.repeating.retain(|k| keep(k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: f32 = 0.12;

    #[test]
    fn consumed_once_even_across_multiple_steps_in_one_frame() {
        let mut buf = PressBuffer::new();
        buf.handle_pressed('a', WINDOW);

        buf.consume_step();
        assert!(buf.just_pressed('a'));

        buf.consume_step();
        assert!(!buf.just_pressed('a'), "a second step must not see the same press again");
    }

    #[test]
    fn survives_a_frame_that_runs_zero_steps() {
        let mut buf = PressBuffer::new();
        buf.handle_pressed('a', WINDOW);

        buf.decay(1.0 / 240.0);
        assert!(
            !buf.just_pressed('a'),
            "no step ran yet, so nothing should be marked just-pressed"
        );

        buf.consume_step();
        assert!(buf.just_pressed('a'), "a press must survive a frame that ran zero steps");
    }

    #[test]
    fn expires_if_unclaimed_past_the_buffer_window() {
        let mut buf = PressBuffer::new();
        buf.handle_pressed('a', WINDOW);

        buf.decay(WINDOW + 0.01);

        buf.consume_step();
        assert!(
            !buf.just_pressed('a'),
            "an unclaimed press should eventually expire, not buffer forever"
        );
    }

    #[test]
    fn press_and_release_within_one_frame_still_registers() {
        let mut buf = PressBuffer::new();
        buf.handle_pressed('a', WINDOW);
        buf.handle_released('a');

        assert!(!buf.is_held('a'));
        assert!(buf.just_released('a'));

        buf.consume_step();
        assert!(
            buf.just_pressed('a'),
            "a tap shorter than one frame must still register as a press"
        );
    }

    #[test]
    fn is_held_is_unbuffered_continuous_state() {
        let mut buf = PressBuffer::new();
        assert!(!buf.is_held('a'));
        buf.handle_pressed('a', WINDOW);
        assert!(buf.is_held('a'));
        buf.consume_step();
        assert!(buf.is_held('a'), "is_held must stay true regardless of buffer consumption");
        buf.handle_released('a');
        assert!(!buf.is_held('a'));
    }

    // ── Tests: 7B-4 repeat signal (docs/ember2d-master-plan.md §5.2, R24) ──

    #[test]
    fn repeat_never_sets_just_pressed() {
        let mut buf = PressBuffer::new();
        buf.handle_pressed('a', WINDOW);
        buf.consume_step(); // claim the real press
        assert!(buf.just_pressed('a'));

        buf.clear();
        buf.consume_step(); // next step, nothing new pending
        buf.handle_repeat('a');
        assert!(
            !buf.just_pressed('a'),
            "an OS repeat event must never look like a fresh just_pressed"
        );
        assert!(buf.is_repeating('a'), "but it must still be observable via is_repeating");
    }

    #[test]
    fn repeat_is_ignored_for_a_key_that_is_not_held() {
        let mut buf = PressBuffer::new();
        buf.handle_repeat('a');
        assert!(!buf.is_repeating('a'), "a repeat for an unheld key must not fabricate a hold");
    }

    #[test]
    fn repeating_is_cleared_every_frame() {
        let mut buf = PressBuffer::new();
        buf.handle_pressed('a', WINDOW);
        buf.handle_repeat('a');
        assert!(buf.is_repeating('a'));

        buf.clear();
        assert!(!buf.is_repeating('a'), "repeating must not carry over past the frame it fired in");
    }
}
