// engine/window.rs — winit 0.30 ApplicationHandler shims (split out of
// engine.rs at R76, docs/ember2d-master-plan.md §3.2 — engine.rs was 789
// real lines, over CLAUDE.md's 750-line limit; this is purely a file split,
// no behavior change).
//
// 7B-1 (docs/ember2d-master-plan.md §5.2): winit 0.30 replaced direct,
// eager window creation (the old `WindowBuilder::new()...build(&event_loop)`,
// callable any time) with `ActiveEventLoop::create_window`, reachable only
// from inside an `ApplicationHandler` callback — `resumed()`, specifically,
// which fires once the platform is ready to create windows/GL contexts.
// Two small handlers, not one, because they run at two different points
// with two different jobs: `WindowInit` runs exactly once, synchronously,
// inside `Engine::new` (pumped in a loop until `resumed()` fires and hands
// back a window) so `Renderer::new` — and therefore `Engine::new` itself —
// keeps returning a fully-initialized, ready-to-use `Engine`, exactly as
// every caller already expects; `EventPump` runs every frame from
// `poll_events`, replacing the closure `pump_events` (deprecated in favor
// of `pump_app_events`) used to take, with the identical `WindowEvent`
// handling that closure had. Same `loop {}`-per-frame shape either way —
// see `Engine::run`'s own loop in the parent module, unchanged by this
// split. `pub(super)`: both handlers are `Engine::new`/`Engine::poll_events`
// implementation detail, never part of the crate's own public surface.

use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::input::{InputManager, Key};
use crate::mouse::{MouseButton, MouseState};
use crate::renderer::Renderer;

/// Exists only to receive the one `resumed()` call `Engine::new` pumps for.
pub(super) struct WindowInit {
    pub(super) width: usize,
    pub(super) height: usize,
    pub(super) title: String,
    pub(super) window: Option<Arc<Window>>,
}

impl ApplicationHandler for WindowInit {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return; // Already created — resumed() can in principle fire again.
        }
        let pixel_width = self.width * crate::renderer::CELL_W;
        let pixel_height = self.height * crate::renderer::CELL_H;
        // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): only a starting
        // guess — no window (and therefore no authoritative
        // `window.scale_factor()`) exists yet to ask. `Renderer::new`
        // re-derives the real scale, cell grid, and `ScreenMapping` from
        // the window's actual `inner_size()`/`scale_factor()` once it
        // exists (`recompute_layout`), so a wrong guess here only costs an
        // extra resize-equivalent recompute, not a lasting mismatch — see
        // that function's own doc comment. `PhysicalSize`, not
        // `LogicalSize`: this guessed scale IS meant to end up as the
        // window's actual physical size, same as the real DPI-derived
        // `scale` will apply later; requesting a `LogicalSize` here would
        // let winit's own OS-level DPI scaling apply on top and
        // double-scale.
        let guessed_scale = event_loop
            .primary_monitor()
            .map(|m| m.scale_factor() as f32)
            .unwrap_or(crate::renderer::INITIAL_SCALE_GUESS)
            .round()
            .max(crate::renderer::MIN_UI_SCALE);
        let attrs = Window::default_attributes().with_title(&self.title).with_inner_size(
            winit::dpi::PhysicalSize::new(
                pixel_width as f32 * guessed_scale,
                pixel_height as f32 * guessed_scale,
            ),
        );
        if let Ok(window) = event_loop.create_window(attrs) {
            self.window = Some(Arc::new(window));
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
        // Nothing to do before a window (and therefore a Renderer) exists.
    }
}

/// The real per-frame window-event handler, built fresh each `poll_events`
/// call from whichever `Engine` fields it needs to mutate — same fields
/// the old closure captured by `&mut` reference.
pub(super) struct EventPump<'a> {
    pub(super) input: &'a mut InputManager,
    pub(super) mouse: &'a mut MouseState,
    pub(super) renderer: &'a mut Renderer,
    pub(super) engine_width: &'a mut usize,
    pub(super) engine_height: &'a mut usize,
    /// Persists across frames (owned by `Engine`, not reset in
    /// `EventPump::new` each poll) — a `ModifiersChanged` event fires only
    /// when the held modifier set actually changes, not every frame, so
    /// this has to remember the last-known state rather than starting
    /// "no modifiers held" on every `poll_events` call. 7B-4, R24
    /// (docs/ember2d-master-plan.md §5.2/§3).
    pub(super) modifiers: &'a mut ModifiersState,
}

