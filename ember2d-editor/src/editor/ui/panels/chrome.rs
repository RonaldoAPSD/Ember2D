// editor/ui/panels/chrome.rs — Drawing functions for small, always-present
// chrome widgets (title bar, status bar, dock tabs, text-input prompt,
// confirm modal, right-click context menu).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).
//
// Routed through `UiPainter` (7D-3 checkpoint 7, master plan §5.4): every
// text measurement/draw here now goes through `painter.measure`/
// `painter.ascent`/`painter.text`, not `font.measure`/`font.ascent`/
// `renderer.draw_text_px` directly — the font is rasterized at its real
// PHYSICAL size (`raster_px = pt * ui_scale`) once `ui_scale` can differ
// from `render_scale`; calling the plain `Font` methods with a raw point
// size would measure/position against the wrong rasterization entirely.

use super::super::frame::{UiFrame, WidgetId};
use super::super::metrics::ChromeMetrics;
use super::super::types::*;
use super::super::widgets::{draw_button_px, draw_row_px, draw_text_row};
use crate::editor::palette::TilePalette;
use ember2d::renderer::{color::Color, Font, Texture, UiPainter, CELL_H, CELL_W};
use ember2d::theme::{PaletteRole, SliceRole, Theme};
use ember2d_sim::level::TileRecord;
use ember2d_sim::math::{Rect, Vec2};

/// A themed box's frame — one `SliceRole::Panel` 9-slice for the whole
/// pixel `rect`, falling back to a flat `PanelBg` fill when the theme
/// doesn't define that role (7D-1's own "no fabricated geometry"
/// contract) — shared by every BOX-shaped modal below
/// (`draw_text_input`/`draw_confirm_modal`/`draw_context_menu`) so none
/// of them reimplements `draw_panel_chrome`'s own fallback logic
/// (`panel/mod.rs`) a third time. `pub(super)` (rather than private) so
/// `modals.rs` — a sibling under `panels/`, theming its own box-shaped
/// overlays — can reuse it instead of a third copy. Takes a pixel `Rect`
/// directly now (docs/ember2d-master-plan.md §5.4, the `UiRect::from_cells`
/// removal) — no longer converts from cell coordinates itself.
pub(super) fn draw_themed_frame(painter: &mut UiPainter, theme: &Theme, chrome_tex: &Texture, rect: Rect) {
    match theme.slice(SliceRole::Panel) {
        Some(slice) => painter.nine_slice(rect, chrome_tex, slice.src, slice.border, Color::White),
        None => painter.fill(rect, theme.role_color(PaletteRole::PanelBg)),
    }
}

/// The title-bar strip on top of a `draw_themed_frame` box — one `row_h`
/// tall, `SliceRole::TitleBar`, same fallback contract as the frame above.
pub(super) fn draw_themed_title_strip(
    painter: &mut UiPainter,
    theme: &Theme,
    chrome_tex: &Texture,
    x: f32,
    y: f32,
    w: f32,
) {
    let rect = Rect::new(x, y, w, theme.metrics.row_h);
    match theme.slice(SliceRole::TitleBar) {
        Some(slice) => painter.nine_slice(rect, chrome_tex, slice.src, slice.border, Color::White),
        None => painter.fill(rect, theme.role_color(PaletteRole::TitleBg)),
    }
}

