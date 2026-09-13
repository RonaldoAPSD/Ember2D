// editor/ui/panels/modals.rs — Drawing functions for full-screen/floating
// overlays (palette editor, advanced color picker, its swatch grid, and the
// keyboard-shortcuts help screen).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::types::*;
use super::super::widgets::{draw_button, draw_swatch, PALETTE_COLORS};
use ember2d::renderer::{color::Color, DrawSurface, Font, Texture};
use ember2d::theme::{PaletteRole, Theme};

use super::chrome::{draw_themed_frame, draw_themed_title_strip};

#[allow(clippy::too_many_arguments)]
pub fn draw_palette_editor_modal(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    chrome_tex: &Texture,
    pal: &crate::editor::palette::TileDefinition,
    focus: Option<&crate::editor::PaletteField>,
    screen_w: usize,
    screen_h: usize,
    frame: &mut UiFrame,
) {
    let mw = 36usize;
    let mh = 18usize;
    let mx = (screen_w.saturating_sub(mw)) / 2;
    let my = (screen_h.saturating_sub(mh)) / 2;

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let input_bg = theme.role_color(PaletteRole::InputBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let danger = theme.role_color(PaletteRole::Danger);

    draw_themed_frame(renderer, theme, chrome_tex, mx, my, mw, mh);
    draw_themed_title_strip(renderer, theme, chrome_tex, mx, my, mw);

    // Title
    let title = format!(" EDITING: {} ", pal.name);
    renderer.draw_str(mx + 2, my, &title, theme.role_color(PaletteRole::TitleText), panel_bg);
    renderer.draw_str(mx + mw - 4, my, "[X]", theme.role_color(PaletteRole::TitleText), panel_bg);

    // Fields
    let cx = mx + 2;
    let focus_fg = accent;

    // Name
    let is_name_focused = matches!(focus, Some(crate::editor::PaletteField::Name));
    let name_val = if is_name_focused { format!("{}█", pal.name) } else { pal.name.clone() };
    renderer.draw_str(cx, my + 2, "Name: ", text_fg, panel_bg);
    renderer.draw_str(
        cx + 7,
        my + 2,
        &format!("[{:<20}]", name_val),
        if is_name_focused { focus_fg } else { text_fg },
        input_bg,
    );

    // Glyph
    let is_glyph_focused = matches!(focus, Some(crate::editor::PaletteField::Glyph));
    let glyph_val = if is_glyph_focused { '█' } else { pal.glyph };
    renderer.draw_str(cx, my + 3, "Glyph:", text_fg, panel_bg);
    renderer.draw_str(
        cx + 7,
        my + 3,
        &format!("['{}']", glyph_val),
        if is_glyph_focused { focus_fg } else { text_fg },
        input_bg,
    );
    renderer.draw_char(cx + 9, my + 3, pal.glyph, pal.fg, pal.bg);

    // Toggles
    renderer.draw_str(
        cx,
        my + 4,
        &format!(
            "Solid: [{}]   Trigger: [{}]",
            if pal.solid { 'x' } else { ' ' },
            if pal.trigger { 'x' } else { ' ' }
        ),
        text_fg,
        panel_bg,
    );

    // Tag
    let is_tag_focused = matches!(focus, Some(crate::editor::PaletteField::Tag));
    let tag_val = if is_tag_focused { format!("{}█", pal.tag) } else { pal.tag.clone() };
    renderer.draw_str(cx, my + 5, "Tag:  ", text_fg, panel_bg);
    renderer.draw_str(
        cx + 7,
        my + 5,
        &format!("[{:<20}]", tag_val),
        if is_tag_focused { focus_fg } else { text_fg },
        input_bg,
    );

    // Color Grids — 7C-1 (master plan §5.3): `draw_swatch` registers each
    // cell's own rect, and `PALETTE_COLORS` replaces this function's,
    // `draw_color_picker`'s, and both of `input/mod.rs`'s independent
    // copies of the same 16-color array (E5's "four color tables"). The
    // swatch colors themselves are literal RGB entries a user is picking
    // from — content, not chrome — so `PALETTE_COLORS` stays untouched by
    // theming, same reasoning as the console/hierarchy/file-browser
    // semantic colors (`dock.rs`).
    renderer.draw_str(cx, my + 7, "Foreground Color:", dim, panel_bg);
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let gx = cx + (i % 8) * 3;
        let gy = my + 8 + (i / 8);
        let ch = if pal.fg == col { '*' } else { '#' };
        draw_swatch(
            renderer,
            frame,
            WidgetId::PaletteEditorSwatch { is_fg: true, index: i },
            gx,
            gy,
            3,
            &format!("[{}]", ch),
            col,
            Color::Black,
        );
    }
    let fg_custom_label = match pal.fg {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "Advanced".to_string(),
    };
    renderer.draw_str(cx, my + 10, &format!("[ {} ]", fg_custom_label), text_fg, input_bg);

    renderer.draw_str(cx, my + 11, "Background Color:", dim, panel_bg);
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let gx = cx + (i % 8) * 3;
        let gy = my + 12 + (i / 8);
        let ch = if pal.bg == col { '*' } else { '#' };
        draw_swatch(
            renderer,
            frame,
            WidgetId::PaletteEditorSwatch { is_fg: false, index: i },
            gx,
            gy,
            3,
            &format!("[{}]", ch),
            col,
            Color::Black,
        );
    }
    let bg_custom_label = match pal.bg {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "Advanced".to_string(),
    };
    renderer.draw_str(cx, my + 14, &format!("[ {} ]", bg_custom_label), text_fg, input_bg);

    // Buttons at the bottom. No themed "text-on-accent" role exists (same
    // gap `chrome.rs`'s `draw_dock_tabs` comments on), so Save & Close
    // stays plain black-on-accent. Delete uses the theme's `Danger` role
    // rather than a literal red — this button really is a destructive
    // action, so it gets the semantic-color treatment, not decorative
    // chrome.
    let btn_y = my + mh - 2;
    renderer.draw_str(mx + 2, btn_y, " [ Save & Close ] ", Color::Black, accent);
    renderer.draw_str(mx + 22, btn_y, " [ Delete ] ", theme.role_color(PaletteRole::TitleText), danger);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_color_picker_modal(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    chrome_tex: &Texture,
    hsv: (f32, f32, f32),
    is_fg: bool,
    screen_w: usize,
    screen_h: usize,
    frame: &mut UiFrame,
) {
    let mw = 44usize;
    let mh = 16usize;
    let mx = (screen_w.saturating_sub(mw)) / 2;
    let my = (screen_h.saturating_sub(mh)) / 2;

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);

    draw_themed_frame(renderer, theme, chrome_tex, mx, my, mw, mh);
    draw_themed_title_strip(renderer, theme, chrome_tex, mx, my, mw);

    // Title
    let title = format!(" ADVANCED COLOR: {} ", if is_fg { "FOREGROUND" } else { "BACKGROUND" });
    let title_fg = theme.role_color(PaletteRole::TitleText);
    renderer.draw_str(mx + 2, my, &title, title_fg, panel_bg);
    // 7C-1 (master plan §5.3): registers the title-close hitbox at the
    // exact point it's drawn, replacing `input/mod.rs`'s own
    // independently-recomputed `mx+mw-4..mx+mw-1` range (E5).
    draw_button(renderer, frame, WidgetId::ColorPickerClose, mx + mw - 4, my, 3, "[X]", title_fg, panel_bg);

    let cx = mx + 2;
    let (h, s, v) = hsv;

    // 1. Hue Bar (0..360) — a continuous drag area, not a discrete
    // button, so it's registered as one rect (`WidgetId::ColorPickerHueBar`)
    // that the input handler reads back via `UiFrame::rect_of` to compute
    // a hue percentage, rather than a per-column push. The bar itself
    // paints literal HSV-derived RGB, same reasoning as `PALETTE_COLORS`
    // above — it's the color being picked, not decorative chrome.
    renderer.draw_str(cx, my + 2, "Hue:", dim, panel_bg);
    let hbar_w = 36;
    let hbar_x = cx + 5;
    for i in 0..hbar_w {
        let hue = (i as f32 / hbar_w as f32) * 360.0;
        let col = Color::from_hsv(hue, 1.0, 1.0);
        renderer.draw_char(hbar_x + i, my + 2, ' ', Color::Reset, col);
    }
    frame.push(WidgetId::ColorPickerHueBar, UiRect::from_cells(hbar_x as i32, (my + 2) as i32, hbar_w, 1));
    let h_indicator_x = hbar_x + ((h / 360.0) * (hbar_w - 1) as f32).round() as usize;
    renderer.draw_char(h_indicator_x, my + 1, 'v', text_fg, panel_bg);

    // 2. SV Map (Saturation vs Value) — same continuous-area reasoning as
    // the hue bar above.
    renderer.draw_str(cx, my + 4, "Sat/Val Map:", dim, panel_bg);
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
    frame.push(WidgetId::ColorPickerSvMap, UiRect::from_cells(map_x as i32, map_y as i32, map_w, map_h));
    // Cursor in map
    let cur_sx = (s * (map_w - 1) as f32).round() as usize;
    let cur_sy = ((1.0 - v) * (map_h - 1) as f32).round() as usize;
    renderer.draw_char(map_x + cur_sx, map_y + cur_sy, '+', Color::White, Color::Reset);

    // 3. Current Color Preview
    let current_col = Color::from_hsv(h, s, v);
    renderer.draw_str(mx + 30, my + 6, "Selected:", dim, panel_bg);
    renderer.draw_rect_filled(mx + 30, my + 7, 8, 3, ' ', Color::Reset, current_col);

    if let Color::Rgb(r, g, b) = current_col {
        renderer.draw_str(mx + 30, my + 11, &format!("#{:02X}{:02X}{:02X}", r, g, b), accent, panel_bg);
    }

    // Buttons. No themed "text-on-accent" role exists (same gap noted
    // throughout `chrome.rs`), so Apply stays plain black-on-accent.
    let btn_y = my + mh - 2;
    draw_button(renderer, frame, WidgetId::ColorPickerApply, mx + 2, btn_y, 11, " [ Apply ] ", Color::Black, accent);
    draw_button(renderer, frame, WidgetId::ColorPickerCancel, mx + mw - 14, btn_y, 12, " [ Cancel ] ", dim, panel_bg);
}

