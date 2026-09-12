// ember2d-editor/tests/common/mod.rs — EditorHarness: a headless stand-in
// for the real editor event loop (7C-5, docs/ember2d-master-plan.md §5.3).
//
// WHY THIS EXISTS: every editor input bug the 7A-2 fixes closed by hand
// (R11-R14) is only reachable through a live `winit` window driving an
// `EditorState` via `&InputManager`/`&MouseState` — there was no way to
// inject a click, a keypress, or typed text from a test. `EditorHarness`
// owns everything a real frame needs (`InputManager`, `MouseState`,
// `GamepadState`, `World`, `EventBus`, the persistent store) at a fixed
// 1280x720 window, and exposes `click`/`key`/`type_text`/`drag`/`frame` —
// see each method's own doc comment for exactly what one call simulates.
//
// HOW A FRAME WORKS HERE: mirrors `Engine::run()`'s real per-frame shape
// (`engine.rs`) exactly, reusing the same `ember2d::sim::step` both the
// real engine and `ember2d/tests/common/mod.rs`'s `TurnHarness` call — see
// that module's own header comment for why it's the one place this
// sequence lives. The one piece `sim::step` doesn't cover is the
// clear-before/decay-after pair `Engine::poll_events`/`run` wrap around
// it (`input.clear()` at the top of a frame, `input.decay()` after
// `sim::step` returns) — `begin_frame`/`end_frame` below reproduce that
// same wrapping by hand.
//
// RENDERING: `EditorState::draw` (7C-5 split `handle_render`'s old body
// out of the full `RenderContext`-based `GameState::render` specifically
// for this) takes any `&mut dyn DrawSurface`, so this harness renders into
// a `NullRenderer` (`ember2d::renderer`) — every draw call is a no-op, but
// 7C-1's `UiFrame::push` calls happen as a side effect of drawing
// regardless of what consumes the draw, so `self.ui_frame` ends up
// populated exactly as it would from a real `Renderer`. `ui/frame.rs`'s
// own header comment explains the resulting one-frame lag (`update()`
// reads the `UiFrame` the PREVIOUS frame's render pass populated) — this
// harness renders once at construction, before any input, for the same
// reason the real engine's first frame does.

use ember2d::gamepad::GamepadState;
use ember2d::input::{InputManager, Key};
use ember2d::mouse::{MouseButton, MouseState};
use ember2d::renderer::{NullRenderer, ScreenMapping, CELL_H, CELL_W};
use ember2d_editor::editor::EditorState;
use ember2d_sim::event::EventBus;
use ember2d_sim::math::Vec2;
use ember2d_sim::world::{EntityId, World};
use std::collections::{BTreeMap, HashMap};

/// Fixed window size (master plan §5.3, 7C-5's own Change text) — large
/// enough for every docked panel's minimum size (`panel/mod.rs`'s
/// `HIER_W`/`INSP_W`/`PAL_W`/`CON_H`/`EDIT_H` constants) to coexist with a
/// real viewport, so tests exercise the same layout the shipped app does,
/// not a degenerate corner case.
pub const PIXEL_W: usize = 1280;
pub const PIXEL_H: usize = 720;

/// One simulated frame's fixed timestep — 60fps, matching `engine.rs`'s
/// own `SIM_DT`. Not imported directly (`ember2d::engine::SIM_DT` isn't
/// `pub`) since every test here only needs a plausible, constant value,
/// not bit-for-bit engine parity.
const FRAME_DT: f32 = 1.0 / 60.0;

pub struct EditorHarness {
    pub state: EditorState,
    input: InputManager,
    mouse: MouseState,
    gamepad: GamepadState,
    world: World,
    events: EventBus,
    persistent: BTreeMap<String, rhai::Dynamic>,
    prev_positions: HashMap<EntityId, Vec2>,
    elapsed: f32,
}

impl EditorHarness {
    /// A fresh editor at the default new-level state, already rendered
    /// once — so the very first `click`/`key` call's `update()` sees a
    /// real `UiFrame` (the menu bar, the default docked panels), not an
    /// empty one from before anything was ever drawn.
    pub fn new() -> Self {
        let mut h = EditorHarness {
            state: EditorState::new("harness.level"),
            input: InputManager::new(),
            mouse: MouseState::new(),
            gamepad: GamepadState::new(),
            world: World::new(),
            events: EventBus::new(),
            persistent: BTreeMap::new(),
            prev_positions: HashMap::new(),
            elapsed: 0.0,
        };
        h.render();
        h
    }

    fn render(&mut self) {
        let mut null_renderer = NullRenderer::new(PIXEL_W, PIXEL_H);
        self.state.draw(&mut null_renderer, &self.mouse);
    }

    /// `Engine::poll_events`'s clear-before-new-input half — see this
    /// module's header comment.
    fn begin_frame(&mut self) {
        self.input.clear();
        self.mouse.clear();
        self.gamepad.clear();
    }

