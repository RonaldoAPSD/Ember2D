// editor/ui/panels.rs — Drawing functions for editor panels (Inspector, Palette, Console, etc.).

use std::collections::HashMap;
use ember2d::renderer::{color::Color, Renderer};
use crate::editor::grid::LevelGrid;
use crate::editor::palette::TilePalette;
use ember2d_sim::level::TileRecord;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use super::types::*;

pub fn draw_title_bar(renderer: &mut Renderer, level_name: &str, unsaved: bool, undo_count: usize, redo_count: usize, scroll: (f32, f32), level_size: (usize, usize)) {
    renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::White, Color::DarkBlue);
    renderer.draw_str(1, 0, "EMBER2D EDITOR", Color::White, Color::DarkBlue);

    let saved_marker = if unsaved { "*" } else { " " };
    let scroll_str = if scroll.0.abs() > 0.001 || scroll.1.abs() > 0.001 { format!(" @{:.1},{:.1}", scroll.0, scroll.1) } else { String::new() };
    let info = format!("{}{}  {}×{}{}  U:{} R:{}", saved_marker, level_name, level_size.0, level_size.1, scroll_str, undo_count, redo_count);
    let col = renderer.width.saturating_sub(info.len() + 1);
    renderer.draw_str(col, 0, &info, Color::Yellow, Color::DarkBlue);
}

pub fn draw_palette_panel(renderer: &mut Renderer, palette: &TilePalette, mode: Option<&str>, scroll: usize, cx: usize, cy: usize, cw: usize, ch: usize) {
    if let Some(m) = mode {
        let header = format!(" {:^width$} ", m, width = cw);
        renderer.draw_str(cx, cy, &header, Color::Black, Color::Cyan);
    }
    
    // 1. Search Bar
    let search_row = cy + 1;
    let search_label = format!(" S: [{:<width$}]", if palette.search.is_empty() { "Search..." } else { &palette.search }, width = cw.saturating_sub(6));
    renderer.draw_str(cx, search_row, &search_label, if palette.search.is_empty() { Color::DarkGrey } else { Color::White }, Color::Black);

    use crate::editor::palette::PaletteRow;
    let layout = palette.build_layout();
    let visible_rows = ch.saturating_sub(3); // Room for header, search, and buttons
    let list_start = cy + 2;

    for (i, row_item) in layout.iter().enumerate().skip(scroll).take(visible_rows) {
        let row = list_start + (i - scroll);
        if row >= cy + ch - 1 { break; }

        match row_item {
            PaletteRow::Header(name) => {
                let is_collapsed = palette.collapsed.contains(name);
                let icon = if is_collapsed { "[+]" } else { "[-]" };
                let label = format!("{} {}", icon, name);
                let clipped: String = format!("{:<width$}", label, width = cw).chars().take(cw).collect();
                renderer.draw_str(cx, row, &clipped, Color::Yellow, Color::DarkGrey);
            }
            PaletteRow::Item(idx) => {
                let tile = &palette.tiles[*idx];
                let is_selected = *idx == palette.selected;
                let row_bg = if is_selected { Color::DarkBlue } else { Color::DarkGrey };
                
                // Row background
                renderer.draw_rect_filled(cx, row, cw, 1, ' ', Color::White, row_bg);

                // Indented glyph container [ # ]
                let gx = cx + 2;
                renderer.draw_str(gx, row, "[   ]", if is_selected { Color::Cyan } else { Color::White }, row_bg);
                renderer.draw_char(gx + 2, row, tile.glyph, tile.fg, tile.bg);

                // Indented name
                let name_col = gx + 5;
                let max_name_len = cw.saturating_sub(gx - cx + 7);
                let name_disp: String = tile.name.chars().take(max_name_len).collect();
                renderer.draw_str(name_col, row, &name_disp, if is_selected { Color::White } else { Color::Grey }, row_bg);

                // Shortcut mapping (4-9, 0)
                let shortcut = match *idx { 0..=5 => Some(format!("{}", *idx + 4)), 6 => Some("0".to_string()), _ => None };
                if let Some(num) = shortcut {
                    renderer.draw_str(cx + cw - 1, row, &num, Color::Yellow, row_bg);
                }
            }
        }
    }

    // [+ New] and [ Edit ] buttons at the bottom row
    let btn_row = cy + ch - 1;
    renderer.draw_str(cx, btn_row, " [ + New ] ", Color::White, Color::DarkCyan);
    renderer.draw_str(cx + cw - 10, btn_row, " [ Edit ] ", Color::White, Color::DarkBlue);
}

