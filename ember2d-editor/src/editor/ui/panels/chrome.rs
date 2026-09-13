// editor/ui/panels/chrome.rs — Drawing functions for small, always-present
// chrome widgets (title bar, status bar, dock tabs, text-input prompt,
// confirm modal, right-click context menu).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::types::*;
use super::super::widgets::{draw_button, draw_row};
use crate::editor::palette::TilePalette;
use ember2d::renderer::{color::Color, DrawSurface, Font, Texture, CELL_H, CELL_W};
use ember2d::theme::{PaletteRole, SliceRole, Theme};
use ember2d_sim::level::TileRecord;
use ember2d_sim::math::Rect;

/// A themed box's frame — one `SliceRole::Panel` 9-slice for the whole
/// `(x, y, w, h)` cell rect, falling back to a flat `PanelBg` fill when
/// the theme doesn't define that role (7D-1's own "no fabricated
/// geometry" contract) — shared by every BOX-shaped modal below
/// (`draw_text_input`/`draw_confirm_modal`/`draw_context_menu`) so none
/// of them reimplements `draw_panel_chrome`'s own fallback logic
/// (`panel/mod.rs`) a third time.
fn draw_themed_frame(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    chrome_tex: &Texture,
    x: usize,
    y: usize,
    w: usize,
    h: usize,
) {
    let rect = Rect::new(
        x as f32 * CELL_W as f32,
        y as f32 * CELL_H as f32,
        w as f32 * CELL_W as f32,
        h as f32 * CELL_H as f32,
    );
    match theme.slice(SliceRole::Panel) {
        Some(slice) => renderer.draw_nine_slice_px(rect, chrome_tex, slice.src, slice.border, Color::White),
        None => renderer.draw_rect_filled(x, y, w, h, ' ', Color::White, theme.role_color(PaletteRole::PanelBg)),
    }
}

