// editor/ui/panels/importer_panel.rs — the tileset importer dialog: slicing
// settings on the left, the sheet with its cell grid on the right.
//
// Step 8-2 (docs/ember2d-master-plan.md §5.7; the user's "full modal"
// scoping choice). Drawn in points through `UiPainter` like every other
// modal since 7D-3, and every clickable thing is pushed into `UiFrame` at
// the exact rect it's drawn (`input/importer.rs` only ever hit-tests those
// rects). The sheet preview is the one place the editor draws an arbitrary
// image (`UiPainter::image`, new in this step).

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::widgets::{draw_button_px, draw_text_row};
use super::chrome::{draw_themed_frame, draw_themed_title_strip};
use crate::editor::importer::{ImportField, TilesetImport};
use ember2d::renderer::{color::Color, Font, Texture, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;

/// One labelled text field: `label` then `[value]`, the field registered as
/// `WidgetId::ImporterField(field)`. Returns the row's bottom edge.
#[allow(clippy::too_many_arguments)]
fn field(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    frame: &mut UiFrame,
    imp: &TilesetImport,
    which: ImportField,
    label: &str,
    value: &str,
    at: (f32, f32),
    width: f32,
) -> f32 {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let focused = imp.focus == Some(which);
    let label_w = painter.measure(font, "Spacing: ", text_px);
    draw_text_row(
        painter,
        font,
        label,
        Rect::new(at.0, at.1, label_w, row_h),
        text_px,
        theme.role_color(PaletteRole::TextPrimary),
        theme.role_color(PaletteRole::PanelBg),
    );
    let shown = if focused { format!("{value}█") } else { value.to_string() };
    let r = Rect::new(at.0 + label_w, at.1, (width - label_w).max(0.0), row_h);
    draw_text_row(
        painter,
        font,
        &format!("[{shown}]"),
        r,
        text_px,
        if focused {
            theme.role_color(PaletteRole::Accent)
        } else {
            theme.role_color(PaletteRole::TextPrimary)
        },
        theme.role_color(PaletteRole::InputBg),
    );
    frame.push(WidgetId::ImporterField(which), UiRect::new(r.x, r.y, r.w, r.h));
    at.1 + row_h
}

/// A 1-point outline around `r` (four thin fills — the painter has no
/// stroke primitive, and none of the chrome needed one before).
fn outline(painter: &mut UiPainter, r: Rect, t: f32, color: Color) {
    painter.fill(Rect::new(r.x, r.y, r.w, t), color);
    painter.fill(Rect::new(r.x, r.y + r.h - t, r.w, t), color);
    painter.fill(Rect::new(r.x, r.y, t, r.h), color);
    painter.fill(Rect::new(r.x + r.w - t, r.y, t, r.h), color);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_tileset_import_modal(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    imp: &TilesetImport,
    screen_w: f32,
    screen_h: f32,
    frame: &mut UiFrame,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let danger = theme.role_color(PaletteRole::Danger);
    let title_fg = theme.role_color(PaletteRole::TitleText);

    let mw = (screen_w - 2.0 * row_h).clamp(320.0, 900.0);
    let mh = (screen_h - 2.0 * row_h).clamp(16.0 * row_h, 640.0);
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);

    let pad = row_h * 0.5;
    let title = " IMPORT TILESET ";
    let title_w = painter.measure(font, title, text_px);
    draw_text_row(
        painter,
        font,
        title,
        Rect::new(mx + pad, my, title_w, row_h),
        text_px,
        title_fg,
        panel_bg,
    );

    // ── Left column: settings ────────────────────────────────────────────
    let col_w = painter.measure(font, "Spacing: [0000000000000]", text_px);
    let x = mx + pad;
    let mut y = my + row_h * 1.5;
    let fields: [(ImportField, &str, &str); 5] = [
        (ImportField::Name, "Name:", &imp.name),
        (ImportField::CellW, "Cell W:", &imp.cell_w),
        (ImportField::CellH, "Cell H:", &imp.cell_h),
        (ImportField::Margin, "Margin:", &imp.margin),
        (ImportField::Spacing, "Spacing:", &imp.spacing),
    ];
    for (which, label, value) in fields {
        y = field(painter, font, theme, frame, imp, which, label, value, (x, y), col_w);
    }

    let (cols, rows) = imp.grid();
    let named =
        imp.names.iter().filter(|((c, r), n)| !n.is_empty() && *c < cols && *r < rows).count();
    let info = [
        format!("Image: {}x{} px", imp.texture.width, imp.texture.height),
        format!("Grid:  {cols} x {rows} cells"),
        format!("Named: {named} region(s)"),
    ];
    y += row_h * 0.5;
    for line in info {
        draw_text_row(painter, font, &line, Rect::new(x, y, col_w, row_h), text_px, dim, panel_bg);
        y += row_h;
    }

    y += row_h * 0.5;
    match imp.selected {
        Some((c, r)) => {
            let label = format!("Cell ({c},{r}) region:");
            draw_text_row(
                painter,
                font,
                &label,
                Rect::new(x, y, col_w, row_h),
                text_px,
                text_fg,
                panel_bg,
            );
            y += row_h;
            let name = imp.selected_name().to_string();
            y = field(
                painter,
                font,
                theme,
                frame,
                imp,
                ImportField::Region,
                "Name:",
                &name,
                (x, y),
                col_w,
            );
        }
        None => {
            for hint in ["Click a cell to", "name it as a region."] {
                draw_text_row(
                    painter,
                    font,
                    hint,
                    Rect::new(x, y, col_w, row_h),
                    text_px,
                    dim,
                    panel_bg,
                );
                y += row_h;
            }
        }
    }
    if let Some(err) = &imp.error {
        y += row_h * 0.5;
        // Wrapped by hand to the column — errors can be a sentence long.
        let max_chars = ((col_w / painter.measure(font, "m", text_px)).floor() as usize).max(8);
        let words: Vec<&str> = err.split(' ').collect();
        let mut line = String::new();
        for w in words {
            if !line.is_empty() && line.len() + 1 + w.len() > max_chars {
                draw_text_row(
                    painter,
                    font,
                    &line,
                    Rect::new(x, y, col_w, row_h),
                    text_px,
                    danger,
                    panel_bg,
                );
                y += row_h;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(w);
        }
        draw_text_row(
            painter,
            font,
            &line,
            Rect::new(x, y, col_w, row_h),
            text_px,
            danger,
            panel_bg,
        );
    }

    let btn_y = my + mh - row_h * 1.5;
    let import_label = " [ Import ] ";
    let import_w = painter.measure(font, import_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::ImporterImport,
        Rect::new(x, btn_y, import_w, row_h),
        text_px,
        import_label,
        Color::Black,
        accent,
    );
    let cancel_label = " [ Cancel ] ";
    let cancel_w = painter.measure(font, cancel_label, text_px);
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::ImporterCancel,
        Rect::new(x + import_w + pad, btn_y, cancel_w, row_h),
        text_px,
        cancel_label,
        dim,
        panel_bg,
    );

    // ── Right: the sheet, scaled to fit, with its cell grid ─────────────
    let area = Rect::new(
        x + col_w + pad,
        my + row_h * 1.5,
        mx + mw - (x + col_w + 2.0 * pad),
        mh - row_h * 2.0,
    );
    if area.w <= 0.0 || area.h <= 0.0 || imp.texture.width == 0 || imp.texture.height == 0 {
        return;
    }
    let (tw, th) = (imp.texture.width as f32, imp.texture.height as f32);
    // Whole-number zoom when the sheet fits at least once (crisp pixel art);
    // a fractional shrink only for a sheet bigger than the area.
    let fit = (area.w / tw).min(area.h / th);
    let zoom = if fit >= 1.0 { fit.floor() } else { fit };
    let sheet = Rect::new(area.x, area.y, tw * zoom, th * zoom);
    painter.fill(sheet, Color::Rgb(40, 40, 48));
    painter.image(sheet, &imp.texture, None, Color::White);
    frame.push(WidgetId::ImporterSheet, UiRect::new(sheet.x, sheet.y, sheet.w, sheet.h));

    if let Some((cw, ch, m, s)) = imp.settings() {
        let grid_color = Color::Rgb(90, 90, 110);
        for c in 0..cols {
            for r in 0..rows {
                let px = (m + c * (cw + s)) as f32 * zoom;
                let py = (m + r * (ch + s)) as f32 * zoom;
                let cell =
                    Rect::new(sheet.x + px, sheet.y + py, cw as f32 * zoom, ch as f32 * zoom);
                let is_named = imp.names.get(&(c, r)).is_some_and(|n| !n.is_empty());
                let color = if is_named { accent } else { grid_color };
                outline(painter, cell, 1.0, color);
            }
        }
        if let Some((c, r)) = imp.selected {
            if c < cols && r < rows {
                let px = (m + c * (cw + s)) as f32 * zoom;
                let py = (m + r * (ch + s)) as f32 * zoom;
                let cell =
                    Rect::new(sheet.x + px, sheet.y + py, cw as f32 * zoom, ch as f32 * zoom);
                outline(painter, cell, 2.0, Color::White);
            }
        }
    }
    let _ = y;
}
