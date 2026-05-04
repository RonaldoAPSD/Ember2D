// editor/ui.rs — Stateless drawing functions for the editor's UI panels.
//
// Layout (dynamic):
//   Row 0        : title bar
//   Row 1        : toolbar (tool buttons, view toggles, action buttons)
//   Rows 2..H-1  : canvas + palette panel + optional inspector panel
//   Last row     : status bar / text-input prompt

use std::collections::HashMap;

use crate::renderer::{color::Color, Renderer};
use crate::editor::grid::LevelGrid;
use crate::editor::palette::TilePalette;
use crate::level::TileRecord;
use crate::mouse::MouseState;
use crate::scripting::{LogEntry, LogLevel};

// ── Hierarchy selection ───────────────────────────────────────────────────────

/// Which entity is currently selected in the hierarchy panel.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HierarchySelection {
    Player,
    Spawn(usize),
}

// ── Tool / toolbar types ──────────────────────────────────────────────────────

/// The currently active drawing tool, reflected in the toolbar highlight.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToolKind {
    Paint,   // default left-click paint
    Select,  // click to pin tile in inspector (Q)
    Rect,    // filled rectangle (Shift+drag or toolbar)
    Line,    // two-click line stamp (L or toolbar)
    Fill,    // flood fill (F or toolbar)
    Copy,    // copy-select drag (C)
    Cut,     // cut-select drag (X)
    Paste,   // click to stamp clipboard (V)
}

/// Which top-level menu is currently open.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuKind { File, Edit, Level, View, Tools }

/// State snapshot used to render menu checkmarks and dim unavailable items.
pub struct MenuState {
    pub can_undo:       bool,
    pub can_redo:       bool,
    pub clipboard_full: bool,
    pub show_palette:   bool,
    pub show_grid:      bool,
    pub show_inspector: bool,
    pub show_console:   bool,
    pub show_stats:     bool,
    pub show_physics:   bool,
    pub active_tool:    ToolKind,
}

/// Action dispatched from a menu item click or keyboard shortcut.
#[derive(Debug, Clone)]
pub enum ToolbarAction {
    // Drawing tools
    SetTool(ToolKind),
    // Edit
    Undo,
    Redo,
    FindReplace,
    // View toggles
    ToggleGrid,
    ToggleInspector,
    ToggleConsole,
    TogglePalette,
    ToggleStats,
    TogglePhysics,
    // File
    New,
    Open,
    Save,
    SaveAs,
    Play,
    Quit,
    // Level
    RenameLevel,
    ResizeLevel,
    SetSpawn,
    AddNamedSpawn,
    NewLevel,
}

// ── Dynamic layout ────────────────────────────────────────────────────────────

/// Canvas and toolbar metrics. Canvas bounds are supplied by PanelManager
/// (via canvas_bounds()) so they account for docked panels dynamically.
#[derive(Clone)]
pub struct Layout {
    pub screen_w:    usize,
    pub screen_h:    usize,
    pub canvas_x:    usize,
    pub canvas_y:    usize,
    pub canvas_w:    usize,
    pub canvas_h:    usize,
    pub toolbar_row: usize,
}

pub const HIER_W: usize = 14; // kept for default panel width reference

// ── Inspector content row offsets (relative to content_y = panel.y + 1) ──────
pub const INSP_GLYPH_OFF:   usize = 2;
pub const INSP_TAG_OFF:     usize = 5;
pub const INSP_SOLID_OFF:   usize = 7;
pub const INSP_TRIG_OFF:    usize = 8;
pub const INSP_CAM_OFF:     usize = 9;
pub const INSP_SCRIPT_OFF:  usize = 12;
pub const INSP_EXIT_OFF:    usize = 13;
pub const INSP_GRAPH_BTN:   usize = 16;

impl Layout {
    /// Default layout with no panel offsets — overridden each frame by
    /// `with_canvas` once PanelManager has computed the actual bounds.
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        Layout {
            screen_w,
            screen_h,
            canvas_x:    0,
            canvas_y:    2,
            canvas_w:    screen_w,
            canvas_h:    screen_h.saturating_sub(3).max(4),
            toolbar_row: 1,
        }
    }

    /// Override canvas bounds (call with PanelManager::canvas_bounds output).
    pub fn with_canvas(mut self, cx: usize, cy: usize, cw: usize, ch: usize) -> Self {
        self.canvas_x = cx;
        self.canvas_y = cy;
        self.canvas_w = cw;
        self.canvas_h = ch;
        self
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Convert a grid coordinate to a canvas screen column/row, given scroll.
/// Returns None if the cell is outside the visible canvas.
pub fn grid_to_screen(gx: i32, gy: i32, scroll: (i32, i32), layout: &Layout) -> Option<(usize, usize)> {
    let sx = gx - scroll.0;
    let sy = gy - scroll.1;
    if sx < 0 || sy < 0 || sx >= layout.canvas_w as i32 || sy >= layout.canvas_h as i32 {
        return None;
    }
    Some((sx as usize + layout.canvas_x, sy as usize + layout.canvas_y))
}

// ── Title bar ─────────────────────────────────────────────────────────────────

pub fn draw_title_bar(
    renderer:   &mut Renderer,
    level_name: &str,
    unsaved:    bool,
    undo_count: usize,
    redo_count: usize,
    scroll:     (i32, i32),
    level_size: (usize, usize),
) {
    renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::White, Color::DarkBlue);
    renderer.draw_str(1, 0, "EMBER2D EDITOR", Color::White, Color::DarkBlue);

    let saved_marker = if unsaved { "*" } else { " " };
    let scroll_str = if scroll != (0, 0) {
        format!(" @{},{}", scroll.0, scroll.1)
    } else {
        String::new()
    };
    let info = format!(
        "{}{}  {}×{}{}  U:{} R:{}",
        saved_marker, level_name,
        level_size.0, level_size.1,
        scroll_str,
        undo_count, redo_count,
    );
    let col = renderer.width.saturating_sub(info.len() + 1);
    renderer.draw_str(col, 0, &info, Color::Yellow, Color::DarkBlue);
}

