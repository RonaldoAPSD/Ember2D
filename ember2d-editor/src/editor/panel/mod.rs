// editor/panel.rs — Floating + dockable panel system for the level editor.
//
// Panels can be dragged freely, docked to a screen edge, or resized.
// Docking: drag a panel within DOCK_THRESHOLD_X/Y pixels of an edge to snap
// it (Phase 7 Part 1c, docs/ember2d-phase7-plan.md — was cells before this).
// Undocking: dragging a docked panel's title bar floats it again.
// Resize: the [~] handle in each panel's bottom-right corner; for docked panels
//   the inner edge is the resize target (right edge for Left-docked, etc.).
//
// PIXEL MIGRATION (Phase 7 Part 1c): `Panel` used to store its geometry as
// `x/y: i32`, `w/h: usize` character-cell coordinates. It now stores a
// single `rect: UiRect` in pixels (`ui::rect`, Part 1b) — every panel's
// pixel rect is still a whole-cell multiple today (nothing here yet
// produces a sub-cell position), so this is purely a coordinate-system
// change with zero visual difference, exactly Part 1's stated goal.
// `cell_x`/`cell_y`/`cell_w`/`cell_h` are the bridge back to the
// still-cell-based DRAWING code (`draw_panel_chrome`, `draw_dock_tabs`)
// that hasn't migrated yet (that's Part 4's job — actual pixel-drawn
// chrome). The bridge goes away entirely once Part 4's restyle removes
// `UiRect::from_cells`.
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

use ember2d::renderer::{color::Color, Renderer};
pub use super::ui::{DockSide, PanelId};
use super::ui::{UiRect, UiFrame, WidgetId};

// ── Panel ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Panel {
    pub id:      PanelId,
    pub title:   &'static str,
    pub rect:    UiRect,
    pub visible: bool,
    pub z:       usize,
    pub dock:    DockSide,
    drag_offset:   Option<(f32, f32)>,
    resize_anchor: Option<(f32, f32, f32, f32)>, // (mx, my, orig_w, orig_h), all pixels
}

/// Width/height of one character cell in pixels — same re-export of
/// `ember2d::renderer::CELL_W`/`CELL_H` as `editor/ui/rect.rs`'s `UiRect`
/// uses (Phase 7 Part 1e, docs/ember2d-phase7-plan.md, E2); this file used
/// to duplicate the two literals independently.
const CELL_W: f32 = ember2d::renderer::CELL_W as f32;
const CELL_H: f32 = ember2d::renderer::CELL_H as f32;

/// Pixel equivalents of the old `10`/`4`-CELL minimum panel size.
const MIN_W: f32 = 10.0 * CELL_W;
const MIN_H: f32 = 4.0 * CELL_H;

impl Panel {
    fn new(id: PanelId, title: &'static str, cx: i32, cy: i32, cw: usize, ch: usize) -> Self {
        Panel {
            id, title,
            rect: UiRect::from_cells(cx, cy, cw, ch),
            visible: false, z: 0, dock: DockSide::None,
            drag_offset: None, resize_anchor: None,
        }
    }

    /// This panel's own position/size in whole CELLS — the bridge back to
    /// the cell-based drawing/hit-testing code named in this file's header
    /// comment. Exact (not just rounded-and-hoped) because `self.rect`'s
    /// pixel values are always whole-cell multiples as of Part 1c.
    pub fn cell_x(&self) -> i32 { (self.rect.x / CELL_W).round() as i32 }
    pub fn cell_y(&self) -> i32 { (self.rect.y / CELL_H).round() as i32 }
    pub fn cell_w(&self) -> usize { (self.rect.w / CELL_W).round() as usize }
    pub fn cell_h(&self) -> usize { (self.rect.h / CELL_H).round() as usize }

    /// Leftmost content column (accounts for left border).
    pub fn content_x(&self) -> usize { (self.cell_x() + 1).max(0) as usize }