impl<'a> ApplicationHandler for EventPump<'a> {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        // The window already exists by the time this pump runs (created in
        // Engine::new via WindowInit) — nothing to do even if the platform
        // re-fires resumed() (e.g. after a suspend/resume cycle).
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => self.input.quit_requested = true,
            WindowEvent::ModifiersChanged(mods) => {
                *self.modifiers = mods.state();
            }
            WindowEvent::KeyboardInput { event: key_event, .. } => {
                // 1. Physical key for state tracking (held/pressed/repeat).
                //
                // R24 (7B-4, docs/ember2d-master-plan.md §5.2/§3): winit's
                // `KeyEvent::repeat` (true for the OS-generated repeats a
                // held key produces, not the original press) used to be
                // read nowhere in this codebase — every repeat event was
                // routed through `handle_pressed` exactly like a fresh
                // press. That happened to be harmless for `held`/`pending`
                // (`handle_pressed` is idempotent while already held) but
                // meant nothing could ever distinguish "held key's OS
                // repeat fired this frame" from "no repeat" — which is
                // exactly the signal a held-key text widget (the script
                // editor's Backspace/arrow/Tab/Enter handling) needs to
                // repeat while held instead of firing once per physical
                // press. `handle_repeat` records that signal without
                // touching `pending`, so it can never look like a second
                // `just_pressed` for the same physical press.
                if let Some(key) = Key::from_winit(key_event.physical_key) {
                    if key_event.state.is_pressed() {
                        if key_event.repeat {
                            self.input.handle_repeat(key);
                        } else {
                            self.input.handle_pressed(key);
                        }
                    } else {
                        self.input.handle_released(key);
                    }
                }

                // 2. Logical key for text entry (characters, symbols, etc.)
                // R44 (7A-11, docs/ember2d-master-plan.md §5.1): was
                // `if let Key::Character(text) = ...` only — see
                // `logical_key_text`'s own doc comment for why that
                // silently dropped every Space press. A repeat still feeds
                // text (holding a letter key must still retype it, same as
                // any text editor), so this stays gated on `is_pressed()`
                // alone, not on `!key_event.repeat`.
                //
                // R24: also gated on Ctrl/Super NOT being held — without
                // this, a shortcut like Ctrl+S typed while a text widget
                // (script editor, palette search, …) had focus leaked an
                // "s" into the text buffer on top of whatever the shortcut
                // itself did. Alt is deliberately excluded from the gate:
                // AltGr-based layouts synthesize printable characters (e.g.
                // "@") as a Ctrl+Alt chord, so gating on Alt too would
                // break typing those.
                if key_event.state.is_pressed()
                    && !self.modifiers.control_key()
                    && !self.modifiers.super_key()
                {
                    self.input.text_buffer.push_str(&Key::logical_key_text(&key_event.logical_key));
                }
            }
            // R24 (7B-4, docs/ember2d-master-plan.md §5.2/§3): committed
            // IME text (a composed CJK/accented sequence, finalized by the
            // platform's input method) arrives here, never as a
            // `KeyboardInput` — nothing previously read this event at all,
            // so IME users could never type into any `text_buffer`
            // consumer (the script editor, palette search, …).
            // `Ime::Enabled`/`Preedit`/`Disabled` are presentation-only
            // (an in-progress composition string, not yet committed text)
            // and out of this step's scope — falls through to `_ => {}`.
            WindowEvent::Ime(winit::event::Ime::Commit(text)) => {
                self.input.text_buffer.push_str(&text);
            }
            // R24: the cursor leaving the window used to leave `in_bounds`
            // stuck `true` forever (nothing ever set it back to `false` —
            // only `handle_move`, on the next `CursorMoved`, which won't
            // fire again until the cursor re-enters). A script or editor
            // surface gating on `mouse.in_bounds` would misread "cursor
            // left the window" as "cursor still over the last position it
            // was at" until some future re-entry. `in_bounds` is already
            // `pub` (mouse.rs) with no dedicated setter — nothing else
            // needs one either.
            WindowEvent::CursorLeft { .. } => {
                self.mouse.in_bounds = false;
            }
            WindowEvent::CursorMoved { position, .. } => {
                // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): was a
                // single-axis, DPI-blind `scale_factor()` pre-division
                // that never accounted for the letterbox origin —
                // `handle_move` does the full physical->logical conversion
                // via `ScreenMapping` now.
                self.mouse.handle_move(
                    position.x as f32,
                    position.y as f32,
                    self.renderer.screen_mapping(),
                );
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let btn = MouseButton::from_winit(button);
                if state.is_pressed() {
                    self.mouse.handle_pressed(btn);
                } else {
                    self.mouse.handle_released(btn);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => self.mouse.handle_scroll(x, y),
                winit::event::MouseScrollDelta::PixelDelta(pos) => {
                    self.mouse.handle_scroll(pos.x as f32 / 8.0, pos.y as f32 / 16.0)
                }
            },
            WindowEvent::Resized(_) => {
                if self.renderer.try_handle_resize() {
                    *self.engine_width = self.renderer.width;
                    *self.engine_height = self.renderer.height;
                }
            }
            // 7B-2 (docs/ember2d-master-plan.md §5.2, R21): the window
            // moved to a monitor with a different DPI scale factor (or the
            // OS scale setting changed) — re-derive `scale`/the cell grid/
            // `ScreenMapping` the same way a resize would. Ignoring
            // `inner_size_writer`: nothing here requests a specific inner
            // size in response, `recompute_layout` just reads whatever the
            // window's size/scale_factor are by the time this fires.
            WindowEvent::ScaleFactorChanged { .. } => {
                if self.renderer.handle_scale_factor_changed() {
                    *self.engine_width = self.renderer.width;
                    *self.engine_height = self.renderer.height;
                }
            }
            _ => {}
        }
    }
}