    /// `Engine::poll_events`'s `finish_frame_text_capture` call, then
    /// `sim::step`, then the real loop's post-step `decay`, then a render
    /// pass to refresh `UiFrame` for the frame after this one.
    fn end_frame(&mut self) {
        self.input.finish_frame_text_capture();
        let viewport_width = PIXEL_W / CELL_W;
        let viewport_height = PIXEL_H / CELL_H;
        let _ = ember2d::sim::step(
            &mut self.state,
            &mut self.world,
            &mut self.input,
            &mut self.mouse,
            &mut self.gamepad,
            &mut self.events,
            &mut self.persistent,
            &mut self.prev_positions,
            FRAME_DT,
            FRAME_DT,
            FRAME_DT,
            self.elapsed,
            viewport_width,
            viewport_height,
            false,
        );
        self.elapsed += FRAME_DT;
        self.input.decay(FRAME_DT);
        self.mouse.decay(FRAME_DT);
        self.gamepad.decay(FRAME_DT);
        self.render();
    }

    /// Advance one frame with no new input — useful to let a mode
    /// transition settle (see `type_text`'s own doc comment) or to check
    /// something decays/times out.
    pub fn frame(&mut self) {
        self.begin_frame();
        self.end_frame();
    }

    /// Move the mouse to logical pixel position `(px, py)` — the same
    /// space `UiRect`/`UiFrame` hit-test in (1 unit = 1 un-scaled cell
    /// pixel). Uses an identity `ScreenMapping` (zero letterbox origin,
    /// 1:1 physical-to-logical) since the harness has no real window to
    /// derive one from — `(px, py)` IS the logical position callers want.
    pub fn move_mouse(&mut self, px: f32, py: f32) {
        let identity =
            ScreenMapping { origin_px: (0.0, 0.0), cell_px: (CELL_W as f32, CELL_H as f32) };
        self.mouse.handle_move(px, py, identity);
    }

    /// A left-click at logical pixel `(px, py)`: move, press (one frame),
    /// release (one frame) — two frames because `EditorState`'s input
    /// handlers distinguish `left_just_pressed`/`left_just_released`
    /// (e.g. paint-on-press, stamp-on-release) exactly the way a real
    /// press-then-release across two frames does.
    pub fn click(&mut self, px: f32, py: f32) {
        self.begin_frame();
        self.move_mouse(px, py);
        self.mouse.handle_pressed(MouseButton::Left);
        self.end_frame();
        self.begin_frame();
        self.mouse.handle_released(MouseButton::Left);
        self.end_frame();
    }

    /// A left-button drag from `from` to `to`: press at `from` (one
    /// frame), move to `to` while still held (one frame), release (one
    /// frame).
    pub fn drag(&mut self, from: (f32, f32), to: (f32, f32)) {
        self.begin_frame();
        self.move_mouse(from.0, from.1);
        self.mouse.handle_pressed(MouseButton::Left);
        self.end_frame();
        self.begin_frame();
        self.move_mouse(to.0, to.1);
        self.end_frame();
        self.begin_frame();
        self.mouse.handle_released(MouseButton::Left);
        self.end_frame();
    }

    /// Press `key` and hold it (does not release) — for a modifier that
    /// must stay held across a following `press_key`/`key` call, e.g.
    /// `press_key(LeftShift); press_key(S);` for Shift+S.
    pub fn press_key(&mut self, key: Key) {
        self.begin_frame();
        self.input.handle_pressed(key);
        self.end_frame();
    }

    /// Release a key `press_key` (or `key`) left held.
    pub fn release_key(&mut self, key: Key) {
        self.begin_frame();
        self.input.handle_released(key);
        self.end_frame();
    }

    /// A tap of `key`: press (one frame), release (one frame) — see
    /// `click`'s own doc comment for why press and release are separate
    /// frames.
    pub fn key(&mut self, key: Key) {
        self.press_key(key);
        self.release_key(key);
    }

    /// Pushes `text` straight into `InputManager::text_buffer` for the
    /// CURRENT frame, without the settling frame `type_text` gives a
    /// newly-focused widget first — models keystrokes that land while
    /// nothing has asked to capture them (R12, input.rs) rather than text
    /// meant for a specific focused field. See `type_text`'s own doc
    /// comment for the normal, settled path.
    pub fn inject_raw_text(&mut self, text: &str) {
        self.begin_frame();
        self.input.text_buffer.push_str(text);
        self.end_frame();
    }

    /// Types `text` into whatever currently has text-capture focus (a
    /// `Prompt`, the script editor, the palette editor/search). Two
    /// frames, not one: `InputManager::begin_text_capture`'s own contract
    /// (input.rs, R12) is that a request only protects the frame AFTER
    /// the one it's made in, so the widget that just gained focus needs
    /// one settling frame to call `begin_text_capture` for the first time
    /// before typed text can survive `finish_frame_text_capture`. Calling
    /// `type_text` again immediately afterward (the focus hasn't moved)
    /// does not need a second settling frame, but paying for one anyway
    /// is harmless — `begin_text_capture` is renewed every frame the
    /// widget stays focused.
    pub fn type_text(&mut self, text: &str) {
        self.frame();
        self.begin_frame();
        self.input.text_buffer.push_str(text);
        self.end_frame();
    }
}
