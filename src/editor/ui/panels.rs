// editor/ui/panels.rs — Drawing functions for editor panels (Inspector, Palette, Console, etc.).

use std::collections::HashMap;
use crate::renderer::{color::Color, Renderer};
use crate::editor::grid::LevelGrid;
use crate::editor::palette::TilePalette;
use crate::level::TileRecord;
use crate::scripting::{LogEntry, LogLevel};
use super::types::*;

pub fn draw_title_bar(renderer: &mut Renderer, level_name: &str, unsaved: bool, undo_count: usize, redo_count: usize, scroll: (i32, i32), level_size: (usize, usize)) {
    renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::White, Color::DarkBlue);
    renderer.draw_str(1, 0, "EMBER2D EDITOR", Color::White, Color::DarkBlue);

    let saved_marker = if unsaved { "*" } else { " " };
    let scroll_str = if scroll != (0, 0) { format!(" @{},{}", scroll.0, scroll.1) } else { String::new() };
    let info = format!("{}{}  {}×{}{}  U:{} R:{}", saved_marker, level_name, level_size.0, level_size.1, scroll_str, undo_count, redo_count);
    let col = renderer.width.saturating_sub(info.len() + 1);
    renderer.draw_str(col, 0, &info, Color::Yellow, Color::DarkBlue);
}

pub fn draw_palette_panel(renderer: &mut Renderer, palette: &TilePalette, mode: Option<&str>, scroll: usize, px: usize, cy: usize, pw: usize, ch: usize) {
    renderer.draw_rect_filled(px, cy, pw, ch, ' ', Color::White, Color::DarkGrey);
    if let Some(m) = mode {
        let header = format!(" {:^width$} ", m, width = pw.saturating_sub(2));
        renderer.draw_str(px, cy, &header, Color::Black, Color::Cyan);
    }
    
    let visible_rows = ch.saturating_sub(2);
    for (i, tile) in palette.tiles.iter().enumerate().skip(scroll).take(visible_rows) {
        let row = cy + 1 + (i - scroll);
        let is_selected = i == palette.selected;
        let row_bg = if is_selected { Color::DarkBlue } else { Color::DarkGrey };
        
        // 1. Draw row background
        renderer.draw_rect_filled(px, row, pw, 1, ' ', Color::White, row_bg);

        // 2. Draw glyph in a "container" [ # ]
        let glyph_col = px;
        renderer.draw_str(glyph_col, row, "[   ]", if is_selected { Color::Cyan } else { Color::White }, row_bg);
        renderer.draw_char(glyph_col + 2, row, tile.glyph, tile.fg, tile.bg);

        // 3. Draw name (well-spaced)
        let name_col = glyph_col + 5;
        let max_name_len = pw.saturating_sub(9); // Leave room for glyph and shortcut
        let name_disp: String = tile.name.chars().take(max_name_len).collect();
        renderer.draw_str(name_col, row, &name_disp, if is_selected { Color::White } else { Color::Grey }, row_bg);

        // 4. Draw shortcut number (4-9, then 0)
        let shortcut = match i {
            0..=5 => Some(format!("{}", i + 4)),
            6     => Some("0".to_string()),
            _     => None,
        };
        if let Some(num) = shortcut {
            let num_col = px + pw - 2;
            renderer.draw_str(num_col, row, &num, Color::Yellow, row_bg);
        }
    }

    // Draw [+ New] button at bottom
    let btn_row = cy + ch - 1;
    renderer.draw_str(px, btn_row, " [ + New Item ] ", Color::White, Color::DarkCyan);
    
    // Scroll indicators
    if scroll > 0 {
        renderer.draw_char(px + pw - 1, cy + 1, '^', Color::Cyan, Color::DarkGrey);
    }
    if scroll + visible_rows < palette.tiles.len() {
        renderer.draw_char(px + pw - 1, cy + visible_rows, 'v', Color::Cyan, Color::DarkGrey);
    }
}

