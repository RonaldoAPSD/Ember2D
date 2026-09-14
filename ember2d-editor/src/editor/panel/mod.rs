// editor/panel.rs — Floating + dockable panel system for the level editor.
//
// Panels can be dragged freely, docked to a screen edge, or resized.
// Docking: drag a panel within `metrics.dock_threshold_x/y` points of a
// screen edge to snap it. Undocking: dragging a docked panel's title bar
// floats it again.
// Resize: the [~] handle in each panel's bottom-right corner; for docked panels
//   the inner edge is the resize target (right edge for Left-docked, etc.).
//
// PIXEL MIGRATION (Phase 7 Part 1c): `Panel` used to store its geometry as
// `x/y: i32`, `w/h: usize` character-cell coordinates. It now stores a
// single `rect: UiRect` in pixels (`ui::rect`, Part 1b).
//
// POINTS MIGRATION (7D-3, docs/ember2d-master-plan.md §5.4): every panel's
// own outer chrome (position, size, title bar/close button/resize grip) now
// sizes itself from the active theme's `ChromeMetrics` (`ui/metrics.rs`),
// not the engine's fixed `CELL_W`/`CELL_H` glyph cell — a theme with a
// taller `row_h` gets a taller title/status/menu bar and a bigger close
// button/resize grip to match. `PanelManager` itself stores no `Theme`
// reference; every method that needs sizing takes a freshly-built
// `&ChromeMetrics` from the caller (`impl_render.rs` rebuilds one from
// `self.theme` every frame), so a theme switch takes effect on the very
// next layout pass with no separate "PanelManager doesn't know the theme
// changed" bookkeeping.
//
// `cell_x`/`cell_y`/`cell_w`/`cell_h`/`content_x`/`content_y`/`content_w`/
// `content_h` (below) are the bridge to the genuinely cell-grid subsystems
// that stay on the engine's fixed `CELL_W`/`CELL_H` forever regardless of
// this step (the node graph's cell-addressed hit-testing, the canvas/
// viewport itself — 7C-9 decision gate, §7.1). The docked script editor
// panel is this bridge's last real consumer as of this step (7D-3, §5.4) —
// its own commit replaces `content_x()`/`content_y()` there with a real
// points-space `ScriptLayout`, at which point this bridge exists only for
// callers with genuinely no pixel-space alternative. Since a panel's
// position is no longer necessarily a whole-cell multiple (a theme's
// `row_h` rarely divides `CELL_H` evenly), `cell_x`/`cell_y` now ROUND to
// the nearest cell rather than reading an always-exact value the way they
// did pre-7D-3 — see each's own doc comment.
//
// HIT-TESTING (Phase 7 Part 1d, docs/ember2d-phase7-plan.md): panel chrome
// and tabs no longer have their own `on_title_bar`/`on_close_btn`/
// `on_resize_handle`/`tab_at` methods — those were a second, independent
// computation of "where is this thing" that could (and did — the close
// button was a real, live instance) drift from where `draw_panel_chrome`/
// `draw_dock_tabs` actually drew it. `ui::UiFrame` (`ui/frame.rs`) replaces
// all four: `draw_panel_chrome`/`draw_dock_tabs` register each widget's hit
// rect at the exact point they draw it, and callers query
// `UiFrame::hit(px, py)` instead. `Panel::contains`/`PanelManager::panel_at`/
// `is_point_on_panel` are the one exception, kept as direct methods — see
// `Panel::contains`'s own doc comment for why those were never prone to
// this class of bug to begin with.

use super::ui::{ChromeMetrics, UiRect};
pub use super::ui::{DockSide, PanelId};

mod chrome;
pub use chrome::draw_panel_chrome;

// ── Panel ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Panel {
    pub id: PanelId,
    pub title: &'static str,
    pub rect: UiRect,
    pub visible: bool,
    pub z: usize,
    pub dock: DockSide,
    drag_offset: Option<(f32, f32)>,
    resize_anchor: Option<(f32, f32, f32, f32)>, // (mx, my, orig_w, orig_h), all points
}