// ── Level boundary ────────────────────────────────────────────────────────────

/// Draw faint edge markers where the level ends, if those edges are in view.
pub fn draw_level_boundary(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    let right  = grid.width  as i32 - scroll.0;
    let bottom = grid.height as i32 - scroll.1;

    if right > 0 && right < layout.canvas_w as i32 {
        let col = right as usize + layout.canvas_x;
        for sy in 0..layout.canvas_h {
            renderer.draw_char(col, sy + layout.canvas_y, '|', Color::DarkGrey, Color::Reset);
        }
    }

    if bottom > 0 && bottom < layout.canvas_h as i32 {
        let row = bottom as usize;
        for sx in 0..layout.canvas_w {
            renderer.draw_char(sx + layout.canvas_x, row + layout.canvas_y, '-', Color::DarkGrey, Color::Reset);
        }
        if right > 0 && right < layout.canvas_w as i32 {
            renderer.draw_char(right as usize + layout.canvas_x, bottom as usize + layout.canvas_y, '+', Color::DarkGrey, Color::Reset);
        }
    }
}

// ── Palette panel ─────────────────────────────────────────────────────────────

/// `mode` overrides the header label and highlights it when a tool is active.
/// `px`,`cy`,`pw`,`ch` are the panel's content area (below title bar).
pub fn draw_palette_panel(
    renderer: &mut Renderer,
    palette:  &TilePalette,
    mode:     Option<&str>,
    px: usize, cy: usize, pw: usize, ch: usize,
) {
    renderer.draw_rect_filled(px, cy, pw, ch, ' ', Color::White, Color::DarkGrey);

    if let Some(m) = mode {
        let header = format!(" {:^width$} ", m, width = pw.saturating_sub(2));
        renderer.draw_str(px, cy, &header, Color::Black, Color::Cyan);
    }

    for (i, tile) in palette.tiles.iter().enumerate() {
        let row = cy + 1 + i;
        if row >= cy + ch { break; }
        let label = format!(" {}:{} {:<6}", i + 1, tile.glyph, tile.name);
        if i == palette.selected {
            renderer.draw_str(px, row, &label, Color::Black, Color::Cyan);
        } else {
            renderer.draw_str(px, row, &label, tile.fg, Color::DarkGrey);
        }
    }
}

// ── Tile statistics ───────────────────────────────────────────────────────────

/// Draw tile type usage counts in the palette panel area.
pub fn draw_stats_panel(renderer: &mut Renderer, grid: &LevelGrid, palette: &TilePalette, px: usize, cy: usize, pw: usize, ch: usize) {
    renderer.draw_rect_filled(px, cy, pw, ch, ' ', Color::White, Color::DarkGrey);

    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, tile) in grid.iter() {
        *counts.entry(tile.tag.clone()).or_insert(0) += 1;
    }

    let total = grid.tiles.len();

    for (i, def) in palette.tiles.iter().enumerate() {
        let row = cy + 1 + i;
        if row >= cy + ch - 2 { break; }
        let count = counts.get(def.tag).copied().unwrap_or(0);
        let label = format!(" {}: {:>4}", def.name, count);
        renderer.draw_str(px, row, &label, def.fg, Color::DarkGrey);
    }

    let total_row = cy + ch - 2;
    if total_row >= cy && total_row < cy + ch {
        renderer.draw_str(px, total_row, &format!(" Total:{:>4}", total), Color::White, Color::DarkGrey);
    }
}

// ── Status bar ────────────────────────────────────────────────────────────────

pub fn draw_status_bar(
    renderer:     &mut Renderer,
    mouse:        &MouseState,
    _palette:     &TilePalette,
    grid_overlay: bool,
    save_path:    &str,
    tile_under:   Option<&TileRecord>,
    mode_hint:    &str,
    scroll:       (i32, i32),
    erase_size:   usize,
    layout:       &Layout,
) {
    let status_row = renderer.height - 1;
    renderer.draw_rect_filled(0, status_row, renderer.width, 1, ' ', Color::White, Color::DarkGrey);

    let cx = mouse.cell_x.saturating_sub(layout.canvas_x) as i32 + scroll.0;
    let cy = mouse.cell_y.saturating_sub(layout.canvas_y) as i32 + scroll.1;
    let pos_str = format!(" ({:3},{:3})", cx, cy);
    renderer.draw_str(0, status_row, &pos_str, Color::Cyan, Color::DarkGrey);

    if !mode_hint.is_empty() {
        renderer.draw_str(9, status_row, &format!("| {}", mode_hint), Color::White, Color::DarkGrey);
    } else if let Some(tile) = tile_under {
        let script_mark = if tile.script.is_some() { "[S]" } else { "   " };
        let props = format!(
            "| [{}] s:{} t:{} {} T=script",
            tile.tag, tile.solid as u8, tile.trigger as u8, script_mark,
        );
        renderer.draw_str(9, status_row, &props, Color::Yellow, Color::DarkGrey);
    } else {
        let grid_hint = if grid_overlay { "Tab:off" } else { "Tab:grd" };
        let erase_hint = format!("E:{}px", erase_size);
        let hints = format!("| {} S:save U:undo R:redo {}", erase_hint, grid_hint);
        renderer.draw_str(9, status_row, &hints, Color::White, Color::DarkGrey);
    }

    if !save_path.is_empty() {
        let path_display = if save_path.len() > 14 {
            format!("..{}", &save_path[save_path.len() - 12..])
        } else {
            save_path.to_string()
        };
        let col = renderer.width.saturating_sub(path_display.len() + 1);
        renderer.draw_str(col, status_row, &path_display, Color::DarkGrey, Color::DarkGrey);
    }
}

