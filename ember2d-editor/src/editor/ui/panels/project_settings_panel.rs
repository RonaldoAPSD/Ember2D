// editor/ui/panels/project_settings_panel.rs — the Project Settings dialog
// (File > Project Settings...).
//
// Step 9-6 (docs/ember2d-master-plan.md §5.8); the logic is
// `editor/project_settings.rs`. One row per setting: the label, then the
// value in an input box; a row that cycles through choices says so with
// `< >` around its value. Every row and the Close button are pushed into
// `UiFrame` at the exact rect they're drawn (`input/project_settings.rs`
// only hit-tests those).

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::widgets::{draw_button_px, draw_text_row};
use super::chrome::{draw_themed_frame, draw_themed_title_strip};
use crate::editor::project_settings::{project_field_value, ProjectField};
use ember2d::project::ProjectData;
use ember2d::renderer::{Font, Texture, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;

#[allow(clippy::too_many_arguments)]
pub fn draw_project_settings_modal(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    project: &ProjectData,
    screen_w: f32,
    screen_h: f32,
    frame: &mut UiFrame,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let input_bg = theme.role_color(PaletteRole::InputBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let title_fg = theme.role_color(PaletteRole::TitleText);

    let label_w = painter.measure(font, "Pixels per unit:  ", text_px);
    let value_w = painter.measure(font, "< Turn-based >  floor1.level", text_px);
    let pad = row_h * 0.5;
    let rows = ProjectField::ALL.len() as f32;
    let hint = "Saved as you go; play mode uses it from the next F5.";
    let hint_w = painter.measure(font, hint, text_px);
    let mw = ((label_w + value_w).max(hint_w) + 2.0 * pad).min(screen_w);
    let mh = (row_h * (rows + 5.0)).min(screen_h);
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);
    let title = " PROJECT SETTINGS ";
    let title_w = painter.measure(font, title, text_px);
    draw_text_row(painter, font, title, Rect::new(mx + pad, my, title_w, row_h), text_px, title_fg, panel_bg);

    let mut y = my + row_h * 1.5;
    for field in ProjectField::ALL {
        let label = Rect::new(mx + pad, y, label_w, row_h);
        draw_text_row(painter, font, &format!("{}:", field.label()), label, text_px, text_fg, panel_bg);
        let value = project_field_value(project, field);
        let shown = if field.cycles() { format!("< {value} >") } else { value };
        let r = Rect::new(mx + pad + label_w, y, value_w, row_h);
        draw_text_row(painter, font, &shown, r, text_px, accent, input_bg);
        frame.push(WidgetId::ProjectSettingsRow(field), UiRect::new(r.x, r.y, r.w, r.h));
        y += row_h;
    }
    y += row_h * 0.5;
    draw_text_row(painter, font, hint, Rect::new(mx + pad, y, mw - 2.0 * pad, row_h), text_px, dim, panel_bg);
    y += row_h * 1.25;
    let close = "[ Close ]";
    let close_w = painter.measure(font, close, text_px);
    let r = Rect::new(mx + mw - pad - close_w, y, close_w, row_h);
    let fg = ember2d::renderer::color::Color::Black;
    draw_button_px(painter, frame, font, WidgetId::ProjectSettingsClose, r, text_px, close, fg, accent);
}
