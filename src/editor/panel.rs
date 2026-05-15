// editor/panel.rs — Floating + dockable panel system for the level editor.
//
// Panels can be dragged freely, docked to a screen edge, or resized.
// Docking: drag a panel within DOCK_THRESHOLD cells of an edge to snap it.
// Undocking: dragging a docked panel's title bar floats it again.
// Resize: the [~] handle in each panel's bottom-right corner; for docked panels
//   the inner edge is the resize target (right edge for Left-docked, etc.).

use crate::renderer::{color::Color, Renderer};
pub use super::ui::{DockSide, PanelId};

// ── Panel ─────────────────────────────────────────────────────────────────────

#[derive(Clone)]
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

    /// Leftmost content column (accounts for left border).
    pub fn content_x(&self) -> usize { (self.x + 1).max(0) as usize }

    /// First content row (accounts for top border/title bar).
    pub fn content_y(&self) -> usize { (self.y + 1).max(0) as usize }

    /// Width of content area (accounts for both side borders).
    pub fn content_w(&self) -> usize { self.w.saturating_sub(2) }

    /// Height of content area (accounts for top and bottom borders).
    pub fn content_h(&self) -> usize { self.h.saturating_sub(2) }

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
        let close_start = px + self.w - 3; // Adjusted for border
        row == self.y.max(0) as usize && col >= close_start && col < px + self.w - 1
    }

    /// Hits the resize handle at the bottom-right corner of the border.
    pub fn on_resize_handle(&self, col: usize, row: usize) -> bool {
        if !self.contains(col, row) { return false; }
        let px = self.x.max(0) as usize;
        let py = self.y.max(0) as usize;
        let bottom = py + self.h - 1;
        let right  = px + self.w - 1;
        row == bottom && col == right
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

const DOCK_THRESHOLD: i32 = 3;

impl PanelManager {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        let canvas_y = 2i32;
        let canvas_h = screen_h.saturating_sub(3).max(4) as i32;

        let insp_x = (screen_w as i32 - INSP_W as i32).max(0);
        let pal_x  = (insp_x - PAL_W as i32 - 2).max(0);
        let con_y  = screen_h as i32 - CON_H as i32 - 1;

        let mut panels = vec![
            Panel::new(PanelId::Hierarchy,    "Hierarchy",     0,       canvas_y, HIER_W, canvas_h as usize),
            Panel::new(PanelId::Inspector,    "Inspector",     insp_x,  canvas_y, INSP_W, canvas_h as usize),
            Panel::new(PanelId::Palette,      "Palette",       pal_x,   canvas_y, PAL_W,  canvas_h as usize),
            Panel::new(PanelId::Console,      "Console",       0,       con_y,    screen_w, CON_H),
            Panel::new(PanelId::Stats,        "Stats",         pal_x,   canvas_y, PAL_W,  canvas_h as usize),
            Panel::new(PanelId::FileBrowser,  "Files",         0,       canvas_y, BROW_W, canvas_h as usize),
            Panel::new(PanelId::ScriptEditor, "Script Editor", 0,       con_y,    screen_w, EDIT_H),
        ];

        // Default docking
        panels[0].dock    = DockSide::Left;
        panels[0].visible = true;  // Hierarchy

        panels[1].dock    = DockSide::Right;
        panels[1].visible = true;  // Inspector

        panels[2].visible = false; // Palette (floating, hidden by default)

        panels[3].dock    = DockSide::Bottom;  // Console (hidden by default)

        panels[5].dock    = DockSide::Left;
        panels[5].visible = false; // FileBrowser

        panels[6].dock    = DockSide::Bottom;
        panels[6].visible = false; // ScriptEditor

        for (i, p) in panels.iter_mut().enumerate() { p.z = i; }

        PanelManager {
            panels, next_z: 7, dragging: None, resizing: None,
            active_left:   Some(PanelId::Hierarchy),
            active_right:  Some(PanelId::Inspector),
            active_bottom: Some(PanelId::Console),
        }
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

    pub fn get_docked_panels(&self, side: DockSide) -> Vec<PanelId> {
        self.panels.iter()
            .filter(|p| p.visible && p.dock == side)
            .map(|p| p.id)
            .collect()
    }

    pub fn tab_at(&self, col: usize, row: usize, _screen_w: usize, _screen_h: usize) -> Option<PanelId> {
        // Check Left dock
        if let Some(active_id) = self.active_left {
            let p = self.get(active_id);
            if row == p.y as usize && col < p.w {
                return self.find_tab_in_row(col, p.x as usize, p.y as usize, p.w, DockSide::Left);
            }
        }
        // Check Right dock
        if let Some(active_id) = self.active_right {
            let p = self.get(active_id);
            if row == p.y as usize && col >= p.x as usize && col < p.x as usize + p.w {
                return self.find_tab_in_row(col, p.x as usize, p.y as usize, p.w, DockSide::Right);
            }
        }
        // Check Bottom dock
        if let Some(active_id) = self.active_bottom {
            let p = self.get(active_id);
            if row == p.y as usize && col >= p.x as usize && col < p.x as usize + p.w {
                return self.find_tab_in_row(col, p.x as usize, p.y as usize, p.w, DockSide::Bottom);
            }
        }
        None
    }

    fn find_tab_in_row(&self, col: usize, x: usize, _y: usize, w: usize, side: DockSide) -> Option<PanelId> {
        let docked = self.get_docked_panels(side);
        let mut cursor_x = x;
        for id in docked {
            let title = self.get(id).title;
            let label_len = title.len() + 2; // " " + title + " "
            if col >= cursor_x && col < cursor_x + label_len {
                return Some(id);
            }
            cursor_x += label_len + 1;
            if cursor_x > x + w { break; }
        }
        None
    }

    // ── Drag ──────────────────────────────────────────────────────────────────

    /// Begin dragging. Undocks the panel so it floats freely.
    pub fn start_drag(&mut self, id: PanelId, mouse_col: i32, mouse_row: i32) {
        let i = self.idx(id);
        if self.panels[i].dock != DockSide::None {
            // Restore to a sensible floating size when undocking
            self.panels[i].w = self.panels[i].w.min(40).max(MIN_W);
            self.panels[i].h = self.panels[i].h.min(20).max(MIN_H);
        }
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
                // Constrain: cannot resize above row 2
                let mut new_h = ((oh as i32 - dy).max(MIN_H as i32)) as usize;
                let orig_bottom = self.panels[i].y + oh as i32;
                let potential_y = orig_bottom - new_h as i32;
                if potential_y < 2 {
                    new_h = (orig_bottom - 2) as usize;
                }
                self.panels[i].h = new_h;
                self.panels[i].y = orig_bottom - new_h as i32;
            }
            DockSide::None => {
                self.panels[i].w = ((ow as i32 + dx).max(MIN_W as i32)) as usize;
                let new_h = ((oh as i32 + dy).max(MIN_H as i32)) as usize;
                // For floating panels, the anchor is top-left, so resizing doesn't move Y.
                // But we should ensure height doesn't exceed screen.
                self.panels[i].h = new_h;
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

        // Ensure active panel markers are valid
        self.validate_active_panels();

        let left_w = self.active_left.map(|id| self.get(id).w).unwrap_or(0);
        let right_w = self.active_right.map(|id| self.get(id).w).unwrap_or(0);
        let bottom_h = self.active_bottom.map(|id| self.get(id).h).unwrap_or(0);

        let right_x = (screen_w as i32 - right_w as i32).max(0);
        let bottom_y = (canvas_bottom as i32 - bottom_h as i32).max(canvas_top as i32);

        for p in &mut self.panels {
            if !p.visible { continue; }
            match p.dock {
                DockSide::Left => {
                    p.x = 0;
                    p.y = canvas_top as i32;
                    p.w = left_w;
                    p.h = full_h;
                }
                DockSide::Right => {
                    p.x = right_x;
                    p.y = canvas_top as i32;
                    p.w = right_w;
                    p.h = full_h;
                }
                DockSide::Bottom => {
                    p.x = left_w as i32;
                    p.y = bottom_y;
                    p.w = screen_w.saturating_sub(left_w + right_w);
                    p.h = bottom_h;
                }
                DockSide::None => {}
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
    /// Returns (canvas_x, canvas_y, canvas_w, canvas_h).
    pub fn canvas_bounds(&self, screen_w: usize, screen_h: usize) -> (usize, usize, usize, usize) {
        let canvas_top = 2usize;
        let left_w = self.active_left.map(|id| self.get(id).w).unwrap_or(0);
        let right_w = self.active_right.map(|id| self.get(id).w).unwrap_or(0);
        let bottom_h = self.active_bottom.map(|id| self.get(id).h).unwrap_or(0);

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
    if w < 2 || h < 2 { return; }

    // 1. Fill panel interior
    renderer.draw_rect_filled(x, y, w, h, ' ', Color::White, Color::DarkGrey);

    // 2. Title Bar (Top border area)
    renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, Color::DarkBlue);
    let dock_indicator = match panel.dock {
        DockSide::Left   => "< ",
        DockSide::Right  => "> ",
        DockSide::Bottom => "v ",
        DockSide::None   => "= ",
    };
    let title = format!("{}{} ", dock_indicator, panel.title);
    let clipped: String = title.chars().take(w.saturating_sub(6)).collect();
    renderer.draw_str(x + 1, y, &clipped, Color::White, Color::DarkBlue);

    if w >= 5 {
        renderer.draw_str(x + w - 4, y, "[X]", Color::White, Color::DarkBlue);
    }

    // 3. Side and Bottom Borders (blended)
    let border_fg = Color::Grey;
    let border_bg = Color::DarkGrey;
    
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
}