/// A bare, frameless swatch strip — unlike `draw_palette_editor_modal`'s
/// grid, this has no call site anywhere in the editor today (grep-verified,
/// 7D-2). Left un-themed: it draws only `PALETTE_COLORS` swatches, no
/// chrome surface of its own to theme.
pub fn draw_color_picker(renderer: &mut dyn DrawSurface, x: usize, y: usize, w: usize) {
    renderer.draw_rect_filled(x, y, w, 3, ' ', Color::White, Color::Black);
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let cx = x + 1 + (i % 8) * 2;
        let cy = y + 1 + (i / 8);
        renderer.draw_char(cx, cy, '■', col, Color::Black);
    }
}

pub fn draw_help_overlay(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    cx: usize,
    cy: usize,
    cw: usize,
    ch: usize,
) {
    let bg = theme.role_color(PaletteRole::PanelBg);
    let fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, bg);
    let title = " EMBER2D EDITOR — KEYBOARD SHORTCUTS ";
    renderer.draw_str(cx + 1, cy + 1, title, accent, bg);
    let sep: String = std::iter::repeat_n('-', cw.saturating_sub(2)).collect();
    renderer.draw_str(cx + 1, cy + 2, &sep, dim, bg);
    let col_w = (cw.saturating_sub(4)) / 3;
    let c1 = cx + 1;
    let c2 = c1 + col_w + 1;
    let c3 = c2 + col_w + 1;
    let row = |n: usize| cy + 4 + n;
    renderer.draw_str(c1, row(0), "TOOLS", accent, bg);
    renderer.draw_str(c1, row(1), " 1-3  Layers", fg, bg);
    renderer.draw_str(c1, row(2), " 4-0  Palette", fg, bg);
    renderer.draw_str(c1, row(3), " L    Line tool", fg, bg);
    renderer.draw_str(c1, row(4), " F    Flood fill", fg, bg);
    renderer.draw_str(c1, row(5), " E    Eraser size", fg, bg);
    renderer.draw_str(c1, row(6), " Q    Select mode", fg, bg);
    renderer.draw_str(c1, row(7), " ;/'  Solid/Trigger", fg, bg);
    renderer.draw_str(c1, row(9), "CANVAS", accent, bg);
    renderer.draw_str(c1, row(10), " Wheel     Zoom", fg, bg);
    renderer.draw_str(c1, row(11), " Ctrl+Whl  Fast Zoom", fg, bg);
    renderer.draw_str(c1, row(12), " Arrows    Scroll", fg, bg);
    renderer.draw_str(c1, row(13), " Mid-drag  Pan", fg, bg);
    renderer.draw_str(c1, row(14), " Home      Reset View", fg, bg);
    renderer.draw_str(c1, row(15), " Delete    Erase", fg, bg);
    renderer.draw_str(c2, row(0), "EDIT", accent, bg);
    renderer.draw_str(c2, row(1), " U/Ctrl+Z  Undo", fg, bg);
    renderer.draw_str(c2, row(2), " R/Ctrl+Y  Redo", fg, bg);
    renderer.draw_str(c2, row(3), " C  Copy select", fg, bg);
    renderer.draw_str(c2, row(4), " X  Cut select", fg, bg);
    renderer.draw_str(c2, row(5), " V  Paste", fg, bg);
    renderer.draw_str(c2, row(6), " I  Edit tag", fg, bg);
    renderer.draw_str(c2, row(9), "LEVEL", accent, bg);
    renderer.draw_str(c2, row(10), " N  Rename", fg, bg);
    renderer.draw_str(c2, row(11), " Z  Resize", fg, bg);
    renderer.draw_str(c2, row(12), " P  Set spawn", fg, bg);
    renderer.draw_str(c2, row(13), " Sh+P  Add spawn", fg, bg);
    renderer.draw_str(c2, row(14), " T  Attach script", fg, bg);
    renderer.draw_str(c2, row(15), " Sh+drag  Rect fill", fg, bg);
    renderer.draw_str(c3, row(0), "VIEW", accent, bg);
    renderer.draw_str(c3, row(1), " Tab  Grid", fg, bg);
    renderer.draw_str(c3, row(2), " G    Physics", fg, bg);
    renderer.draw_str(c3, row(3), " B    Palette", fg, bg);
    renderer.draw_str(c3, row(4), " H    Hierarchy", fg, bg);
    renderer.draw_str(c3, row(5), " `    Stats", fg, bg);
    renderer.draw_str(c3, row(6), " F1   Console", fg, bg);
    renderer.draw_str(c3, row(7), " F2   Inspector", fg, bg);
    renderer.draw_str(c3, row(9), "FILE", accent, bg);
    renderer.draw_str(c3, row(10), " S    Save", fg, bg);
    renderer.draw_str(c3, row(11), " Sh+S  Save As", fg, bg);
    renderer.draw_str(c3, row(12), " O    Open", fg, bg);
    renderer.draw_str(c3, row(13), " F5   Play preview", fg, bg);
    renderer.draw_str(c3, row(14), " Esc  Cancel/Close", fg, bg);
    renderer.draw_str(c3, row(15), " ?    This screen", fg, bg);
    let hint = "Press ? or Esc to close";
    let hcol = cx + (cw.saturating_sub(cells(font, hint))) / 2;
    renderer.draw_str(hcol, cy + ch - 2, hint, dim, bg);
}