impl Panel {
    /// `x`/`y`/`w`/`h` are points directly (7D-3, docs/ember2d-master-plan.md
    /// §5.4 — was cell coordinates multiplied out by `CELL_W`/`CELL_H`
    /// here; every caller now computes real point positions itself, using
    /// `ChromeMetrics`).
    fn new(id: PanelId, title: &'static str, x: f32, y: f32, w: f32, h: f32) -> Self {
        Panel {
            id,
            title,
            rect: UiRect::new(x, y, w, h),
            visible: false,
            z: 0,
            dock: DockSide::None,
            drag_offset: None,
            resize_anchor: None,
        }
    }

    /// This panel's own position/size in whole ENGINE CELLS (`CELL_W`/
    /// `CELL_H`) — the bridge back to the cell-based drawing/hit-testing
    /// code named in this file's header comment. Since panel positions are
    /// no longer necessarily whole-cell multiples (a theme's `row_h` rarely
    /// divides `CELL_H` evenly), this ROUNDS to the nearest cell — exact
    /// only incidentally now, not by construction the way it was pre-7D-3.
    /// Nothing that still calls this needs sub-cell precision (the node
    /// graph and, until its own points conversion lands, the docked script
    /// editor's OUTER frame position, not per-character content) — see
    /// `content_rect()` for the pixel-exact form real chrome content
    /// should use instead.
    pub fn cell_x(&self) -> i32 {
        (self.rect.x / ember2d::renderer::CELL_W as f32).round() as i32
    }
    pub fn cell_y(&self) -> i32 {
        (self.rect.y / ember2d::renderer::CELL_H as f32).round() as i32
    }
    pub fn cell_w(&self) -> usize {
        (self.rect.w / ember2d::renderer::CELL_W as f32).round() as usize
    }
    pub fn cell_h(&self) -> usize {
        (self.rect.h / ember2d::renderer::CELL_H as f32).round() as usize
    }

    /// Leftmost content column (accounts for left border).
    pub fn content_x(&self) -> usize {
        (self.cell_x() + 1).max(0) as usize
    }

    /// First content row (accounts for top border/title bar).
    pub fn content_y(&self) -> usize {
        (self.cell_y() + 1).max(0) as usize
    }

    /// Width of content area (accounts for both side borders).
    pub fn content_w(&self) -> usize {
        self.cell_w().saturating_sub(2)
    }

    /// Height of content area (accounts for top and bottom borders).
    pub fn content_h(&self) -> usize {
        self.cell_h().saturating_sub(2)
    }

    /// This panel's content area (inside its border/title bar), in points —
    /// the points-native counterpart to `content_x`/`content_y`/`content_w`/
    /// `content_h` above (7C-3, master plan §5.3, E4; point-ized 7D-3, §5.4).
    /// Exact, not a cell-rounded re-derivation: unlike those four (which
    /// exist for callers still working in cells), this reads `self.rect`
    /// directly, so it can never disagree with what `draw_panel_chrome`
    /// actually drew. `inset` is `metrics.border` on the sides,
    /// `metrics.bar_h` on top (the title bar) and bottom (kept symmetric
    /// with the top for a simple, even frame — the old cell-based version's
    /// "one `CELL_H` top/bottom" was itself just a border choice, not a
    /// constraint this type enforces).
    pub fn content_rect(&self, metrics: &ChromeMetrics) -> UiRect {
        UiRect::new(
            self.rect.x + metrics.border,
            self.rect.y + metrics.bar_h,
            (self.rect.w - 2.0 * metrics.border).max(0.0),
            (self.rect.h - 2.0 * metrics.bar_h).max(0.0),
        )
    }

    /// True if the pixel point `(px, py)` falls within this panel's rect.
    /// NOT part of the `UiFrame` migration (Phase 7 Part 1d,
    /// docs/ember2d-phase7-plan.md): unlike the title bar/close button/
    /// resize handle/tabs (each drawn by one function and, before this
    /// phase, hit-tested by a separate one — defect E5), a panel's overall
    /// bounds have always had exactly one source of truth, `self.rect`,
    /// which is also what `draw_panel_chrome` draws its background from.
    /// There was never a second computation for this one to drift from.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        self.rect.contains(px, py)
    }
}

