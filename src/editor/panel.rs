// editor/panel.rs — Floating + dockable panel system for the level editor.
//
// Panels can be dragged freely, docked to a screen edge, or resized.
// Docking: drag a panel within DOCK_THRESHOLD cells of an edge to snap it.
// Undocking: dragging a docked panel's title bar floats it again.
// Resize: the [~] handle in each panel's bottom-right corner; for docked panels
//   the inner edge is the resize target (right edge for Left-docked, etc.).

use crate::renderer::{color::Color, Renderer};

// ── DockSide ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockSide { None, Left, Right, Bottom }

// ── PanelId ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PanelId {
    Hierarchy,
    Palette,
    Inspector,
    Console,
    Stats,
}

// ── Panel ─────────────────────────────────────────────────────────────────────

pub struct Panel {
    pub id:      PanelId,
    pub title:   &'static str,
    pub x:       i32,
    pub y:       i32,
    pub w:       usize,
    pub h:       usize,
    pub visible: bool,
    pub z:       usize,
    pub dock:    DockSide,
    drag_offset:   Option<(i32, i32)>,
    resize_anchor: Option<(i32, i32, usize, usize)>, // (mx, my, orig_w, orig_h)
}

const MIN_W: usize = 10;
const MIN_H: usize = 4;

impl Panel {
    fn new(id: PanelId, title: &'static str, x: i32, y: i32, w: usize, h: usize) -> Self {
        Panel {
            id, title, x, y, w, h,
            visible: false, z: 0, dock: DockSide::None,
            drag_offset: None, resize_anchor: None,
        }
    }

    /// First content row (title bar is at panel.y).
    pub fn content_y(&self) -> usize { (self.y + 1).max(0) as usize }

    /// Height of content area.
    pub fn content_h(&self) -> usize { self.h.saturating_sub(1) }

    pub fn contains(&self, col: usize, row: usize) -> bool {
        let px = self.x.max(0) as usize;
        let py = self.y.max(0) as usize;
        col >= px && col < px + self.w && row >= py && row < py + self.h
    }

    pub fn on_title_bar(&self, col: usize, row: usize) -> bool {
        let px = self.x.max(0) as usize;
        row == self.y.max(0) as usize && col >= px && col < px + self.w
    }

    pub fn on_close_btn(&self, col: usize, row: usize) -> bool {
        if self.w < 5 { return false; }
        let px = self.x.max(0) as usize;
        let close_start = px + self.w - 4;
        row == self.y.max(0) as usize && col >= close_start && col < close_start + 3
    }

    /// True if (col, row) hits the resize handle for this panel.
    /// Floating: [~] indicator at bottom-right corner (last row, last 2 cols).
    /// Left-docked: right edge column, any row.
    /// Right-docked: left edge column, any row.
    /// Bottom-docked: [~] at bottom-right (avoids conflict with title bar drag).
    pub fn on_resize_handle(&self, col: usize, row: usize) -> bool {
        if !self.contains(col, row) { return false; }
        let px = self.x.max(0) as usize;
        let py = self.y.max(0) as usize;
        match self.dock {
            DockSide::Left => col == px + self.w - 1,
            DockSide::Right => col == px,
            DockSide::Bottom | DockSide::None => {
                // Bottom-right corner: last row, last 2 cols
                let bottom = py + self.h - 1;
                let right  = px + self.w - 1;
                row == bottom && col >= right.saturating_sub(1)
            }
        }
    }
}

// ── PanelManager ──────────────────────────────────────────────────────────────

pub struct PanelManager {
    panels:   Vec<Panel>,
    next_z:   usize,
    dragging: Option<PanelId>,
    resizing: Option<PanelId>,
}

pub const HIER_W: usize = 14;
pub const INSP_W: usize = 30;
pub const PAL_W:  usize = 14;
pub const CON_H:  usize = 9;

const DOCK_THRESHOLD: i32 = 3;

impl PanelManager {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        let canvas_y = 2i32;
        let canvas_h = screen_h.saturating_sub(3).max(4) as i32;