pub fn draw_stats_panel(renderer: &mut Renderer, grid: &LevelGrid, palette: &TilePalette, px: usize, cy: usize, pw: usize, ch: usize) {
    renderer.draw_rect_filled(px, cy, pw, ch, ' ', Color::White, Color::DarkGrey);
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, tile) in grid.iter() { *counts.entry(tile.tag.clone()).or_insert(0) += 1; }
    let total = grid.tiles.len();
    for (i, def) in palette.tiles.iter().enumerate() {
        let row = cy + 1 + i;
        if row >= cy + ch - 2 { break; }
        let count = counts.get(&def.tag).copied().unwrap_or(0);
        let label = format!(" {}: {:>4}", def.name, count);
        renderer.draw_str(px, row, &label, def.fg, Color::DarkGrey);
    }
    let total_row = cy + ch - 2;
    if total_row >= cy && total_row < cy + ch {
        renderer.draw_str(px, total_row, &format!(" Total:{:>4}", total), Color::White, Color::DarkGrey);
    }
}

pub fn draw_status_bar(renderer: &mut Renderer, mouse: &crate::mouse::MouseState, _palette: &TilePalette, grid_overlay: bool, save_path: &str, tile_under: Option<&TileRecord>, mode_hint: &str, scroll: (i32, i32), active_layer: u8, erase_size: usize, layout: &Layout) {
    let status_row = renderer.height - 1;
    renderer.draw_rect_filled(0, status_row, renderer.width, 1, ' ', Color::White, Color::DarkGrey);
    let cx = mouse.cell_x.saturating_sub(layout.canvas_x) as i32 + scroll.0;
    let cy = mouse.cell_y.saturating_sub(layout.canvas_y) as i32 + scroll.1;
    let pos_str = format!(" ({:3},{:3})", cx, cy);
    renderer.draw_str(0, status_row, &pos_str, Color::Cyan, Color::DarkGrey);
    
    let lyr_name = match active_layer { 0 => "Background", 1 => "Main", 2 => "Foreground", _ => "Unknown" };
    let lyr_str = format!("[LAYER: {}]", lyr_name);
    renderer.draw_str(10, status_row, &lyr_str, Color::White, Color::DarkGrey);

    if !mode_hint.is_empty() {
        renderer.draw_str(30, status_row, &format!("| {}", mode_hint), Color::White, Color::DarkGrey);
    } else if let Some(tile) = tile_under {
        let script_mark = if tile.script.is_some() { "[S]" } else { "   " };
        let props = format!("| [{}] s:{} t:{} {} T=script", tile.tag, tile.solid as u8, tile.trigger as u8, script_mark);
        renderer.draw_str(30, status_row, &props, Color::Yellow, Color::DarkGrey);
    } else {
        let grid_hint = if grid_overlay { "Tab:off" } else { "Tab:grd" };
        let erase_hint = format!("E:{}px", erase_size);
        let hints = format!("| {} S:save U:undo R:redo {}", erase_hint, grid_hint);
        renderer.draw_str(30, status_row, &hints, Color::White, Color::DarkGrey);
    }
    if !save_path.is_empty() {
        let path_display = if save_path.len() > 14 { format!("..{}", &save_path[save_path.len() - 12..]) } else { save_path.to_string() };
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

pub fn draw_file_browser(renderer: &mut Renderer, files: &[String], cursor: usize, layout: &Layout) {
    renderer.draw_rect_filled(layout.canvas_x, layout.canvas_y, layout.canvas_w, layout.canvas_h, ' ', Color::DarkGrey, Color::Black);
    let title = " OPEN LEVEL — ↑↓: navigate  Enter: open  Esc: cancel ";
    renderer.draw_str(layout.canvas_x + 1, layout.canvas_y + 1, title, Color::White, Color::Black);
    let list_start = layout.canvas_y + 3;
    let max_visible = layout.canvas_h.saturating_sub(4);
    let list_offset = if cursor >= max_visible { cursor - max_visible + 1 } else { 0 };
    if files.is_empty() {
        renderer.draw_str(layout.canvas_x + 3, list_start, "No .level files found in current directory.", Color::DarkGrey, Color::Black);
        return;
    }
    for (i, file) in files.iter().enumerate().skip(list_offset).take(max_visible) {
        let row = list_start + (i - list_offset);
        if row >= layout.canvas_y + layout.canvas_h { break; }
        let display = file.rfind('/').or_else(|| file.rfind('\\')).map(|p| &file[p + 1..]).unwrap_or(file.as_str());
        let label = format!("  {}  ", display);
        if i == cursor { renderer.draw_str(layout.canvas_x + 1, row, &label, Color::Black, Color::Cyan); }
        else { renderer.draw_str(layout.canvas_x + 1, row, &label, Color::White, Color::Black); }
    }
}

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

pub fn draw_inspector(renderer: &mut Renderer, tile: Option<&TileRecord>, pal_item: Option<&crate::editor::palette::TileDefinition>, palette_color_picker: Option<bool>, pos: Option<(i32, i32)>, mode_tag: &str, ix: usize, cy: usize, iw: usize, ch: usize) {
    renderer.draw_rect_filled(ix, cy, iw, ch, ' ', Color::White, Color::DarkGrey);
    let mode_line = format!(" {:<width$}", mode_tag, width = iw.saturating_sub(1));
    renderer.draw_str(ix, cy, &mode_line, Color::Black, Color::Cyan);

    let sep: String = std::iter::once(' ').chain(std::iter::repeat('-').take(iw.saturating_sub(1))).collect();

    if let Some(pal) = pal_item {
        // ── Palette Edit Mode ────────────────────────────────────────────────
        renderer.draw_str(ix, cy + 1, " EDITING PALETTE", Color::Yellow, Color::DarkGrey);
        let name_line = format!(" Name: {:<width$}", pal.name, width = iw.saturating_sub(7));
        renderer.draw_str(ix, cy + INSP_NAME_OFF, &name_line, Color::White, Color::DarkBlue);
        
        let glyph_str = format!(" Glyph: '{}' ", pal.glyph);
        renderer.draw_str(ix, cy + INSP_GLYPH_OFF, &glyph_str, pal.fg, Color::DarkBlue);
        
        renderer.draw_str(ix, cy + 4, &sep, Color::DarkGrey, Color::DarkGrey);
        
        // FG Color with block
        let fg_row = cy + INSP_FG_OFF;
        renderer.draw_str(ix, fg_row, " FG: [ ] ", Color::White, Color::DarkBlue);
        renderer.draw_char(ix + 6, fg_row, '■', pal.fg, Color::DarkBlue);
        renderer.draw_str(ix + 9, fg_row, &format!("{:?}", pal.fg), Color::Grey, Color::DarkBlue);

        // BG Color with block
        let bg_row = cy + INSP_BG_OFF;
        renderer.draw_str(ix, bg_row, " BG: [ ] ", Color::White, Color::DarkBlue);
        renderer.draw_char(ix + 6, bg_row, '■', pal.bg, Color::DarkBlue);
        renderer.draw_str(ix + 9, bg_row, &format!("{:?}", pal.bg), Color::Grey, Color::DarkBlue);

        if let Some(is_fg) = palette_color_picker {
            draw_color_picker(renderer, ix + 1, if is_fg { fg_row + 1 } else { bg_row + 1 }, iw - 2);
        }

        renderer.draw_str(ix, cy + 8, &sep, Color::DarkGrey, Color::DarkGrey);
        renderer.draw_str(ix, cy + INSP_SOLID_OFF, &format!(" [{}] Solid", if pal.solid { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
        renderer.draw_str(ix, cy + INSP_TRIG_OFF, &format!(" [{}] Trigger", if pal.trigger { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
        
        renderer.draw_str(ix, cy + 11, &sep, Color::DarkGrey, Color::DarkGrey);
        renderer.draw_str(ix, cy + 12, " Tag:", Color::DarkGrey, Color::DarkGrey);
        let tag_line = format!("  {:<width$}", if pal.tag.is_empty() { "(none)" } else { &pal.tag }, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + 13, &tag_line, Color::White, Color::DarkBlue);
        return;
    }

    let Some(tile) = tile else {
        let hint = if pos.is_some() { "(empty cell)" } else { "hover a tile" };
        renderer.draw_str(ix + 1, cy + 2, hint, Color::DarkGrey, Color::DarkGrey);
        return;
    };
    
    // ── Tile Edit Mode ───────────────────────────────────────────────────
    if let Some((gx, gy)) = pos { renderer.draw_str(ix, cy + 1, &format!(" ({},{})", gx, gy), Color::Cyan, Color::DarkGrey); }
    let glyph_str = format!("  '{}' {:<width$}", tile.glyph, "glyph", width = iw.saturating_sub(6));
    renderer.draw_str(ix, cy + INSP_GLYPH_OFF, &glyph_str, tile.fg, Color::DarkBlue);
    renderer.draw_str(ix, cy + 4, &sep, Color::DarkGrey, Color::DarkGrey);
    renderer.draw_str(ix, cy + 5, " Tag:", Color::DarkGrey, Color::DarkGrey);
    let tag_disp = if tile.tag.is_empty() { "(none)" } else { &tile.tag };
    let tag_line = format!("  {:<width$}", tag_disp, width = iw.saturating_sub(3));
    renderer.draw_str(ix, cy + INSP_TAG_OFF + 1, &tag_line, Color::White, Color::DarkBlue);
    renderer.draw_str(ix, cy + 7, &sep, Color::DarkGrey, Color::DarkGrey);
    renderer.draw_str(ix, cy + INSP_SOLID_OFF, &format!(" [{}] Solid", if tile.solid { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    renderer.draw_str(ix, cy + INSP_TRIG_OFF, &format!(" [{}] Trigger", if tile.trigger { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    renderer.draw_str(ix, cy + INSP_CAM_OFF, &format!(" [{}] Camera follow", if tile.camera_follow { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    renderer.draw_str(ix, cy + 12, &sep, Color::DarkGrey, Color::DarkGrey);
    renderer.draw_str(ix, cy + 13, " Script:", Color::DarkGrey, Color::DarkGrey);
    let (script_disp, script_fg) = match &tile.script {
        Some(path) => {
            let short = path.rfind('/').or_else(|| path.rfind('\\')).map(|i| &path[i+1..]).unwrap_or(path.as_str());
            (format!("  {:<width$}", short, width = iw.saturating_sub(3)), Color::White)
        }
        None => (format!("  {:<width$}", "(none)", width = iw.saturating_sub(3)), Color::DarkGrey),
    };
    renderer.draw_str(ix, cy + INSP_SCRIPT_OFF, &script_disp, script_fg, Color::DarkBlue);
    let (exit_disp, exit_fg) = match &tile.next_level {
        Some(p) => (format!("  >{:<width$}", p, width = iw.saturating_sub(4)), Color::Cyan),
        None    => (format!("  {:<width$}", "(no exit)", width = iw.saturating_sub(3)), Color::DarkGrey),
    };
    renderer.draw_str(ix, cy + INSP_EXIT_OFF, &exit_disp, exit_fg, Color::DarkBlue);
    renderer.draw_str(ix, cy + 16, &sep, Color::DarkGrey, Color::DarkGrey);
    if cy + 17 < cy + ch { renderer.draw_str(ix, cy + 17, " Scripting:", Color::DarkGrey, Color::DarkGrey); }
    if cy + INSP_GRAPH_BTN < cy + ch {
        if tile.graph.is_some() {
            let n = tile.graph.as_ref().map(|g| g.nodes.len()).unwrap_or(0);
            let e = tile.graph.as_ref().map(|g| g.edges.len()).unwrap_or(0);
            let btn = format!("  [Edit Graph]");
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::Cyan);
            if cy + 19 < cy + ch {
                let info = format!("  {} nodes  {} edges", n, e);
                let info: String = info.chars().take(iw).collect();
                renderer.draw_str(ix, cy + 19, &info, Color::DarkGrey, Color::DarkGrey);
            }
        } else {
            let btn = format!("  [New Graph]");
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::DarkGreen);
        }
    }
}

pub fn draw_hierarchy(renderer: &mut Renderer, grid: &LevelGrid, hier_sel: Option<HierarchySelection>, hx: usize, hy: usize, hw: usize, hh: usize) {
    renderer.draw_rect_filled(hx, hy, hw, hh, ' ', Color::White, Color::DarkGrey);
    let sep: String = std::iter::repeat('-').take(hw).collect();
    renderer.draw_str(hx, hy, &sep, Color::DarkGrey, Color::DarkGrey);
    if hh > 1 {
        let player_sel = hier_sel == Some(HierarchySelection::Player);
        let label = format!(" {} Player{}", grid.player.glyph, " ".repeat(hw.saturating_sub(9)));
        if player_sel { renderer.draw_str(hx, hy + 1, &label, Color::Black, Color::Cyan); }
        else { renderer.draw_str(hx, hy + 1, &label, Color::Green, Color::DarkGrey); }
    }
    for (i, (name, _, _)) in grid.extra_spawns.iter().enumerate() {
        let row = hy + 2 + i;
        if row >= hy + hh { break; }
        let spawn_sel = hier_sel == Some(HierarchySelection::Spawn(i));
        let max_name = hw.saturating_sub(3);
        let short: String = name.chars().take(max_name).collect();
        let label = format!(" ! {:<width$}", short, width = max_name);
        if spawn_sel { renderer.draw_str(hx, row, &label, Color::Black, Color::Cyan); }
        else { renderer.draw_str(hx, row, &label, Color::Yellow, Color::DarkGrey); }
    }
}

pub fn draw_color_picker(renderer: &mut Renderer, x: usize, y: usize, w: usize) {
    let colors = [
        Color::Black, Color::White, Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Cyan, Color::Magenta,
        Color::DarkGrey, Color::Grey, Color::DarkRed, Color::DarkGreen, Color::DarkBlue, Color::DarkYellow, Color::DarkCyan, Color::DarkMagenta,
    ];
    renderer.draw_rect_filled(x, y, w, 3, ' ', Color::White, Color::Black);
    for (i, &col) in colors.iter().enumerate() {
        let cx = x + 1 + (i % 8) * 2;
        let cy = y + 1 + (i / 8);
        renderer.draw_char(cx, cy, '■', col, Color::Black);
    }
}

pub fn draw_help_overlay(renderer: &mut Renderer, layout: &Layout) {
    let cx = layout.canvas_x; let cw = layout.canvas_w; let cy = layout.canvas_y; let ch = layout.canvas_h;
    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, Color::Black);
    let title = " EMBER2D EDITOR — KEYBOARD SHORTCUTS ";
    renderer.draw_str(cx + 1, cy + 1, title, Color::Cyan, Color::Black);
    let sep: String = std::iter::repeat('-').take(cw.saturating_sub(2)).collect();
    renderer.draw_str(cx + 1, cy + 2, &sep, Color::DarkGrey, Color::Black);
    let col_w = (cw.saturating_sub(4)) / 3;
    let c1 = cx + 1; let c2 = c1 + col_w + 1; let c3 = c2 + col_w + 1;
    let row = |n: usize| cy + 4 + n;
    renderer.draw_str(c1, row(0), "TOOLS", Color::Yellow, Color::Black);
    renderer.draw_str(c1, row(1), " 1-3  Layers", Color::White, Color::Black);
    renderer.draw_str(c1, row(2), " 4-0  Palette", Color::White, Color::Black);
    renderer.draw_str(c1, row(3), " L    Line tool", Color::White, Color::Black);
    renderer.draw_str(c1, row(4), " F    Flood fill", Color::White, Color::Black);
    renderer.draw_str(c1, row(5), " E    Eraser size", Color::White, Color::Black);
    renderer.draw_str(c1, row(6), " Q    Select mode", Color::White, Color::Black);
    renderer.draw_str(c1, row(7), " ;/'  Solid/Trigger", Color::White, Color::Black);
    renderer.draw_str(c1, row(9), "CANVAS", Color::Yellow, Color::Black);
    renderer.draw_str(c1, row(10), " Wheel     Zoom", Color::White, Color::Black);
    renderer.draw_str(c1, row(11), " Ctrl+Whl  Fast Zoom", Color::White, Color::Black);
    renderer.draw_str(c1, row(12), " Arrows    Scroll", Color::White, Color::Black);
    renderer.draw_str(c1, row(13), " Mid-drag  Pan", Color::White, Color::Black);
    renderer.draw_str(c1, row(14), " Home      Reset View", Color::White, Color::Black);
    renderer.draw_str(c1, row(15), " Delete    Erase", Color::White, Color::Black);
    renderer.draw_str(c2, row(0), "EDIT", Color::Yellow, Color::Black);
    renderer.draw_str(c2, row(1), " U/Ctrl+Z  Undo", Color::White, Color::Black);
    renderer.draw_str(c2, row(2), " R/Ctrl+Y  Redo", Color::White, Color::Black);
    renderer.draw_str(c2, row(3), " C  Copy select", Color::White, Color::Black);
    renderer.draw_str(c2, row(4), " X  Cut select", Color::White, Color::Black);
    renderer.draw_str(c2, row(5), " V  Paste", Color::White, Color::Black);
    renderer.draw_str(c2, row(6), " I  Edit tag", Color::White, Color::Black);
    renderer.draw_str(c2, row(9), "LEVEL", Color::Yellow, Color::Black);
    renderer.draw_str(c2, row(10), " N  Rename", Color::White, Color::Black);
    renderer.draw_str(c2, row(11), " Z  Resize", Color::White, Color::Black);
    renderer.draw_str(c2, row(12), " P  Set spawn", Color::White, Color::Black);
    renderer.draw_str(c2, row(13), " Sh+P  Add spawn", Color::White, Color::Black);
    renderer.draw_str(c2, row(14), " T  Attach script", Color::White, Color::Black);
    renderer.draw_str(c2, row(15), " Sh+drag  Rect fill", Color::White, Color::Black);
    renderer.draw_str(c3, row(0), "VIEW", Color::Yellow, Color::Black);
    renderer.draw_str(c3, row(1), " Tab  Grid", Color::White, Color::Black);
    renderer.draw_str(c3, row(2), " G    Physics", Color::White, Color::Black);
    renderer.draw_str(c3, row(3), " B    Palette", Color::White, Color::Black);
    renderer.draw_str(c3, row(4), " H    Hierarchy", Color::White, Color::Black);
    renderer.draw_str(c3, row(5), " `    Stats", Color::White, Color::Black);
    renderer.draw_str(c3, row(6), " F1   Console", Color::White, Color::Black);
    renderer.draw_str(c3, row(7), " F2   Inspector", Color::White, Color::Black);
    renderer.draw_str(c3, row(9), "FILE", Color::Yellow, Color::Black);
    renderer.draw_str(c3, row(10), " S    Save", Color::White, Color::Black);
    renderer.draw_str(c3, row(11), " Sh+S  Save As", Color::White, Color::Black);
    renderer.draw_str(c3, row(12), " O    Open", Color::White, Color::Black);
    renderer.draw_str(c3, row(13), " F5   Play preview", Color::White, Color::Black);
    renderer.draw_str(c3, row(14), " Esc  Cancel/Close", Color::White, Color::Black);
    renderer.draw_str(c3, row(15), " ?    This screen", Color::White, Color::Black);
    let hint = "Press ? or Esc to close";
    let hcol = cx + (cw.saturating_sub(hint.len())) / 2;
    renderer.draw_str(hcol, cy + ch - 2, hint, Color::DarkGrey, Color::Black);
}