// ── PanelManager ──────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct PanelManager {
    panels: Vec<Panel>,
    next_z: usize,
    dragging: Option<PanelId>,
    resizing: Option<PanelId>,
    /// The window's own point size, as last given to `apply_layout` — 7C-3
    /// (master plan §5.3, E4) gives this a home here instead of the deleted
    /// `Layout::screen_w`/`screen_h`, since `PanelManager` already receives
    /// it every frame to reposition docked panels; storing it makes this
    /// the one place both panel geometry AND overall screen size live,
    /// rather than two independently-updated copies of the same number.
    screen_size_pt: (f32, f32),
    pub active_left: Option<PanelId>,
    pub active_right: Option<PanelId>,
    pub active_bottom: Option<PanelId>,
}

impl PanelManager {
    /// `screen_w`/`screen_h` are points (7D-3, docs/ember2d-master-plan.md
    /// §5.4 — was cell counts, multiplied out internally by `CELL_W`/
    /// `CELL_H`; every caller now passes the real point size directly).
    pub fn new(screen_w: f32, screen_h: f32, metrics: &ChromeMetrics) -> Self {
        let canvas_y = metrics.chrome_top();
        // Same floor the old cell-based version enforced (`.max(4)` cells,
        // which — at the engine's fixed `CELL_H` — is exactly `metrics.min_h`
        // (both `64.0`) by construction, not a coincidence this rewrite
        // introduced.
        let canvas_h = (screen_h - metrics.chrome_top() - metrics.bar_h).max(metrics.min_h);

        let insp_x = (screen_w - metrics.insp_w).max(0.0);
        let pal_x = (insp_x - metrics.pal_w - metrics.padding * 2.0).max(0.0);
        let con_y = screen_h - metrics.con_h - metrics.bar_h;

        let mut panels = vec![
            Panel::new(PanelId::Viewport, "Viewport", 0.0, canvas_y, screen_w, canvas_h),
            Panel::new(PanelId::Hierarchy, "Hierarchy", 0.0, canvas_y, metrics.hier_w, canvas_h),
            Panel::new(PanelId::Inspector, "Inspector", insp_x, canvas_y, metrics.insp_w, canvas_h),
            Panel::new(PanelId::Palette, "Palette", pal_x, canvas_y, metrics.pal_w, canvas_h),
            Panel::new(PanelId::Console, "Console", 0.0, con_y, screen_w, metrics.con_h),
            Panel::new(PanelId::Stats, "Stats", pal_x, canvas_y, metrics.pal_w, canvas_h),
            Panel::new(PanelId::FileBrowser, "Files", 0.0, con_y, screen_w, metrics.con_h),
            Panel::new(
                PanelId::ScriptEditor,
                "Script Editor",
                0.0,
                con_y,
                screen_w,
                metrics.edit_h,
            ),
        ];

        panels[0].visible = true; // Viewport
        panels[0].z = 0;

        panels[1].dock = DockSide::Left;
        panels[1].visible = true; // Hierarchy

        panels[2].dock = DockSide::Right;
        panels[2].visible = true; // Inspector

        panels[3].visible = false; // Palette (floating, hidden by default)

        // 7D layout default: Unity-style — Hierarchy left, Inspector
        // right, Console and Files tabbed together at the bottom, both
        // visible out of the box (was: both hidden, and Files docked
        // Left where it would have fought with Hierarchy for the same
        // side). Console stays the initially active bottom tab.
        panels[4].dock = DockSide::Bottom;
        panels[4].visible = true; // Console

        panels[6].dock = DockSide::Bottom;
        panels[6].visible = true; // FileBrowser

        panels[7].dock = DockSide::Bottom;
        panels[7].visible = false; // ScriptEditor

        for (i, p) in panels.iter_mut().enumerate() {
            if p.id != PanelId::Viewport {
                p.z = i + 10;
            }
        }

        PanelManager {
            panels,
            next_z: 20,
            dragging: None,
            resizing: None,
            screen_size_pt: (screen_w, screen_h),
            active_left: Some(PanelId::Hierarchy),
            active_right: Some(PanelId::Inspector),
            active_bottom: Some(PanelId::Console),
        }
    }