    /// First content row (accounts for top border/title bar).
    pub fn content_y(&self) -> usize { (self.cell_y() + 1).max(0) as usize }

    /// Width of content area (accounts for both side borders).
    pub fn content_w(&self) -> usize { self.cell_w().saturating_sub(2) }

    /// Height of content area (accounts for top and bottom borders).
    pub fn content_h(&self) -> usize { self.cell_h().saturating_sub(2) }

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
    panels:   Vec<Panel>,
    next_z:   usize,
    dragging: Option<PanelId>,
    resizing: Option<PanelId>,
    pub active_left:   Option<PanelId>,
    pub active_right:  Option<PanelId>,
    pub active_bottom: Option<PanelId>,
}

pub const HIER_W: usize = 14;
pub const BROW_W: usize = 20;
pub const INSP_W: usize = 30;
pub const PAL_W:  usize = 24;
pub const CON_H:  usize = 9;
pub const EDIT_H: usize = 12;

/// How close (in pixels) a dragged panel's edge must land to a screen edge
/// to dock. Two constants, not one, because the original cell-based
/// version applied "3 cells" identically to horizontal cell-columns (8px
/// each) and vertical cell-rows (16px each) despite them being physically
/// different sizes — kept per-axis here so docking distance is bit-for-bit
/// unchanged, not just "3 of some new unified unit."
const DOCK_THRESHOLD_X: f32 = 3.0 * CELL_W; // 24px
const DOCK_THRESHOLD_Y: f32 = 3.0 * CELL_H; // 48px

impl PanelManager {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        let canvas_y = 2i32;
        let canvas_h = screen_h.saturating_sub(3).max(4) as i32;

        let insp_x = (screen_w as i32 - INSP_W as i32).max(0);
        let pal_x  = (insp_x - PAL_W as i32 - 2).max(0);
        let con_y  = screen_h as i32 - CON_H as i32 - 1;

        let mut panels = vec![
            Panel::new(PanelId::Viewport,      "Viewport",      0,       canvas_y, screen_w, canvas_h as usize),
            Panel::new(PanelId::Hierarchy,    "Hierarchy",     0,       canvas_y, HIER_W, canvas_h as usize),
            Panel::new(PanelId::Inspector,    "Inspector",     insp_x,  canvas_y, INSP_W, canvas_h as usize),
            Panel::new(PanelId::Palette,      "Palette",       pal_x,   canvas_y, PAL_W,  canvas_h as usize),
            Panel::new(PanelId::Console,      "Console",       0,       con_y,    screen_w, CON_H),
            Panel::new(PanelId::Stats,        "Stats",         pal_x,   canvas_y, PAL_W,  canvas_h as usize),
            Panel::new(PanelId::FileBrowser,  "Files",         0,       canvas_y, BROW_W, canvas_h as usize),
            Panel::new(PanelId::ScriptEditor, "Script Editor", 0,       con_y,    screen_w, EDIT_H),
        ];

        panels[0].visible = true; // Viewport
        panels[0].z       = 0;

        panels[1].dock    = DockSide::Left;
        panels[1].visible = true;  // Hierarchy

        panels[2].dock    = DockSide::Right;
        panels[2].visible = true;  // Inspector

        panels[3].visible = false; // Palette (floating, hidden by default)

        panels[4].dock    = DockSide::Bottom;  // Console (hidden by default)

        panels[6].dock    = DockSide::Left;
        panels[6].visible = false; // FileBrowser

        panels[7].dock    = DockSide::Bottom;
        panels[7].visible = false; // ScriptEditor

        for (i, p) in panels.iter_mut().enumerate() {
            if p.id != PanelId::Viewport { p.z = i + 10; }
        }

