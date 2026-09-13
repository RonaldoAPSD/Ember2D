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
use super::super::widgets::{draw_button_px, draw_swatch_px, draw_text_row, PALETTE_COLORS};
use ember2d::renderer::{color::Color, DrawSurface, Font, Texture, CELL_H, CELL_W};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;

use super::chrome::{draw_themed_frame, draw_themed_title_strip};

/// `row_h` here is deliberately `CELL_H`, NOT `theme.metrics.row_h` the
/// way every other converted panel uses — `input/mod.rs`'s
/// `handle_palette_editor_input` (and `handle_color_picker_input` below)
/// still hit-test most of this modal's rows by comparing `mouse.cell_y`
/// against `my + N` (a genuine, pre-existing E5-class independent
/// recompute, never migrated to `UiFrame` for this modal). Real `row_h`
/// would land rows on non-integer cell boundaries and break that
/// comparison outright, not just misalign it by a few pixels — migrating
/// this modal's own input handling to `UiFrame` is real, separate work
/// this conversion isn't attempting. Text still renders through the
/// theme's real font at `font_sizes.body`; only the vertical RHYTHM
/// stays cell-locked.
#[allow(clippy::too_many_arguments)]
pub fn draw_palette_editor_modal(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    pal: &crate::editor::palette::TileDefinition,
    focus: Option<&crate::editor::PaletteField>,
    screen_w: f32,
    screen_h: f32,
    frame: &mut UiFrame,
) {
    const MH_ROWS: usize = 18;
    let row_h = CELL_H as f32;
    let text_px = theme.font_sizes.body;
    let mw = 36.0 * CELL_W as f32;
    let mh = MH_ROWS as f32 * row_h;
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    let row_rect = |i: usize| Rect::new(mx, my + i as f32 * row_h, mw, row_h);

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let input_bg = theme.role_color(PaletteRole::InputBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let danger = theme.role_color(PaletteRole::Danger);

    draw_themed_frame(renderer, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(renderer, theme, chrome_tex, mx, my, mw);

    // Title
    let title = format!(" EDITING: {} ", pal.name);
    let title_fg = theme.role_color(PaletteRole::TitleText);
    let cx = mx + 2.0 * CELL_W as f32;
    let title_w = font.measure(&title, text_px).0;
    draw_text_row(renderer, font, &title, Rect::new(cx, my, title_w, row_h), text_px, title_fg, panel_bg);
    let close_w = font.measure("[X]", text_px).0;
    draw_text_row(renderer, font, "[X]", Rect::new(mx + mw - close_w - CELL_W as f32, my, close_w, row_h), text_px, title_fg, panel_bg);

    // Name
    let is_name_focused = matches!(focus, Some(crate::editor::PaletteField::Name));
    let name_val = if is_name_focused { format!("{}█", pal.name) } else { pal.name.clone() };
    draw_text_row(renderer, font, "Name: ", row_rect(2), text_px, text_fg, panel_bg);
    let name_x = cx + font.measure("Name: ", text_px).0;
    let name_field = format!("[{:<20}]", name_val);
    let name_field_w = font.measure(&name_field, text_px).0;
    draw_text_row(
        renderer,
        font,
        &name_field,
        Rect::new(name_x, row_rect(2).y, name_field_w, row_h),
        text_px,
        if is_name_focused { accent } else { text_fg },
        input_bg,
    );

    // Glyph
    let is_glyph_focused = matches!(focus, Some(crate::editor::PaletteField::Glyph));
    let glyph_val = if is_glyph_focused { '█' } else { pal.glyph };
    draw_text_row(renderer, font, "Glyph:", row_rect(3), text_px, text_fg, panel_bg);
    let glyph_x = cx + font.measure("Glyph:", text_px).0;
    let glyph_field = format!("['{}']", glyph_val);
    let glyph_field_w = font.measure(&glyph_field, text_px).0;
    draw_text_row(
        renderer,
        font,
        &glyph_field,
        Rect::new(glyph_x, row_rect(3).y, glyph_field_w, row_h),
        text_px,
        if is_glyph_focused { accent } else { text_fg },
        input_bg,
    );
    // The glyph preview stays on the engine's own bitmap-font pipeline —
    // same "literal in-game preview, not chrome" reasoning `dock.rs`'s
    // `draw_palette_panel` already documents for its own glyph preview.
    let preview_cell_x = ((glyph_x + font.measure("['", text_px).0) / CELL_W as f32).round() as usize;
    let preview_cell_y = (row_rect(3).y / CELL_H as f32).round() as usize;
    renderer.draw_char(preview_cell_x, preview_cell_y, pal.glyph, pal.fg, pal.bg);

    // Toggles
    let toggles = format!(
        "Solid: [{}]   Trigger: [{}]",
        if pal.solid { 'x' } else { ' ' },
        if pal.trigger { 'x' } else { ' ' }
    );
    draw_text_row(renderer, font, &toggles, row_rect(4), text_px, text_fg, panel_bg);

    // Tag
    let is_tag_focused = matches!(focus, Some(crate::editor::PaletteField::Tag));
    let tag_val = if is_tag_focused { format!("{}█", pal.tag) } else { pal.tag.clone() };
    draw_text_row(renderer, font, "Tag:  ", row_rect(5), text_px, text_fg, panel_bg);
    let tag_x = cx + font.measure("Tag:  ", text_px).0;
    let tag_field = format!("[{:<20}]", tag_val);
    let tag_field_w = font.measure(&tag_field, text_px).0;
    draw_text_row(
        renderer,
        font,
        &tag_field,
        Rect::new(tag_x, row_rect(5).y, tag_field_w, row_h),
        text_px,
        if is_tag_focused { accent } else { text_fg },
        input_bg,
    );

    // Color Grids — 7C-1 (master plan §5.3): `draw_swatch_px` registers
    // each cell's own rect, and `PALETTE_COLORS` replaces this function's,
    // `draw_color_picker`'s, and both of `input/mod.rs`'s independent
    // copies of the same 16-color array (E5's "four color tables"). The
    // swatch colors themselves are literal RGB entries a user is picking
    // from — content, not chrome — so `PALETTE_COLORS` stays untouched by
    // theming, same reasoning as the console/hierarchy/file-browser
    // semantic colors (`dock.rs`).
    draw_text_row(renderer, font, "Foreground Color:", row_rect(7), text_px, dim, panel_bg);
    let swatch_w = font.measure("[#]", text_px).0;
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let gx = cx + (i % 8) as f32 * swatch_w;
        let gy = row_rect(8 + i / 8).y;
        let ch = if pal.fg == col { '*' } else { '#' };
        draw_swatch_px(
            renderer,
            frame,
            font,
            WidgetId::PaletteEditorSwatch { is_fg: true, index: i },
            Rect::new(gx, gy, swatch_w, row_h),
            text_px,
            &format!("[{}]", ch),
            col,
            Color::Black,
        );
    }
    let fg_custom_label = format!("[ {} ]", custom_color_label(pal.fg));
    draw_text_row(renderer, font, &fg_custom_label, row_rect(10), text_px, text_fg, input_bg);

    draw_text_row(renderer, font, "Background Color:", row_rect(11), text_px, dim, panel_bg);
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let gx = cx + (i % 8) as f32 * swatch_w;
        let gy = row_rect(12 + i / 8).y;
        let ch = if pal.bg == col { '*' } else { '#' };
        draw_swatch_px(
            renderer,
            frame,
            font,
            WidgetId::PaletteEditorSwatch { is_fg: false, index: i },
            Rect::new(gx, gy, swatch_w, row_h),
            text_px,
            &format!("[{}]", ch),
            col,
            Color::Black,
        );
    }
    let bg_custom_label = format!("[ {} ]", custom_color_label(pal.bg));
    draw_text_row(renderer, font, &bg_custom_label, row_rect(14), text_px, text_fg, input_bg);

    // Buttons at the bottom — drawn only, no `frame.push`: `input/mod.rs`'s
    // own click handling for these two reads fixed cell ranges
    // (`mx+2..mx+20`, `mx+22..mx+34`) independently, same pre-existing
    // E5-class gap `row_h`'s own doc comment above names; positioned at
    // those same fixed cell offsets here so the two stay visually
    // aligned. No themed "text-on-accent" role exists (same gap
    // `chrome.rs`'s `draw_dock_tabs` comments on), so Save & Close stays
    // plain black-on-accent. Delete uses the theme's `Danger` role rather
    // than a literal red — this button really is a destructive action,
    // so it gets the semantic-color treatment, not decorative chrome.
    let btn_row = row_rect(MH_ROWS - 2);
    let save_x = mx + 2.0 * CELL_W as f32;
    let save_w = 18.0 * CELL_W as f32;
    draw_text_row(renderer, font, " [ Save & Close ] ", Rect::new(save_x, btn_row.y, save_w, row_h), text_px, Color::Black, accent);
    let delete_x = mx + 22.0 * CELL_W as f32;
    let delete_w = 12.0 * CELL_W as f32;
    draw_text_row(
        renderer,
        font,
        " [ Delete ] ",
        Rect::new(delete_x, btn_row.y, delete_w, row_h),
        text_px,
        theme.role_color(PaletteRole::TitleText),
        danger,
    );
}