    fn idx(&self, id: PanelId) -> usize {
        self.panels
            .iter()
            .position(|p| p.id == id)
            .unwrap_or_else(|| panic!("PanelManager: unknown PanelId: {:?}", id))
    }

    pub fn get(&self, id: PanelId) -> &Panel {
        &self.panels[self.idx(id)]
    }

    /// The Viewport panel — 7C-3 (master plan §5.3, E4): the single source
    /// of truth `mouse_to_grid` and every canvas `ui::draw_*` function now
    /// read, replacing the deleted `Layout.canvas_x`/`canvas_y`/`canvas_w`/
    /// `canvas_h`, which used to rebuild an independent (cell-based) copy
    /// of this same panel's rect every frame.
    pub fn viewport(&self) -> &Panel {
        self.get(PanelId::Viewport)
    }

    /// The window's own point size, as of the last `apply_layout` call —
    /// see `screen_size_pt`'s own doc comment for why this lives here now.
    pub fn screen_size_pt(&self) -> (f32, f32) {
        self.screen_size_pt
    }

    pub fn get_mut(&mut self, id: PanelId) -> &mut Panel {
        let i = self.idx(id);
        &mut self.panels[i]
    }

    pub fn visible(&self, id: PanelId) -> bool {
        self.get(id).visible
    }

    pub fn show(&mut self, id: PanelId) {
        let i = self.idx(id);
        self.panels[i].visible = true;
        let dock = self.panels[i].dock;
        match dock {
            DockSide::Left => self.active_left = Some(id),
            DockSide::Right => self.active_right = Some(id),
            DockSide::Bottom => self.active_bottom = Some(id),
            DockSide::None => {}
        }
        self.bring_to_front(id);
    }

    pub fn set_active(&mut self, id: PanelId) {
        let i = self.idx(id);
        if !self.panels[i].visible {
            return;
        }
        match self.panels[i].dock {
            DockSide::Left => self.active_left = Some(id),
            DockSide::Right => self.active_right = Some(id),
            DockSide::Bottom => self.active_bottom = Some(id),
            DockSide::None => {}
        }
    }

    pub fn hide(&mut self, id: PanelId) {
        let i = self.idx(id);
        self.panels[i].visible = false;
        if self.dragging == Some(id) {
            self.dragging = None;
        }
        if self.resizing == Some(id) {
            self.resizing = None;
        }
    }

    pub fn toggle(&mut self, id: PanelId) {
        if self.visible(id) {
            self.hide(id);
        } else {
            self.show(id);
        }
    }

    pub fn bring_to_front(&mut self, id: PanelId) {
        if id == PanelId::Viewport {
            return;
        }
        let z = self.next_z;
        self.next_z += 1;
        let i = self.idx(id);
        self.panels[i].z = z;
    }

    /// Panel IDs sorted lowest-z first (back-to-front draw order).
    pub fn in_draw_order(&self) -> Vec<PanelId> {
        let mut order: Vec<(usize, PanelId)> = self
            .panels
            .iter()
            .filter(|p| {
                if !p.visible {
                    return false;
                }
                match p.dock {
                    DockSide::Left => self.active_left == Some(p.id),
                    DockSide::Right => self.active_right == Some(p.id),
                    DockSide::Bottom => self.active_bottom == Some(p.id),
                    DockSide::None => true,
                }
            })
            .map(|p| (p.z, p.id))
            .collect();
        order.sort_by_key(|(z, _)| *z);
        order.into_iter().map(|(_, id)| id).collect()
    }