/// The title bar, status bar, and dock tab strip stay flat, theme-colored
/// fills rather than 9-slice (7D-2, master plan §5.4) — each is one cell
/// tall, and this theme's own 6px border (`themes/ember-clean/chrome.png`)
/// would consume most of that height as border with almost no visible
/// center, looking disproportionate. 9-slice is reserved for genuinely
/// box-shaped chrome below (`draw_text_input`/`draw_confirm_modal`/
/// `draw_context_menu`), where a border reads as a border rather than
/// swallowing the whole element.
pub fn draw_title_bar(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    metrics: &ChromeMetrics,
    level_name: &str,
    unsaved: bool,
    undo_count: usize,
    redo_count: usize,
    scroll: (f32, f32),
    level_size: (usize, usize),
) {
    let bg = theme.role_color(PaletteRole::TitleBg);
    let fg = theme.role_color(PaletteRole::TitleText);
    let accent = theme.role_color(PaletteRole::Accent);
    // `metrics.bar_h` (7D-3, docs/ember2d-master-plan.md §5.4 — was a
    // hardcoded `CELL_H`) — the menu bar drawn right below this
    // (`draw_menu_toolbar`, `ui/menu.rs`) now starts at exactly
    // `metrics.bar_h` too, so the two stay in sync through the same
    // `ChromeMetrics` value rather than a shared magic constant.
    let row_h = metrics.bar_h;
    let text_px = theme.font_sizes.body;
    let (pixel_w, _) = painter.space().screen_pt();
    let pad = painter.measure(font, " ", text_px);

    painter.fill(Rect::new(0.0, 0.0, pixel_w, row_h), bg);
    let baseline_y = row_h_baseline(painter, 0.0, font, text_px);
    painter.text(font, "EMBER2D EDITOR", Vec2::new(pad, baseline_y), text_px, fg);

    let saved_marker = if unsaved { "*" } else { " " };
    let scroll_str = if scroll.0.abs() > 0.001 || scroll.1.abs() > 0.001 {
        format!(" @{:.1},{:.1}", scroll.0, scroll.1)
    } else {
        String::new()
    };
    let info = format!(
        "{}{}  {}×{}{}  U:{} R:{}",
        saved_marker, level_name, level_size.0, level_size.1, scroll_str, undo_count, redo_count
    );
    let info_x = (pixel_w - painter.measure(font, &info, text_px) - pad).max(0.0);
    painter.text(font, &info, Vec2::new(info_x, baseline_y), text_px, accent);
}

