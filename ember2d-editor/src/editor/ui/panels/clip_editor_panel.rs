// editor/ui/panels/clip_editor_panel.rs — the animation clip editor dialog.
//
// Step 8-3 (docs/ember2d-master-plan.md §5.7: "build clips, scrub frames,
// preview looping"). Left: the project's clips (click to edit) and this
// clip's settings. Right, top: the tileset sheet — click a named region to
// append it as a frame. Right, bottom: the frame strip (click a frame to
// scrub to it), frame controls, and a large live preview. Drawn in points
// through `UiPainter` like every modal since 7D-3; every clickable thing is
// pushed into `UiFrame` at the rect it's drawn (`input/clip_editor.rs`
// only hit-tests those).

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::widgets::{draw_button_px, draw_text_row};
use super::chrome::{draw_themed_frame, draw_themed_title_strip};
use crate::editor::clip_editor::{ClipEditor, ClipField};
use crate::editor::sprites::{fit_inside, SpriteAssets};
use ember2d::renderer::{color::Color, Font, Texture, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;
use ember2d_sim::tileset::SpriteRef;

/// A 1-point (or `t`-point) outline — the importer's helper, repeated
/// rather than shared since the two dialogs are each other's only users.
fn outline(painter: &mut UiPainter, r: Rect, t: f32, color: Color) {
    painter.fill(Rect::new(r.x, r.y, r.w, t), color);
    painter.fill(Rect::new(r.x, r.y + r.h - t, r.w, t), color);
    painter.fill(Rect::new(r.x, r.y, t, r.h), color);
    painter.fill(Rect::new(r.x + r.w - t, r.y, t, r.h), color);
}

/// Draw frame `region` of `tileset` fitted into `slot` (aspect kept).
fn thumb(painter: &mut UiPainter, sprites: &SpriteAssets, tileset: &str, region: &str, slot: Rect) {
    if let Some((tex, src)) = sprites.resolve(&SpriteRef::new(tileset, region)) {
        painter.image(fit_inside(slot, src.w, src.h), tex, Some(src), Color::White);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_clip_editor_modal(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    ce: &ClipEditor,
    sprites: &SpriteAssets,
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
    let danger = theme.role_color(PaletteRole::Danger);
    let selection = theme.role_color(PaletteRole::Selection);
    let title_fg = theme.role_color(PaletteRole::TitleText);

    let mw = (screen_w - 2.0 * row_h).clamp(480.0, 1000.0);
    let mh = (screen_h - 2.0 * row_h).clamp(20.0 * row_h, 720.0);
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);
    let pad = row_h * 0.5;
    let title = " ANIMATION CLIPS ";
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

    let text =
        |painter: &mut UiPainter, font: &mut dyn Font, s: &str, r: Rect, fg: Color, bg: Color| {
            draw_text_row(painter, font, s, r, text_px, fg, bg);
        };
    let button = |painter: &mut UiPainter,
                  font: &mut dyn Font,
                  frame: &mut UiFrame,
                  id: WidgetId,
                  label: &str,
                  at: (f32, f32),
                  fg: Color,
                  bg: Color|
     -> f32 {
        let w = painter.measure(font, label, text_px);
        draw_button_px(
            painter,
            frame,
            font,
            id,
            Rect::new(at.0, at.1, w, row_h),
            text_px,
            label,
            fg,
            bg,
        );
        w
    };

    // ── Left column: the project's clips, then this clip's settings ──────
    let col_w = painter.measure(font, "Tileset: [< tileset_name_ >]", text_px);
    let x = mx + pad;
    let mut y = my + row_h * 1.5;
    text(painter, font, "Clips:", Rect::new(x, y, col_w, row_h), dim, panel_bg);
    y += row_h;
    let max_rows = 6;
    for (i, name) in sprites.clips.keys().enumerate().take(max_rows) {
        let r = Rect::new(x, y, col_w, row_h);
        let bg = if *name == ce.name { selection } else { panel_bg };
        painter.fill(r, bg);
        text(painter, font, &format!(" {name}"), r, text_fg, bg);
        frame.push(WidgetId::ClipListRow(i), UiRect::new(r.x, r.y, r.w, r.h));
        y += row_h;
    }
    if sprites.clips.is_empty() {
        text(painter, font, " (none saved yet)", Rect::new(x, y, col_w, row_h), dim, panel_bg);
        y += row_h;
    }
    button(painter, font, frame, WidgetId::ClipNew, " [ + New ] ", (x, y), Color::Black, accent);
    y += row_h * 1.5;

    let label_w = painter.measure(font, "Tileset: ", text_px);
    for (field, label, value) in
        [(ClipField::Name, "Name:", &ce.name), (ClipField::Fps, "FPS:", &ce.fps)]
    {
        text(painter, font, label, Rect::new(x, y, label_w, row_h), text_fg, panel_bg);
        let focused = ce.focus == Some(field);
        let shown = if focused { format!("[{value}█]") } else { format!("[{value}]") };
        let r = Rect::new(x + label_w, y, col_w - label_w, row_h);
        text(painter, font, &shown, r, if focused { accent } else { text_fg }, input_bg);
        frame.push(WidgetId::ClipField(field), UiRect::new(r.x, r.y, r.w, r.h));
        y += row_h;
    }
    let loop_label = if ce.looping { "[x] Loop" } else { "[ ] Loop" };
    button(painter, font, frame, WidgetId::ClipLoop, loop_label, (x, y), text_fg, panel_bg);
    y += row_h;
    text(painter, font, "Tileset:", Rect::new(x, y, label_w, row_h), text_fg, panel_bg);
    let ts = ce.tileset.as_deref().unwrap_or("(none)");
    let cycle = format!("[< {ts} >]");
    button(
        painter,
        font,
        frame,
        WidgetId::ClipTilesetCycle,
        &cycle,
        (x + label_w, y),
        accent,
        panel_bg,
    );
    y += row_h * 1.5;

    if let Some((ok, msg)) = &ce.status {
        let max_chars = ((col_w / painter.measure(font, "m", text_px)).floor() as usize).max(8);
        let mut line = String::new();
        let color = if *ok { accent } else { danger };
        for w in msg.split(' ') {
            if !line.is_empty() && line.len() + 1 + w.len() > max_chars {
                text(painter, font, &line, Rect::new(x, y, col_w, row_h), color, panel_bg);
                y += row_h;
                line.clear();
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(w);
        }
        text(painter, font, &line, Rect::new(x, y, col_w, row_h), color, panel_bg);
    }

    let by = my + mh - row_h * 1.5;
    let mut bx = x;
    bx += button(
        painter,
        font,
        frame,
        WidgetId::ClipSave,
        " [ Save ] ",
        (bx, by),
        Color::Black,
        accent,
    ) + pad;
    bx += button(
        painter,
        font,
        frame,
        WidgetId::ClipAddToPalette,
        " [ Add to Palette ] ",
        (bx, by),
        Color::Black,
        accent,
    ) + pad;
    button(painter, font, frame, WidgetId::ClipClose, " [ Close ] ", (bx, by), dim, panel_bg);

    // ── Right: sheet on top, frames + preview below ─────────────────────
    let rx = x + col_w + pad;
    let rw = mx + mw - pad - rx;
    let top = my + row_h * 1.5;
    let body_h = by - pad - top;
    if rw <= 0.0 || body_h <= 0.0 {
        return;
    }
    let sheet_h = body_h * 0.5;
    let Some(tileset) = ce.tileset.as_ref().and_then(|t| sprites.tilesets.get(t)) else {
        text(
            painter,
            font,
            "No tileset: use File > Import Tileset... first.",
            Rect::new(rx, top, rw, row_h),
            dim,
            panel_bg,
        );
        return;
    };
    if let Some(tex) = tileset.texture.as_ref() {
        let (tw, th) = (tex.width as f32, tex.height as f32);
        let fit = (rw / tw).min((sheet_h - row_h) / th);
        let zoom = if fit >= 1.0 { fit.floor() } else { fit };
        let sheet = Rect::new(rx, top, tw * zoom, th * zoom);
        painter.fill(sheet, Color::Rgb(40, 40, 48));
        painter.image(sheet, tex, None, Color::White);
        frame.push(WidgetId::ClipSheet, UiRect::new(sheet.x, sheet.y, sheet.w, sheet.h));
        for reg in &tileset.data.regions {
            let c = tileset.data.cell_rect(reg.col, reg.row, reg.w, reg.h);
            let r = Rect::new(sheet.x + c.x * zoom, sheet.y + c.y * zoom, c.w * zoom, c.h * zoom);
            outline(painter, r, 1.0, accent);
        }
        text(
            painter,
            font,
            "Click a named (outlined) cell to add it as the next frame.",
            Rect::new(rx, sheet.y + sheet.h + 2.0, rw, row_h),
            dim,
            panel_bg,
        );
    }

    // Frames strip (wraps), then frame controls.
    let ts_name = tileset.data.name.as_str();
    let strip_top = top + sheet_h + pad;
    let preview_side = (body_h - sheet_h - pad).min(rw * 0.35).max(row_h * 2.0);
    let strip_w = rw - preview_side - pad;
    let cell = (row_h * 2.0).min(strip_w.max(row_h));
    let per_row = ((strip_w / (cell + 4.0)).floor() as usize).max(1);
    let playing_frame = ce.preview_frame();
    let mut strip_bottom = strip_top;
    for (i, region) in ce.frames.iter().enumerate() {
        let (c, r) = (i % per_row, i / per_row);
        let slot = Rect::new(
            rx + c as f32 * (cell + 4.0),
            strip_top + r as f32 * (cell + 4.0),
            cell,
            cell,
        );
        painter.fill(slot, Color::Rgb(40, 40, 48));
        thumb(painter, sprites, ts_name, region, slot);
        if Some(i) == ce.selected {
            outline(painter, slot, 2.0, Color::White);
        } else if Some(i) == playing_frame {
            outline(painter, slot, 1.0, accent);
        }
        frame.push(WidgetId::ClipFrame(i), UiRect::new(slot.x, slot.y, slot.w, slot.h));
        strip_bottom = slot.y + slot.h;
    }
    if ce.frames.is_empty() {
        text(
            painter,
            font,
            "(no frames yet)",
            Rect::new(rx, strip_top, strip_w, row_h),
            dim,
            panel_bg,
        );
        strip_bottom = strip_top + row_h;
    }
    let cy = strip_bottom + pad;
    let mut cx = rx;
    for (id, label) in [
        (WidgetId::ClipMoveLeft, " [<] "),
        (WidgetId::ClipMoveRight, " [>] "),
        (WidgetId::ClipDeleteFrame, " [Del] "),
        (WidgetId::ClipPlay, if ce.playing { " [ Pause ] " } else { " [ Play ] " }),
    ] {
        cx += button(painter, font, frame, id, label, (cx, cy), text_fg, input_bg) + 4.0;
    }
    let info = match (playing_frame, ce.frames.len()) {
        (Some(f), n) => format!("frame {}/{}: {}", f + 1, n, ce.frames[f]),
        _ => String::new(),
    };
    text(painter, font, &info, Rect::new(rx, cy + row_h, strip_w, row_h), dim, panel_bg);

    // Live preview — the clip as it will play.
    let pv = Rect::new(rx + strip_w + pad, strip_top, preview_side, preview_side);
    painter.fill(pv, Color::Rgb(24, 24, 30));
    outline(painter, pv, 1.0, dim);
    if let Some(f) = playing_frame {
        let inner = Rect::new(pv.x + 4.0, pv.y + 4.0, pv.w - 8.0, pv.h - 8.0);
        thumb(painter, sprites, ts_name, &ce.frames[f], inner);
    }
}
