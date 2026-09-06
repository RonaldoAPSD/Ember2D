// editor/ui/frame.rs — UiFrame: one pass produces both drawing and
// hit-testing (Phase 7 Part 1d, docs/ember2d-phase7-plan.md).
//
// THE BUG THIS STRUCTURALLY CLOSES (defect E5, docs/ember2d-refactor-plan.md):
// before this, a panel's title bar, close button, and resize handle were
// each drawn by one piece of code (`draw_panel_chrome`) and hit-tested by a
// SEPARATE piece of code (`Panel::on_title_bar`/`on_close_btn`/
// `on_resize_handle`) that recomputed the same rect from scratch, by
// convention, with no compiler-enforced link between the two. The close
// button was a real, live instance of exactly this: `draw_panel_chrome`
// drew `"[X]"` at one cell offset, while `on_close_btn`'s hitbox covered a
// DIFFERENT (one-cell-shifted) range — a bug that could only be found by
// reading both functions side by side, not by anything the type system
// could catch. Every one of the old bugs in `Issues.txt` was this same
// class: a click landing somewhere other than where the thing was drawn.
//
// THE FIX: a widget's rect is registered at the exact point it's drawn —
// `UiFrame::push` takes literally the same `UiRect` value the draw call
// just used, not a recomputed copy. There is only one rect. A widget can
// no longer drift out of sync with its own hitbox, because there's nothing
// left to drift apart from.
//
// THE ONE-FRAME LAG, AND WHY IT'S FINE: `Engine::run()`'s per-frame order
// is `update()` (reads input) then `render()` (draws, and — after this
// step — populates `UiFrame`). That means an `update()` call always reads
// the `UiFrame` `render()` populated on the PREVIOUS frame, not the one
// about to be drawn this frame. This is harmless here: panel geometry only
// changes in response to input in the first place (a drag, a resize, a
// dock-side reflow), so "this frame's hit-test reflects last frame's
// layout" is indistinguishable from "this frame's hit-test reflects this
// frame's layout" to a human clicking at 60fps — the layout was already
// stable by the time a frame that could observe a change even renders.
//
// MIGRATION ORDER (this phase's own plan, Part 1d): panel chrome and tabs
// first (this step — E5's actual site), then the toolbar, then palette
// entries, then inspector rows, each independently verifiable by clicking
// things. `WidgetId` grows one variant at a time as each of those lands.

use super::rect::UiRect;
use super::types::{MenuKind, PanelId};

/// Identifies one interactive chrome element a frame's draw pass registered
/// a hit rect for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WidgetId {
    TitleBar(PanelId),
    CloseBtn(PanelId),
    ResizeHandle(PanelId),
    Tab(PanelId),
    /// One of the top menu bar's labels ("File", "Edit", …).
    MenuLabel(MenuKind),
    /// One row of an open dropdown menu, by index into
    /// `menu_entries(kind)` — not the resolved `ToolbarAction` itself
    /// (which isn't cheaply `Eq`/`Hash`), so the caller re-resolves it via
    /// the same `menu_entries` list the draw call used, exactly as the
    /// removed `menu_item_at` did internally.
    MenuItem(MenuKind, usize),
    /// The palette panel's search field.
    PaletteSearchBar,
    /// The palette's "[+ New]" button.
    PaletteNewBtn,
    /// The palette's "[ Edit ]" button.
    PaletteEditBtn,
    /// One row (header or item) of the palette list, by index into
    /// `TilePalette::build_layout()` — same "index, not the resolved value"
    /// reasoning as `MenuItem`.
    PaletteRow(usize),
    /// One editable field in the Inspector panel. The same field set backs
    /// both the Player-editing and tile-editing modes (they share one row
    /// layout, the `INSP_*_OFF` constants in `ui/types.rs`) — the caller
    /// (`input/panels.rs`) decides which underlying data a hit mutates from
    /// `self.hierarchy_sel`, exactly as the removed row-position `match`
    /// already did. `Exit`/`GraphBtn` are meaningful only in tile mode
    /// (there is no player equivalent), same as before this migration.
    InspectorRow(InspectorField),
}

/// See `WidgetId::InspectorRow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InspectorField {
    Glyph, Tag, Solid, Trigger, CameraFollow, Script, Exit, Layer, Mask, GraphBtn,
}

pub struct UiHit {
    pub id: WidgetId,
    pub rect: UiRect,
}

