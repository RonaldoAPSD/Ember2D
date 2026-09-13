// editor/panel/chrome.rs — draw_panel_chrome, split out of panel/mod.rs
// (7D-3, docs/ember2d-master-plan.md §5.4) alongside that file's own
// conversion to `ChromeMetrics`-driven sizing, to keep it under CLAUDE.md's
// 750-line hard limit.

use super::super::ui::{UiFrame, UiRect, WidgetId};
use super::{ChromeMetrics, DockSide, Panel, PanelId};
use ember2d::renderer::{color::Color, Font, Texture, UiPainter};
use ember2d::theme::{PaletteRole, SliceRole, Theme};
use ember2d_sim::math::{Rect, Vec2};

/// Draw a panel's frame, title bar, close button, and resize handle through
/// the editor's theme (7D-2, master plan §5.4 — this was the first
/// function converted; every panel's own CONTENT — inspector rows,
/// console text, dock tabs, modals, the script editor, the node graph —
/// is themed too now, each in its own file under `ui/`). Registers each
/// interactive element's hit rect in `frame` at the exact point it's
/// drawn (Phase 7 Part 1d) — see `ui/frame.rs`'s header comment for why
/// this one function doing both is what closes defect E5; that
/// discipline is unchanged by which pixels actually land.
///
/// `theme`/`chrome_tex`/`font` come from `EditorState::theme`/
/// `theme_chrome_tex`/`font` (see `load_editor_theme_named`'s own doc
/// comment, `theme_loader.rs`) — resolved once, passed in rather than
/// looked up here, so this function stays free of any asset/GPU dependency
/// itself (same reasoning `DrawSurface::draw_nine_slice_px` documents for
/// taking an already-resolved `&Texture`).
///
/// `metrics` (7D-3, master plan §5.4) is a `ChromeMetrics` built fresh from
/// `theme` by the caller every frame (`impl_render.rs`) — every size below
/// (bar height, border/close/grip) comes from it, not the engine's fixed
/// `CELL_W`/`CELL_H`, so a theme switch resizes this panel's whole frame on
/// the very next draw.
///
/// The viewport is excluded entirely (still a flat black fill, no chrome)
/// — it stays on the engine's own renderer regardless of theme (7C-9
/// decision gate, §7.1); it's the one panel this function never themes.
/// Its own fill now uses `panel.rect` directly, in pixels — not a
/// cell-rounded re-derivation the way it did pre-7D-3 (part of R66, §3 in
/// the master plan: that rounding was one of several independent
/// re-derivations of the viewport rect that could disagree with each
/// other after a sub-cell resize).
pub fn draw_panel_chrome(
    painter: &mut UiPainter,
    panel: &Panel,
    frame: &mut UiFrame,
    theme: &Theme,
    chrome_tex: &Texture,
    font: &mut dyn Font,
    metrics: &ChromeMetrics,
) {
    // A panel smaller than one border + one bar in either dimension has no
    // sensible frame to draw — same "too small to bother" guard the old
    // cell-based `w < 2 || h < 2` check made, expressed in points now.
    if panel.rect.w < 2.0 * metrics.border || panel.rect.h < 2.0 * metrics.bar_h {
        return;
    }

    if panel.id == PanelId::Viewport {
        painter.fill(Rect::new(panel.rect.x, panel.rect.y, panel.rect.w, panel.rect.h), Color::Black);
        return;
    }

    let full_rect = Rect::new(panel.rect.x, panel.rect.y, panel.rect.w, panel.rect.h);
    let title_rect = Rect::new(panel.rect.x, panel.rect.y, panel.rect.w, metrics.bar_h);

    // 1. Frame — one 9-slice covering the WHOLE panel, corners/edges/center
    // (its center IS the interior fill the old flat implementation drew as
    // a separate step). Falls back to a flat fill if the theme doesn't
    // define this role (7D-1's own "missing slice, no fabricated
    // geometry" contract — see `Theme::slice`'s own doc comment). Routed
    // through `UiPainter::nine_slice` (7D-3 checkpoint 7) — its own
    // `border_scale` is `ui_scale / render_scale`, not the `1.0` every
    // earlier checkpoint of this step hardcoded while the two were pinned
    // equal.
    match theme.slice(SliceRole::Panel) {
        Some(slice) => painter.nine_slice(full_rect, chrome_tex, slice.src, slice.border, Color::White),
        None => painter.fill(full_rect, theme.role_color(PaletteRole::PanelBg)),
    }

    // 2. Title bar — a second 9-slice over just the top strip, drawn AFTER
    // the frame so it wins there.
    match theme.slice(SliceRole::TitleBar) {
        Some(slice) => painter.nine_slice(title_rect, chrome_tex, slice.src, slice.border, Color::White),
        None => painter.fill(title_rect, theme.role_color(PaletteRole::TitleBg)),
    }
    frame.push(
        WidgetId::TitleBar(panel.id),
        UiRect::new(panel.rect.x, panel.rect.y, panel.rect.w, metrics.bar_h),
    );

    // 3. Title text — measure-centered in the title bar, minus the close
    // button's own reserved width on the right.
    let dock_indicator = match panel.dock {
        DockSide::Left => "< ",
        DockSide::Right => "> ",
        DockSide::Bottom => "v ",
        DockSide::None => "",
    };
    let title = format!("{}{}", dock_indicator, panel.title);
    let title_px = theme.font_sizes.body;
    let close_w = metrics.close_w;
    let avail_w = (panel.rect.w - close_w).max(0.0);
    let measured_w = painter.measure(font, &title, title_px);
    let text_x = panel.rect.x + ((avail_w - measured_w).max(0.0) / 2.0).round();
    let baseline_y = (panel.rect.y + painter.ascent(font, title_px)).round();
    painter.text(font, &title, Vec2::new(text_x, baseline_y), title_px, theme.role_color(PaletteRole::TitleText));

    // 4. Close button — a small themed square at the title bar's right
    // edge, an "X" drawn in the same theme font as the title.
    let close_rect = Rect::new(panel.rect.right() - close_w, panel.rect.y, close_w, metrics.bar_h);
    if let Some(slice) = theme.slice(SliceRole::Button) {
        painter.nine_slice(close_rect, chrome_tex, slice.src, slice.border, Color::White);
    }
    let x_w = painter.measure(font, "X", title_px);
    let x_x = (close_rect.x + (close_w - x_w) / 2.0).round();
    painter.text(font, "X", Vec2::new(x_x, baseline_y), title_px, theme.role_color(PaletteRole::TextPrimary));
    frame.push(
        WidgetId::CloseBtn(panel.id),
        UiRect::new(close_rect.x, close_rect.y, close_rect.w, close_rect.h),
    );

    // 5. Resize handle — a themed square at the bottom-right corner,
    // exactly `2 * border` on a side (R74, §3 in the master plan — the old
    // fixed 8px grip with a 6+6px border overlapped its own corners; this
    // size is ALL corner, never overlapping, at any theme's own border
    // width).
    let grip_size = metrics.grip;
    let grip_rect =
        Rect::new(panel.rect.right() - grip_size, panel.rect.bottom() - grip_size, grip_size, grip_size);
    if let Some(slice) = theme.slice(SliceRole::ResizeGrip) {
        painter.nine_slice(grip_rect, chrome_tex, slice.src, slice.border, Color::White);
    }
    frame.push(
        WidgetId::ResizeHandle(panel.id),
        UiRect::new(grip_rect.x, grip_rect.y, grip_rect.w, grip_rect.h),
    );
}