    /// Topmost visible panel that contains pixel `(px, py)`. Also used for
    /// general focus tracking — not migrated to `UiFrame` (Part 1d); see
    /// `Panel::contains`'s own doc comment for why this one was never
    /// prone to E5-style drift in the first place.
    pub fn panel_at(&self, px: f32, py: f32) -> Option<PanelId> {
        self.panels
            .iter()
            .filter(|p| p.visible && p.contains(px, py))
            .max_by_key(|p| p.z)
            .map(|p| p.id)
    }

    pub fn is_point_on_panel(&self, px: f32, py: f32) -> bool {
        self.panels.iter().any(|p| p.visible && p.id != PanelId::Viewport && p.contains(px, py))
    }

    pub fn get_docked_panels(&self, side: DockSide) -> Vec<PanelId> {
        self.panels.iter().filter(|p| p.visible && p.dock == side).map(|p| p.id).collect()
    }

    // Tab hit-testing (`tab_at`/`find_tab_in_row`) removed in Phase 7 Part 1d
    // (docs/ember2d-phase7-plan.md) — replaced by `UiFrame::hit` reading
    // `WidgetId::Tab` entries `ui::draw_dock_tabs` now pushes at the exact
    // cell span it draws each tab label at. See `ui/frame.rs`'s header
    // comment for why this closes defect E5, including a real quirk the
    // old independent hit-test had that this fix also resolves: a
    // single-panel dock (nothing else sharing its side, so `draw_dock_tabs`
    // is never even called for it — see `impl_render.rs`'s `docked.len() >
    // 1` guard) used to still claim an invisible tab hitbox over its own
    // title row's first `title.len()+2` cells, intercepting what should
    // have been a title-bar-drag click.

    // ── Drag ──────────────────────────────────────────────────────────────────

    /// Begin dragging. Undocks the panel so it floats freely. `mouse_x`/
    /// `mouse_y` are points.
    pub fn start_drag(&mut self, id: PanelId, mouse_x: f32, mouse_y: f32, metrics: &ChromeMetrics) {
        let i = self.idx(id);
        if self.panels[i].dock != DockSide::None {
            // Restore to a sensible floating size when undocking.
            let w = self.panels[i].rect.w.min(metrics.undock_max_w).max(metrics.min_w);
            let h = self.panels[i].rect.h.min(metrics.undock_max_h).max(metrics.min_h);
            self.panels[i].rect.w = w;
            self.panels[i].rect.h = h;
        }
        self.panels[i].dock = DockSide::None; // undock on drag start
        self.panels[i].drag_offset =
            Some((mouse_x - self.panels[i].rect.x, mouse_y - self.panels[i].rect.y));
        self.dragging = Some(id);
        self.bring_to_front(id);
    }

    /// `mouse_x`/`mouse_y`/`screen_w`/`screen_h` are all points.
    pub fn update_drag(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        screen_w: f32,
        screen_h: f32,
        metrics: &ChromeMetrics,
    ) {
        let Some(id) = self.dragging else { return };
        let i = self.idx(id);
        let Some((ox, oy)) = self.panels[i].drag_offset else { return };
        let new_x = (mouse_x - ox).max(0.0).min(screen_w - self.panels[i].rect.w);
        let new_y = (mouse_y - oy).max(metrics.chrome_top()).min(screen_h - metrics.chrome_top());
        self.panels[i].rect.x = new_x;
        self.panels[i].rect.y = new_y;
    }

    /// End drag and snap to an edge if within `metrics.dock_threshold_x/y`.
    /// `screen_w`/`screen_h` are points.
    pub fn end_drag(&mut self, screen_w: f32, screen_h: f32, metrics: &ChromeMetrics) {
        let Some(id) = self.dragging.take() else { return };
        let i = self.idx(id);
        self.panels[i].drag_offset = None;

        let p = &self.panels[i];
        let x = p.rect.x;
        let y = p.rect.y;
        let pw = p.rect.w;
        let ph = p.rect.h;

        let new_dock = if x <= metrics.dock_threshold_x {
            DockSide::Left
        } else if x + pw >= screen_w - metrics.dock_threshold_x {
            DockSide::Right
        } else if y + ph >= screen_h - metrics.dock_threshold_y - metrics.bar_h {
            DockSide::Bottom
        } else {
            DockSide::None
        };
        self.panels[i].dock = new_dock;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }

