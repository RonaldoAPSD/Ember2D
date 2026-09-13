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
//
// `mod common;` is pulled in separately by every `tests/*.rs` file
// (`editor_input.rs`, `editor_undo.rs`, …), each compiled as its own
// independent crate — so any helper only one of those files' tests calls
// looks like dead code to the OTHERS' own compilation. Real, and harmless:
// suppressed at the module level rather than tracking which helper each
// binary happens to use.
#![allow(dead_code)]

use ember2d::gamepad::GamepadState;
use ember2d::input::{InputManager, Key};
use ember2d::mouse::{MouseButton, MouseState};
use ember2d::renderer::draw_log::DrawOp;
use ember2d::renderer::{DisplayScale, NullRenderer, ScreenMapping, CELL_H, CELL_W};
use ember2d_editor::editor::ui::{
    menu_entries, theme_menu_entries, MenuEntry, MenuKind, ToolbarAction, WidgetId,
};
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

/// `cargo test` runs this binary with CWD set to `ember2d-editor/` (this
/// crate's own manifest dir), not the repo root — but `EditorState::new`/
/// `::load` load the chrome theme from a `themes/<name>/` path relative to
/// CWD (`theme_loader.rs`), which only exists at the repo root. Without
/// this, every test-constructed `EditorState` silently got
/// `Theme::fallback()` (magenta chrome, no slices, the built-in bitmap
/// font) instead of the real shipped theme — found investigating 7D-4
/// (master plan §5.4), a pre-existing gap since 7D-2's very first slice
/// that no earlier test happened to assert on real theme content closely
/// enough to catch. Same idiom as `ember2d/tests/common/mod.rs`'s own
/// `ensure_workspace_root_cwd` for level-loading tests. Must run BEFORE
/// `EditorState::new`/`::load` is called — calling it only inside
/// `EditorHarness::with_state` is too late for a caller that builds the
/// `EditorState` as that call's own argument expression, which several
/// tests below do.
pub fn ensure_workspace_root_cwd() {
    let _ = std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
}

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
    /// The `NullRenderer`'s reported render/OS display scale (7D-3, master
    /// plan §5.4) — `(1, 1.0)` by default, matching every pre-7D-3 test's
    /// implicit assumption that points == logical pixels. `with_display`/
    /// `with_state_and_display` are the only constructors that set anything
    /// else.
    display: DisplayScale,
    /// The window size a real `Renderer` would report — separate fields
    /// (not the `PIXEL_W`/`PIXEL_H` consts directly) so `resize` can change
    /// them mid-test (7D-3, master plan §5.4 — exercising `EditorState`'s
    /// own resize-driven layout code, e.g. R69, without a real window).
    pixel_w: usize,
    pixel_h: usize,
    /// Whether `render()`'s throwaway `NullRenderer` should record its own
    /// draw calls (7D-3, master plan §5.4) — off by default (no behavior
    /// or perf change for any pre-7D-3 test); `start_recording` turns it on.
    recording: bool,
    last_ops: Vec<DrawOp>,
}

impl EditorHarness {
    /// A fresh editor at the default new-level state, already rendered
    /// once — so the very first `click`/`key` call's `update()` sees a
    /// real `UiFrame` (the menu bar, the default docked panels), not an
    /// empty one from before anything was ever drawn.
    pub fn new() -> Self {
        ensure_workspace_root_cwd();
        Self::with_state(EditorState::new("harness.level"))
    }

    /// Same as `new()`, but starting from a caller-built `EditorState`
    /// (e.g. `EditorState::new(path)` with a save path inside a specific
    /// project folder, or `EditorState::load(...)`) instead of the
    /// harness's own default — for tests that need control over
    /// `save_path`/`grid` before the first frame renders.
    pub fn with_state(state: EditorState) -> Self {
        Self::with_state_and_display(state, DisplayScale { render_scale: 1, os_scale_factor: 1.0 })
    }

    /// As `new()`, but reporting `display` instead of the `(1, 1.0)`
    /// default (7D-3, master plan §5.4) — for a test exercising
    /// `EditorState::effective_ui_scale`/`UiSpace` at a specific render
    /// scale. `display.render_scale` also drives `move_mouse`'s own
    /// physical<->logical conversion, so callers keep passing LOGICAL
    /// pixel positions regardless of which render scale is in effect.
    pub fn with_display(display: DisplayScale) -> Self {
        Self::with_state_and_display(EditorState::new("harness.level"), display)
    }