        let insp_x = (screen_w as i32 - INSP_W as i32).max(0);
        let pal_x  = (insp_x - PAL_W as i32 - 2).max(0);
        let con_y  = screen_h as i32 - CON_H as i32 - 1;

        let mut panels = vec![
            Panel::new(PanelId::Hierarchy, "Hierarchy", 0,       canvas_y, HIER_W, canvas_h as usize),
            Panel::new(PanelId::Inspector, "Inspector", insp_x,  canvas_y, INSP_W, canvas_h as usize),
            Panel::new(PanelId::Palette,   "Palette",   pal_x,   canvas_y, PAL_W,  canvas_h as usize),
            Panel::new(PanelId::Console,   "Console",   0,       con_y,    screen_w, CON_H),
            Panel::new(PanelId::Stats,     "Stats",     pal_x,   canvas_y, PAL_W,  canvas_h as usize),
        ];

        // Default docking
        panels[0].dock    = DockSide::Left;
        panels[0].visible = true;  // Hierarchy

        panels[1].dock    = DockSide::Right;
        panels[1].visible = true;  // Inspector

        panels[2].visible = true;  // Palette (floating)

        panels[3].dock    = DockSide::Bottom;  // Console (hidden by default)

        for (i, p) in panels.iter_mut().enumerate() { p.z = i; }