pub fn draw_stats_panel(renderer: &mut Renderer, grid: &LevelGrid, palette: &TilePalette, cx: usize, cy: usize, cw: usize, ch: usize) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, tile) in grid.iter() { *counts.entry(tile.tag.clone()).or_insert(0) += 1; }
    let total = grid.tiles.len();
    for (i, def) in palette.tiles.iter().enumerate() {
        let row = cy + i;
        if row >= cy + ch - 1 { break; }
        let count = counts.get(&def.tag).copied().unwrap_or(0);
        let label = format!(" {}: {:>4}", def.name, count);
        let clipped: String = label.chars().take(cw).collect();
        renderer.draw_str(cx, row, &clipped, def.fg, Color::DarkGrey);
    }
    renderer.draw_str(cx, cy + ch - 1, &format!(" Total:{:>4}", total), Color::White, Color::DarkGrey);
}

pub fn draw_status_bar(renderer: &mut Renderer, mouse: &ember2d::mouse::MouseState, _palette: &TilePalette, grid_overlay: bool, _save_path: &str, tile_under: Option<&TileRecord>, mode_hint: &str, scroll: (f32, f32), active_layer: u8, erase_size: usize, layout: &Layout) {
    let status_row = renderer.height - 1;
    renderer.draw_rect_filled(0, status_row, renderer.width, 1, ' ', Color::White, Color::DarkGrey);
    let cx = mouse.cell_x.saturating_sub(layout.canvas_x) as f32 / layout.zoom + scroll.0;
    let cy = mouse.cell_y.saturating_sub(layout.canvas_y) as f32 / layout.zoom + scroll.1;
    let pos_str = format!(" ({:3.1},{:3.1})", cx, cy);
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
}

pub fn draw_dock_tabs(renderer: &mut Renderer, x: usize, y: usize, w: usize, panels: &[(PanelId, &str)], active: Option<PanelId>) {
    if panels.is_empty() { return; }
    
    // Background for the tab bar
    renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, Color::Black);

    let mut cursor_x = x;
    for (id, title) in panels {
        let is_active = Some(*id) == active;
        let fg = if is_active { Color::White } else { Color::Grey };
        let bg = if is_active { Color::DarkBlue } else { Color::DarkGrey };
        
        let label = format!(" {} ", title);
        if cursor_x + label.len() > x + w { break; }
        
        renderer.draw_str(cursor_x, y, &label, fg, bg);
        cursor_x += label.len() + 1;
    }
}

