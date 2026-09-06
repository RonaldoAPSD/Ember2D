// editor/ui/panels/chrome.rs — Drawing functions for small, always-present
// chrome widgets (title bar, status bar, dock tabs, text-input prompt,
// confirm modal, right-click context menu).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use ember2d::renderer::{color::Color, Font, Renderer};
use ember2d_sim::level::TileRecord;
use crate::editor::palette::TilePalette;
use super::super::types::*;
use super::super::rect::UiRect;
use super::super::frame::{UiFrame, WidgetId};

pub fn draw_title_bar(renderer: &mut Renderer, font: &mut dyn Font, level_name: &str, unsaved: bool, undo_count: usize, redo_count: usize, scroll: (f32, f32), level_size: (usize, usize)) {
    renderer.draw_rect_filled(0, 0, renderer.width, 1, ' ', Color::White, Color::DarkBlue);
    renderer.draw_str(1, 0, "EMBER2D EDITOR", Color::White, Color::DarkBlue);

    let saved_marker = if unsaved { "*" } else { " " };
    let scroll_str = if scroll.0.abs() > 0.001 || scroll.1.abs() > 0.001 { format!(" @{:.1},{:.1}", scroll.0, scroll.1) } else { String::new() };
    let info = format!("{}{}  {}×{}{}  U:{} R:{}", saved_marker, level_name, level_size.0, level_size.1, scroll_str, undo_count, redo_count);
    let col = renderer.width.saturating_sub(cells(font, &info) + 1);
    renderer.draw_str(col, 0, &info, Color::Yellow, Color::DarkBlue);
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

/// Draws the dock's tab strip and registers each tab's hit rect in `frame`
/// at the exact cell span it was just drawn at (Phase 7 Part 1d,
/// docs/ember2d-phase7-plan.md) — see `ui/frame.rs`'s header comment for
/// why this one function doing both is what closes defect E5 (tab hitboxes
/// computed independently of tab drawing, in the removed
/// `PanelManager::tab_at`/`find_tab_in_row`). Callers only invoke this when
/// a dock actually has more than one panel (`docked.len() > 1`,
/// `impl_render.rs`), so a single-panel dock registers no tab hit at all —
/// fixing a real quirk the old independent hit-test had, where a
/// single-panel dock's title row still claimed an invisible "tab" hitbox
/// over its first `title.len()+2` cells even though no tab was ever drawn.
pub fn draw_dock_tabs(renderer: &mut Renderer, font: &mut dyn Font, x: usize, y: usize, w: usize, panels: &[(PanelId, &str)], active: Option<PanelId>, frame: &mut UiFrame) {
    if panels.is_empty() { return; }

    // Background for the tab bar
    renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, Color::Black);

    let mut cursor_x = x;
    for (id, title) in panels {
        let is_active = Some(*id) == active;
        let fg = if is_active { Color::White } else { Color::Grey };
        let bg = if is_active { Color::DarkBlue } else { Color::DarkGrey };

        let label = format!(" {} ", title);
        let label_w = cells(font, &label);
        if cursor_x + label_w > x + w { break; }

        renderer.draw_str(cursor_x, y, &label, fg, bg);
        frame.push(WidgetId::Tab(*id), UiRect::from_cells(cursor_x as i32, y as i32, label_w, 1));
        cursor_x += label_w + 1;
    }
}

pub fn draw_text_input(renderer: &mut Renderer, font: &mut dyn Font, prompt: &str, buffer: &str, layout: &Layout) {
    let mw = 40usize;
    let mh = 7usize;
    let mx = (layout.screen_w.saturating_sub(mw)) / 2;
    let my = (layout.screen_h.saturating_sub(mh)) / 2;

    // 1. Fill background and Draw Borders
    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);

    // Top Title Bar
    renderer.draw_rect_filled(mx, my, mw, 1, ' ', Color::White, Color::DarkBlue);
    let title = format!(" {} ", prompt.to_uppercase());
    // Bounded by this modal's own fixed cell width, not by `title`'s own
    // measured length — no `measure`/`glyph` call needed here (Phase 7
    // Part 2c, docs/ember2d-phase7-plan.md: only WIDTH-FROM-STRING sites
    // are in scope, and a fixed budget like `mw` isn't one).
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
    let clipped_buf: String = if cells(font, &label) > max_buf {
        // Keep the TAIL of the buffer visible (so the cursor at the end
        // stays on-screen while typing), prefixed with "..". Walks
        // characters from the end accumulating `Font::glyph` advances
        // rather than byte-slicing `label` at a `.len()`-derived offset —
        // routes through the trait (Part 2c) and, as a side effect, is
        // safe on non-ASCII input the old byte slice wasn't.
        let budget = max_buf.saturating_sub(2); // reserve room for the ".." prefix
        let chars: Vec<char> = label.chars().collect();
        let mut suffix_w = 0usize;
        let mut start = chars.len();
        for (i, &ch) in chars.iter().enumerate().rev() {
            let cw = (font.glyph(ch, 8.0).map(|g| g.advance).unwrap_or(8.0) / 8.0).round() as usize;
            if suffix_w + cw > budget { break; }
            suffix_w += cw;
            start = i;
        }
        format!("..{}", chars[start..].iter().collect::<String>())
    } else {
        label
    };
    let input_x = mx + (mw.saturating_sub(cells(font, &clipped_buf))) / 2;
    renderer.draw_str(input_x, input_y, &clipped_buf, Color::Black, Color::Cyan);

    // 3. Helper Text
    let hint = "[Enter] Confirm   [Esc] Cancel";
    let hint_x = mx + (mw.saturating_sub(cells(font, hint))) / 2;
    renderer.draw_str(hint_x, my + mh - 3, hint, Color::DarkGrey, Color::DarkGrey);
}

pub fn draw_confirm_modal(renderer: &mut Renderer, font: &mut dyn Font, title: &str, message: &str, layout: &Layout) {
    let mw = 40usize;
    let mh = 8usize;
    let mx = (layout.screen_w.saturating_sub(mw)) / 2;
    let my = (layout.screen_h.saturating_sub(mh)) / 2;

    renderer.draw_rect_filled(mx, my, mw, mh, ' ', Color::White, Color::DarkGrey);
    renderer.draw_rect_filled(mx, my, mw, 1, ' ', Color::White, Color::DarkBlue);
    renderer.draw_str(mx + 1, my, &format!(" {} ", title.to_uppercase()), Color::White, Color::DarkBlue);

    // Message
    let msg_x = mx + (mw.saturating_sub(cells(font, message))) / 2;
    renderer.draw_str(msg_x, my + 2, message, Color::White, Color::DarkGrey);

    // Buttons
    let btn_y = my + 5;
    let yes_x = mx + 8;
    let no_x  = mx + mw - 15;

    renderer.draw_str(yes_x, btn_y, " [ YES ] ", Color::Black, Color::Cyan);
    renderer.draw_str(no_x,  btn_y, " [ NO ]  ", Color::White, Color::Black);
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
