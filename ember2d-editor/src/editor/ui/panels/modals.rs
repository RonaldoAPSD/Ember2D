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
use super::super::widgets::{
    draw_button_px, draw_swatch_px, draw_text_row, draw_tile_glyph_in, draw_tile_preview_in,
    PALETTE_COLORS,
};
use ember2d::renderer::{color::Color, DrawSurface, Font, Texture, UiPainter, CELL_H, CELL_W};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;

use super::chrome::{draw_themed_frame, draw_themed_title_strip};

/// The advanced color picker's hue bar step count — shared with
/// `input/palette_editor.rs`'s own `handle_color_picker_input` so both
/// sides name the same 36 columns instead of each spelling the literal
/// out separately (R65's own "one source of truth" reasoning, just for a
/// constant rather than a rect).
pub const HUE_BAR_STEPS: usize = 36;
/// The advanced color picker's saturation/value map dimensions — same
/// shared-constant reasoning as `HUE_BAR_STEPS`.
pub const SV_MAP_W: usize = 20;
pub const SV_MAP_H: usize = 8;

/// R65 (§3 in the master plan): used to run at `CELL_H`, NOT
/// `theme.metrics.row_h`, because `input/mod.rs`'s `handle_palette_editor_input`
/// hit-tested every row by comparing `mouse.cell_y`/`mouse.cell_x` against
/// `my`/`mx` values it recomputed independently — in CELL units, from a
/// ROUNDED `screen_cells()`, while this function centered in real px. When
/// the leftover cell-count remainder was odd (true at the default
/// 1280×720), the two disagreed by half a row, landing a click on the
/// wrong field. Fixed by giving every interactive row its own `WidgetId`,
/// pushed at the exact point it's drawn here — `handle_palette_editor_input`
/// now reads them back via `UiFrame::hit` instead of recomputing any of
/// this layout itself, the same fix already applied to every other panel
/// in this step. `row_h` is `theme.metrics.row_h` now, like the rest of
/// this step's chrome.
#[allow(clippy::too_many_arguments)]
pub fn draw_palette_editor_modal(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    pal: &crate::editor::palette::TileDefinition,
    sprites: &crate::editor::sprites::SpriteAssets,
    anim_time: f32,
    focus: Option<&crate::editor::PaletteField>,
    screen_w: f32,
    screen_h: f32,
    frame: &mut UiFrame,
) {
    use crate::editor::PaletteField;

    const MH_ROWS: usize = 18;
    let row_h = theme.metrics.row_h;
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

    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);

    // Title
    let title = format!(" EDITING: {} ", pal.name);
    let title_fg = theme.role_color(PaletteRole::TitleText);
    let cx = mx + 2.0 * CELL_W as f32;
    let title_w = painter.measure(font, &title, text_px);
    draw_text_row(
        painter,
        font,
        &title,
        Rect::new(cx, my, title_w, row_h),
        text_px,
        title_fg,
        panel_bg,
    );
    let close_w = painter.measure(font, "[X]", text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorClose,
        Rect::new(mx + mw - close_w - CELL_W as f32, my, close_w, row_h),
        text_px,
        "[X]",
        title_fg,
        panel_bg,
    );

    // Name — the whole row is the click target (matches the old
    // `mouse.cell_y == my + N` gate, which didn't care about `x` either).
    let is_name_focused = matches!(focus, Some(PaletteField::Name));
    let name_val = if is_name_focused { format!("{}█", pal.name) } else { pal.name.clone() };
    draw_text_row(painter, font, "Name: ", row_rect(2), text_px, text_fg, panel_bg);
    let name_x = cx + painter.measure(font, "Name: ", text_px);
    let name_field = format!("[{:<20}]", name_val);
    let name_field_w = painter.measure(font, &name_field, text_px);
    draw_text_row(
        painter,
        font,
        &name_field,
        Rect::new(name_x, row_rect(2).y, name_field_w, row_h),
        text_px,
        if is_name_focused { accent } else { text_fg },
        input_bg,
    );
    let r = row_rect(2);
    frame.push(WidgetId::PaletteEditorField(PaletteField::Name), UiRect::new(r.x, r.y, r.w, r.h));

    // Glyph
    let is_glyph_focused = matches!(focus, Some(PaletteField::Glyph));
    // R95: unfocused, the field's own text leaves a blank between the
    // quotes and the real tile glyph is drawn into it below (it used to
    // print the glyph in the theme font AND overlay a cell-addressed
    // bitmap copy that, at most UI scales, landed somewhere else entirely —
    // the stray red mark under the Tag field).
    let glyph_val = if is_glyph_focused { '█' } else { ' ' };
    draw_text_row(painter, font, "Glyph:", row_rect(3), text_px, text_fg, panel_bg);
    let glyph_x = cx + painter.measure(font, "Glyph:", text_px);
    let glyph_field = format!("['{}']", glyph_val);
    let glyph_field_w = painter.measure(font, &glyph_field, text_px);
    draw_text_row(
        painter,
        font,
        &glyph_field,
        Rect::new(glyph_x, row_rect(3).y, glyph_field_w, row_h),
        text_px,
        if is_glyph_focused { accent } else { text_fg },
        input_bg,
    );
    let r = row_rect(3);
    frame.push(WidgetId::PaletteEditorField(PaletteField::Glyph), UiRect::new(r.x, r.y, r.w, r.h));
    // The glyph preview stays on the engine's own bitmap-font pipeline —
    // same "literal in-game preview, not chrome" reasoning `dock.rs`'s
    // `draw_palette_panel` already documents for its own glyph preview —
    // drawn into the blank between the quotes, in points (R95).
    if !is_glyph_focused {
        let slot_x = glyph_x + painter.measure(font, "['", text_px);
        let slot_w = painter.measure(font, " ", text_px);
        let slot = Rect::new(slot_x, row_rect(3).y, slot_w, row_h);
        // Step 8-2: the entry's sprite thumbnail when it has one — the same
        // preview the palette panel row shows.
        draw_tile_preview_in(painter, slot, pal, sprites, anim_time);
    }
    // Step 8-2: which tileset region this entry paints with, read-only (set
    // by the importer, not typed here).
    if let Some(ref sprite) = pal.sprite {
        let note = format!("  sprite: {}/{}", sprite.tileset, sprite.region);
        let note_x = glyph_x + glyph_field_w;
        let note_w = painter.measure(font, &note, text_px);
        draw_text_row(
            painter,
            font,
            &note,
            Rect::new(note_x, row_rect(3).y, note_w, row_h),
            text_px,
            dim,
            panel_bg,
        );
    }

    // Toggles — two independent widgets, side by side, each sized to its
    // own measured label (was one shared text row with fixed `cx..cx+10`/
    // `cx+13..cx+25` cell ranges, R65).
    let solid_label = format!("Solid: [{}]", if pal.solid { 'x' } else { ' ' });
    let solid_w = painter.measure(font, &solid_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorToggle { is_solid: true },
        Rect::new(cx, row_rect(4).y, solid_w, row_h),
        text_px,
        &solid_label,
        text_fg,
        panel_bg,
    );
    let trigger_x = cx + solid_w + painter.measure(font, "   ", text_px);
    let trigger_label = format!("Trigger: [{}]", if pal.trigger { 'x' } else { ' ' });
    let trigger_w = painter.measure(font, &trigger_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorToggle { is_solid: false },
        Rect::new(trigger_x, row_rect(4).y, trigger_w, row_h),
        text_px,
        &trigger_label,
        text_fg,
        panel_bg,
    );

    // Tag
    let is_tag_focused = matches!(focus, Some(PaletteField::Tag));
    let tag_val = if is_tag_focused { format!("{}█", pal.tag) } else { pal.tag.clone() };
    draw_text_row(painter, font, "Tag:  ", row_rect(5), text_px, text_fg, panel_bg);
    let tag_x = cx + painter.measure(font, "Tag:  ", text_px);
    let tag_field = format!("[{:<20}]", tag_val);
    let tag_field_w = painter.measure(font, &tag_field, text_px);
    draw_text_row(
        painter,
        font,
        &tag_field,
        Rect::new(tag_x, row_rect(5).y, tag_field_w, row_h),
        text_px,
        if is_tag_focused { accent } else { text_fg },
        input_bg,
    );
    let r = row_rect(5);
    frame.push(WidgetId::PaletteEditorField(PaletteField::Tag), UiRect::new(r.x, r.y, r.w, r.h));

    // Color Grids — 7C-1 (master plan §5.3): `draw_swatch_px` registers
    // each cell's own rect, and `PALETTE_COLORS` replaces this function's,
    // `draw_color_picker`'s, and both of `input/mod.rs`'s independent
    // copies of the same 16-color array (E5's "four color tables"). The
    // swatch colors themselves are literal RGB entries a user is picking
    // from — content, not chrome — so `PALETTE_COLORS` stays untouched by
    // theming, same reasoning as the console/hierarchy/file-browser
    // semantic colors (`dock.rs`).
    draw_text_row(painter, font, "Foreground Color:", row_rect(7), text_px, dim, panel_bg);
    let swatch_w = painter.measure(font, "[#]", text_px);
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let gx = cx + (i % 8) as f32 * swatch_w;
        let gy = row_rect(8 + i / 8).y;
        let ch = if pal.fg == col { '*' } else { '#' };
        draw_swatch_px(
            painter,
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
    let fg_custom_w = painter.measure(font, &fg_custom_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorCustomColor { is_fg: true },
        Rect::new(cx, row_rect(10).y, fg_custom_w, row_h),
        text_px,
        &fg_custom_label,
        text_fg,
        input_bg,
    );

    draw_text_row(painter, font, "Background Color:", row_rect(11), text_px, dim, panel_bg);
    for (i, &col) in PALETTE_COLORS.iter().enumerate() {
        let gx = cx + (i % 8) as f32 * swatch_w;
        let gy = row_rect(12 + i / 8).y;
        let ch = if pal.bg == col { '*' } else { '#' };
        draw_swatch_px(
            painter,
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
    let bg_custom_w = painter.measure(font, &bg_custom_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorCustomColor { is_fg: false },
        Rect::new(cx, row_rect(14).y, bg_custom_w, row_h),
        text_px,
        &bg_custom_label,
        text_fg,
        input_bg,
    );

    // Buttons at the bottom — R65: each is now its own `WidgetId`, sized to
    // its own measured label, instead of a fixed cell range
    // (`mx+2..mx+20`/`mx+22..mx+34`) `input/mod.rs` used to recompute
    // independently. No themed "text-on-accent" role exists (same gap
    // `chrome.rs`'s `draw_dock_tabs` comments on), so Save & Close stays
    // plain black-on-accent. Delete uses the theme's `Danger` role rather
    // than a literal red — this button really is a destructive action, so
    // it gets the semantic-color treatment, not decorative chrome.
    let btn_row = row_rect(MH_ROWS - 2);
    let save_label = " [ Save & Close ] ";
    let save_w = painter.measure(font, save_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorSaveClose,
        Rect::new(cx, btn_row.y, save_w, row_h),
        text_px,
        save_label,
        Color::Black,
        accent,
    );
    let delete_label = " [ Delete ] ";
    let delete_w = painter.measure(font, delete_label, text_px);
    let delete_x = cx + save_w + painter.measure(font, "   ", text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditorDelete,
        Rect::new(delete_x, btn_row.y, delete_w, row_h),
        text_px,
        delete_label,
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
    painter: &mut UiPainter,
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

    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);

    // Title
    let title = format!(" ADVANCED COLOR: {} ", if is_fg { "FOREGROUND" } else { "BACKGROUND" });
    let title_fg = theme.role_color(PaletteRole::TitleText);
    let title_w = painter.measure(font, &title, text_px);
    draw_text_row(
        painter,
        font,
        &title,
        Rect::new(mx, my, title_w, row_h),
        text_px,
        title_fg,
        panel_bg,
    );
    // 7C-1 (master plan §5.3): registers the title-close hitbox at the
    // exact point it's drawn, replacing `input/mod.rs`'s own
    // independently-recomputed `mx+mw-4..mx+mw-1` range (E5).
    let close_w = painter.measure(font, "[X]", text_px);
    draw_button_px(
        painter,
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
    // R95 (master plan §3.2): every swatch below used to be painted on the
    // CELL grid (`draw_char(cell_x, cell_y, ' ', .., col)` /
    // `draw_rect_filled(cell ..)`) from a points-space layout divided by
    // `CELL_W`/`CELL_H` — right only at the one UI scale where points and
    // cells coincide; at 1.5x the hue bar ran off the dialog's right edge,
    // the map sat over the buttons, and the preview landed on the
    // Inspector. They're plain points-space `fill`s now, one `unit_w` x
    // `row_h` block per step: the same step size (`CELL_W`/`CELL_H`, in
    // points) the input side (`input/palette_editor.rs`) already divides
    // the registered rects by, so hit-testing needs no change.
    let unit_w = CELL_W as f32;
    draw_text_row(painter, font, "Hue:", row_rect(2), text_px, dim, panel_bg);
    let hbar_w = HUE_BAR_STEPS;
    let hbar_x = cx + 5.0 * unit_w;
    let hbar_y = row_rect(2).y;
    for i in 0..hbar_w {
        let hue = (i as f32 / hbar_w as f32) * 360.0;
        let col = Color::from_hsv(hue, 1.0, 1.0);
        painter.fill(Rect::new(hbar_x + i as f32 * unit_w, hbar_y, unit_w, row_h), col);
    }
    frame.push(
        WidgetId::ColorPickerHueBar,
        UiRect::new(hbar_x, hbar_y, hbar_w as f32 * unit_w, row_h),
    );
    // The current-hue marker, one row above the bar — a literal glyph on
    // the bitmap pipeline like the palette previews (`draw_tile_glyph_in`).
    let h_step = ((h / 360.0) * (hbar_w - 1) as f32).round();
    draw_tile_glyph_in(
        painter,
        Rect::new(hbar_x + h_step * unit_w, hbar_y - row_h, unit_w, row_h),
        'v',
        text_fg,
        Color::Reset,
    );

    // 2. SV Map (Saturation vs Value) — same continuous-area reasoning as
    // the hue bar above.
    draw_text_row(painter, font, "Sat/Val Map:", row_rect(4), text_px, dim, panel_bg);
    let map_w = SV_MAP_W;
    let map_h = SV_MAP_H;
    let map_x = hbar_x;
    let map_y = row_rect(5).y;
    for sy in 0..map_h {
        for sx in 0..map_w {
            let sat = sx as f32 / (map_w - 1) as f32;
            let val = 1.0 - (sy as f32 / (map_h - 1) as f32);
            let col = Color::from_hsv(h, sat, val);
            painter.fill(
                Rect::new(map_x + sx as f32 * unit_w, map_y + sy as f32 * row_h, unit_w, row_h),
                col,
            );
        }
    }
    frame.push(
        WidgetId::ColorPickerSvMap,
        UiRect::new(map_x, map_y, map_w as f32 * unit_w, map_h as f32 * row_h),
    );
    // Cursor in map
    let cur_sx = (s * (map_w - 1) as f32).round();
    let cur_sy = ((1.0 - v) * (map_h - 1) as f32).round();
    draw_tile_glyph_in(
        painter,
        Rect::new(map_x + cur_sx * unit_w, map_y + cur_sy * row_h, unit_w, row_h),
        '+',
        Color::White,
        Color::Reset,
    );

    // 3. Current Color Preview — its "Selected:" label sits directly above
    // the swatch (R95: it used to start at the dialog's left edge, on top
    // of the SV map's first columns).
    let current_col = Color::from_hsv(h, s, v);
    let preview_x = mx + 30.0 * unit_w;
    let label_w = painter.measure(font, "Selected:", text_px);
    draw_text_row(
        painter,
        font,
        "Selected:",
        Rect::new(preview_x, row_rect(6).y, label_w, row_h),
        text_px,
        dim,
        panel_bg,
    );
    painter.fill(Rect::new(preview_x, row_rect(7).y, 8.0 * unit_w, 3.0 * row_h), current_col);

    if let Color::Rgb(r, g, b) = current_col {
        let hex = format!("#{:02X}{:02X}{:02X}", r, g, b);
        let hex_x = preview_x;
        let hex_w = painter.measure(font, &hex, text_px);
        draw_text_row(
            painter,
            font,
            &hex,
            Rect::new(hex_x, row_rect(11).y, hex_w, row_h),
            text_px,
            accent,
            panel_bg,
        );
    }

    // Buttons. No themed "text-on-accent" role exists (same gap noted
    // throughout `chrome.rs`), so Apply stays plain black-on-accent.
    let btn_row = row_rect(14);
    let apply_label = " [ Apply ] ";
    let apply_w = painter.measure(font, apply_label, text_px);
    draw_button_px(
        painter,
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
    let cancel_w = painter.measure(font, cancel_label, text_px);
    draw_button_px(
        painter,
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
pub fn draw_help_overlay(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    content: Rect,
) {
    let bg = theme.role_color(PaletteRole::PanelBg);
    let fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let pad = CELL_W as f32;
    painter.fill(content, bg);

    let title = " EMBER2D EDITOR — KEYBOARD SHORTCUTS ";
    let title_w = painter.measure(font, title, text_px);
    draw_text_row(
        painter,
        font,
        title,
        Rect::new(content.x + pad, content.y + row_h, title_w, row_h),
        text_px,
        accent,
        bg,
    );
    let sep_w = content.w - 2.0 * pad;
    let sep: String = "-".repeat((sep_w / painter.measure(font, "-", text_px).max(1.0)) as usize);
    draw_text_row(
        painter,
        font,
        &sep,
        Rect::new(content.x + pad, content.y + 2.0 * row_h, sep_w, row_h),
        text_px,
        dim,
        bg,
    );

    let col_w = (content.w - 4.0 * pad) / 3.0;
    let c1 = content.x + pad;
    let c2 = c1 + col_w + pad;
    let c3 = c2 + col_w + pad;
    let row = |n: usize| content.y + (4 + n) as f32 * row_h;
    let line = |col: f32,
                n: usize,
                text: &str,
                color: Color,
                painter: &mut UiPainter,
                font: &mut dyn Font| {
        draw_text_row(
            painter,
            font,
            text,
            Rect::new(col, row(n), col_w, row_h),
            text_px,
            color,
            bg,
        );
    };
    line(c1, 0, "TOOLS", accent, painter, font);
    line(c1, 1, " 1-3  Layers", fg, painter, font);
    line(c1, 2, " 4-0  Palette", fg, painter, font);
    line(c1, 3, " L    Line tool", fg, painter, font);
    line(c1, 4, " F    Flood fill", fg, painter, font);
    line(c1, 5, " E    Eraser size", fg, painter, font);
    line(c1, 6, " Q    Select mode", fg, painter, font);
    line(c1, 7, " ;/'  Solid/Trigger", fg, painter, font);
    line(c1, 9, "CANVAS", accent, painter, font);
    line(c1, 10, " Wheel     Zoom", fg, painter, font);
    line(c1, 11, " Ctrl+Whl  Fast Zoom", fg, painter, font);
    line(c1, 12, " Arrows    Scroll", fg, painter, font);
    line(c1, 13, " Mid-drag  Pan", fg, painter, font);
    line(c1, 14, " Home      Reset View", fg, painter, font);
    line(c1, 15, " Delete    Erase", fg, painter, font);
    line(c2, 0, "EDIT", accent, painter, font);
    line(c2, 1, " U/Ctrl+Z  Undo", fg, painter, font);
    line(c2, 2, " R/Ctrl+Y  Redo", fg, painter, font);
    line(c2, 3, " C  Copy select", fg, painter, font);
    line(c2, 4, " X  Cut select", fg, painter, font);
    line(c2, 5, " V  Paste", fg, painter, font);
    line(c2, 6, " I  Edit tag", fg, painter, font);
    line(c2, 9, "LEVEL", accent, painter, font);
    line(c2, 10, " N  Rename", fg, painter, font);
    line(c2, 11, " Z  Resize", fg, painter, font);
    line(c2, 12, " P  Set spawn", fg, painter, font);
    line(c2, 13, " Sh+P  Add spawn", fg, painter, font);
    line(c2, 14, " T  Attach script", fg, painter, font);
    line(c2, 15, " Sh+drag  Rect fill", fg, painter, font);
    line(c3, 0, "VIEW", accent, painter, font);
    line(c3, 1, " Tab  Grid", fg, painter, font);
    line(c3, 2, " G    Physics", fg, painter, font);
    line(c3, 3, " B    Palette", fg, painter, font);
    line(c3, 4, " H    Hierarchy", fg, painter, font);
    line(c3, 5, " `    Stats", fg, painter, font);
    line(c3, 6, " F1   Console", fg, painter, font);
    line(c3, 7, " F2   Inspector", fg, painter, font);
    line(c3, 9, "FILE", accent, painter, font);
    line(c3, 10, " S    Save", fg, painter, font);
    line(c3, 11, " Sh+S  Save As", fg, painter, font);
    line(c3, 12, " O    Open", fg, painter, font);
    line(c3, 13, " F5   Play preview", fg, painter, font);
    line(c3, 14, " Esc  Cancel/Close", fg, painter, font);
    line(c3, 15, " ?    This screen", fg, painter, font);

    let hint = "Press ? or Esc to close";
    let hint_w = painter.measure(font, hint, text_px);
    let hint_x = content.x + ((content.w - hint_w) / 2.0).max(0.0);
    draw_text_row(
        painter,
        font,
        hint,
        Rect::new(hint_x, content.y + content.h - 2.0 * row_h, hint_w, row_h),
        text_px,
        dim,
        bg,
    );
}
