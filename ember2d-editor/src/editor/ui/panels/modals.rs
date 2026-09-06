// editor/ui/panels/modals.rs — Drawing functions for full-screen/floating
// overlays (palette editor, advanced color picker, its swatch grid, and the
// keyboard-shortcuts help screen).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use ember2d::renderer::{color::Color, Font, Renderer};
use super::super::types::*;

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

pub fn draw_help_overlay(renderer: &mut Renderer, font: &mut dyn Font, layout: &Layout) {
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
    let hcol = cx + (cw.saturating_sub(cells(font, hint))) / 2;
    renderer.draw_str(hcol, cy + ch - 2, hint, Color::DarkGrey, Color::Black);
}