/// A row's baseline `y`, given its own top `y=0`-relative top — shared by
/// every chrome function in this file that draws more than one text run
/// on the same row (so each `text` call agrees on the exact same baseline
/// instead of each re-deriving it).
fn row_h_baseline(painter: &UiPainter, row_top: f32, font: &mut dyn Font, px: f32) -> f32 {
    row_top + painter.ascent(font, px)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_status_bar(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    metrics: &ChromeMetrics,
    mouse: &ember2d::mouse::MouseState,
    _palette: &TilePalette,
    grid_overlay: bool,
    _save_path: &str,
    tile_under: Option<&TileRecord>,
    mode_hint: &str,
    scroll: (f32, f32),
    active_layer: u8,
    erase_size: usize,
    canvas_origin_px: (f32, f32),
    zoom: f32,
) {
    let bg = theme.role_color(PaletteRole::PanelBg);
    let fg = theme.role_color(PaletteRole::TextPrimary);
    let accent = theme.role_color(PaletteRole::Accent);
    // `metrics.bar_h` (7D-3, docs/ember2d-master-plan.md §5.4 — was a
    // hardcoded `CELL_H`) — `PanelManager::apply_layout` reserves exactly
    // one `metrics.bar_h`-tall row at the bottom of the screen for this
    // bar, through the same `ChromeMetrics` value, so the two can't drift.
    let row_h = metrics.bar_h;
    let text_px = theme.font_sizes.body;
    let (pixel_w, pixel_h) = painter.space().screen_pt();
    let status_y = pixel_h - row_h;
    painter.fill(Rect::new(0.0, status_y, pixel_w, row_h), bg);
    let baseline_y = row_h_baseline(painter, status_y, font, text_px);

    // R66-D (§3 in the master plan): was `mouse.cell_x/cell_y` (viewport
    // cells) minus a CELL-ROUNDED `canvas_x`/`canvas_y` (the viewport
    // panel's own `content_x()`/`content_y()` bridge, which rounds to the
    // nearest engine cell — no longer exact once the viewport's real pixel
    // origin isn't a whole-cell multiple, e.g. after the chrome bars above
    // it grew to `metrics.bar_h`). Recomputed here from the EXACT pixel
    // origin instead, matching `mouse_to_grid`'s own formula
    // (`impl_state/viewport.rs`) exactly, so the readout can never disagree
    // with where a click would actually land.
    let cx = (mouse.pixel_x - canvas_origin_px.0) / CELL_W as f32 / zoom + scroll.0;
    let cy = (mouse.pixel_y - canvas_origin_px.1) / CELL_H as f32 / zoom + scroll.1;
    let pos_str = format!(" ({:3.1},{:3.1})", cx, cy);
    painter.text(font, &pos_str, Vec2::new(0.0, baseline_y), text_px, accent);

    let lyr_name = match active_layer {
        0 => "Background",
        1 => "Main",
        2 => "Foreground",
        _ => "Unknown",
    };
    let lyr_str = format!("[LAYER: {}]", lyr_name);
    // Measured from `pos_str`'s own real width rather than a fixed pixel
    // literal (was `10.0 * CELL_W` — a fixed-width-bitmap-glyph column
    // that a real, proportionally-advancing Cascadia `font` (7D-3) can run
    // past, overlapping this label). A small gap keeps the old rhythm at
    // the common case while never colliding when the readout is wide.
    let gap = metrics.padding;
    let col10 = painter.measure(font, &pos_str, text_px) + gap;
    let col30 = col10 + painter.measure(font, &lyr_str, text_px) + gap;
    painter.text(font, &lyr_str, Vec2::new(col10, baseline_y), text_px, fg);

    if !mode_hint.is_empty() {
        painter.text(font, &format!("| {}", mode_hint), Vec2::new(col30, baseline_y), text_px, fg);
    } else if let Some(tile) = tile_under {
        let script_mark = if tile.script.is_some() { "[S]" } else { "   " };
        let props = format!(
            "| [{}] s:{} t:{} {} T=script",
            tile.tag, tile.solid as u8, tile.trigger as u8, script_mark
        );
        painter.text(font, &props, Vec2::new(col30, baseline_y), text_px, accent);
    } else {
        let grid_hint = if grid_overlay { "Tab:off" } else { "Tab:grd" };
        let erase_hint = format!("E:{}px", erase_size);
        let hints = format!("| {} S:save U:undo R:redo {}", erase_hint, grid_hint);
        painter.text(font, &hints, Vec2::new(col30, baseline_y), text_px, fg);
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
pub fn draw_dock_tabs(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    strip: Rect,
    panels: &[(PanelId, &str)],
    active: Option<PanelId>,
    frame: &mut UiFrame,
) {
    if panels.is_empty() {
        return;
    }

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_px = theme.font_sizes.body;
    // Background for the tab bar
    painter.fill(strip, panel_bg);

    let gap = painter.measure(font, " ", text_px);
    let mut cursor_x = strip.x;
    for (id, title) in panels {
        let is_active = Some(*id) == active;
        // No themed "text-on-accent" role exists (7D-1's `PaletteRole` has
        // no such variant) — plain black reads fine on the bright amber
        // active-tab fill, matching how the close button/other
        // bright-background chrome elsewhere already handles this gap.
        let (fg, bg) = if is_active {
            (Color::Black, theme.role_color(PaletteRole::Accent))
        } else {
            (theme.role_color(PaletteRole::TabInactive), panel_bg)
        };

        let label = format!(" {} ", title);
        let label_w = painter.measure(font, &label, text_px);
        if cursor_x + label_w > strip.x + strip.w {
            break;
        }

        let tab_rect = Rect::new(cursor_x, strip.y, label_w, strip.h);
        draw_row_px(painter, frame, font, WidgetId::Tab(*id), tab_rect, text_px, &label, fg, bg);
        cursor_x += label_w + gap;
    }
}

pub fn draw_text_input(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    prompt: &str,
    buffer: &str,
    screen_w: f32,
    screen_h: f32,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    // Same overall footprint as the old 40x7-cell modal (`CELL_W`-based
    // width; height in real rows now, not `CELL_H`-based) — not
    // re-measured from content, just carried forward as a fixed budget
    // generous enough for the prompts this draws.
    let mw = 40.0 * CELL_W as f32;
    let mh = 7.0 * row_h;
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    let panel_fg = theme.role_color(PaletteRole::TextPrimary);
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let dim = theme.role_color(PaletteRole::TextDim);

    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);
    let title = format!(" {} ", prompt.to_uppercase());
    // `panel_bg`, not `TitleBg`, behind the title text itself — matches
    // the strip's own center-fill color closely enough to read as
    // intentional, not a bug, and recurs identically in
    // `draw_confirm_modal` below; sized to the text's own measured width
    // (not the whole strip) so it doesn't paint over the rest of the
    // themed title strip just drawn.
    let title_w = painter.measure(font, &title, text_px);
    let title_rect = Rect::new(mx, my, title_w, row_h);
    draw_text_row(painter, font, &title, title_rect, text_px, theme.role_color(PaletteRole::TitleText), panel_bg);

    // Input field
    let input_row = Rect::new(mx, my + 2.0 * row_h, mw, row_h);
    let label = format!("> {}█", buffer);
    let max_buf_w = mw - 2.0 * painter.measure(font, " ", text_px);
    let clipped_buf: String = if painter.measure(font, &label, text_px) > max_buf_w {
        // Keep the TAIL of the buffer visible (so the cursor at the end
        // stays on-screen while typing), prefixed with "..". Walks
        // characters from the end accumulating the font's own per-glyph
        // advance, safe on non-ASCII input a byte-length budget wouldn't
        // be. `painter.space()` measures each glyph at the font's real
        // raster size, same "don't measure at the wrong size" reasoning
        // this whole file now follows.
        let space = painter.space();
        let dots_w = painter.measure(font, "..", text_px);
        let budget = (max_buf_w - dots_w).max(0.0);
        let chars: Vec<char> = label.chars().collect();
        let mut suffix_w = 0.0;
        let mut start = chars.len();
        for (i, &ch) in chars.iter().enumerate().rev() {
            let cw = space.advance(font, ch, text_px);
            if suffix_w + cw > budget {
                break;
            }
            suffix_w += cw;
            start = i;
        }
        format!("..{}", chars[start..].iter().collect::<String>())
    } else {
        label
    };
    let buf_w = painter.measure(font, &clipped_buf, text_px);
    let input_x = mx + ((mw - buf_w) / 2.0).max(0.0);
    // Exactly the string's own width, matching the old `draw_str`'s own
    // per-character background fill — not the whole modal width, so the
    // highlighted `InputBg` box hugs the text instead of stretching to
    // the modal's right edge.
    let input_rect = Rect::new(input_x, input_row.y, buf_w, row_h);
    draw_text_row(painter, font, &clipped_buf, input_rect, text_px, panel_fg, theme.role_color(PaletteRole::InputBg));

    // Helper text
    let hint = "[Enter] Confirm   [Esc] Cancel";
    let hint_w = painter.measure(font, hint, text_px);
    let hint_x = mx + ((mw - hint_w) / 2.0).max(0.0);
    let hint_rect = Rect::new(hint_x, my + mh - 3.0 * row_h, hint_w, row_h);
    draw_text_row(painter, font, hint, hint_rect, text_px, dim, panel_bg);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_confirm_modal(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    title: &str,
    message: &str,
    screen_w: f32,
    screen_h: f32,
    frame: &mut UiFrame,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let mw = 40.0 * CELL_W as f32;
    let mh = 8.0 * row_h;
    let mx = ((screen_w - mw) / 2.0).max(0.0);
    let my = ((screen_h - mh) / 2.0).max(0.0);
    let panel_bg = theme.role_color(PaletteRole::PanelBg);

    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    draw_themed_title_strip(painter, theme, chrome_tex, mx, my, mw);
    // `panel_bg` behind the title text itself, sized to its own measured
    // width — see `draw_text_input`'s own comment on this same pattern.
    let title_text = format!(" {} ", title.to_uppercase());
    let title_w = painter.measure(font, &title_text, text_px);
    draw_text_row(
        painter,
        font,
        &title_text,
        Rect::new(mx, my, title_w, row_h),
        text_px,
        theme.role_color(PaletteRole::TitleText),
        panel_bg,
    );

    // Message
    let msg_w = painter.measure(font, message, text_px);
    let msg_x = mx + ((mw - msg_w) / 2.0).max(0.0);
    draw_text_row(
        painter,
        font,
        message,
        Rect::new(msg_x, my + 2.0 * row_h, msg_w, row_h),
        text_px,
        theme.role_color(PaletteRole::TextPrimary),
        panel_bg,
    );

    // Buttons — 7C-1 (master plan §5.3): `draw_button_px` registers each
    // rect at the exact point it's drawn, replacing `input/modal.rs`'s
    // own independently-recomputed `yes_x`/`no_x`/`btn_y` (E5). No
    // themed "text-on-accent" role exists (same gap `draw_dock_tabs`
    // documents above), so YES stays plain black-on-accent.
    let btn_y = my + 5.0 * row_h;
    let yes_label = " [ YES ] ";
    let no_label = " [ NO ]  ";
    let yes_w = painter.measure(font, yes_label, text_px);
    let no_w = painter.measure(font, no_label, text_px);
    // Fixed pixel insets (8/15 "cells" worth of the old 8px grid) — same
    // literal-pixel-anchor reasoning `draw_status_bar`'s own `col10`/
    // `col30` comment gives.
    let yes_x = mx + 8.0 * CELL_W as f32;
    let no_x = mx + mw - 15.0 * CELL_W as f32;
    let accent = theme.role_color(PaletteRole::Accent);
    let dim = theme.role_color(PaletteRole::TextDim);

    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::ConfirmYes,
        Rect::new(yes_x, btn_y, yes_w, row_h),
        text_px,
        yes_label,
        Color::Black,
        accent,
    );
    draw_button_px(
        painter,
        frame,
        font,
        WidgetId::ConfirmNo,
        Rect::new(no_x, btn_y, no_w, row_h),
        text_px,
        no_label,
        dim,
        panel_bg,
    );
}

pub fn draw_context_menu(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    menu: &ContextMenu,
    frame: &mut UiFrame,
) {
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let mw = 20.0 * CELL_W as f32;
    let mh = (menu.items.len() as f32 + 1.0) * row_h;
    // `menu.x`/`menu.y` are still cell coordinates (the mouse's own click
    // position, `mouse.cell_x`/`cell_y` — a genuinely cell-based reading,
    // not chrome layout) — converted to pixels once here rather than
    // changing what `ContextMenu` itself stores (R83, §3 in the master
    // plan: logged, not fixed — the conversion below is exact either way).
    let mx = menu.x as f32 * CELL_W as f32;
    let my = menu.y as f32 * CELL_H as f32;

    // Boundary check
    let (pixel_w, pixel_h) = painter.space().screen_pt();
    let mx = if mx + mw > pixel_w { (pixel_w - mw).max(0.0) } else { mx };
    let my = if my + mh > pixel_h { (pixel_h - mh).max(0.0) } else { my };

    draw_themed_frame(painter, theme, chrome_tex, Rect::new(mx, my, mw, mh));
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let accent = theme.role_color(PaletteRole::Accent);

    for (i, (label, _)) in menu.items.iter().enumerate() {
        let row_rect = Rect::new(mx, my + (i as f32 + 1.0) * row_h, mw, row_h);
        let is_selected = i == menu.selected;
        // No themed "text-on-accent" role — see `draw_dock_tabs`'s own
        // comment on this same gap.
        let (fg, bg) = if is_selected { (Color::Black, accent) } else { (text_fg, panel_bg) };

        let text = format!(" {} ", label);
        // 7C-1 (master plan §5.3): `draw_row_px` registers this rect at
        // the exact point it's drawn, replacing `input/context_menu.rs`'s
        // own independently-recomputed `mw`/`mh`/boundary-clamp math (E5).
        draw_row_px(painter, frame, font, WidgetId::ContextMenuRow(i), row_rect, text_px, &text, fg, bg);
    }
}