    /// The full combination of `with_state`/`with_display` — see each's own
    /// doc comment.
    pub fn with_state_and_display(state: EditorState, display: DisplayScale) -> Self {
        ensure_workspace_root_cwd();
        let mut h = EditorHarness {
            state,
            input: InputManager::new(),
            mouse: MouseState::new(),
            gamepad: GamepadState::new(),
            world: World::new(),
            events: EventBus::new(),
            persistent: BTreeMap::new(),
            prev_positions: HashMap::new(),
            elapsed: 0.0,
            display,
            pixel_w: PIXEL_W,
            pixel_h: PIXEL_H,
            recording: false,
            last_ops: Vec::new(),
        };
        h.render();
        h
    }

    /// Changes the simulated window size and re-renders (7D-3, master plan
    /// §5.4) — for a test exercising resize-driven layout without a real
    /// window (e.g. R69: `PanelManager`'s own layout going stale in
    /// script/graph mode across a resize).
    pub fn resize(&mut self, pixel_w: usize, pixel_h: usize) {
        self.pixel_w = pixel_w;
        self.pixel_h = pixel_h;
        self.render();
    }

    /// Turns on `render()`'s `NullRenderer` draw-op recording — see
    /// `draw_ops`'s own doc comment.
    pub fn start_recording(&mut self) {
        self.recording = true;
    }

    /// The previous `render()` pass's recorded draw ops (7D-3, master plan
    /// §5.4) — empty unless `start_recording` was called first, and empty
    /// regardless until a chrome draw call actually goes through
    /// `UiPainter` (a later checkpoint of this same step); present now so
    /// those checkpoints' own tests have the harness support ready.
    pub fn draw_ops(&self) -> &[DrawOp] {
        &self.last_ops
    }