/// One frame's worth of (widget, rect) pairs, in draw order. Cleared once
/// at the start of every `render()` call, then rebuilt as that pass draws;
/// queried by the FOLLOWING frame's `update()` call — see this module's
/// header comment for why that one-frame lag doesn't matter here.
#[derive(Default)]
pub struct UiFrame {
    hits: Vec<UiHit>,
}

impl UiFrame {
    pub fn new() -> Self {
        UiFrame { hits: Vec::new() }
    }

    /// Call once at the very start of a render pass, before any drawing.
    pub fn clear(&mut self) {
        self.hits.clear();
    }

    /// Register `rect` as `id`'s hit target. Call this at the exact point
    /// something is drawn, passing the exact rect the draw call just used
    /// — never a value recomputed independently. That discipline is the
    /// entire point of this type; see this module's header comment.
    pub fn push(&mut self, id: WidgetId, rect: UiRect) {
        self.hits.push(UiHit { id, rect });
    }

    /// The topmost widget at pixel `(px, py)`, or `None`. Hits are pushed
    /// in draw order (back-to-front — panels are drawn via
    /// `PanelManager::in_draw_order`, lowest z first, and a panel's own
    /// sub-widgets are pushed in the order `draw_panel_chrome` draws them),
    /// so the LAST push containing the point is the topmost. This is also
    /// how a more specific widget nested inside a broader one (the close
    /// button sitting inside the title bar's own row) wins: as long as the
    /// specific one is pushed after the broader one, it's found first here.
    pub fn hit(&self, px: f32, py: f32) -> Option<WidgetId> {
        self.hits.iter().rev().find(|h| h.rect.contains(px, py)).map(|h| h.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_returns_none_on_an_empty_frame() {
        let frame = UiFrame::new();
        assert_eq!(frame.hit(10.0, 10.0), None);
    }

    #[test]
    fn hit_finds_a_widget_whose_rect_contains_the_point() {
        let mut frame = UiFrame::new();
        frame.push(WidgetId::TitleBar(PanelId::Inspector), UiRect::new(0.0, 0.0, 100.0, 20.0));
        assert_eq!(frame.hit(50.0, 10.0), Some(WidgetId::TitleBar(PanelId::Inspector)));
        assert_eq!(frame.hit(150.0, 10.0), None, "outside every pushed rect must miss");
    }

    #[test]
    fn hit_returns_the_topmost_last_pushed_widget_when_rects_overlap() {
        // Exactly the close-button-inside-title-bar case: a later, more
        // specific push must win over an earlier, broader one that also
        // contains the same point.
        let mut frame = UiFrame::new();
        frame.push(WidgetId::TitleBar(PanelId::Inspector), UiRect::new(0.0, 0.0, 100.0, 20.0));
        frame.push(WidgetId::CloseBtn(PanelId::Inspector), UiRect::new(80.0, 0.0, 20.0, 20.0));
        assert_eq!(frame.hit(90.0, 10.0), Some(WidgetId::CloseBtn(PanelId::Inspector)), "the more specific, later-pushed close button must win over the broader title bar underneath it");
        assert_eq!(frame.hit(10.0, 10.0), Some(WidgetId::TitleBar(PanelId::Inspector)), "outside the close button's rect, the title bar still wins");
    }

    #[test]
    fn hit_prefers_a_later_pushed_panel_over_an_earlier_one_at_the_same_point() {
        // Two unrelated, fully overlapping panels — the one drawn later
        // (higher z, per PanelManager::in_draw_order) must win.
        let mut frame = UiFrame::new();
        frame.push(WidgetId::TitleBar(PanelId::Console), UiRect::new(0.0, 0.0, 50.0, 20.0));
        frame.push(WidgetId::TitleBar(PanelId::Stats), UiRect::new(0.0, 0.0, 50.0, 20.0));
        assert_eq!(frame.hit(10.0, 10.0), Some(WidgetId::TitleBar(PanelId::Stats)));
    }

    #[test]
    fn clear_removes_every_previously_pushed_hit() {
        let mut frame = UiFrame::new();
        frame.push(WidgetId::ResizeHandle(PanelId::Console), UiRect::new(0.0, 0.0, 10.0, 10.0));
        frame.clear();
        assert_eq!(frame.hit(5.0, 5.0), None, "a cleared frame must not remember last frame's hits");
    }
}