pub fn draw_text_input(renderer: &mut Renderer, prompt: &str, buffer: &str) {
    let status_row = renderer.height - 1;
    renderer.draw_rect_filled(0, status_row, renderer.width, 1, ' ', Color::White, Color::DarkGrey);
    let full = format!(" {}: {}█", prompt, buffer);
    renderer.draw_str(0, status_row, &full, Color::Black, Color::Cyan);
}

// ── File browser overlay ──────────────────────────────────────────────────────

/// Draw a file-browser overlay on the canvas area.
/// `files` is the list of discovered .level files.
/// `cursor` is the currently highlighted entry index.
pub fn draw_file_browser(renderer: &mut Renderer, files: &[String], cursor: usize, layout: &Layout) {
    renderer.draw_rect_filled(layout.canvas_x, layout.canvas_y, layout.canvas_w, layout.canvas_h, ' ', Color::DarkGrey, Color::Black);

    let title = " OPEN LEVEL — ↑↓: navigate  Enter: open  Esc: cancel ";
    renderer.draw_str(layout.canvas_x + 1, layout.canvas_y + 1, title, Color::White, Color::Black);

    let list_start = layout.canvas_y + 3;
    let max_visible = layout.canvas_h.saturating_sub(4);

    // Scroll the list if cursor is near the bottom.
    let list_offset = if cursor >= max_visible { cursor - max_visible + 1 } else { 0 };

    if files.is_empty() {
        renderer.draw_str(layout.canvas_x + 3, list_start, "No .level files found in current directory.", Color::DarkGrey, Color::Black);
        return;
    }

    for (i, file) in files.iter().enumerate().skip(list_offset).take(max_visible) {
        let row = list_start + (i - list_offset);
        if row >= layout.canvas_y + layout.canvas_h { break; }
        let display = file.rfind('/').or_else(|| file.rfind('\\'))
            .map(|p| &file[p + 1..])
            .unwrap_or(file.as_str());
        let label = format!("  {}  ", display);
        if i == cursor {
            renderer.draw_str(layout.canvas_x + 1, row, &label, Color::Black, Color::Cyan);
        } else {
            renderer.draw_str(layout.canvas_x + 1, row, &label, Color::White, Color::Black);
        }
    }
}

// ── Canvas tiles ──────────────────────────────────────────────────────────────

pub fn draw_grid(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for ((gx, gy), tile) in grid.iter() {
        if let Some((col, row)) = grid_to_screen(*gx, *gy, scroll, layout) {
            renderer.draw_char(col, row, tile.glyph, tile.fg, tile.bg);
        }
    }
}

pub fn draw_grid_overlay(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for sy in 0..layout.canvas_h {
        for sx in 0..layout.canvas_w {
            let gx = sx as i32 + scroll.0;
            let gy = sy as i32 + scroll.1;
            let on_col = gx % 5 == 0;
            let on_row = gy % 5 == 0;
            if !on_col && !on_row { continue; }
            if grid.get(gx, gy).is_some() { continue; }
            let ch = if on_col && on_row { '+' } else { '.' };
            renderer.draw_char(sx + layout.canvas_x, sy + layout.canvas_y, ch, Color::DarkGrey, Color::Reset);
        }
    }
}

// ── Cursor & spawn markers ────────────────────────────────────────────────────

/// `select_mode` — when true uses a yellow cursor instead of white to signal select mode.
pub fn draw_cursor_highlight(renderer: &mut Renderer, mouse: &MouseState, palette: &TilePalette, select_mode: bool, layout: &Layout) {
    if !mouse.in_bounds { return; }
    let col = mouse.cell_x;
    let row = mouse.cell_y;
    if col < layout.canvas_x || col >= layout.canvas_x + layout.canvas_w
        || row < layout.canvas_y || row >= layout.canvas_y + layout.canvas_h { return; }
    if select_mode {
        renderer.draw_char(col, row, '+', Color::Yellow, Color::DarkGrey);
    } else {
        let tile = palette.current();
        renderer.draw_char(col, row, tile.glyph, Color::Black, Color::White);
    }
}

pub fn draw_spawn_marker(renderer: &mut Renderer, spawn: (f32, f32), scroll: (i32, i32), layout: &Layout) {
    let gx = spawn.0.round() as i32;
    let gy = spawn.1.round() as i32;
    if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
        renderer.draw_char(col, row, '@', Color::Green, Color::Reset);
    }
}

pub fn draw_extra_spawns(renderer: &mut Renderer, spawns: &[(String, f32, f32)], scroll: (i32, i32), layout: &Layout) {
    for (name, x, y) in spawns {
        let gx = x.round() as i32;
        let gy = y.round() as i32;
        if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
            renderer.draw_char(col, row, '!', Color::Magenta, Color::Reset);
            let label: String = name.chars().take(3).collect();
            if col + 1 + label.len() < layout.canvas_w {
                renderer.draw_str(col + 1, row, &label, Color::Magenta, Color::Reset);
            }
        }
    }
}

// ── Drawing tool previews ─────────────────────────────────────────────────────

pub fn draw_rect_preview(
    renderer: &mut Renderer,
    anchor:   (i32, i32),
    current:  (i32, i32),
    glyph:    char,
    scroll:   (i32, i32),
    layout:   &Layout,
) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);

    for gy in y0..=y1 {
        for gx in x0..=x1 {
            if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
                renderer.draw_char(col, row, glyph, Color::Black, Color::White);
            }
        }
    }
}

pub fn draw_line_preview(
    renderer: &mut Renderer,
    anchor:   (i32, i32),
    current:  (i32, i32),
    glyph:    char,
    scroll:   (i32, i32),
    layout:   &Layout,
) {
    for (gx, gy) in bresenham(anchor, current) {
        if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
            renderer.draw_char(col, row, glyph, Color::Black, Color::Cyan);
        }
    }
}