    // ── Resize ────────────────────────────────────────────────────────────────

    pub fn start_resize(&mut self, id: PanelId, mx: f32, my: f32) {
        let i = self.idx(id);
        let (w, h) = (self.panels[i].rect.w, self.panels[i].rect.h);
        self.panels[i].resize_anchor = Some((mx, my, w, h));
        self.resizing = Some(id);
        self.bring_to_front(id);
    }

    pub fn update_resize(&mut self, mx: f32, my: f32, metrics: &ChromeMetrics) {
        let Some(id) = self.resizing else { return };
        let i = self.idx(id);
        let Some((ax, ay, ow, oh)) = self.panels[i].resize_anchor else { return };
        let dx = mx - ax;
        let dy = my - ay;
        match self.panels[i].dock {
            DockSide::Left => {
                self.panels[i].rect.w = (ow + dx).max(metrics.min_w);
            }
            DockSide::Right => {
                // Right edge fixed: grow left → x decreases, w increases
                let new_w = (ow - dx).max(metrics.min_w);
                let orig_right = self.panels[i].rect.x + ow;
                self.panels[i].rect.w = new_w;
                self.panels[i].rect.x = orig_right - new_w;
            }
            DockSide::Bottom => {
                // Bottom edge fixed: grow up → y decreases, h increases
                // Constrain: cannot resize above the canvas top.
                let mut new_h = (oh - dy).max(metrics.min_h);
                let orig_bottom = self.panels[i].rect.y + oh;
                let potential_y = orig_bottom - new_h;
                if potential_y < metrics.chrome_top() {
                    new_h = orig_bottom - metrics.chrome_top();
                }
                self.panels[i].rect.h = new_h;
                self.panels[i].rect.y = orig_bottom - new_h;
            }
            DockSide::None => {
                self.panels[i].rect.w = (ow + dx).max(metrics.min_w);
                // For floating panels, the anchor is top-left, so resizing
                // doesn't move Y.
                self.panels[i].rect.h = (oh + dy).max(metrics.min_h);
            }
        }
    }

    pub fn end_resize(&mut self) {
        if let Some(id) = self.resizing.take() {
            let i = self.idx(id);
            self.panels[i].resize_anchor = None;
        }
    }

    pub fn is_resizing(&self) -> bool {
        self.resizing.is_some()
    }

    // ── Layout ────────────────────────────────────────────────────────────────