        PanelManager {
            panels, next_z: 20, dragging: None, resizing: None,
            active_left:   Some(PanelId::Hierarchy),
            active_right:  Some(PanelId::Inspector),
            active_bottom: Some(PanelId::Console),
        }
    }

    fn idx(&self, id: PanelId) -> usize {
        self.panels.iter().position(|p| p.id == id)
            .unwrap_or_else(|| panic!("PanelManager: unknown PanelId: {:?}", id))
    }

    pub fn get(&self, id: PanelId) -> &Panel { &self.panels[self.idx(id)] }

    pub fn get_mut(&mut self, id: PanelId) -> &mut Panel {
        let i = self.idx(id);
        &mut self.panels[i]
    }

    pub fn visible(&self, id: PanelId) -> bool { self.get(id).visible }

    pub fn show(&mut self, id: PanelId) {
        let i = self.idx(id);
        self.panels[i].visible = true;
        let dock = self.panels[i].dock;
        match dock {
            DockSide::Left   => self.active_left = Some(id),
            DockSide::Right  => self.active_right = Some(id),
            DockSide::Bottom => self.active_bottom = Some(id),
            DockSide::None   => {}
        }
        self.bring_to_front(id);
    }

    pub fn set_active(&mut self, id: PanelId) {
        let i = self.idx(id);
        if !self.panels[i].visible { return; }
        match self.panels[i].dock {
            DockSide::Left   => self.active_left = Some(id),
            DockSide::Right  => self.active_right = Some(id),
            DockSide::Bottom => self.active_bottom = Some(id),
            DockSide::None   => {}
        }
    }

    pub fn hide(&mut self, id: PanelId) {
        let i = self.idx(id);
        self.panels[i].visible = false;
        if self.dragging == Some(id) { self.dragging = None; }
        if self.resizing == Some(id) { self.resizing = None; }
    }

    pub fn toggle(&mut self, id: PanelId) {
        if self.visible(id) { self.hide(id); } else { self.show(id); }
    }

    pub fn bring_to_front(&mut self, id: PanelId) {
        if id == PanelId::Viewport { return; }
        let z = self.next_z;
        self.next_z += 1;
        let i = self.idx(id);
        self.panels[i].z = z;
    }

    /// Panel IDs sorted lowest-z first (back-to-front draw order).
    pub fn in_draw_order(&self) -> Vec<PanelId> {
        let mut order: Vec<(usize, PanelId)> = self.panels.iter()
            .filter(|p| {
                if !p.visible { return false; }
                match p.dock {
                    DockSide::Left   => self.active_left   == Some(p.id),
                    DockSide::Right  => self.active_right  == Some(p.id),
                    DockSide::Bottom => self.active_bottom == Some(p.id),
                    DockSide::None   => true,
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
        self.panels.iter()
            .filter(|p| p.visible && p.contains(px, py))
            .max_by_key(|p| p.z)
            .map(|p| p.id)
    }

    pub fn is_point_on_panel(&self, px: f32, py: f32) -> bool {
        self.panels.iter().any(|p| p.visible && p.id != PanelId::Viewport && p.contains(px, py))
    }

    pub fn get_docked_panels(&self, side: DockSide) -> Vec<PanelId> {
        self.panels.iter()
            .filter(|p| p.visible && p.dock == side)
            .map(|p| p.id)
            .collect()
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
    /// `mouse_y` are pixels (Phase 7 Part 1c — cells before this).
    pub fn start_drag(&mut self, id: PanelId, mouse_x: f32, mouse_y: f32) {
        let i = self.idx(id);
        if self.panels[i].dock != DockSide::None {
            // Restore to a sensible floating size when undocking — pixel
            // equivalents of the old 40×20-cell clamp.
            let w = self.panels[i].rect.w.min(40.0 * CELL_W).max(MIN_W);
            let h = self.panels[i].rect.h.min(20.0 * CELL_H).max(MIN_H);
            self.panels[i].rect.w = w;
            self.panels[i].rect.h = h;
        }
        self.panels[i].dock = DockSide::None;   // undock on drag start
        self.panels[i].drag_offset = Some((
            mouse_x - self.panels[i].rect.x,
            mouse_y - self.panels[i].rect.y,
        ));
        self.dragging = Some(id);
        self.bring_to_front(id);
    }

    /// `mouse_x`/`mouse_y`/`screen_w`/`screen_h` are all pixels (Part 1c).
    pub fn update_drag(&mut self, mouse_x: f32, mouse_y: f32, screen_w: f32, screen_h: f32) {
        let Some(id) = self.dragging else { return };
        let i = self.idx(id);
        let Some((ox, oy)) = self.panels[i].drag_offset else { return };
        let new_x = (mouse_x - ox).max(0.0).min(screen_w - self.panels[i].rect.w);
        let new_y = (mouse_y - oy).max(2.0 * CELL_H).min(screen_h - 2.0 * CELL_H);
        self.panels[i].rect.x = new_x;
        self.panels[i].rect.y = new_y;
    }

    /// End drag and snap to an edge if within `DOCK_THRESHOLD_X`/`_Y`.
    /// `screen_w`/`screen_h` are pixels (Part 1c).
    pub fn end_drag(&mut self, screen_w: f32, screen_h: f32) {
        let Some(id) = self.dragging.take() else { return };
        let i = self.idx(id);
        self.panels[i].drag_offset = None;

        let p = &self.panels[i];
        let x  = p.rect.x;
        let y  = p.rect.y;
        let pw = p.rect.w;
        let ph = p.rect.h;

        let new_dock = if x <= DOCK_THRESHOLD_X {
            DockSide::Left
        } else if x + pw >= screen_w - DOCK_THRESHOLD_X {
            DockSide::Right
        } else if y + ph >= screen_h - DOCK_THRESHOLD_Y - CELL_H {
            DockSide::Bottom
        } else {
            DockSide::None
        };
        self.panels[i].dock = new_dock;
    }

    pub fn is_dragging(&self) -> bool { self.dragging.is_some() }

    // ── Resize ────────────────────────────────────────────────────────────────

    pub fn start_resize(&mut self, id: PanelId, mx: f32, my: f32) {
        let i = self.idx(id);
        let (w, h) = (self.panels[i].rect.w, self.panels[i].rect.h);
        self.panels[i].resize_anchor = Some((mx, my, w, h));
        self.resizing = Some(id);
        self.bring_to_front(id);
    }

    pub fn update_resize(&mut self, mx: f32, my: f32) {
        let Some(id) = self.resizing else { return };
        let i = self.idx(id);
        let Some((ax, ay, ow, oh)) = self.panels[i].resize_anchor else { return };
        let dx = mx - ax;
        let dy = my - ay;
        match self.panels[i].dock {
            DockSide::Left => {
                self.panels[i].rect.w = (ow + dx).max(MIN_W);
            }
            DockSide::Right => {
                // Right edge fixed: grow left → x decreases, w increases
                let new_w = (ow - dx).max(MIN_W);
                let orig_right = self.panels[i].rect.x + ow;
                self.panels[i].rect.w = new_w;
                self.panels[i].rect.x = orig_right - new_w;
            }
            DockSide::Bottom => {
                // Bottom edge fixed: grow up → y decreases, h increases
                // Constrain: cannot resize above the canvas top (2 cells).
                let mut new_h = (oh - dy).max(MIN_H);
                let orig_bottom = self.panels[i].rect.y + oh;
                let potential_y = orig_bottom - new_h;
                if potential_y < 2.0 * CELL_H {
                    new_h = orig_bottom - 2.0 * CELL_H;
                }
                self.panels[i].rect.h = new_h;
                self.panels[i].rect.y = orig_bottom - new_h;
            }
            DockSide::None => {
                self.panels[i].rect.w = (ow + dx).max(MIN_W);
                // For floating panels, the anchor is top-left, so resizing
                // doesn't move Y.
                self.panels[i].rect.h = (oh + dy).max(MIN_H);
            }
        }
    }

    pub fn end_resize(&mut self) {
        if let Some(id) = self.resizing.take() {
            let i = self.idx(id);
            self.panels[i].resize_anchor = None;
        }
    }

    pub fn is_resizing(&self) -> bool { self.resizing.is_some() }

    // ── Layout ────────────────────────────────────────────────────────────────

    /// Reposition docked panels to fill their edge. Call every render frame
    /// before drawing so positions are always current. `screen_w`/
    /// `screen_h` are pixels (Part 1c — cells before this; callers now
    /// pass `Renderer::pixel_width`/`pixel_height`, not `width`/`height`).
    pub fn apply_layout(&mut self, screen_w: usize, screen_h: usize) {
        let screen_w = screen_w as f32;
        let screen_h = screen_h as f32;

        // Status bar takes the last cell row; toolbar + title take the
        // first two cell rows; canvas starts below them. Still expressed
        // via CELL_H here (Part 1c) — this chrome-row layout stays
        // cell-native until Part 4's restyle.
        let canvas_top    = 2.0 * CELL_H;
        let canvas_bottom = (screen_h - CELL_H).max(0.0); // row above status bar
        let full_h        = (canvas_bottom - canvas_top).max(0.0);

        // Ensure active panel markers are valid
        self.validate_active_panels();

        let left_w = self.active_left.map(|id| self.get(id).rect.w).unwrap_or(0.0);
        let right_w = self.active_right.map(|id| self.get(id).rect.w).unwrap_or(0.0);
        let bottom_h = self.active_bottom.map(|id| self.get(id).rect.h).unwrap_or(0.0);

        let right_x = (screen_w - right_w).max(0.0);
        let bottom_y = (canvas_bottom - bottom_h).max(canvas_top);

        for p in &mut self.panels {
            if !p.visible { continue; }
            match p.id {
                PanelId::Viewport => {
                    p.rect.x = left_w;
                    p.rect.y = canvas_top;
                    p.rect.w = (screen_w - left_w - right_w).max(0.0);
                    p.rect.h = (canvas_bottom - canvas_top - bottom_h).max(0.0);
                    p.dock = DockSide::None;
                }
                _ => {
                    match p.dock {
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
                        DockSide::None => {}
                    }
                }
            }
        }
    }

    fn validate_active_panels(&mut self) {
        // If an active panel is no longer visible or no longer docked to that side, clear it.
        if let Some(id) = self.active_left {
            let p = self.get(id);
            if !p.visible || p.dock != DockSide::Left { self.active_left = None; }
        }
        if let Some(id) = self.active_right {
            let p = self.get(id);
            if !p.visible || p.dock != DockSide::Right { self.active_right = None; }
        }
        if let Some(id) = self.active_bottom {
            let p = self.get(id);
            if !p.visible || p.dock != DockSide::Bottom { self.active_bottom = None; }
        }

        // If a side has visible docked panels but no active one, pick the first.
        if self.active_left.is_none() {
            self.active_left = self.panels.iter().find(|p| p.visible && p.dock == DockSide::Left).map(|p| p.id);
        }
        if self.active_right.is_none() {
            self.active_right = self.panels.iter().find(|p| p.visible && p.dock == DockSide::Right).map(|p| p.id);
        }
        if self.active_bottom.is_none() {
            self.active_bottom = self.panels.iter().find(|p| p.visible && p.dock == DockSide::Bottom).map(|p| p.id);
        }
    }

    /// Canvas bounds after accounting for all docked panels.
    /// Returns (canvas_x, canvas_y, canvas_w, canvas_h) — still CELLS
    /// (via `content_x`/`content_y`/`content_w`/`content_h`), since
    /// `Layout` and every `ui::draw_*` function that consumes this stay
    /// cell-based until Part 1e/Part 4.
    pub fn canvas_bounds(&self, _screen_w: usize, _screen_h: usize) -> (usize, usize, usize, usize) {
        let vp = self.get(PanelId::Viewport);
        (vp.content_x(), vp.content_y(), vp.content_w(), vp.content_h())
    }
}

// ── draw_panel_chrome ─────────────────────────────────────────────────────────

/// Draw the title bar and resize handle for a panel, and register each
/// interactive element's hit rect in `frame` at the exact point it's drawn
/// (Phase 7 Part 1d, docs/ember2d-phase7-plan.md) — see `ui/frame.rs`'s
/// header comment for why this one function doing both is what closes
/// defect E5.
///
/// Title bar at panel.y: `= Title ... [X]`
/// Resize handle [~] at bottom-right corner of the panel.
///
/// Still entirely cell-based drawing itself (Phase 7 Part 1c) — reads the
/// panel's geometry through `cell_x`/`cell_y`/`cell_w`/`cell_h` rather than
/// raw fields, which no longer exist on `Panel`. Migrating the drawing
/// itself to `fill_rect_px`/`draw_nine_slice` is Part 4's job; the hit
/// rects pushed here are pixels regardless (`panel.rect` and
/// `UiRect::from_cells`), matching every other `UiFrame` entry.
pub fn draw_panel_chrome(renderer: &mut Renderer, panel: &Panel, frame: &mut UiFrame) {
    let x = panel.cell_x().max(0) as usize;
    let y = panel.cell_y().max(0) as usize;
    let w = panel.cell_w();
    let h = panel.cell_h();
    if w < 2 || h < 2 { return; }

    // 1. Fill panel interior
    let interior_bg = if panel.id == PanelId::Viewport { Color::Black } else { Color::DarkGrey };
    renderer.draw_rect_filled(x, y, w, h, ' ', Color::White, interior_bg);

    // 2. Title Bar (Top border area)
    renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, Color::DarkBlue);
    frame.push(WidgetId::TitleBar(panel.id), UiRect::new(panel.rect.x, panel.rect.y, panel.rect.w, CELL_H));

    let dock_indicator = match panel.dock {
        DockSide::Left   => "< ",
        DockSide::Right  => "> ",
        DockSide::Bottom => "v ",
        DockSide::None   => "= ",
    };
    let title = format!("{}{} ", dock_indicator, panel.title);
    let clipped: String = title.chars().take(w.saturating_sub(6)).collect();
    renderer.draw_str(x + 1, y, &clipped, Color::White, Color::DarkBlue);

    if w >= 5 && panel.id != PanelId::Viewport {
        renderer.draw_str(x + w - 4, y, "[X]", Color::White, Color::DarkBlue);
        // Pushed at the SAME `x + w - 4` cell offset the draw call above
        // just used, three cells wide (matching the three characters
        // "[X]") — this fixes a pre-existing one-cell mismatch the old,
        // independently-computed `Panel::on_close_btn` hitbox had (it
        // covered cells [w-3, w-2), one cell right of the actual glyphs).
        // That drift is exactly defect E5; there is now only one place
        // that decides where this button is.
        frame.push(WidgetId::CloseBtn(panel.id), UiRect::from_cells((x + w - 4) as i32, y as i32, 3, 1));
    }

    // 3. Side and Bottom Borders (blended)
    let border_fg = Color::Grey;
    let border_bg = interior_bg;

    // Left & Right
    for row in (y + 1)..(y + h - 1) {
        renderer.draw_char(x, row, '|', border_fg, border_bg);
        renderer.draw_char(x + w - 1, row, '|', border_fg, border_bg);
    }
    // Bottom
    let bot_str: String = std::iter::repeat('-').take(w).collect();
    renderer.draw_str(x, y + h - 1, &bot_str, border_fg, border_bg);

    // 4. Corners
    renderer.draw_char(x, y, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(x + w - 1, y, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(x, y + h - 1, '+', border_fg, border_bg);

    // 5. Resize handle [+] at bottom-right corner
    renderer.draw_char(x + w - 1, y + h - 1, '+', Color::Cyan, border_bg);
    frame.push(WidgetId::ResizeHandle(panel.id), UiRect::new(panel.rect.right() - CELL_W, panel.rect.bottom() - CELL_H, CELL_W, CELL_H));
}

// tests.rs: split out in Part 1f to stay under the 600-line limit.
#[cfg(test)]
mod tests;