pub fn draw_text_input(renderer: &mut Renderer, prompt: &str, buffer: &str, layout: &Layout) {
    let mw = 40usize;
    let mh = 7usize;
    let mx = (layout.screen_w.saturating_sub(mw)) / 2;
    let my = (layout.screen_h.saturating_sub(mh)) / 2;

    // 1. Fill background and Draw Borders
    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);
    
    // Top Title Bar
    renderer.draw_rect_filled(mx, my, mw, 1, ' ', Color::White, Color::DarkBlue);
    let title = format!(" {} ", prompt.to_uppercase());
    let clipped: String = title.chars().take(mw.saturating_sub(2)).collect();
    renderer.draw_str(mx + 1, my, &clipped, Color::White, Color::DarkBlue);

    // Blended Side/Bottom borders
    let bfg = Color::Grey;
    let bbg = Color::DarkGrey;
    for row in (my + 1)..(my + mh - 1) {
        renderer.draw_char(mx, row, '|', bfg, bbg);
        renderer.draw_char(mx + mw - 1, row, '|', bfg, bbg);
    }
    let bot_line: String = std::iter::repeat('-').take(mw).collect();
    renderer.draw_str(mx, my + mh - 1, &bot_line, bfg, bbg);
    renderer.draw_char(mx, my, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(mx + mw - 1, my, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(mx, my + mh - 1, '+', bfg, bbg);
    renderer.draw_char(mx + mw - 1, my + mh - 1, '+', bfg, bbg);

    // 2. Input Field
    let input_y = my + 2;
    let label = format!("> {}█", buffer);
    let max_buf = mw.saturating_sub(6);
    let clipped_buf: String = if label.len() > max_buf {
        format!("..{}", &label[label.len() - (max_buf - 2)..])
    } else {
        label
    };
    let input_x = mx + (mw.saturating_sub(clipped_buf.len())) / 2;
    renderer.draw_str(input_x, input_y, &clipped_buf, Color::Black, Color::Cyan);

    // 3. Helper Text
    let hint = "[Enter] Confirm   [Esc] Cancel";
    let hint_x = mx + (mw.saturating_sub(hint.len())) / 2;
    renderer.draw_str(hint_x, my + mh - 3, hint, Color::DarkGrey, Color::DarkGrey);
}

pub fn draw_console(renderer: &mut Renderer, log: &[LogEntry], cx: usize, cy: usize, cw: usize, ch: usize) {
    let visible = ch;
    let start   = log.len().saturating_sub(visible);
    for (i, entry) in log.iter().skip(start).enumerate() {
        let row = cy + i;
        if row >= cy + ch { break; }
        let (prefix, pfg, tbg) = match entry.level {
            LogLevel::Error   => ("[ERR]", Color::Red,       Color::Black),
            LogLevel::Warning => ("[WRN]", Color::Yellow,    Color::Black),
            LogLevel::Info    => ("[OK] ", Color::DarkGreen, Color::Black),
        };
        let max_text = cw.saturating_sub(7);
        let text = if entry.text.len() > max_text { &entry.text[..max_text] } else { &entry.text };
        renderer.draw_str(cx,     row, prefix, pfg,         tbg);
        renderer.draw_str(cx + 6, row, text,   Color::White, tbg);
    }
}

pub fn draw_inspector(renderer: &mut Renderer, tile: Option<&TileRecord>, pos: Option<(i32, i32)>, mode_tag: &str, ix: usize, cy: usize, iw: usize, ch: usize) {
    renderer.draw_rect_filled(ix, cy, iw, ch, ' ', Color::White, Color::DarkGrey);
    let mode_line = format!(" {:<width$}", mode_tag, width = iw.saturating_sub(1));
    renderer.draw_str(ix, cy, &mode_line, Color::Black, Color::Cyan);

    let sep: String = std::iter::once(' ').chain(std::iter::repeat('-').take(iw.saturating_sub(1))).collect();

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
    if cy + 20 < cy + ch { renderer.draw_str(ix, cy + 20, &sep, Color::DarkGrey, Color::DarkGrey); }
    if cy + INSP_LAYER_OFF < cy + ch {
        renderer.draw_str(ix, cy + INSP_LAYER_OFF, " Layer:", Color::DarkGrey, Color::DarkGrey);
        let layer_disp = if tile.collider_layer.is_empty() { "(any)" } else { &tile.collider_layer };
        let layer_line = format!("  {:<width$}", layer_disp, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + INSP_LAYER_OFF, &layer_line, Color::Cyan, Color::DarkBlue);
    }
    if cy + INSP_MASK_OFF < cy + ch {
        renderer.draw_str(ix, cy + INSP_MASK_OFF, " Mask:", Color::DarkGrey, Color::DarkGrey);
        let mask_str = if tile.collider_mask.is_empty() { "(all layers)".to_string() } else { tile.collider_mask.join(",") };
        let mask_line = format!("  {:<width$}", mask_str, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + INSP_MASK_OFF, &mask_line, Color::Cyan, Color::DarkBlue);
    }
}

pub fn draw_palette_editor_modal(renderer: &mut Renderer, pal: &crate::editor::palette::TileDefinition, focus: Option<&crate::editor::PaletteField>, layout: &Layout) {
    let mw = 36usize;
    let mh = 18usize;
    let mx = (layout.screen_w.saturating_sub(mw)) / 2;
    let my = (layout.screen_h.saturating_sub(mh)) / 2;

    // Window borders
    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);
    let border_str: String = std::iter::repeat('-').take(mw).collect();
    renderer.draw_str(mx, my, &border_str, Color::Grey, Color::DarkGrey);
    renderer.draw_str(mx, my + mh - 1, &border_str, Color::Grey, Color::DarkGrey);
    for row in (my + 1)..(my + mh - 1) {
        renderer.draw_char(mx, row, '|', Color::Grey, Color::DarkGrey);
        renderer.draw_char(mx + mw - 1, row, '|', Color::Grey, Color::DarkGrey);
    }
    renderer.draw_char(mx, my, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(mx + mw - 1, my, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(mx, my + mh - 1, '+', Color::Grey, Color::DarkGrey);
    renderer.draw_char(mx + mw - 1, my + mh - 1, '+', Color::Grey, Color::DarkGrey);

    // Title
    let title = format!(" EDITING: {} ", pal.name);
    let title_bg = Color::DarkBlue;
    renderer.draw_rect_filled(mx + 1, my, mw - 2, 1, ' ', Color::White, title_bg);
    renderer.draw_str(mx + 2, my, &title, Color::White, title_bg);
    renderer.draw_str(mx + mw - 4, my, "[X]", Color::White, title_bg);

    // Fields
    let cx = mx + 2;
    let focus_fg = Color::Cyan;
    
    // Name
    let is_name_focused = matches!(focus, Some(crate::editor::PaletteField::Name));
    let name_val = if is_name_focused { format!("{}█", pal.name) } else { pal.name.clone() };
    renderer.draw_str(cx, my + 2, "Name: ", Color::White, Color::DarkGrey);
    renderer.draw_str(cx + 7, my + 2, &format!("[{:<20}]", name_val), if is_name_focused { focus_fg } else { Color::White }, Color::Black);

    // Glyph
    let is_glyph_focused = matches!(focus, Some(crate::editor::PaletteField::Glyph));
    let glyph_val = if is_glyph_focused { '█' } else { pal.glyph };
    renderer.draw_str(cx, my + 3, "Glyph:", Color::White, Color::DarkGrey);
    renderer.draw_str(cx + 7, my + 3, &format!("['{}']", glyph_val), if is_glyph_focused { focus_fg } else { Color::White }, Color::Black);
    renderer.draw_char(cx + 9, my + 3, pal.glyph, pal.fg, pal.bg);
    
    // Toggles
    renderer.draw_str(cx, my + 4, &format!("Solid: [{}]   Trigger: [{}]", if pal.solid {'x'} else {' '}, if pal.trigger {'x'} else {' '}), Color::White, Color::DarkGrey);
    
    // Tag
    let is_tag_focused = matches!(focus, Some(crate::editor::PaletteField::Tag));
    let tag_val = if is_tag_focused { format!("{}█", pal.tag) } else { pal.tag.clone() };
    renderer.draw_str(cx, my + 5, "Tag:  ", Color::White, Color::DarkGrey);
    renderer.draw_str(cx + 7, my + 5, &format!("[{:<20}]", tag_val), if is_tag_focused { focus_fg } else { Color::White }, Color::Black);

    // Color Grids
    let colors = [
        Color::Black, Color::White, Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Cyan, Color::Magenta,
        Color::DarkGrey, Color::Grey, Color::DarkRed, Color::DarkGreen, Color::DarkBlue, Color::DarkYellow, Color::DarkCyan, Color::DarkMagenta,
    ];

    renderer.draw_str(cx, my + 7, "Foreground Color:", Color::Yellow, Color::DarkGrey);
    for (i, &col) in colors.iter().enumerate() {
        let gx = cx + (i % 8) * 3;
        let gy = my + 8 + (i / 8);
        let ch = if pal.fg == col { '*' } else { '#' };
        renderer.draw_str(gx, gy, &format!("[{}]", ch), col, Color::Black);
    }
    let fg_custom_label = match pal.fg {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "Advanced".to_string(),
    };
    renderer.draw_str(cx, my + 10, &format!("[ {} ]", fg_custom_label), Color::White, Color::DarkBlue);

    renderer.draw_str(cx, my + 11, "Background Color:", Color::Yellow, Color::DarkGrey);
    for (i, &col) in colors.iter().enumerate() {
        let gx = cx + (i % 8) * 3;
        let gy = my + 12 + (i / 8);
        let ch = if pal.bg == col { '*' } else { '#' };
        renderer.draw_str(gx, gy, &format!("[{}]", ch), col, Color::Black);
    }
    let bg_custom_label = match pal.bg {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "Advanced".to_string(),
    };
    renderer.draw_str(cx, my + 14, &format!("[ {} ]", bg_custom_label), Color::White, Color::DarkBlue);

    // Buttons at the bottom
    let btn_y = my + mh - 2;
    renderer.draw_str(mx + 2, btn_y, " [ Save & Close ] ", Color::Black, Color::Cyan);
    renderer.draw_str(mx + 22, btn_y, " [ Delete ] ", Color::White, Color::DarkRed);
}

pub fn draw_color_picker_modal(renderer: &mut Renderer, hsv: (f32, f32, f32), is_fg: bool, layout: &Layout) {
    let mw = 44usize;
    let mh = 16usize;
    let mx = (layout.screen_w.saturating_sub(mw)) / 2;
    let my = (layout.screen_h.saturating_sub(mh)) / 2;

    // Window borders
    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);
    let border_str: String = std::iter::repeat('-').take(mw).collect();
    renderer.draw_str(mx, my, &border_str, Color::Grey, Color::DarkGrey);
    renderer.draw_str(mx, my + mh - 1, &border_str, Color::Grey, Color::DarkGrey);
    for row in (my + 1)..(my + mh - 1) {
        renderer.draw_char(mx, row, '|', Color::Grey, Color::DarkGrey);
        renderer.draw_char(mx + mw - 1, row, '|', Color::Grey, Color::DarkGrey);
    }
    renderer.draw_char(mx, my, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(mx + mw - 1, my, '+', Color::White, Color::DarkBlue);
    renderer.draw_char(mx, my + mh - 1, '+', Color::Grey, Color::DarkGrey);
    renderer.draw_char(mx + mw - 1, my + mh - 1, '+', Color::Grey, Color::DarkGrey);

    // Title
    let title = format!(" ADVANCED COLOR: {} ", if is_fg { "FOREGROUND" } else { "BACKGROUND" });
    let title_bg = Color::DarkBlue;
    renderer.draw_rect_filled(mx + 1, my, mw - 2, 1, ' ', Color::White, title_bg);
    renderer.draw_str(mx + 2, my, &title, Color::White, title_bg);
    renderer.draw_str(mx + mw - 4, my, "[X]", Color::White, title_bg);

    let cx = mx + 2;
    let (h, s, v) = hsv;

    // 1. Hue Bar (0..360)
    renderer.draw_str(cx, my + 2, "Hue:", Color::Yellow, Color::DarkGrey);
    let hbar_w = 36;
    let hbar_x = cx + 5;
    for i in 0..hbar_w {
        let hue = (i as f32 / hbar_w as f32) * 360.0;
        let col = Color::from_hsv(hue, 1.0, 1.0);
        renderer.draw_char(hbar_x + i, my + 2, ' ', Color::Reset, col);
    }
    let h_indicator_x = hbar_x + ((h / 360.0) * (hbar_w - 1) as f32).round() as usize;
    renderer.draw_char(h_indicator_x, my + 1, 'v', Color::White, Color::DarkGrey);

    // 2. SV Map (Saturation vs Value)
    renderer.draw_str(cx, my + 4, "Sat/Val Map:", Color::Yellow, Color::DarkGrey);
    let map_w = 20;
    let map_h = 8;
    let map_x = cx + 5;
    let map_y = my + 5;
    for sy in 0..map_h {
        for sx in 0..map_w {
            let sat = sx as f32 / (map_w - 1) as f32;
            let val = 1.0 - (sy as f32 / (map_h - 1) as f32);
            let col = Color::from_hsv(h, sat, val);
            renderer.draw_char(map_x + sx, map_y + sy, ' ', Color::Reset, col);
        }
    }
    // Cursor in map
    let cur_sx = (s * (map_w - 1) as f32).round() as usize;
    let cur_sy = ((1.0 - v) * (map_h - 1) as f32).round() as usize;
    renderer.draw_char(map_x + cur_sx, map_y + cur_sy, '+', Color::White, Color::Reset);

    // 3. Current Color Preview
    let current_col = Color::from_hsv(h, s, v);
    renderer.draw_str(mx + 30, my + 6, "Selected:", Color::White, Color::DarkGrey);
    renderer.draw_rect_filled(mx + 30, my + 7, 8, 3, ' ', Color::Reset, current_col);
    
    match current_col {
        Color::Rgb(r, g, b) => {
            renderer.draw_str(mx + 30, my + 11, &format!("#{:02X}{:02X}{:02X}", r, g, b), Color::Cyan, Color::DarkGrey);
        }
        _ => {}
    }

    // Buttons
    let btn_y = my + mh - 2;
    renderer.draw_str(mx + 2, btn_y, " [ Apply ] ", Color::Black, Color::Cyan);
    renderer.draw_str(mx + mw - 14, btn_y, " [ Cancel ] ", Color::White, Color::Black);
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

pub fn draw_file_browser_panel(renderer: &mut Renderer, files: &[String], cursor: usize, scroll: usize, current_folder: &str, cx: usize, cy: usize, cw: usize, ch: usize) {
    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, Color::DarkGrey);
    
    // Breadcrumbs / Current Path
    let path_label = format!(" Content > {}", current_folder.replace("./", "").replace("/", " > "));
    let header = format!(" {:<width$}", path_label, width = cw.saturating_sub(1));
    renderer.draw_str(cx, cy, &header, Color::Black, Color::Cyan);

    let list_start = cy + 1;
    let max_visible = ch.saturating_sub(1);
    
    if files.is_empty() {
        renderer.draw_str(cx + 1, list_start, "(empty folder)", Color::Grey, Color::DarkGrey);
        return;
    }

    for (i, raw_line) in files.iter().enumerate().skip(scroll).take(max_visible) {
        let row = list_start + (i - scroll);
        if row >= cy + ch { break; }
        
        let is_selected = i == cursor;
        let bg = if is_selected { Color::DarkBlue } else { Color::DarkGrey };
        
        renderer.draw_rect_filled(cx, row, cw, 1, ' ', Color::White, bg);

        if raw_line.contains("[UP]") {
            renderer.draw_str(cx + 1, row, " .. [PARENT FOLDER] ", Color::Yellow, bg);
            continue;
        }

        let (icon, fg, skip) = if raw_line.starts_with("/ ") {
            ("DIR", Color::Yellow, 2)
        } else if raw_line.starts_with("[] ") {
            ("LVL", Color::Cyan, 3)
        } else if raw_line.starts_with("{} ") {
            ("SCR", Color::Green, 3)
        } else {
            ("---", Color::Grey, 3)
        };

        let name = if raw_line.len() > skip { &raw_line[skip..] } else { raw_line };
        let icon_tag = format!("[{}]", icon);
        renderer.draw_str(cx + 1, row, &icon_tag, fg, bg);
        
        let name_x = cx + 7;
        let max_name_w = cw.saturating_sub(8);
        let clipped_name: String = name.chars().take(max_name_w).collect();
        renderer.draw_str(name_x, row, &clipped_name, if is_selected { Color::White } else { Color::White }, bg);
    }
}

pub fn draw_confirm_modal(renderer: &mut Renderer, title: &str, message: &str, layout: &Layout) {
    let mw = 40usize;
    let mh = 8usize;
    let mx = (layout.screen_w.saturating_sub(mw)) / 2;
    let my = (layout.screen_h.saturating_sub(mh)) / 2;

    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);
    renderer.draw_rect_filled(mx, my, mw, 1, ' ', Color::White, Color::DarkBlue);
    renderer.draw_str(mx + 1, my, &format!(" {} ", title.to_uppercase()), Color::White, Color::DarkBlue);

    // Message
    let msg_x = mx + (mw.saturating_sub(message.len())) / 2;
    renderer.draw_str(msg_x, my + 2, message, Color::White, Color::DarkGrey);

    // Buttons
    let btn_y = my + 5;
    let yes_x = mx + 8;
    let no_x  = mx + mw - 15;

    renderer.draw_str(yes_x, btn_y, " [ YES ] ", Color::Black, Color::Cyan);
    renderer.draw_str(no_x,  btn_y, " [ NO ]  ", Color::White, Color::Black);
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

pub fn draw_context_menu(renderer: &mut Renderer, menu: &ContextMenu) {
    let mw = 20usize;
    let mh = menu.items.len() + 2;
    let mx = menu.x;
    let my = menu.y;

    // Boundary check
    let mx = if mx + mw > renderer.width { renderer.width.saturating_sub(mw) } else { mx };
    let my = if my + mh > renderer.height { renderer.height.saturating_sub(mh) } else { my };

    // Fill background
    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);
    
    // Borders
    let bfg = Color::Grey;
    let bbg = Color::DarkGrey;
    for row in my..(my + mh) {
        renderer.draw_char(mx, row, '│', bfg, bbg);
        renderer.draw_char(mx + mw - 1, row, '│', bfg, bbg);
    }
    let top_line: String = std::iter::repeat('─').take(mw).collect();
    renderer.draw_str(mx, my, &top_line, bfg, bbg);
    renderer.draw_str(mx, my + mh - 1, &top_line, bfg, bbg);
    renderer.draw_char(mx, my, '┌', bfg, bbg);
    renderer.draw_char(mx + mw - 1, my, '┐', bfg, bbg);
    renderer.draw_char(mx, my + mh - 1, '└', bfg, bbg);
    renderer.draw_char(mx + mw - 1, my + mh - 1, '┘', bfg, bbg);

    for (i, (label, _)) in menu.items.iter().enumerate() {
        let row = my + 1 + i;
        let is_selected = i == menu.selected;
        let fg = if is_selected { Color::Black } else { Color::White };
        let bg = if is_selected { Color::Cyan } else { Color::DarkGrey };
        
        let mut text = format!(" {:<width$} ", label, width = mw - 2);
        if text.len() > mw { text = text.chars().take(mw).collect(); }
        renderer.draw_str(mx, row, &text, fg, bg);
    }
}