fn custom_color_label(color: Color) -> String {
    match color {
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        _ => "Advanced".to_string(),
    }
}

/// `row_h` here is `CELL_H`, not `theme.metrics.row_h` — see
/// `draw_palette_editor_modal`'s own doc comment for why. Unlike that
/// modal, everything interactive HERE already reads back from `UiFrame`
/// (`input/mod.rs`'s `handle_color_picker_input`, fully migrated), so the
/// real constraint is narrower: the hue bar/SV map's own `rect.w`/`rect.h`
/// must stay exact `CELL_W`/`CELL_H` multiples, since the input handler
/// divides by those constants to recover a discrete hue/saturation/value
/// step (`cells_w = rect.w / CELL_W`, etc.) — an arbitrary pixel width
/// would silently change that step count. Everything else in this
/// function (title, labels, buttons, the hex readout) is free to use the
/// theme's real font; only the hue bar/SV map's own per-cell color grid
/// stays on literal `draw_char` cells, the same "literal color content,
/// not chrome" reasoning already applied to `PALETTE_COLORS` elsewhere.
#[allow(clippy::too_many_arguments)]
pub fn draw_color_picker_modal(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    hsv: (f32, f32, f32),
    is_fg: bool,
    screen_w: f32,
    screen_h: f32,
    frame: &mut UiFrame,
) {
    let row_h = CELL_H as f32;
    let text_px = theme.font_sizes.body;
    let mw = 44.0 * CELL_W as f32;
    let mh = 16.0 * row_h;
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    let row_rect = |i: usize| Rect::new(mx, my + i as f32 * row_h, mw, row_h);

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);

    draw_themed_frame(renderer, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(renderer, theme, chrome_tex, mx, my, mw);

    // Title
    let title = format!(" ADVANCED COLOR: {} ", if is_fg { "FOREGROUND" } else { "BACKGROUND" });
    let title_fg = theme.role_color(PaletteRole::TitleText);
    let title_w = font.measure(&title, text_px).0;
    draw_text_row(renderer, font, &title, Rect::new(mx, my, title_w, row_h), text_px, title_fg, panel_bg);
    // 7C-1 (master plan §5.3): registers the title-close hitbox at the
    // exact point it's drawn, replacing `input/mod.rs`'s own
    // independently-recomputed `mx+mw-4..mx+mw-1` range (E5).
    let close_w = font.measure("[X]", text_px).0;
    draw_button_px(
        renderer,
        frame,
        font,
        WidgetId::ColorPickerClose,
        Rect::new(mx + mw - close_w - CELL_W as f32, my, close_w, row_h),
        text_px,
        "[X]",
        title_fg,
        panel_bg,
    );

    let cx = mx + 2.0 * CELL_W as f32;
    let (h, s, v) = hsv;

    // 1. Hue Bar (0..360) — a continuous drag area, not a discrete
    // button, so it's registered as one rect (`WidgetId::ColorPickerHueBar`)
    // that the input handler reads back via `UiFrame::rect_of` to compute
    // a hue percentage, rather than a per-column push. The bar itself
    // paints literal HSV-derived RGB, same reasoning as `PALETTE_COLORS`
    // above — it's the color being picked, not decorative chrome.
    draw_text_row(renderer, font, "Hue:", row_rect(2), text_px, dim, panel_bg);
    let hbar_w = 36;
    let hbar_x_cell = ((cx + 5.0 * CELL_W as f32) / CELL_W as f32).round() as usize;
    let hbar_row_cell = (row_rect(2).y / CELL_H as f32).round() as usize;
    for i in 0..hbar_w {
        let hue = (i as f32 / hbar_w as f32) * 360.0;
        let col = Color::from_hsv(hue, 1.0, 1.0);
        renderer.draw_char(hbar_x_cell + i, hbar_row_cell, ' ', Color::Reset, col);
    }
    frame.push(
        WidgetId::ColorPickerHueBar,
        UiRect::from_cells(hbar_x_cell as i32, hbar_row_cell as i32, hbar_w, 1),
    );
    let h_indicator_x = hbar_x_cell + ((h / 360.0) * (hbar_w - 1) as f32).round() as usize;
    renderer.draw_char(h_indicator_x, hbar_row_cell.saturating_sub(1), 'v', text_fg, panel_bg);

    // 2. SV Map (Saturation vs Value) — same continuous-area reasoning as
    // the hue bar above.
    draw_text_row(renderer, font, "Sat/Val Map:", row_rect(4), text_px, dim, panel_bg);
    let map_w = 20;
    let map_h = 8;
    let map_x = hbar_x_cell;
    let map_y = (row_rect(5).y / CELL_H as f32).round() as usize;
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
    let preview_x_cell = ((mx + 30.0 * CELL_W as f32) / CELL_W as f32).round() as usize;
    draw_text_row(renderer, font, "Selected:", row_rect(6), text_px, dim, panel_bg);
    let preview_row_cell = (row_rect(7).y / CELL_H as f32).round() as usize;
    renderer.draw_rect_filled(preview_x_cell, preview_row_cell, 8, 3, ' ', Color::Reset, current_col);

    if let Color::Rgb(r, g, b) = current_col {
        let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let hex_x = preview_x_cell as f32 * CELL_W as f32;
        let hex_w = font.measure(&hex, text_px).0;
        draw_text_row(renderer, font, &hex, Rect::new(hex_x, row_rect(11).y, hex_w, row_h), text_px, accent, panel_bg);
    }

    // Buttons. No themed "text-on-accent" role exists (same gap noted
    // throughout `chrome.rs`), so Apply stays plain black-on-accent.
    let btn_row = row_rect(14);
    let apply_label = " [ Apply ] ";
    let apply_w = font.measure(apply_label, text_px).0;
    draw_button_px(
        renderer,
        frame,
        font,
        WidgetId::ColorPickerApply,
        Rect::new(cx, btn_row.y, apply_w, row_h),
        text_px,
        apply_label,
        Color::Black,
        accent,
    );
    let cancel_label = " [ Cancel ] ";
    let cancel_w = font.measure(cancel_label, text_px).0;
    draw_button_px(
        renderer,
        frame,
        font,
        WidgetId::ColorPickerCancel,
        Rect::new(mx + mw - cancel_w - CELL_W as f32, btn_row.y, cancel_w, row_h),
        text_px,
        cancel_label,
        dim,
        panel_bg,
    );
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

/// No hit-testing anywhere in this overlay (dismissed only by key press,
/// never a click) — the one function in this file with nothing else
/// constraining its layout, so it's free to use the theme's real
/// `metrics.row_h` throughout, unlike the modals above it.
pub fn draw_help_overlay(renderer: &mut dyn DrawSurface, font: &mut dyn Font, theme: &Theme, content: Rect) {
    let bg = theme.role_color(PaletteRole::PanelBg);
    let fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let pad = CELL_W as f32;
    renderer.fill_rect_px(content, bg);

    let title = " EMBER2D EDITOR — KEYBOARD SHORTCUTS ";
    let title_w = font.measure(title, text_px).0;
    draw_text_row(renderer, font, title, Rect::new(content.x + pad, content.y + row_h, title_w, row_h), text_px, accent, bg);
    let sep_w = content.w - 2.0 * pad;
    let sep: String = "-".repeat((sep_w / font.measure("-", text_px).0.max(1.0)) as usize);
    draw_text_row(renderer, font, &sep, Rect::new(content.x + pad, content.y + 2.0 * row_h, sep_w, row_h), text_px, dim, bg);

    let col_w = (content.w - 4.0 * pad) / 3.0;
    let c1 = content.x + pad;
    let c2 = c1 + col_w + pad;
    let c3 = c2 + col_w + pad;
    let row = |n: usize| content.y + (4 + n) as f32 * row_h;
    let line = |col: f32, n: usize, text: &str, color: Color, renderer: &mut dyn DrawSurface, font: &mut dyn Font| {
        draw_text_row(renderer, font, text, Rect::new(col, row(n), col_w, row_h), text_px, color, bg);
    };
    line(c1, 0, "TOOLS", accent, renderer, font);
    line(c1, 1, " 1-3  Layers", fg, renderer, font);
    line(c1, 2, " 4-0  Palette", fg, renderer, font);
    line(c1, 3, " L    Line tool", fg, renderer, font);
    line(c1, 4, " F    Flood fill", fg, renderer, font);
    line(c1, 5, " E    Eraser size", fg, renderer, font);
    line(c1, 6, " Q    Select mode", fg, renderer, font);
    line(c1, 7, " ;/'  Solid/Trigger", fg, renderer, font);
    line(c1, 9, "CANVAS", accent, renderer, font);
    line(c1, 10, " Wheel     Zoom", fg, renderer, font);
    line(c1, 11, " Ctrl+Whl  Fast Zoom", fg, renderer, font);
    line(c1, 12, " Arrows    Scroll", fg, renderer, font);
    line(c1, 13, " Mid-drag  Pan", fg, renderer, font);
    line(c1, 14, " Home      Reset View", fg, renderer, font);
    line(c1, 15, " Delete    Erase", fg, renderer, font);
    line(c2, 0, "EDIT", accent, renderer, font);
    line(c2, 1, " U/Ctrl+Z  Undo", fg, renderer, font);
    line(c2, 2, " R/Ctrl+Y  Redo", fg, renderer, font);
    line(c2, 3, " C  Copy select", fg, renderer, font);
    line(c2, 4, " X  Cut select", fg, renderer, font);
    line(c2, 5, " V  Paste", fg, renderer, font);
    line(c2, 6, " I  Edit tag", fg, renderer, font);
    line(c2, 9, "LEVEL", accent, renderer, font);
    line(c2, 10, " N  Rename", fg, renderer, font);
    line(c2, 11, " Z  Resize", fg, renderer, font);
    line(c2, 12, " P  Set spawn", fg, renderer, font);
    line(c2, 13, " Sh+P  Add spawn", fg, renderer, font);
    line(c2, 14, " T  Attach script", fg, renderer, font);
    line(c2, 15, " Sh+drag  Rect fill", fg, renderer, font);
    line(c3, 0, "VIEW", accent, renderer, font);
    line(c3, 1, " Tab  Grid", fg, renderer, font);
    line(c3, 2, " G    Physics", fg, renderer, font);
    line(c3, 3, " B    Palette", fg, renderer, font);
    line(c3, 4, " H    Hierarchy", fg, renderer, font);
    line(c3, 5, " `    Stats", fg, renderer, font);
    line(c3, 6, " F1   Console", fg, renderer, font);
    line(c3, 7, " F2   Inspector", fg, renderer, font);
    line(c3, 9, "FILE", accent, renderer, font);
    line(c3, 10, " S    Save", fg, renderer, font);
    line(c3, 11, " Sh+S  Save As", fg, renderer, font);
    line(c3, 12, " O    Open", fg, renderer, font);
    line(c3, 13, " F5   Play preview", fg, renderer, font);
    line(c3, 14, " Esc  Cancel/Close", fg, renderer, font);
    line(c3, 15, " ?    This screen", fg, renderer, font);

    let hint = "Press ? or Esc to close";
    let hint_w = font.measure(hint, text_px).0;
    let hint_x = content.x + ((content.w - hint_w) / 2.0).max(0.0);
    draw_text_row(renderer, font, hint, Rect::new(hint_x, content.y + content.h - 2.0 * row_h, hint_w, row_h), text_px, dim, bg);
}
