// editor/ui/panels/palette_panel.rs — the Palette panel: its search bar,
// category headers, one row per tile definition (glyph preview or, since
// Step 8-2, sprite thumbnail), and the [+ New] / [ Edit ] buttons.
//
// Moved out of dock.rs at Step 8-2 (docs/ember2d-master-plan.md §5.7) —
// dock.rs was at 744 of CLAUDE.md's 750 lines and 8-2 adds the sprite
// thumbnail to exactly this function. Re-exported from `panels/mod.rs` like
// every other panel, so `ui::draw_palette_panel` is unchanged.

use super::super::frame::{UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::widgets::{draw_row_px, draw_text_row, draw_tile_preview_in};
use crate::editor::palette::TilePalette;
use crate::editor::sprites::SpriteAssets;
use ember2d::renderer::{color::Color, Font, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::{Rect, Vec2};

/// Draws the palette panel and registers every interactive row/button's hit
/// rect in `frame` at the exact point it's drawn (Phase 7 Part 1d,
/// docs/ember2d-phase7-plan.md) — see `ui/frame.rs`'s header comment for
/// why. Fixes two real, if minor, pre-existing draw/hit-test mismatches
/// along the way: the old independent hit-test for "[+ New]" covered only
/// 10 cells against an 11-cell-wide drawn button (`" [ + New ] "`), and the
/// old "[ Edit ]" hit-test had no upper bound at all (anything past its
/// left edge counted, out to the edge of the panel) against its actual
/// 10-cell drawn width — both now register exactly what's drawn, nothing more
/// or less.
#[allow(clippy::too_many_arguments)]
pub fn draw_palette_panel(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    palette: &TilePalette,
    sprites: &SpriteAssets,
    anim_time: f32,
    mode: Option<&str>,
    scroll: usize,
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
    let max_rows = ((content.h / row_h).floor() as usize).max(1);

    if let Some(m) = mode {
        // Centered through real measurement now, not a `{:^width$}`
        // character-count center (Phase 7 Part 2c's own "1 char = 1 cell"
        // assumption, no longer true once this draws through the theme's
        // real font).
        let header_rect = Rect::new(content.x, content.y, content.w, row_h);
        let text_w = painter.measure(font, m, text_px);
        let text_x = content.x + ((content.w - text_w) / 2.0).max(0.0);
        painter.fill(header_rect, accent);
        let baseline_y = header_rect.y + painter.ascent(font, text_px);
        painter.text(font, m, Vec2::new(text_x, baseline_y), text_px, Color::Black);
    }

    // 1. Search Bar — always the second row, whether or not a mode header
    // drew into the first (matching the old cell math's unconditional
    // `cy + 1`).
    let search_rect = Rect::new(content.x, content.y + row_h, content.w, row_h);
    let search_label =
        format!(" S: [{}]", if palette.search.is_empty() { "Search..." } else { &palette.search });
    let search_fg = if palette.search.is_empty() { dim } else { text_fg };
    draw_text_row(painter, font, &search_label, search_rect, text_px, search_fg, panel_bg);
    frame.push(
        WidgetId::PaletteSearchBar,
        UiRect::new(search_rect.x, search_rect.y, search_rect.w, search_rect.h),
    );

    use crate::editor::palette::PaletteRow;
    let layout = palette.build_layout();
    let visible_rows = max_rows.saturating_sub(3); // Room for header, search, and buttons

    for (i, row_item) in layout.iter().enumerate().skip(scroll).take(visible_rows) {
        let row_index = 2 + (i - scroll);
        if row_index + 1 >= max_rows {
            break;
        }
        let row_rect = Rect::new(content.x, content.y + row_index as f32 * row_h, content.w, row_h);

        match row_item {
            PaletteRow::Header(name) => {
                let is_collapsed = palette.collapsed.contains(name);
                let icon = if is_collapsed { "[+]" } else { "[-]" };
                let label = format!("{} {}", icon, name);
                draw_text_row(painter, font, &label, row_rect, text_px, accent, panel_bg);
            }
            PaletteRow::Item(idx) => {
                let tile = &palette.tiles[*idx];
                let is_selected = *idx == palette.selected;
                let row_bg = if is_selected { selection } else { panel_bg };

                painter.fill(row_rect, row_bg);

                // Indented glyph container [ # ] — the glyph itself stays
                // on the engine's own bitmap-font pipeline, not the theme's
                // font: it's a literal preview of how this tile glyph
                // renders in-game, the same "never themed" reasoning the
                // 7C-9 decision gate applies to the viewport itself, just
                // for one glyph instead of the whole canvas. R95: placed in
                // points (`draw_tile_glyph_in`), centered between the
                // brackets — it used to be cell-addressed, which put it in
                // the wrong cell at any UI scale but one.
                let gx = row_rect.x + painter.measure(font, "  ", text_px);
                let baseline_y = row_rect.y + painter.ascent(font, text_px);
                painter.text(
                    font,
                    "[   ]",
                    Vec2::new(gx, baseline_y),
                    text_px,
                    if is_selected { accent } else { text_fg },
                );
                let slot_x = gx + painter.measure(font, "[", text_px);
                let slot_w = painter.measure(font, "   ", text_px);
                let slot = Rect::new(slot_x, row_rect.y, slot_w, row_h);
                // Step 8-2: a sprite thumbnail when the entry has one.
                draw_tile_preview_in(painter, slot, tile, sprites, anim_time);

                // Indented name
                let name_x = gx + painter.measure(font, "[   ] ", text_px);
                let name_rect = Rect::new(
                    name_x,
                    row_rect.y,
                    (row_rect.x + row_rect.w - name_x).max(0.0),
                    row_h,
                );
                draw_text_row(
                    painter,
                    font,
                    &tile.name,
                    name_rect,
                    text_px,
                    if is_selected { text_fg } else { dim },
                    row_bg,
                );

                // Shortcut mapping (4-9, 0)
                let shortcut = match *idx {
                    0..=5 => Some(format!("{}", *idx + 4)),
                    6 => Some("0".to_string()),
                    _ => None,
                };
                if let Some(num) = shortcut {
                    // R95: one space of padding from the row's right edge —
                    // flush right, the panel border clipped the digit.
                    let num_x = row_rect.x + row_rect.w
                        - painter.measure(font, &num, text_px)
                        - painter.measure(font, " ", text_px);
                    painter.text(font, &num, Vec2::new(num_x, baseline_y), text_px, accent);
                }
            }
        }
        frame.push(
            WidgetId::PaletteRow(i),
            UiRect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h),
        );
    }

    // [+ New] and [ Edit ] buttons at the bottom row
    let btn_row_rect =
        Rect::new(content.x, content.y + (max_rows - 1) as f32 * row_h, content.w, row_h);
    let new_label = " [ + New ] ";
    let new_w = painter.measure(font, new_label, text_px);
    let new_rect = Rect::new(btn_row_rect.x, btn_row_rect.y, new_w, row_h);
    draw_row_px(
        painter,
        frame,
        font,
        WidgetId::PaletteNewBtn,
        new_rect,
        text_px,
        new_label,
        text_fg,
        selection,
    );
    let edit_label = " [ Edit ] ";
    let edit_w = painter.measure(font, edit_label, text_px);
    let edit_rect =
        Rect::new(btn_row_rect.x + btn_row_rect.w - edit_w, btn_row_rect.y, edit_w, row_h);
    draw_row_px(
        painter,
        frame,
        font,
        WidgetId::PaletteEditBtn,
        edit_rect,
        text_px,
        edit_label,
        text_fg,
        panel_bg,
    );
}
