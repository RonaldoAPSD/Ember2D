// editor/ui/panels/dock.rs — Drawing functions for the dockable content
// panels (Palette, Stats, Console, Inspector, Hierarchy, File Browser).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use super::super::frame::{UiFrame, WidgetId};
use super::super::types::*;
use super::super::widgets::{draw_row_px, draw_text_row};
use crate::editor::grid::LevelGrid;
use crate::editor::palette::TilePalette;
use ember2d::renderer::{color::Color, Font, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use std::collections::HashMap;

/// The first panel converted off the character-cell grid entirely
/// (docs/ember2d-master-plan.md §5.4, the `UiRect::from_cells` removal) —
/// picked as the proof-of-pattern slice because it has no interactive
/// hit-rects to re-derive, the smallest possible surface for a first real
/// conversion. `content` is `Panel::content_rect()` (already pixel-native,
/// unlike the `content_x/y/w/h` cell-int bridge methods every OTHER
/// caller in this file still uses) — rows are `theme.metrics.row_h`
/// pixels tall (20px in `ember-clean`, not the old fixed 16px `CELL_H`),
/// and text draws through the theme's own font at `font_sizes.body` via
/// `draw_text_row`, not the cell-quantized `draw_str`. This is also the
/// first place the theme's real font (Cascadia Mono TTF) actually renders
/// anywhere but `draw_panel_chrome`'s title bar — every other panel still
/// renders through `draw_str`, which ignores the theme's font entirely
/// (see the master plan's own note on why that's not a quick fix).
pub fn draw_stats_panel(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    grid: &LevelGrid,
    palette: &TilePalette,
    content: Rect,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, tile) in grid.iter() {
        *counts.entry(tile.tag.clone()).or_insert(0) += 1;
    }
    let total = grid.tiles.len();
    // At least 1, so a panel too short for even one real row still gets
    // its "Total" line rather than dividing by zero below.
    let max_rows = ((content.h / row_h).floor() as usize).max(1);
    for (i, def) in palette.tiles.iter().enumerate() {
        // Leave the last row for "Total", same reservation the old
        // cell-based `row >= cy + ch - 1` check made.
        if i + 1 >= max_rows {
            break;
        }
        let row_rect = Rect::new(content.x, content.y + i as f32 * row_h, content.w, row_h);
        let count = counts.get(&def.tag).copied().unwrap_or(0);
        let label = format!(" {}: {:>4}", def.name, count);
        draw_text_row(painter, font, &label, row_rect, text_px, def.fg, panel_bg);
    }
    let total_row =
        Rect::new(content.x, content.y + (max_rows - 1) as f32 * row_h, content.w, row_h);
    draw_text_row(
        painter,
        font,
        &format!(" Total:{:>4}", total),
        total_row,
        text_px,
        text_fg,
        panel_bg,
    );
}

pub fn draw_console(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    log: &[LogEntry],
    content: Rect,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let visible = ((content.h / row_h).floor() as usize).max(1);
    let start = log.len().saturating_sub(visible);
    // All three prefixes below are the same 6 characters wide
    // ("[ERR] "/"[WRN] "/"[OK]  ") and Cascadia Mono is a real monospace
    // face, so measuring any one of them gives the message column's fixed
    // pixel start — matches the old cell math's `cx + 6` exactly, just
    // derived from the real font instead of assuming 6 fixed 8px cells.
    let message_x = content.x + painter.measure(font, "[ERR] ", text_px);
    // Rough character budget for `truncate_chars`, sized from the font's
    // own average advance — not pixel-exact (no scissor clips this panel
    // either way, same as before this conversion), just enough to avoid
    // drawing wildly more text than could ever fit.
    let avg_char_w = (painter.measure(font, "MMMMMMMMMM", text_px) / 10.0).max(1.0);
    let max_text = ((content.w - (message_x - content.x)) / avg_char_w).floor().max(0.0) as usize;
    for (i, entry) in log.iter().skip(start).enumerate() {
        let row_rect = Rect::new(content.x, content.y + i as f32 * row_h, content.w, row_h);
        // Log-level colors are semantic status signals (error/warning/ok),
        // not decorative chrome — left as literal Red/Yellow/DarkGreen
        // rather than theme roles, same as `draw_status_bar`'s own
        // untouched danger-adjacent hints elsewhere.
        let (prefix, pfg) = match entry.level {
            LogLevel::Error => ("[ERR]", Color::Red),
            LogLevel::Warning => ("[WRN]", Color::Yellow),
            LogLevel::Info => ("[OK] ", Color::DarkGreen),
        };
        let text = truncate_chars(&entry.text, max_text);
        draw_text_row(painter, font, prefix, row_rect, text_px, pfg, panel_bg);
        let message_rect =
            Rect::new(message_x, row_rect.y, content.w - (message_x - content.x), row_h);
        draw_text_row(painter, font, &text, message_rect, text_px, text_fg, panel_bg);
    }
}