pub fn draw_selection_preview(
    renderer: &mut Renderer,
    anchor:   (i32, i32),
    current:  (i32, i32),
    scroll:   (i32, i32),
    layout:   &Layout,
) {
    let x0 = anchor.0.min(current.0);
    let y0 = anchor.1.min(current.1);
    let x1 = anchor.0.max(current.0);
    let y1 = anchor.1.max(current.1);

    for gy in y0..=y1 {
        for gx in x0..=x1 {
            let on_edge = gx == x0 || gx == x1 || gy == y0 || gy == y1;
            if !on_edge { continue; }
            if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
                renderer.draw_char(col, row, '+', Color::Yellow, Color::DarkGrey);
            }
        }
    }
}

pub fn draw_paste_preview(
    renderer:  &mut Renderer,
    clipboard: &[(i32, i32, TileRecord)],
    cursor:    (i32, i32),
    flip_x:    bool,
    flip_y:    bool,
    rotate:    i32,
    scroll:    (i32, i32),
    layout:    &Layout,
) {
    let max_dx = clipboard.iter().map(|(dx, _, _)| *dx).max().unwrap_or(0);
    let max_dy = clipboard.iter().map(|(_, dy, _)| *dy).max().unwrap_or(0);

    for (dx, dy, tile) in clipboard {
        let (tdx, tdy) = transform_offset(*dx, *dy, max_dx, max_dy, flip_x, flip_y, rotate);
        let gx = cursor.0 + tdx;
        let gy = cursor.1 + tdy;
        if let Some((col, row)) = grid_to_screen(gx, gy, scroll, layout) {
            renderer.draw_char(col, row, tile.glyph, Color::Black, Color::Yellow);
        }
    }
}

// ── Bresenham line algorithm ──────────────────────────────────────────────────

pub fn bresenham(a: (i32, i32), b: (i32, i32)) -> Vec<(i32, i32)> {
    let (mut x0, mut y0) = a;
    let (x1, y1) = b;
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut points = Vec::new();

    loop {
        points.push((x0, y0));
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 > -dy { err -= dy; x0 += sx; }
        if e2 <  dx { err += dx; y0 += sy; }
    }
    points
}

// ── Clipboard transform helpers ───────────────────────────────────────────────

// ── Console overlay ───────────────────────────────────────────────────────────

/// Draw the console panel content area. `px/cy/pw/ch` are the content coords (below title bar).
pub fn draw_console(renderer: &mut Renderer, log: &[LogEntry], px: usize, cy: usize, pw: usize, ch: usize) {
    renderer.draw_rect_filled(px, cy, pw, ch, ' ', Color::White, Color::Black);

    let header = " F1:close  F3:clear";
    renderer.draw_str(px, cy, header, Color::Black, Color::DarkGrey);
    let err_count = log.iter().filter(|e| e.level == LogLevel::Error).count();
    if err_count > 0 {
        let badge = format!(" {} ERR ", err_count);
        let col = (px + pw).saturating_sub(badge.len());
        renderer.draw_str(col, cy, &badge, Color::White, Color::Red);
    }

    let visible = ch.saturating_sub(1);
    let start   = log.len().saturating_sub(visible);
    for (i, entry) in log.iter().skip(start).enumerate() {
        let row = cy + 1 + i;
        if row >= cy + ch { break; }
        let (prefix, pfg, tbg) = match entry.level {
            LogLevel::Error   => ("[ERR]", Color::Red,       Color::Black),
            LogLevel::Warning => ("[WRN]", Color::Yellow,    Color::Black),
            LogLevel::Info    => ("[OK] ", Color::DarkGreen, Color::Black),
        };
        let max_text = pw.saturating_sub(7);
        let text = if entry.text.len() > max_text { &entry.text[..max_text] } else { &entry.text };
        renderer.draw_str(px,     row, prefix, pfg,         tbg);
        renderer.draw_str(px + 6, row, text,   Color::White, tbg);
    }
}

// ── Inspector panel ───────────────────────────────────────────────────────────

