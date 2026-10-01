// editor/ui/panels/file_browser.rs — the File Browser panel, and the ghost
// drawn under the mouse while an asset is dragged out of it.
//
// Moved out of `dock.rs` at Step 8-4 (docs/ember2d-master-plan.md §5.7),
// which gave the panel asset rows with thumbnails (an image, a tileset's
// sheet, a clip's current frame) and a preview pane for the selected asset
// — enough new drawing that it stopped being a small corner of `dock.rs`.
// Which row is which asset is decided by `editor/assets.rs`; the caller
// (`impl_render`) resolves each row's thumbnail and the preview up front and
// passes them in, so this file only draws.

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::widgets::draw_text_row;
use crate::editor::sprites::fit_inside;
use ember2d::renderer::{color::Color, Font, Texture, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::{Rect, Vec2};

/// What the preview pane shows for the selected asset row.
pub struct FilePreview<'a> {
    pub title: String,
    pub info: String,
    pub image: Option<(&'a Texture, Rect)>,
}

/// The preview pane only appears when the panel is at least this wide
/// (points) — narrower, the list needs all of it.
const PREVIEW_MIN_PANEL_W: f32 = 360.0;

/// `text` cut (on a character boundary, with a trailing "..") to fit
/// `max_w` points.
fn fit_text(painter: &UiPainter, font: &mut dyn Font, text: &str, pt: f32, max_w: f32) -> String {
    if painter.measure(font, text, pt) <= max_w {
        return text.to_string();
    }
    let mut out: String = text.to_string();
    while !out.is_empty() {
        out.pop();
        let candidate = format!("{out}..");
        if painter.measure(font, &candidate, pt) <= max_w {
            return candidate;
        }
    }
    String::new()
}

/// `thumbs[i]` is row `i`'s thumbnail (`None` for a non-asset row or one
/// whose picture isn't available); `preview` is the selected row's, if it's
/// an asset.
#[allow(clippy::too_many_arguments)]
pub fn draw_file_browser_panel(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    files: &[String],
    thumbs: &[Option<(&Texture, Rect)>],
    preview: Option<&FilePreview>,
    cursor: usize,
    scroll: usize,
    current_folder: &str,
    content: Rect,
    frame: &mut UiFrame,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let selection = theme.role_color(PaletteRole::Selection);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;

    painter.fill(content, panel_bg);

    // Breadcrumbs / Current Path — no themed "text-on-accent" role exists
    // (same gap `chrome.rs`'s `draw_dock_tabs` comments on), so this
    // header stays plain black-on-accent.
    let path_label = format!(" Content > {}", current_folder.replace("./", "").replace("/", " > "));
    let header_rect = Rect::new(content.x, content.y, content.w, row_h);
    draw_text_row(painter, font, &path_label, header_rect, text_px, Color::Black, accent);

    let list_top = content.y + row_h;
    let max_visible = ((content.h - row_h) / row_h).floor().max(0.0) as usize;

    // Step 8-4: with an asset selected and room for it, the right part of
    // the panel previews it; the list (and its row hit rects) keeps the
    // rest.
    let preview = preview.filter(|_| content.w >= PREVIEW_MIN_PANEL_W);
    let preview_w = if preview.is_some() { (content.w * 0.4).min(360.0) } else { 0.0 };
    let list_w = content.w - preview_w;

    if files.is_empty() {
        let empty_rect = Rect::new(content.x, list_top, list_w, row_h);
        draw_text_row(painter, font, " (empty folder)", empty_rect, text_px, dim, panel_bg);
    }

    // Fixed pixel columns for the icon tag, the thumbnail slot, and the
    // name that follows them — every icon tag is the same 5-character
    // width ("[DIR]"/"[LVL]"/"[SCR]"/"[IMG]"/"[SET]"/"[CLP]"/"[---]"), and
    // Cascadia Mono is a real monospace face, so measuring any one of them
    // gives a stable column start (derived from the real font instead of
    // assuming fixed 8px cells). Step 8-4 added the one-row-tall square
    // thumbnail slot between tag and name, on every row so names stay in
    // one column.
    let tag_w = painter.measure(font, "[DIR] ", text_px);
    let name_x = content.x + tag_w + row_h;
    for (i, raw_line) in files.iter().enumerate().skip(scroll).take(max_visible) {
        let row_index = i - scroll + 1;
        let row_rect = Rect::new(content.x, content.y + row_index as f32 * row_h, list_w, row_h);

        let is_selected = i == cursor;
        let bg = if is_selected { selection } else { panel_bg };
        painter.fill(row_rect, bg);
        // 7C-1 (master plan §5.3): registers this row's rect at the exact
        // point it's drawn, replacing `handle_file_browser_click`'s own
        // independently-recomputed `row_idx` arithmetic (E5). Pushed once
        // here (rather than through `draw_row_px`) since a file browser
        // row draws more than one text run — an icon tag, a thumbnail,
        // then the name — depending on which content branch below runs.
        frame.push(
            WidgetId::FileBrowserRow(i),
            UiRect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
        );

        if raw_line.contains("[UP]") {
            let baseline_y = row_rect.y + painter.ascent(font, text_px);
            painter.text(
                font,
                " .. [PARENT FOLDER] ",
                Vec2::new(row_rect.x, baseline_y),
                text_px,
                accent,
            );
            continue;
        }

        // File-kind colors are semantic icon tags, not chrome — left
        // literal, same reasoning as `draw_console`'s log-level colors and
        // `draw_hierarchy`'s entity-kind colors. Step 8-4's three asset
        // kinds (`editor/assets.rs`) get their own.
        let (icon, fg, skip) = if raw_line.starts_with("/ ") {
            ("DIR", Color::Yellow, 2)
        } else if raw_line.starts_with("[] ") {
            ("LVL", Color::Cyan, 3)
        } else if raw_line.starts_with("{} ") {
            ("SCR", Color::Green, 3)
        } else if raw_line.starts_with("<> ") {
            ("IMG", Color::Magenta, 3)
        } else if raw_line.starts_with("## ") {
            ("SET", Color::Rgb(120, 170, 255), 3)
        } else if raw_line.starts_with("~~ ") {
            ("CLP", Color::Rgb(255, 170, 80), 3)
        } else {
            ("---", dim, 3)
        };

        let name = raw_line.get(skip..).unwrap_or(raw_line);
        let icon_tag = format!("[{}]", icon);
        let baseline_y = row_rect.y + painter.ascent(font, text_px);
        painter.text(font, &icon_tag, Vec2::new(row_rect.x, baseline_y), text_px, fg);
        if let Some(Some((tex, src))) = thumbs.get(i) {
            let slot =
                Rect::new(content.x + tag_w + 1.0, row_rect.y + 1.0, row_h - 2.0, row_h - 2.0);
            painter.image(fit_inside(slot, src.w, src.h), tex, Some(*src), Color::White);
        }
        let name = fit_text(painter, font, name, text_px, (content.x + list_w - name_x).max(0.0));
        painter.text(font, &name, Vec2::new(name_x, baseline_y), text_px, text_fg);
    }

    if let Some(p) = preview {
        draw_preview(
            painter,
            font,
            theme,
            p,
            Rect::new(content.x + list_w, list_top, preview_w, content.h - row_h),
        );
    }
}

/// The preview pane: the asset's name and description, and its picture
/// fitted (aspect kept) into the space below them.
fn draw_preview(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    p: &FilePreview,
    r: Rect,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    painter.fill(r, theme.role_color(PaletteRole::Selection));
    let inner = Rect::new(r.x + 1.0, r.y, r.w - 1.0, r.h);
    painter.fill(inner, theme.role_color(PaletteRole::PanelBg));
    let pad = 6.0;
    let text_w = (inner.w - 2.0 * pad).max(0.0);
    let title = fit_text(painter, font, &p.title, text_px, text_w);
    let info = fit_text(painter, font, &p.info, text_px * 0.85, text_w);
    let ascent = painter.ascent(font, text_px);
    let accent = theme.role_color(PaletteRole::Accent);
    let dim = theme.role_color(PaletteRole::TextDim);
    painter.text(font, &title, Vec2::new(inner.x + pad, inner.y + ascent), text_px, accent);
    painter.text(
        font,
        &info,
        Vec2::new(inner.x + pad, inner.y + row_h + ascent * 0.85),
        text_px * 0.85,
        dim,
    );
    let pic = Rect::new(
        inner.x + pad,
        inner.y + 2.0 * row_h + pad * 0.5,
        text_w,
        (inner.h - 2.0 * row_h - pad * 1.5).max(0.0),
    );
    if let Some((tex, src)) = p.image {
        if pic.w > 4.0 && pic.h > 4.0 {
            let dest = fit_inside(pic, src.w, src.h);
            painter.fill(dest, Color::Black);
            painter.image(dest, tex, Some(src), Color::White);
        }
    }
}

/// Step 8-4: what follows the mouse while an asset is dragged — its
/// picture and name — plus a hint naming what releasing here would do.
/// `at` is the mouse in points.
pub fn draw_asset_drag_ghost(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    label: &str,
    image: Option<(&Texture, Rect)>,
    hint: &str,
    at: Vec2,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let size = row_h * 2.0;
    let text_w =
        painter.measure(font, label, text_px).max(painter.measure(font, hint, text_px * 0.85));
    let body = Rect::new(at.x + 14.0, at.y + 14.0, size + text_w + 18.0, size + 6.0);
    painter.fill(body, theme.role_color(PaletteRole::Accent));
    let inner = Rect::new(body.x + 1.0, body.y + 1.0, body.w - 2.0, body.h - 2.0);
    painter.fill(inner, theme.role_color(PaletteRole::PanelBg));
    let slot = Rect::new(inner.x + 2.0, inner.y + 2.0, size, size);
    if let Some((tex, src)) = image {
        painter.image(fit_inside(slot, src.w, src.h), tex, Some(src), Color::White);
    }
    let x = slot.x + size + 8.0;
    let ascent = painter.ascent(font, text_px);
    let fg = theme.role_color(PaletteRole::TextPrimary);
    painter.text(font, label, Vec2::new(x, inner.y + 2.0 + ascent), text_px, fg);
    let dim = theme.role_color(PaletteRole::TextDim);
    painter.text(
        font,
        hint,
        Vec2::new(x, inner.y + 2.0 + row_h + ascent * 0.85),
        text_px * 0.85,
        dim,
    );
}

/// Step 8-4: an accent outline around `r` — the palette panel, while a
/// drag over it would drop there.
pub fn draw_drop_outline(painter: &mut UiPainter, theme: &Theme, r: Rect) {
    let c = theme.role_color(PaletteRole::Accent);
    let t = 2.0;
    painter.fill(Rect::new(r.x, r.y, r.w, t), c);
    painter.fill(Rect::new(r.x, r.y + r.h - t, r.w, t), c);
    painter.fill(Rect::new(r.x, r.y, t, r.h), c);
    painter.fill(Rect::new(r.x + r.w - t, r.y, t, r.h), c);
}