// `draw_inspector` moved to `inspector.rs` at Step 9-6 (docs/ember2d-
// master-plan.md §5.8), rebuilt as a scrolling row list — see that file.

#[allow(clippy::too_many_arguments)]
pub fn draw_hierarchy(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    grid: &LevelGrid,
    hier_sel: Option<HierarchySelection>,
    content: Rect,
    frame: &mut UiFrame,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let row_h = theme.metrics.row_h;
    let text_px = theme.font_sizes.body;
    let max_rows = ((content.h / row_h).floor() as usize).max(1);
    painter.fill(content, panel_bg);
    let sep_row = Rect::new(content.x, content.y, content.w, row_h);
    let sep = "-".repeat((content.w / painter.measure(font, "-", text_px).max(1.0)) as usize);
    draw_text_row(painter, font, &sep, sep_row, text_px, dim, panel_bg);
    // 7C-1 (master plan §5.3): `draw_row_px` registers each row at the
    // exact point it's drawn, replacing `handle_hierarchy_click`'s own
    // independently-recomputed `hier_row` arithmetic (E5). Entity-kind
    // colors (player green, spawn yellow) are semantic, not chrome — left
    // literal, same reasoning as `draw_console`'s log-level colors; only
    // the SELECTED state and background follow the theme. Labels no
    // longer manually pad to the panel's full width (the old cell-based
    // `" ".repeat(hw - 9)`) — `draw_row_px`'s own `fill_rect_px` already
    // covers the whole row rect regardless of the label's length.
    if max_rows > 1 {
        let player_sel = hier_sel == Some(HierarchySelection::Player);
        let label = format!(" {} Player", grid.player.glyph);
        let (fg, bg) = if player_sel { (Color::Black, accent) } else { (Color::Green, panel_bg) };
        let row_rect = Rect::new(content.x, content.y + row_h, content.w, row_h);
        draw_row_px(
            painter,
            frame,
            font,
            WidgetId::HierarchyRow(HierarchySelection::Player),
            row_rect,
            text_px,
            &label,
            fg,
            bg,
        );
    }
    // Rough character budget for a spawn name, sized from the font's own
    // average advance — not pixel-exact (no scissor clips this panel
    // either way), just enough to stop an extreme name from drawing far
    // past the panel's right edge, same reasoning `draw_console` uses.
    let avg_char_w = (painter.measure(font, "MMMMMMMMMM", text_px) / 10.0).max(1.0);
    let max_name_chars = ((content.w / avg_char_w) as usize).saturating_sub(3);
    for (i, (name, _, _)) in grid.extra_spawns.iter().enumerate() {
        let row_index = 2 + i;
        if row_index >= max_rows {
            break;
        }
        let spawn_sel = hier_sel == Some(HierarchySelection::Spawn(i));
        let short: String = name.chars().take(max_name_chars).collect();
        let label = format!(" ! {}", short);
        let (fg, bg) = if spawn_sel { (Color::Black, accent) } else { (Color::Yellow, panel_bg) };
        let row_rect = Rect::new(content.x, content.y + row_index as f32 * row_h, content.w, row_h);
        draw_row_px(
            painter,
            frame,
            font,
            WidgetId::HierarchyRow(HierarchySelection::Spawn(i)),
            row_rect,
            text_px,
            &label,
            fg,
            bg,
        );
    }
}

// `draw_file_browser_panel` moved to `file_browser.rs` at Step 8-4 (it
// grew asset thumbnails and a preview pane — see that file's header).

/// R11 (7A-2, docs/ember2d-master-plan.md): `draw_console`'s own truncation
/// used to byte-slice (`&entry.text[..max_text]`), which panicked whenever
/// the cut point landed inside a multi-byte character (an em dash, an
/// accented letter — anything a script's `ctx.log()` can produce).
/// `chars()` always cuts on character boundaries. Pulled out to its own
/// function so the fix has a direct unit test — `draw_console` itself needs
/// a real `Renderer` to call at all.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncating_a_line_with_a_wide_character_at_the_cut_point_does_not_panic() {
        let text = "log: caf\u{e9} temperature \u{2014} rising"; // café ... — rising
        let truncated = truncate_chars(text, 10);
        assert_eq!(truncated.chars().count(), 10);
        assert_eq!(truncated, "log: caf\u{e9} ");
    }
}