    /// Reposition docked panels to fill their edge. Call every render frame
    /// before drawing so positions are always current. `screen_w`/
    /// `screen_h` are points.
    pub fn apply_layout(&mut self, screen_w: f32, screen_h: f32, metrics: &ChromeMetrics) {
        self.screen_size_pt = (screen_w, screen_h);

        // Status bar takes the last row; toolbar + title take the first
        // two rows; canvas starts below them — all sized from the active
        // theme's own `bar_h` now (7D-3, docs/ember2d-master-plan.md §5.4),
        // not the engine's fixed `CELL_H`.
        let canvas_top = metrics.chrome_top();
        let canvas_bottom = metrics.chrome_bottom(screen_h).max(0.0);
        let full_h = (canvas_bottom - canvas_top).max(0.0);

        // Ensure active panel markers are valid
        self.validate_active_panels();

        let left_w = self.active_left.map(|id| self.get(id).rect.w).unwrap_or(0.0);
        let right_w = self.active_right.map(|id| self.get(id).rect.w).unwrap_or(0.0);
        let bottom_h = self.active_bottom.map(|id| self.get(id).rect.h).unwrap_or(0.0);

        let right_x = (screen_w - right_w).max(0.0);
        let bottom_y = (canvas_bottom - bottom_h).max(canvas_top);

        for p in &mut self.panels {
            if !p.visible {
                continue;
            }
            match p.id {
                // Unconditional every frame, regardless of anything a drag
                // in progress did to `p.rect`/`p.dock` this frame (7C-3,
                // master plan §5.3: `handle_panel_chrome_click` no longer
                // excludes the Viewport's title bar from `start_drag`, so
                // it CAN be picked up and dragged like any other panel) —
                // this is what keeps the Viewport "master-fill" a real
                // guarantee rather than just the common case: whatever a
                // drag momentarily did, the very next frame always wins.
                PanelId::Viewport => {
                    p.rect.x = left_w;
                    p.rect.y = canvas_top;
                    p.rect.w = (screen_w - left_w - right_w).max(0.0);
                    p.rect.h = (canvas_bottom - canvas_top - bottom_h).max(0.0);
                    p.dock = DockSide::None;
                }
                _ => match p.dock {
                    DockSide::Left => {
                        p.rect.x = 0.0;
                        p.rect.y = canvas_top;
                        p.rect.w = left_w;
                        p.rect.h = full_h;
                    }
                    DockSide::Right => {
                        p.rect.x = right_x;
                        p.rect.y = canvas_top;
                        p.rect.w = right_w;
                        p.rect.h = full_h;
                    }
                    DockSide::Bottom => {
                        p.rect.x = left_w;
                        p.rect.y = bottom_y;
                        p.rect.w = (screen_w - left_w - right_w).max(0.0);
                        p.rect.h = bottom_h;
                    }
                    // 7D-3 checkpoint 7 (master plan §5.4): a floating
                    // panel's own `rect.x/y` is never touched by anything
                    // above (unlike a docked one, rebuilt from scratch every
                    // frame) — `input/context_menu.rs`'s `FloatPanel` action
                    // places it at a fixed points offset, and dragging
                    // leaves it wherever the user let go. Neither ever
                    // accounted for the window's own points BUDGET shrinking
                    // out from under it when `ui_scale` grows (a bigger
                    // `ui_scale` means fewer points fit in the same
                    // window) — a panel floated at a fixed offset could end
                    // up with its own title bar entirely off-screen, with no
                    // way to drag it back since that title bar is what a
                    // drag grabs in the first place. Clamped back into
                    // `[0, screen_w] x [canvas_top, canvas_bottom]` every
                    // frame, the same way a real OS window manager keeps a
                    // dragged window's title bar reachable.
                    DockSide::None => {
                        let max_x = (screen_w - p.rect.w).max(0.0);
                        let max_y = (canvas_bottom - p.rect.h).max(canvas_top);
                        p.rect.x = p.rect.x.clamp(0.0, max_x);
                        p.rect.y = p.rect.y.clamp(canvas_top, max_y);
                    }
                },
            }
        }
    }

    fn validate_active_panels(&mut self) {
        // If an active panel is no longer visible or no longer docked to that side, clear it.
        if let Some(id) = self.active_left {
            let p = self.get(id);
            if !p.visible || p.dock != DockSide::Left {
                self.active_left = None;
            }
        }
        if let Some(id) = self.active_right {
            let p = self.get(id);
            if !p.visible || p.dock != DockSide::Right {
                self.active_right = None;
            }
        }
        if let Some(id) = self.active_bottom {
            let p = self.get(id);
            if !p.visible || p.dock != DockSide::Bottom {
                self.active_bottom = None;
            }
        }

        // If a side has visible docked panels but no active one, pick the first.
        if self.active_left.is_none() {
            self.active_left =
                self.panels.iter().find(|p| p.visible && p.dock == DockSide::Left).map(|p| p.id);
        }
        if self.active_right.is_none() {
            self.active_right =
                self.panels.iter().find(|p| p.visible && p.dock == DockSide::Right).map(|p| p.id);
        }
        if self.active_bottom.is_none() {
            self.active_bottom =
                self.panels.iter().find(|p| p.visible && p.dock == DockSide::Bottom).map(|p| p.id);
        }
    }
}

// tests.rs: split out in Part 1f to stay under the 600-line limit.
#[cfg(test)]
mod tests;