        PanelManager { panels, next_z: 5, dragging: None, resizing: None }
    }

    fn idx(&self, id: PanelId) -> usize {
        self.panels.iter().position(|p| p.id == id)
            .expect("PanelManager: unknown PanelId")
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
        self.bring_to_front(id);
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
        let z = self.next_z;
        self.next_z += 1;
        let i = self.idx(id);
        self.panels[i].z = z;
    }

    /// Panel IDs sorted lowest-z first (back-to-front draw order).
    pub fn in_draw_order(&self) -> Vec<PanelId> {
        let mut order: Vec<(usize, PanelId)> = self.panels.iter()
            .filter(|p| p.visible)
            .map(|p| (p.z, p.id))
            .collect();
        order.sort_by_key(|(z, _)| *z);
        order.into_iter().map(|(_, id)| id).collect()
    }

    /// Topmost visible panel whose title bar is at (col, row).
    pub fn title_bar_at(&self, col: usize, row: usize) -> Option<PanelId> {
        self.panels.iter()
            .filter(|p| p.visible && p.on_title_bar(col, row))
            .max_by_key(|p| p.z)
            .map(|p| p.id)
    }

    /// Topmost visible panel whose close button is at (col, row).
    pub fn close_btn_at(&self, col: usize, row: usize) -> Option<PanelId> {
        self.panels.iter()
            .filter(|p| p.visible && p.on_close_btn(col, row))
            .max_by_key(|p| p.z)
            .map(|p| p.id)
    }

    /// Topmost visible panel whose resize handle is at (col, row).
    pub fn resize_handle_at(&self, col: usize, row: usize) -> Option<PanelId> {
        self.panels.iter()
            .filter(|p| p.visible && p.on_resize_handle(col, row))
            .max_by_key(|p| p.z)
            .map(|p| p.id)
    }

    /// Topmost visible panel that contains (col, row).
    pub fn panel_at(&self, col: usize, row: usize) -> Option<PanelId> {
        self.panels.iter()
            .filter(|p| p.visible && p.contains(col, row))
            .max_by_key(|p| p.z)
            .map(|p| p.id)
    }

    pub fn is_point_on_panel(&self, col: usize, row: usize) -> bool {
        self.panels.iter().any(|p| p.visible && p.contains(col, row))
    }

    // ── Drag ──────────────────────────────────────────────────────────────────

    /// Begin dragging. Undocks the panel so it floats freely.
    pub fn start_drag(&mut self, id: PanelId, mouse_col: i32, mouse_row: i32) {
        let i = self.idx(id);
        self.panels[i].dock = DockSide::None;   // undock on drag start
        self.panels[i].drag_offset = Some((
            mouse_col - self.panels[i].x,
            mouse_row - self.panels[i].y,
        ));
        self.dragging = Some(id);
        self.bring_to_front(id);
    }

    pub fn update_drag(&mut self, mouse_col: i32, mouse_row: i32, screen_w: usize, screen_h: usize) {
        let Some(id) = self.dragging else { return };
        let i = self.idx(id);
        let Some((ox, oy)) = self.panels[i].drag_offset else { return };
        let new_x = (mouse_col - ox).max(0).min(screen_w as i32 - self.panels[i].w as i32);
        let new_y = (mouse_row - oy).max(2).min(screen_h as i32 - 2);
        self.panels[i].x = new_x;
        self.panels[i].y = new_y;
    }

    /// End drag and snap to an edge if within DOCK_THRESHOLD.
    pub fn end_drag(&mut self, screen_w: usize, screen_h: usize) {
        let Some(id) = self.dragging.take() else { return };
        let i = self.idx(id);
        self.panels[i].drag_offset = None;

        let p = &self.panels[i];
        let x  = p.x;
        let y  = p.y;
        let pw = p.w as i32;
        let ph = p.h as i32;
        let sw = screen_w as i32;
        let sh = screen_h as i32;

        let new_dock = if x <= DOCK_THRESHOLD {
            DockSide::Left
        } else if x + pw >= sw - DOCK_THRESHOLD {
            DockSide::Right
        } else if y + ph >= sh - DOCK_THRESHOLD - 1 {
            DockSide::Bottom
        } else {
            DockSide::None
        };
        self.panels[i].dock = new_dock;
    }

    pub fn is_dragging(&self) -> bool { self.dragging.is_some() }

    // ── Resize ────────────────────────────────────────────────────────────────

    pub fn start_resize(&mut self, id: PanelId, mx: i32, my: i32) {
        let i = self.idx(id);
        let (w, h) = (self.panels[i].w, self.panels[i].h);
        self.panels[i].resize_anchor = Some((mx, my, w, h));
        self.resizing = Some(id);
        self.bring_to_front(id);
    }

    pub fn update_resize(&mut self, mx: i32, my: i32) {
        let Some(id) = self.resizing else { return };
        let i = self.idx(id);
        let Some((ax, ay, ow, oh)) = self.panels[i].resize_anchor else { return };
        let dx = mx - ax;
        let dy = my - ay;
        match self.panels[i].dock {
            DockSide::Left => {
                self.panels[i].w = ((ow as i32 + dx).max(MIN_W as i32)) as usize;
            }
            DockSide::Right => {
                // Right edge fixed: grow left → x decreases, w increases
                let new_w = ((ow as i32 - dx).max(MIN_W as i32)) as usize;
                let orig_right = self.panels[i].x + ow as i32;
                self.panels[i].w = new_w;
                self.panels[i].x = orig_right - new_w as i32;
            }
            DockSide::Bottom => {
                // Bottom edge fixed: grow up → y decreases, h increases
                let new_h = ((oh as i32 - dy).max(MIN_H as i32)) as usize;
                let orig_bottom = self.panels[i].y + oh as i32;
                self.panels[i].h = new_h;
                self.panels[i].y = orig_bottom - new_h as i32;
            }
            DockSide::None => {
                self.panels[i].w = ((ow as i32 + dx).max(MIN_W as i32)) as usize;
                self.panels[i].h = ((oh as i32 + dy).max(MIN_H as i32)) as usize;
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
    /// before drawing so positions are always current.
    pub fn apply_layout(&mut self, screen_w: usize, screen_h: usize) {
        // Status bar takes last row; toolbar + title take rows 0-1; canvas starts at 2.
        let canvas_top    = 2usize;
        let canvas_bottom = screen_h.saturating_sub(1); // row above status bar
        let full_h        = canvas_bottom.saturating_sub(canvas_top);

        // Compute total docked widths/heights first (for bottom panel width calc).
        let left_w: usize  = self.panels.iter().filter(|p| p.visible && p.dock == DockSide::Left).map(|p| p.w).sum();
        let right_w: usize = self.panels.iter().filter(|p| p.visible && p.dock == DockSide::Right).map(|p| p.w).sum();

        let mut left_cursor  = 0usize;
        let mut right_cursor = screen_w;
        let mut bottom_y     = canvas_bottom;

        for p in &mut self.panels {
            if !p.visible { continue; }
            match p.dock {
                DockSide::Left => {
                    p.x = left_cursor as i32;
                    p.y = canvas_top as i32;
                    p.h = full_h;
                    left_cursor += p.w;
                }
                DockSide::Right => {
                    right_cursor = right_cursor.saturating_sub(p.w);
                    p.x = right_cursor as i32;
                    p.y = canvas_top as i32;
                    p.h = full_h;
                }
                DockSide::Bottom => {
                    bottom_y = bottom_y.saturating_sub(p.h);
                    p.x = left_w as i32;
                    p.y = bottom_y as i32;
                    p.w = screen_w.saturating_sub(left_w + right_w);
                }
                DockSide::None => {}
            }
        }
    }

    /// Canvas bounds after accounting for all docked panels.
    /// Returns (canvas_x, canvas_y, canvas_w, canvas_h).
    pub fn canvas_bounds(&self, screen_w: usize, screen_h: usize) -> (usize, usize, usize, usize) {
        let canvas_top = 2usize;
        let left_w: usize  = self.panels.iter().filter(|p| p.visible && p.dock == DockSide::Left).map(|p| p.w).sum();
        let right_w: usize = self.panels.iter().filter(|p| p.visible && p.dock == DockSide::Right).map(|p| p.w).sum();
        let bottom_h: usize = self.panels.iter().filter(|p| p.visible && p.dock == DockSide::Bottom).map(|p| p.h).sum();

        let canvas_x = left_w;
        let canvas_w = screen_w.saturating_sub(left_w + right_w).max(20);
        let canvas_h = screen_h.saturating_sub(3 + bottom_h).max(4);
        (canvas_x, canvas_top, canvas_w, canvas_h)
    }
}

// ── draw_panel_chrome ─────────────────────────────────────────────────────────

/// Draw the title bar and resize handle for a panel.
/// Title bar at panel.y: `= Title ... [X]`
/// Resize handle [~] at bottom-right corner of the panel.
pub fn draw_panel_chrome(renderer: &mut Renderer, panel: &Panel) {
    let x = panel.x.max(0) as usize;
    let y = panel.y.max(0) as usize;
    let w = panel.w;
    let h = panel.h;
    if w == 0 || h == 0 { return; }

    // Title bar
    renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, Color::DarkBlue);

    let dock_indicator = match panel.dock {
        DockSide::Left   => "< ",
        DockSide::Right  => "> ",
        DockSide::Bottom => "v ",
        DockSide::None   => "= ",
    };
    let title = format!("{}{} ", dock_indicator, panel.title);
    let clipped: String = title.chars().take(w.saturating_sub(4)).collect();
    renderer.draw_str(x, y, &clipped, Color::White, Color::DarkBlue);

    // Close button
    if w >= 4 {
        renderer.draw_str(x + w - 4, y, "[X]", Color::White, Color::DarkBlue);
    }

    // Resize handle [~] at bottom-right corner
    if h >= 2 && w >= 2 {
        let ry = y + h - 1;
        let rx = x + w - 1;
        // For docked panels, show the resize cursor on the inner edge indicator
        match panel.dock {
            DockSide::Left => {
                renderer.draw_char(rx, y + h / 2, '|', Color::DarkGrey, Color::DarkGrey);
            }
            DockSide::Right => {
                renderer.draw_char(x, y + h / 2, '|', Color::DarkGrey, Color::DarkGrey);
            }
            _ => {
                renderer.draw_char(rx, ry, '~', Color::DarkGrey, Color::DarkGrey);
            }
        }
    }
}