/// The title-bar strip on top of a `draw_themed_frame` box — one cell
/// tall, `SliceRole::TitleBar`, same fallback contract as the frame above.
fn draw_themed_title_strip(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    chrome_tex: &Texture,
    x: usize,
    y: usize,
    w: usize,
) {
    let rect =
        Rect::new(x as f32 * CELL_W as f32, y as f32 * CELL_H as f32, w as f32 * CELL_W as f32, CELL_H as f32);
    match theme.slice(SliceRole::TitleBar) {
        Some(slice) => renderer.draw_nine_slice_px(rect, chrome_tex, slice.src, slice.border, Color::White),
        None => renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, theme.role_color(PaletteRole::TitleBg)),
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
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
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
    renderer.draw_rect_filled(0, 0, renderer.width(), 1, ' ', Color::White, bg);
    renderer.draw_str(1, 0, "EMBER2D EDITOR", fg, bg);

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
    let col = renderer.width().saturating_sub(cells(font, &info) + 1);
    renderer.draw_str(col, 0, &info, accent, bg);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_status_bar(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    mouse: &ember2d::mouse::MouseState,
    _palette: &TilePalette,
    grid_overlay: bool,
    _save_path: &str,
    tile_under: Option<&TileRecord>,
    mode_hint: &str,
    scroll: (f32, f32),
    active_layer: u8,
    erase_size: usize,
    canvas_x: usize,
    canvas_y: usize,
    zoom: f32,
) {
    let bg = theme.role_color(PaletteRole::PanelBg);
    let fg = theme.role_color(PaletteRole::TextPrimary);
    let accent = theme.role_color(PaletteRole::Accent);
    let status_row = renderer.height() - 1;
    renderer.draw_rect_filled(0, status_row, renderer.width(), 1, ' ', Color::White, bg);
    let cx = mouse.cell_x.saturating_sub(canvas_x) as f32 / zoom + scroll.0;
    let cy = mouse.cell_y.saturating_sub(canvas_y) as f32 / zoom + scroll.1;
    let pos_str = format!(" ({:3.1},{:3.1})", cx, cy);
    renderer.draw_str(0, status_row, &pos_str, accent, bg);

    let lyr_name = match active_layer {
        0 => "Background",
        1 => "Main",
        2 => "Foreground",
        _ => "Unknown",
    };
    let lyr_str = format!("[LAYER: {}]", lyr_name);
    renderer.draw_str(10, status_row, &lyr_str, fg, bg);

    if !mode_hint.is_empty() {
        renderer.draw_str(30, status_row, &format!("| {}", mode_hint), fg, bg);
    } else if let Some(tile) = tile_under {
        let script_mark = if tile.script.is_some() { "[S]" } else { "   " };
        let props = format!(
            "| [{}] s:{} t:{} {} T=script",
            tile.tag, tile.solid as u8, tile.trigger as u8, script_mark
        );
        renderer.draw_str(30, status_row, &props, accent, bg);
    } else {
        let grid_hint = if grid_overlay { "Tab:off" } else { "Tab:grd" };
        let erase_hint = format!("E:{}px", erase_size);
        let hints = format!("| {} S:save U:undo R:redo {}", erase_hint, grid_hint);
        renderer.draw_str(30, status_row, &hints, fg, bg);
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
#[allow(clippy::too_many_arguments)]
pub fn draw_dock_tabs(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    x: usize,
    y: usize,
    w: usize,
    panels: &[(PanelId, &str)],
    active: Option<PanelId>,
    frame: &mut UiFrame,
) {
    if panels.is_empty() {
        return;
    }

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    // Background for the tab bar
    renderer.draw_rect_filled(x, y, w, 1, ' ', Color::White, panel_bg);

    let mut cursor_x = x;
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
        let label_w = cells(font, &label);
        if cursor_x + label_w > x + w {
            break;
        }

        renderer.draw_str(cursor_x, y, &label, fg, bg);
        frame.push(WidgetId::Tab(*id), UiRect::from_cells(cursor_x as i32, y as i32, label_w, 1));
        cursor_x += label_w + 1;
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_text_input(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    prompt: &str,
    buffer: &str,
    screen_w: usize,
    screen_h: usize,
) {
    let mw = 40usize;
    let mh = 7usize;
    let mx = (screen_w.saturating_sub(mw)) / 2;
    let my = (screen_h.saturating_sub(mh)) / 2;
    let panel_fg = theme.role_color(PaletteRole::TextPrimary);
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let dim = theme.role_color(PaletteRole::TextDim);

    draw_themed_frame(renderer, theme, chrome_tex, mx, my, mw, mh);
    draw_themed_title_strip(renderer, theme, chrome_tex, mx, my, mw);
    let title = format!(" {} ", prompt.to_uppercase());
    // Bounded by this modal's own fixed cell width, not by `title`'s own
    // measured length — no `measure`/`glyph` call needed here (Phase 7
    // Part 2c, docs/ember2d-phase7-plan.md: only WIDTH-FROM-STRING sites
    // are in scope, and a fixed budget like `mw` isn't one).
    let clipped: String = title.chars().take(mw.saturating_sub(2)).collect();
    renderer.draw_str(mx + 1, my, &clipped, theme.role_color(PaletteRole::TitleText), panel_bg);

    // Input field
    let input_y = my + 2;
    let label = format!("> {}█", buffer);
    let max_buf = mw.saturating_sub(6);
    let clipped_buf: String = if cells(font, &label) > max_buf {
        // Keep the TAIL of the buffer visible (so the cursor at the end
        // stays on-screen while typing), prefixed with "..". Walks
        // characters from the end accumulating `Font::glyph` advances
        // rather than byte-slicing `label` at a `.len()`-derived offset —
        // routes through the trait (Part 2c) and, as a side effect, is
        // safe on non-ASCII input the old byte slice wasn't.
        let budget = max_buf.saturating_sub(2); // reserve room for the ".." prefix
        let chars: Vec<char> = label.chars().collect();
        let mut suffix_w = 0usize;
        let mut start = chars.len();
        // 7C-2 (master plan §5.3, E2): was a literal `8.0` duplicating
        // CELL_W, the font size this file's monospace UI always renders at.
        let font_px = ember2d::renderer::CELL_W as f32;
        for (i, &ch) in chars.iter().enumerate().rev() {
            let cw = (font.glyph(ch, font_px).map(|g| g.advance).unwrap_or(font_px) / font_px).round() as usize;
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
    let input_x = mx + (mw.saturating_sub(cells(font, &clipped_buf))) / 2;
    renderer.draw_str(input_x, input_y, &clipped_buf, panel_fg, theme.role_color(PaletteRole::InputBg));

    // Helper text
    let hint = "[Enter] Confirm   [Esc] Cancel";
    let hint_x = mx + (mw.saturating_sub(cells(font, hint))) / 2;
    renderer.draw_str(hint_x, my + mh - 3, hint, dim, panel_bg);
}

pub fn draw_confirm_modal(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    chrome_tex: &Texture,
    title: &str,
    message: &str,
    screen_w: usize,
    screen_h: usize,
    frame: &mut UiFrame,
) {
    let mw = 40usize;
    let mh = 8usize;
    let mx = (screen_w.saturating_sub(mw)) / 2;
    let my = (screen_h.saturating_sub(mh)) / 2;
    let panel_bg = theme.role_color(PaletteRole::PanelBg);

    draw_themed_frame(renderer, theme, chrome_tex, mx, my, mw, mh);
    draw_themed_title_strip(renderer, theme, chrome_tex, mx, my, mw);
    renderer.draw_str(
        mx + 1,
        my,
        &format!(" {} ", title.to_uppercase()),
        theme.role_color(PaletteRole::TitleText),
        panel_bg,
    );

    // Message
    let msg_x = mx + (mw.saturating_sub(cells(font, message))) / 2;
    renderer.draw_str(msg_x, my + 2, message, theme.role_color(PaletteRole::TextPrimary), panel_bg);

    // Buttons — 7C-1 (master plan §5.3): `draw_button` registers each
    // rect at the exact point it's drawn, replacing `input/modal.rs`'s
    // own independently-recomputed `yes_x`/`no_x`/`btn_y` (E5). No
    // themed "text-on-accent" role exists (same gap `draw_dock_tabs`
    // documents above), so YES stays plain black-on-accent.
    let btn_y = my + 5;
    let yes_x = mx + 8;
    let no_x = mx + mw - 15;
    let accent = theme.role_color(PaletteRole::Accent);
    let dim = theme.role_color(PaletteRole::TextDim);

    draw_button(renderer, frame, WidgetId::ConfirmYes, yes_x, btn_y, 9, " [ YES ] ", Color::Black, accent);
    draw_button(renderer, frame, WidgetId::ConfirmNo, no_x, btn_y, 9, " [ NO ]  ", dim, panel_bg);
}

pub fn draw_context_menu(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    chrome_tex: &Texture,
    menu: &ContextMenu,
    frame: &mut UiFrame,
) {
    let mw = 20usize;
    let mh = menu.items.len() + 2;
    let mx = menu.x;
    let my = menu.y;

    // Boundary check
    let mx = if mx + mw > renderer.width() { renderer.width().saturating_sub(mw) } else { mx };
    let my = if my + mh > renderer.height() { renderer.height().saturating_sub(mh) } else { my };

    draw_themed_frame(renderer, theme, chrome_tex, mx, my, mw, mh);
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let accent = theme.role_color(PaletteRole::Accent);

    for (i, (label, _)) in menu.items.iter().enumerate() {
        let row = my + 1 + i;
        let is_selected = i == menu.selected;
        // No themed "text-on-accent" role — see `draw_dock_tabs`'s own
        // comment on this same gap.
        let (fg, bg) = if is_selected { (Color::Black, accent) } else { (text_fg, panel_bg) };

        let mut text = format!(" {:<width$} ", label, width = mw - 2);
        if text.len() > mw {
            text = text.chars().take(mw).collect();
        }
        // 7C-1 (master plan §5.3): `draw_row` registers this rect at the
        // exact point it's drawn, replacing `input/context_menu.rs`'s own
        // independently-recomputed `mw`/`mh`/boundary-clamp math (E5).
        draw_row(renderer, frame, WidgetId::ContextMenuRow(i), mx, row, mw, &text, fg, bg);
    }
}