    fn render(&mut self) {
        let mut null_renderer =
            NullRenderer::with_display(self.pixel_w, self.pixel_h, self.display);
        if self.recording {
            null_renderer.start_recording();
        }
        self.state.draw(&mut null_renderer, &self.mouse);
        if self.recording {
            self.last_ops = null_renderer.ops().to_vec();
        }
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
        let viewport_width = self.pixel_w / CELL_W;
        let viewport_height = self.pixel_h / CELL_H;
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
    /// pixel), REGARDLESS of `self.display.render_scale` (7D-3, master
    /// plan §5.4): the physical position fed to `handle_move` is scaled up
    /// by the render scale first, through a real (not identity, unless
    /// `render_scale == 1`) `ScreenMapping`, so it converts back down to
    /// exactly `(px, py)` — callers never need to think in physical pixels
    /// themselves, at any render scale the harness is constructed with.
    pub fn move_mouse(&mut self, px: f32, py: f32) {
        let r = self.display.render_scale as f32;
        let mapping = ScreenMapping {
            origin_px: (0.0, 0.0),
            cell_px: (CELL_W as f32 * r, CELL_H as f32 * r),
        };
        self.mouse.handle_move(px * r, py * r, mapping);
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

    /// A right-click at logical pixel `(px, py)` — see `click`'s own doc
    /// comment for the two-frame press/release shape.
    pub fn right_click(&mut self, px: f32, py: f32) {
        self.begin_frame();
        self.move_mouse(px, py);
        self.mouse.handle_pressed(MouseButton::Right);
        self.end_frame();
        self.begin_frame();
        self.mouse.handle_released(MouseButton::Right);
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

    /// A left-button drag through every point in `path` in order — see
    /// `drag_button_through`'s own doc comment for exactly what one call
    /// simulates. Unlike `drag`, models a real multi-cell freehand stroke
    /// rather than a single straight press-move-release.
    pub fn drag_through(&mut self, path: &[(f32, f32)]) {
        self.drag_button_through(MouseButton::Left, path);
    }

    /// A `button` drag through every point in `path` in order: press at
    /// `path[0]` (one frame), move through each remaining point (one frame
    /// each, button still held), release at the last point (one frame).
    pub fn drag_button_through(&mut self, button: MouseButton, path: &[(f32, f32)]) {
        let (first, rest) =
            path.split_first().expect("drag_button_through needs at least one point");
        self.begin_frame();
        self.move_mouse(first.0, first.1);
        self.mouse.handle_pressed(button);
        self.end_frame();
        for &(x, y) in rest {
            self.begin_frame();
            self.move_mouse(x, y);
            self.end_frame();
        }
        self.begin_frame();
        self.mouse.handle_released(button);
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

// ── Shared test helpers ──────────────────────────────────────────────────
//
// Used by both `tests/editor_input.rs` (7C-5) and `tests/editor_undo.rs`
// (7C-6) — moved here (not duplicated) when splitting the latter out kept
// `editor_input.rs` under CLAUDE.md's 750-line limit.

/// Opens `kind`'s dropdown by clicking its menu-bar label (found via the
/// last render pass's `UiFrame`, not a hardcoded pixel guess — the same
/// discipline 7C-1 requires of the editor's own input handlers).
pub fn open_menu(h: &mut EditorHarness, kind: MenuKind) {
    let rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::MenuLabel(kind))
        .expect("menu label not found in the last render pass");
    h.click(rect.x + 1.0, rect.y + 1.0);
    assert_eq!(h.state.active_menu(), Some(kind), "clicking the label did not open its dropdown");
}

/// With `kind`'s dropdown already open (see `open_menu`), clicks whichever
/// entry's action matches `pred`.
pub fn click_menu_item(
    h: &mut EditorHarness,
    kind: MenuKind,
    pred: impl Fn(&ToolbarAction) -> bool,
) {
    let entries = menu_entries(kind);
    let idx = entries
        .iter()
        .position(|e| matches!(e, MenuEntry::Item { action, .. } if pred(action)))
        .expect("no entry in this menu matches the requested action");
    let rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::MenuItem(kind, idx))
        .expect("dropdown item not found in the last render pass");
    h.click(rect.x + 1.0, rect.y + 1.0);
}

/// `click_menu_item`'s counterpart for `MenuKind::Theme` (7D-4, master plan
/// §5.4): that menu's entries come from `EditorState::available_themes` at
/// runtime (`theme_menu_entries`), not `menu_entries`'s fixed per-kind
/// list, so `click_menu_item`'s own lookup can't find them.
pub fn click_theme_menu_item(h: &mut EditorHarness, name: &str) {
    let entries = theme_menu_entries(h.state.available_themes());
    let idx = entries
        .iter()
        .position(|e| matches!(e, MenuEntry::DynamicItem { label, .. } if label == name))
        .unwrap_or_else(|| panic!("no theme menu entry named {name:?}"));
    let rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::MenuItem(MenuKind::Theme, idx))
        .expect("theme dropdown item not found in the last render pass");
    h.click(rect.x + 1.0, rect.y + 1.0);
}

/// Brings `id` to the front of its own dock side by clicking its tab (7D
/// layout default: Console and FileBrowser share the Bottom dock) — a
/// panel that's merely `visible` but not the active tab on its side is
/// excluded from `PanelManager::in_draw_order` entirely (see that
/// method's own filter), so its rows/content never draw, and neither does
/// its own `WidgetId::Tab` entry unless a render pass has already run
/// with at least one panel on that side active. Call this instead of the
/// old "toggle the panel via the View menu" pattern whenever a test needs
/// a specific BOTTOM/LEFT/RIGHT-docked panel's own content to actually
/// render, not just be nominally visible.
pub fn select_dock_tab(h: &mut EditorHarness, id: ember2d_editor::editor::panel::PanelId) {
    h.frame();
    let rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::Tab(id))
        .unwrap_or_else(|| panic!("{id:?}'s dock tab was not drawn in the last render pass"));
    h.click(rect.x + 1.0, rect.y + 1.0);
    h.frame();
}

/// The center of the viewport's own content area — a real on-canvas pixel
/// position, read from the current layout rather than guessed. Not
/// necessarily a valid grid cell for a small level at zoom 1 (the
/// viewport shows far more cells than a 32x20 level has) — use
/// `canvas_pixel_for_grid` when the click needs to actually land on the
/// level's own tiles.
pub fn canvas_center(h: &EditorHarness) -> (f32, f32) {
    let vp = h.state.panels().viewport().content_rect();
    (vp.x + vp.w / 2.0, vp.y + vp.h / 2.0)
}

/// The pixel position `mouse_to_grid` maps back to grid cell `(gx, gy)`,
/// given the current viewport rect/scroll/zoom (`impl_state/mod.rs`'s own
/// `mouse_to_grid` formula, inverted) — a fresh harness's default level is
/// 32x20 at zoom 1.0/scroll (0,0), so a small `(gx, gy)` lands inside it.
pub fn canvas_pixel_for_grid(h: &EditorHarness, gx: i32, gy: i32) -> (f32, f32) {
    let vp = h.state.panels().viewport().content_rect();
    let zoom = 1.0; // EditorState::new's default
    let local_x = (gx as f32 - 0.0 /* scroll.0 */ + 0.5) * zoom;
    let local_y = (gy as f32 - 0.0 /* scroll.1 */ + 0.5) * zoom;
    (vp.x + local_x * CELL_W as f32, vp.y + local_y * CELL_H as f32)
}
