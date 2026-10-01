// scripting/widgets.rs — the script side of engine-owned menus and dialogue
// boxes: `menu_open`/`menu_selection`/`menu_closed`/`menu_close`,
// `draw_dialogue`/`dialogue_advance`/`dialogue_open`/`dialogue_done`/
// `close_dialogue`, and the `wrap_text` helper.
//
// Step 9-3 (docs/ember2d-master-plan.md §5.8). The state and the keyboard
// handling are `crate::ui` (see its header for why they're simulation
// state); this file queues `UiOp`s like every other deferred write and
// reads back the model as it stood when the pass began. Opening a widget
// hands its id back immediately — ids come from a per-pass counter seeded
// from the model, so two opens in one pass get different ids.

use std::rc::Rc;

use rhai::{Array, Dynamic, Map};

use crate::ui::{paginate, wrap_text, Dialogue, Menu, MenuState, UiModel, UiOp};

use super::api::ScriptCtx;
use super::engine::ScriptEngine;

/// Columns a dialogue box gives up to its border and padding (two each
/// side) — the rest of the viewport width is its line length.
pub const DIALOGUE_MARGIN_COLS: usize = 4;

/// The widget state a `ScriptState` carries: the model at the start of the
/// pass (read-only), this pass's requests, and the next id to hand out.
#[derive(Default)]
pub struct UiCtx {
    pub(super) view: Rc<UiModel>,
    pub(super) ops: Vec<UiOp>,
    pub(super) next_id: i64,
}

impl ScriptEngine {
    /// What widget reads see — set by `Simulation` whenever its `UiModel`
    /// changes.
    pub fn set_ui_view(&mut self, model: UiModel) {
        self.ui_view = Rc::new(model);
    }

    pub(super) fn ui_ctx(&self) -> UiCtx {
        UiCtx { view: self.ui_view.clone(), ops: Vec::new(), next_id: self.ui_view.next_id }
    }
}

fn strings(items: &Array) -> Vec<String> {
    items.iter().map(|v| v.to_string()).collect()
}

fn opt_i64(opts: &Map, key: &str) -> Option<i64> {
    opts.get(key).and_then(|v| v.as_int().ok().or_else(|| v.as_float().ok().map(|f| f as i64)))
}

impl ScriptCtx {
    fn next_ui_id(&mut self) -> i64 {
        let mut state = self.inner.borrow_mut();
        let id = state.ui.next_id;
        state.ui.next_id += 1;
        id
    }

    /// Open a menu of `items`; returns its id.
    pub fn menu_open(&mut self, items: Array) -> i64 {
        self.menu_open_with(items, Map::new())
    }

    /// `menu_open` with options: `title`, `x`/`y` (cells; default centred),
    /// `width` (cells), `cancelable` (default true), `selected` (start row).
    pub fn menu_open_with(&mut self, items: Array, opts: Map) -> i64 {
        let items = strings(&items);
        let n = items.len();
        let menu = Menu {
            items,
            selected: opt_i64(&opts, "selected").unwrap_or(0).clamp(0, n.saturating_sub(1) as i64)
                as usize,
            state: MenuState::Open,
            title: opts.get("title").map(|v| v.to_string()).unwrap_or_default(),
            x: opt_i64(&opts, "x"),
            y: opt_i64(&opts, "y"),
            width: opt_i64(&opts, "width").filter(|w| *w > 0),
            cancelable: opts.get("cancelable").and_then(|v| v.as_bool().ok()).unwrap_or(true),
            owner: self.entity_id,
        };
        let id = self.next_ui_id();
        self.inner.borrow_mut().ui.ops.push(UiOp::OpenMenu(id, menu));
        id
    }

    /// The highlighted row of an open menu, the chosen row once confirmed,
    /// `-1` if cancelled (or unknown).
    pub fn menu_selection(&mut self, id: i64) -> i64 {
        self.inner.borrow().ui.view.menu_selection(id)
    }

    /// True once the menu was confirmed or cancelled.
    pub fn menu_closed(&mut self, id: i64) -> bool {
        self.inner.borrow().ui.view.menu_closed(id)
    }

    /// Close (or forget) a menu.
    pub fn menu_close(&mut self, id: i64) {
        self.inner.borrow_mut().ui.ops.push(UiOp::CloseMenu(id));
    }

    /// Show `text` in the dialogue box under `speaker` (may be `""`);
    /// returns its id. Wrapped to the viewport and paged three lines at a
    /// time; Enter/Space turns the page. The same text and speaker while
    /// that dialogue is still showing does nothing (returns its id).
    pub fn draw_dialogue(&mut self, text: String, speaker: String) -> i64 {
        let showing = self.inner.borrow().ui.view.open_dialogue().cloned();
        if let Some(d) = showing.filter(|d| d.text == text && d.speaker == speaker) {
            return d.id;
        }
        let width = {
            let state = self.inner.borrow();
            state.viewport_size.0.saturating_sub(DIALOGUE_MARGIN_COLS).max(8)
        };
        let id = self.next_ui_id();
        let owner = self.entity_id;
        let pages = paginate(&text, width);
        let dialogue = Dialogue { id, speaker, pages, text, page: 0, done: false, owner };
        self.inner.borrow_mut().ui.ops.push(UiOp::OpenDialogue(dialogue));
        id
    }

    /// Turn the dialogue's page (closing it after the last).
    pub fn dialogue_advance(&mut self) {
        self.inner.borrow_mut().ui.ops.push(UiOp::AdvanceDialogue);
    }

    /// True while a dialogue box is showing.
    pub fn dialogue_open(&mut self) -> bool {
        self.inner.borrow().ui.view.open_dialogue().is_some()
    }

    /// True once dialogue `id` was read through, closed or replaced.
    pub fn dialogue_done(&mut self, id: i64) -> bool {
        self.inner.borrow().ui.view.dialogue_done(id)
    }

    /// Close the dialogue box.
    pub fn close_dialogue(&mut self) {
        self.inner.borrow_mut().ui.ops.push(UiOp::CloseDialogue);
    }

    /// `text` word-wrapped to `width` characters — an array of lines.
    pub fn wrap_text(&mut self, text: String, width: i64) -> Array {
        wrap_text(&text, width.max(1) as usize).into_iter().map(Dynamic::from).collect()
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("menu_open", ScriptCtx::menu_open);
    engine.register_fn("menu_open", ScriptCtx::menu_open_with);
    engine.register_fn("menu_selection", ScriptCtx::menu_selection);
    engine.register_fn("menu_closed", ScriptCtx::menu_closed);
    engine.register_fn("menu_close", ScriptCtx::menu_close);
    engine.register_fn("draw_dialogue", ScriptCtx::draw_dialogue);
    engine.register_fn("dialogue_advance", ScriptCtx::dialogue_advance);
    engine.register_fn("dialogue_open", ScriptCtx::dialogue_open);
    engine.register_fn("dialogue_done", ScriptCtx::dialogue_done);
    engine.register_fn("close_dialogue", ScriptCtx::close_dialogue);
    engine.register_fn("wrap_text", ScriptCtx::wrap_text);
}