/// Draw the inspector panel content area (below the panel chrome title bar).
/// `ix/cy/iw/ch` are the content-area coordinates. `sel_rule` highlights a rule row.
pub fn draw_inspector(
    renderer: &mut Renderer,
    tile:     Option<&TileRecord>,
    pos:      Option<(i32, i32)>,
    mode_tag: &str,
    ix: usize, cy: usize, iw: usize, ch: usize,
) {
    renderer.draw_rect_filled(ix, cy, iw, ch, ' ', Color::White, Color::DarkGrey);

    // Mode status line (cy+0)
    let mode_line = format!(" {:<width$}", mode_tag, width = iw.saturating_sub(1));
    renderer.draw_str(ix, cy, &mode_line, Color::Black, Color::Cyan);

    let Some(tile) = tile else {
        let hint = if pos.is_some() { "(empty cell)" } else { "hover a tile" };
        renderer.draw_str(ix + 1, cy + 2, hint, Color::DarkGrey, Color::DarkGrey);
        return;
    };

    let sep: String = std::iter::once(' ').chain(std::iter::repeat('-').take(iw.saturating_sub(1))).collect();

    // Position (cy+1)
    if let Some((gx, gy)) = pos {
        renderer.draw_str(ix, cy + 1, &format!(" ({},{})", gx, gy), Color::Cyan, Color::DarkGrey);
    }

    // Glyph (cy + INSP_GLYPH_OFF = cy+2)
    let glyph_str = format!("  '{}' {:<width$}", tile.glyph, "glyph", width = iw.saturating_sub(6));
    renderer.draw_str(ix, cy + INSP_GLYPH_OFF, &glyph_str, tile.fg, Color::DarkBlue);

    renderer.draw_str(ix, cy + 3, &sep, Color::DarkGrey, Color::DarkGrey);

    // Tag (cy+4 label, cy + INSP_TAG_OFF = cy+5 field)
    renderer.draw_str(ix, cy + 4, " Tag:", Color::DarkGrey, Color::DarkGrey);
    let tag_disp = if tile.tag.is_empty() { "(none)" } else { &tile.tag };
    let tag_line = format!("  {:<width$}", tag_disp, width = iw.saturating_sub(3));
    renderer.draw_str(ix, cy + INSP_TAG_OFF, &tag_line, Color::White, Color::DarkBlue);

    renderer.draw_str(ix, cy + 6, &sep, Color::DarkGrey, Color::DarkGrey);

    // Bool toggles (cy+7,8,9)
    renderer.draw_str(ix, cy + INSP_SOLID_OFF, &format!(" [{}] Solid",         if tile.solid         { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    renderer.draw_str(ix, cy + INSP_TRIG_OFF,  &format!(" [{}] Trigger",       if tile.trigger       { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    renderer.draw_str(ix, cy + INSP_CAM_OFF,   &format!(" [{}] Camera follow", if tile.camera_follow { 'x' } else { ' ' }), Color::White, Color::DarkBlue);

    renderer.draw_str(ix, cy + 10, &sep, Color::DarkGrey, Color::DarkGrey);

    // Script (cy+11 label, cy+12 = INSP_SCRIPT_OFF field)
    renderer.draw_str(ix, cy + 11, " Script:", Color::DarkGrey, Color::DarkGrey);
    let (script_disp, script_fg) = match &tile.script {
        Some(path) => {
            let short = path.rfind('/').or_else(|| path.rfind('\\'))
                .map(|i| &path[i+1..])
                .unwrap_or(path.as_str());
            (format!("  {:<width$}", short, width = iw.saturating_sub(3)), Color::White)
        }
        None => (format!("  {:<width$}", "(none)", width = iw.saturating_sub(3)), Color::DarkGrey),
    };
    renderer.draw_str(ix, cy + INSP_SCRIPT_OFF, &script_disp, script_fg, Color::DarkBlue);

    // Exit level (cy + INSP_EXIT_OFF = cy+13)
    let (exit_disp, exit_fg) = match &tile.next_level {
        Some(p) => (format!("  >{:<width$}", p, width = iw.saturating_sub(4)), Color::Cyan),
        None    => (format!("  {:<width$}", "(no exit)", width = iw.saturating_sub(3)), Color::DarkGrey),
    };
    renderer.draw_str(ix, cy + INSP_EXIT_OFF, &exit_disp, exit_fg, Color::DarkBlue);

    renderer.draw_str(ix, cy + 14, &sep, Color::DarkGrey, Color::DarkGrey);

    // Scripting section (cy+15 label, cy+16 button, cy+17 summary)
    if cy + 15 < cy + ch {
        renderer.draw_str(ix, cy + 15, " Scripting:", Color::DarkGrey, Color::DarkGrey);
    }
    if cy + INSP_GRAPH_BTN < cy + ch {
        if tile.graph.is_some() {
            let n = tile.graph.as_ref().map(|g| g.nodes.len()).unwrap_or(0);
            let e = tile.graph.as_ref().map(|g| g.edges.len()).unwrap_or(0);
            let btn = format!("  [Edit Graph]");
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::Cyan);
            if cy + 17 < cy + ch {
                let info = format!("  {} nodes  {} edges", n, e);
                let info: String = info.chars().take(iw).collect();
                renderer.draw_str(ix, cy + 17, &info, Color::DarkGrey, Color::DarkGrey);
            }
        } else {
            let btn = format!("  [New Graph]");
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::DarkGreen);
        }
    }

}

// ── Hierarchy panel ───────────────────────────────────────────────────────────

/// Draw the hierarchy panel content area (panel chrome draws the title bar above).
/// Layout: hy+0 = separator, hy+1 = Player, hy+2+ = named spawns.
pub fn draw_hierarchy(
    renderer:  &mut Renderer,
    grid:      &LevelGrid,
    hier_sel:  Option<HierarchySelection>,
    hx: usize, hy: usize, hw: usize, hh: usize,
) {
    renderer.draw_rect_filled(hx, hy, hw, hh, ' ', Color::White, Color::DarkGrey);

    let sep: String = std::iter::repeat('-').take(hw).collect();
    renderer.draw_str(hx, hy, &sep, Color::DarkGrey, Color::DarkGrey);

    // Player entry (hy+1)
    if hh > 1 {
        let player_sel = hier_sel == Some(HierarchySelection::Player);
        let label = format!(" {} Player{}", grid.player.glyph, " ".repeat(hw.saturating_sub(9)));
        if player_sel {
            renderer.draw_str(hx, hy + 1, &label, Color::Black, Color::Cyan);
        } else {
            renderer.draw_str(hx, hy + 1, &label, Color::Green, Color::DarkGrey);
        }
    }

    // Named spawns (hy+2+)
    for (i, (name, _, _)) in grid.extra_spawns.iter().enumerate() {
        let row = hy + 2 + i;
        if row >= hy + hh { break; }
        let spawn_sel = hier_sel == Some(HierarchySelection::Spawn(i));
        let max_name = hw.saturating_sub(3);
        let short: String = name.chars().take(max_name).collect();
        let label = format!(" ! {:<width$}", short, width = max_name);
        if spawn_sel {
            renderer.draw_str(hx, row, &label, Color::Black, Color::Cyan);
        } else {
            renderer.draw_str(hx, row, &label, Color::Yellow, Color::DarkGrey);
        }
    }
}

// ── Menu system ───────────────────────────────────────────────────────────────

/// Width of a dropdown panel in character columns.
pub const MENU_W: usize = 22;

/// One entry in a dropdown menu.
pub enum MenuEntry {
    Item { label: &'static str, shortcut: &'static str, action: ToolbarAction },
    Sep,
}

/// Positions and widths of each top-level menu label on the toolbar.
/// Returns slices of (start_col, label_str, kind).
fn menu_label_defs() -> &'static [(usize, &'static str, MenuKind)] {
    &[
        (1,  "File",  MenuKind::File),
        (7,  "Edit",  MenuKind::Edit),
        (13, "Level", MenuKind::Level),
        (20, "View",  MenuKind::View),
        (26, "Tools", MenuKind::Tools),
    ]
}

/// Return the start column for a given menu's label.
fn menu_label_col(kind: MenuKind) -> usize {
    menu_label_defs().iter()
        .find(|(_, _, k)| *k == kind)
        .map(|(col, _, _)| *col)
        .unwrap_or(1)
}

/// Return the full list of entries for a given menu.
/// Index 0 = first row below the menu bar; separators occupy one row each.
pub fn menu_entries(kind: MenuKind) -> Vec<MenuEntry> {
    use MenuEntry::*;
    use ToolbarAction::*;
    use ToolKind::*;
    match kind {
        MenuKind::File => vec![
            Item { label: "New Level",  shortcut: "    ", action: New    },
            Item { label: "Open...",    shortcut: "O   ", action: Open   },
            Item { label: "Save",       shortcut: "S   ", action: Save   },
            Item { label: "Save As...", shortcut: "S+S ", action: SaveAs },
            Sep,
            Item { label: "Play",       shortcut: "F5  ", action: Play   },
            Sep,
            Item { label: "Quit",       shortcut: "Esc ", action: Quit   },
        ],
        MenuKind::Edit => vec![
            Item { label: "Undo",         shortcut: "U/^Z", action: Undo        },
            Item { label: "Redo",         shortcut: "R/^Y", action: Redo        },
            Sep,
            Item { label: "Copy Select",  shortcut: "C   ", action: SetTool(Copy)  },
            Item { label: "Cut Select",   shortcut: "X   ", action: SetTool(Cut)   },
            Item { label: "Paste",        shortcut: "V   ", action: SetTool(Paste) },
            Sep,
            Item { label: "Find/Replace", shortcut: "/   ", action: FindReplace },
        ],
        MenuKind::Level => vec![
            Item { label: "New Level",    shortcut: "    ", action: NewLevel      },
            Sep,
            Item { label: "Rename Level", shortcut: "N   ", action: RenameLevel  },
            Item { label: "Resize Level", shortcut: "Z   ", action: ResizeLevel  },
            Sep,
            Item { label: "Set Spawn",    shortcut: "P   ", action: SetSpawn     },
            Item { label: "Add Spawn...", shortcut: "S+P ", action: AddNamedSpawn},
        ],
        MenuKind::View => vec![
            Item { label: "Palette",   shortcut: "B   ", action: TogglePalette   },
            Item { label: "Grid",      shortcut: "Tab ", action: ToggleGrid      },
            Item { label: "Physics",   shortcut: "G   ", action: TogglePhysics   },
            Item { label: "Stats",     shortcut: "`   ", action: ToggleStats     },
            Item { label: "Inspector", shortcut: "F2  ", action: ToggleInspector },
            Item { label: "Console",   shortcut: "F1  ", action: ToggleConsole   },
        ],
        MenuKind::Tools => vec![
            Item { label: "Paint",  shortcut: "    ", action: SetTool(Paint)  },
            Item { label: "Select", shortcut: "Q   ", action: SetTool(Select) },
            Item { label: "Rect",   shortcut: "    ", action: SetTool(Rect)   },
            Item { label: "Line",   shortcut: "L   ", action: SetTool(Line)   },
            Item { label: "Fill",   shortcut: "F   ", action: SetTool(Fill)   },
            Sep,
            Item { label: "Copy",  shortcut: "C   ", action: SetTool(Copy)   },
            Item { label: "Cut",   shortcut: "X   ", action: SetTool(Cut)    },
            Item { label: "Paste", shortcut: "V   ", action: SetTool(Paste)  },
        ],
    }
}

/// Return which menu label `col` lands on, or None if between labels.
pub fn menu_label_at(col: usize) -> Option<MenuKind> {
    for &(start, label, kind) in menu_label_defs() {
        if col >= start && col < start + label.len() {
            return Some(kind);
        }
    }
    None
}

/// Return the ToolbarAction for a click at (click_col, click_row).
/// click_row is an absolute screen row; the dropdown starts at toolbar_row + 1.
/// Returns None for separators, out-of-range, or clicks outside the column band.
pub fn menu_item_at(menu: MenuKind, click_col: usize, click_row: usize, layout: &Layout) -> Option<ToolbarAction> {
    let start_col = menu_label_col(menu);
    if click_col < start_col || click_col >= start_col + MENU_W { return None; }
    let item_idx  = click_row.saturating_sub(layout.toolbar_row + 1);
    match menu_entries(menu).into_iter().nth(item_idx) {
        Some(MenuEntry::Item { action, .. }) => Some(action),
        _ => None,
    }
}

fn menu_checkmark(action: &ToolbarAction, ms: &MenuState) -> char {
    match action {
        ToolbarAction::SetTool(t)      => if *t == ms.active_tool { '>' } else { ' ' },
        ToolbarAction::TogglePalette   => if ms.show_palette   { 'x' } else { ' ' },
        ToolbarAction::ToggleGrid      => if ms.show_grid       { 'x' } else { ' ' },
        ToolbarAction::TogglePhysics   => if ms.show_physics    { 'x' } else { ' ' },
        ToolbarAction::ToggleInspector => if ms.show_inspector  { 'x' } else { ' ' },
        ToolbarAction::ToggleConsole   => if ms.show_console    { 'x' } else { ' ' },
        ToolbarAction::ToggleStats     => if ms.show_stats      { 'x' } else { ' ' },
        _ => ' ',
    }
}

fn is_action_enabled(action: &ToolbarAction, ms: &MenuState) -> bool {
    match action {
        ToolbarAction::Undo                    => ms.can_undo,
        ToolbarAction::Redo                    => ms.can_redo,
        ToolbarAction::SetTool(ToolKind::Paste)=> ms.clipboard_full,
        _ => true,
    }
}

/// Draw the menu bar (toolbar row 1).
pub fn draw_menu_toolbar(
    renderer:    &mut Renderer,
    active_menu: Option<MenuKind>,
    active_tool: ToolKind,
    layout:      &Layout,
) {
    let row = layout.toolbar_row;
    renderer.draw_rect_filled(0, row, renderer.width, 1, ' ', Color::White, Color::DarkGrey);

    // Menu labels
    for &(col, label, kind) in menu_label_defs() {
        let open = active_menu == Some(kind);
        let (fg, bg) = if open { (Color::Black, Color::Cyan) } else { (Color::White, Color::DarkGrey) };
        let padded = format!(" {} ", label);
        renderer.draw_str(col.saturating_sub(1), row, &padded, fg, bg);
    }

    // Active tool indicator on the right
    let tool_name = match active_tool {
        ToolKind::Paint  => "Paint ",
        ToolKind::Select => "Select",
        ToolKind::Rect   => "Rect  ",
        ToolKind::Line   => "Line  ",
        ToolKind::Fill   => "Fill  ",
        ToolKind::Copy   => "Copy  ",
        ToolKind::Cut    => "Cut   ",
        ToolKind::Paste  => "Paste ",
    };
    let indicator = format!("[ {} ]", tool_name);
    let col = renderer.width.saturating_sub(indicator.len() + 1);
    renderer.draw_str(col, row, &indicator, Color::Cyan, Color::DarkGrey);
}

/// Draw the dropdown panel for an open menu.
/// Highlights the row the mouse is currently hovering over.
pub fn draw_menu_dropdown(
    renderer:  &mut Renderer,
    menu:      MenuKind,
    mouse_col: usize,
    mouse_row: usize,
    ms:        &MenuState,
    layout:    &Layout,
) {
    let start_col = menu_label_col(menu);
    let start_row = layout.toolbar_row + 1;
    let entries   = menu_entries(menu);

    // Background panel
    renderer.draw_rect_filled(start_col, start_row, MENU_W, entries.len(), ' ', Color::White, Color::Black);

    for (i, entry) in entries.iter().enumerate() {
        let row = start_row + i;
        match entry {
            MenuEntry::Sep => {
                let line: String = std::iter::repeat('-').take(MENU_W).collect();
                renderer.draw_str(start_col, row, &line, Color::DarkGrey, Color::Black);
            }
            MenuEntry::Item { label, shortcut, action } => {
                let hovered  = mouse_row == row
                    && mouse_col >= start_col
                    && mouse_col < start_col + MENU_W;
                let enabled  = is_action_enabled(action, ms);
                let check    = menu_checkmark(action, ms);
                let (fg, bg) = if !enabled {
                    (Color::DarkGrey, if hovered { Color::DarkBlue } else { Color::Black })
                } else if hovered {
                    (Color::Black, Color::Cyan)
                } else {
                    (Color::White, Color::Black)
                };
                // " [check] label....shortcut "  fits in MENU_W=22
                let text = format!(" {} {:<11} {} ", check, label, shortcut);
                renderer.draw_str(start_col, row, &text, fg, bg);
            }
        }
    }
}

// ── Physics overlay ───────────────────────────────────────────────────────────

/// Draw a physics gizmo overlay: solid tiles get a red background tint,
/// trigger tiles get a yellow tint. Drawn on top of the normal tile layer.
pub fn draw_physics_overlay(renderer: &mut Renderer, grid: &LevelGrid, scroll: (i32, i32), layout: &Layout) {
    for ((gx, gy), tile) in grid.iter() {
        let is_exit = tile.next_level.is_some();
        if !tile.solid && !tile.trigger && !is_exit { continue; }
        if let Some((col, row)) = grid_to_screen(*gx, *gy, scroll, layout) {
            if is_exit {
                renderer.draw_char(col, row, tile.glyph, Color::White, Color::Cyan);
            } else if tile.solid {
                renderer.draw_char(col, row, tile.glyph, Color::White, Color::DarkRed);
            } else {
                renderer.draw_char(col, row, tile.glyph, Color::Black, Color::DarkYellow);
            }
        }
    }
}

// ── Erase brush preview ───────────────────────────────────────────────────────

/// Draw the erase brush region as a red highlight before the erase is committed.
/// Only drawn when erase_size > 1 so single-cell erases don't obscure normal cursor.
pub fn draw_erase_preview(
    renderer:   &mut Renderer,
    grid_pos:   (i32, i32),
    erase_size: usize,
    scroll:     (i32, i32),
    layout:     &Layout,
) {
    if erase_size <= 1 { return; }
    let half = (erase_size as i32) / 2;
    let (gx, gy) = grid_pos;
    for dy in -half..=half {
        for dx in -half..=half {
            if let Some((col, row)) = grid_to_screen(gx + dx, gy + dy, scroll, layout) {
                renderer.draw_char(col, row, 'X', Color::Red, Color::DarkRed);
            }
        }
    }
}

// ── Help overlay ──────────────────────────────────────────────────────────────

/// Draw a full-canvas shortcut reference overlay. Press ? to open/close.
pub fn draw_help_overlay(renderer: &mut Renderer, layout: &Layout) {
    let cx = layout.canvas_x;
    let cw = layout.canvas_w;
    let cy = layout.canvas_y;
    let ch = layout.canvas_h;

    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, Color::Black);

    let title = " EMBER2D EDITOR — KEYBOARD SHORTCUTS ";
    renderer.draw_str(cx + 1, cy + 1, title, Color::Cyan, Color::Black);

    let sep: String = std::iter::repeat('-').take(cw.saturating_sub(2)).collect();
    renderer.draw_str(cx + 1, cy + 2, &sep, Color::DarkGrey, Color::Black);

    let col_w   = (cw.saturating_sub(4)) / 3;
    let c1 = cx + 1;
    let c2 = c1 + col_w + 1;
    let c3 = c2 + col_w + 1;

    let row = |n: usize| cy + 4 + n;

    // Column 1: Tools & Canvas
    renderer.draw_str(c1, row(0),  "TOOLS",              Color::Yellow, Color::Black);
    renderer.draw_str(c1, row(1),  " 1-9  Palette",      Color::White,  Color::Black);
    renderer.draw_str(c1, row(2),  " K    Eyedropper",   Color::White,  Color::Black);
    renderer.draw_str(c1, row(3),  " L    Line tool",    Color::White,  Color::Black);
    renderer.draw_str(c1, row(4),  " F    Flood fill",   Color::White,  Color::Black);
    renderer.draw_str(c1, row(5),  " E    Eraser size",  Color::White,  Color::Black);
    renderer.draw_str(c1, row(6),  " Q    Select mode",  Color::White,  Color::Black);
    renderer.draw_str(c1, row(7),  " ;/'  Solid/Trigger",Color::White,  Color::Black);
    renderer.draw_str(c1, row(9),  "CANVAS",             Color::Yellow, Color::Black);
    renderer.draw_str(c1, row(10), " Wheel   V-scroll",  Color::White,  Color::Black);
    renderer.draw_str(c1, row(11), " Sh+Whl  H-scroll",  Color::White,  Color::Black);
    renderer.draw_str(c1, row(12), " Arrows  Scroll",    Color::White,  Color::Black);
    renderer.draw_str(c1, row(13), " Mid-drag  Pan",     Color::White,  Color::Black);
    renderer.draw_str(c1, row(14), " Home  Center",      Color::White,  Color::Black);
    renderer.draw_str(c1, row(15), " Delete  Erase",     Color::White,  Color::Black);

    // Column 2: Edit & Level
    renderer.draw_str(c2, row(0),  "EDIT",               Color::Yellow, Color::Black);
    renderer.draw_str(c2, row(1),  " U/Ctrl+Z  Undo",    Color::White,  Color::Black);
    renderer.draw_str(c2, row(2),  " R/Ctrl+Y  Redo",    Color::White,  Color::Black);
    renderer.draw_str(c2, row(3),  " C  Copy select",    Color::White,  Color::Black);
    renderer.draw_str(c2, row(4),  " X  Cut select",     Color::White,  Color::Black);
    renderer.draw_str(c2, row(5),  " V  Paste",          Color::White,  Color::Black);
    renderer.draw_str(c2, row(6),  " /  Find+Replace",   Color::White,  Color::Black);
    renderer.draw_str(c2, row(7),  " I  Edit tag",       Color::White,  Color::Black);
    renderer.draw_str(c2, row(9),  "LEVEL",              Color::Yellow, Color::Black);
    renderer.draw_str(c2, row(10), " N  Rename",         Color::White,  Color::Black);
    renderer.draw_str(c2, row(11), " Z  Resize",         Color::White,  Color::Black);
    renderer.draw_str(c2, row(12), " P  Set spawn",      Color::White,  Color::Black);
    renderer.draw_str(c2, row(13), " Sh+P  Add spawn",   Color::White,  Color::Black);
    renderer.draw_str(c2, row(14), " T  Attach script",  Color::White,  Color::Black);
    renderer.draw_str(c2, row(15), " Sh+drag  Rect fill",Color::White,  Color::Black);

    // Column 3: View & File
    renderer.draw_str(c3, row(0),  "VIEW",               Color::Yellow, Color::Black);
    renderer.draw_str(c3, row(1),  " Tab  Grid",         Color::White,  Color::Black);
    renderer.draw_str(c3, row(2),  " G    Physics",      Color::White,  Color::Black);
    renderer.draw_str(c3, row(3),  " B    Palette",      Color::White,  Color::Black);
    renderer.draw_str(c3, row(4),  " `    Stats",        Color::White,  Color::Black);
    renderer.draw_str(c3, row(5),  " F1   Console",      Color::White,  Color::Black);
    renderer.draw_str(c3, row(6),  " F2   Inspector",    Color::White,  Color::Black);
    renderer.draw_str(c3, row(9),  "FILE",               Color::Yellow, Color::Black);
    renderer.draw_str(c3, row(10), " S    Save",         Color::White,  Color::Black);
    renderer.draw_str(c3, row(11), " Sh+S  Save As",     Color::White,  Color::Black);
    renderer.draw_str(c3, row(12), " O    Open",         Color::White,  Color::Black);
    renderer.draw_str(c3, row(13), " F5   Play preview", Color::White,  Color::Black);
    renderer.draw_str(c3, row(14), " Esc  Quit",         Color::White,  Color::Black);
    renderer.draw_str(c3, row(15), " ?    This screen",  Color::White,  Color::Black);

    let hint = "Press ? or Esc to close";
    let hcol = cx + (cw.saturating_sub(hint.len())) / 2;
    renderer.draw_str(hcol, cy + ch - 2, hint, Color::DarkGrey, Color::Black);
}

/// Apply flip/rotate transform to a clipboard offset (dx, dy).
pub fn transform_offset(
    dx: i32, dy: i32,
    max_dx: i32, max_dy: i32,
    flip_x: bool, flip_y: bool,
    rotate: i32,  // 0=none, 1=90°CW, 2=180°, 3=270°CW
) -> (i32, i32) {
    let (dx, dy) = if flip_x { (max_dx - dx, dy) } else { (dx, dy) };
    let (dx, dy) = if flip_y { (dx, max_dy - dy) } else { (dx, dy) };
    match rotate % 4 {
        0 => (dx, dy),
        1 => (max_dy - dy, dx),          // 90° CW
        2 => (max_dx - dx, max_dy - dy), // 180°
        3 => (dy, max_dx - dx),          // 270° CW
        _ => (dx, dy),
    }
}
