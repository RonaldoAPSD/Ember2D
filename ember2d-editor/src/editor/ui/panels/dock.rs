// editor/ui/panels/dock.rs — Drawing functions for the dockable content
// panels (Palette, Stats, Console, Inspector, Hierarchy, File Browser).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use super::super::frame::{InspectorField, UiFrame, WidgetId};
use super::super::rect::UiRect;
use super::super::types::*;
use super::super::widgets::{draw_row_px, draw_text_row};
use crate::editor::grid::LevelGrid;
use crate::editor::palette::TilePalette;
use ember2d::renderer::{color::Color, DrawSurface, Font, CELL_H, CELL_W};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::level::TileRecord;
use ember2d_sim::math::{Rect, Vec2};
use ember2d_sim::scripting::{LogEntry, LogLevel};
use std::collections::HashMap;

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
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    palette: &TilePalette,
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
        let text_w = font.measure(m, text_px).0;
        let text_x = content.x + ((content.w - text_w) / 2.0).max(0.0);
        renderer.fill_rect_px(header_rect, accent);
        let baseline_y = header_rect.y + font.ascent(text_px);
        renderer.draw_text_px(font, m, Vec2::new(text_x, baseline_y), text_px, Color::Black);
    }

    // 1. Search Bar — always the second row, whether or not a mode header
    // drew into the first (matching the old cell math's unconditional
    // `cy + 1`).
    let search_rect = Rect::new(content.x, content.y + row_h, content.w, row_h);
    let search_label =
        format!(" S: [{}]", if palette.search.is_empty() { "Search..." } else { &palette.search });
    let search_fg = if palette.search.is_empty() { dim } else { text_fg };
    draw_text_row(renderer, font, &search_label, search_rect, text_px, search_fg, panel_bg);
    frame.push(WidgetId::PaletteSearchBar, UiRect::new(search_rect.x, search_rect.y, search_rect.w, search_rect.h));

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
                draw_text_row(renderer, font, &label, row_rect, text_px, accent, panel_bg);
            }
            PaletteRow::Item(idx) => {
                let tile = &palette.tiles[*idx];
                let is_selected = *idx == palette.selected;
                let row_bg = if is_selected { selection } else { panel_bg };

                renderer.fill_rect_px(row_rect, row_bg);

                // Indented glyph container [ # ] — the glyph itself stays
                // on the engine's own bitmap-font pipeline (`draw_char`,
                // cell-addressed), not the theme's font: it's a literal
                // preview of how this tile glyph renders in-game, the same
                // "never themed" reasoning the 7C-9 decision gate applies
                // to the viewport itself, just for one glyph instead of
                // the whole canvas.
                let gx = row_rect.x + font.measure("  ", text_px).0;
                let baseline_y = row_rect.y + font.ascent(text_px);
                renderer.draw_text_px(
                    font,
                    "[   ]",
                    Vec2::new(gx, baseline_y),
                    text_px,
                    if is_selected { accent } else { text_fg },
                );
                let glyph_cell_x = ((gx + font.measure("[ ", text_px).0) / CELL_W as f32).round() as usize;
                let glyph_cell_y = (row_rect.y / CELL_H as f32).round() as usize;
                renderer.draw_char(glyph_cell_x, glyph_cell_y, tile.glyph, tile.fg, tile.bg);

                // Indented name
                let name_x = gx + font.measure("[   ] ", text_px).0;
                let name_rect = Rect::new(name_x, row_rect.y, (row_rect.x + row_rect.w - name_x).max(0.0), row_h);
                draw_text_row(
                    renderer,
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
                    let num_x = row_rect.x + row_rect.w - font.measure(&num, text_px).0;
                    renderer.draw_text_px(font, &num, Vec2::new(num_x, baseline_y), text_px, accent);
                }
            }
        }
        frame.push(WidgetId::PaletteRow(i), UiRect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h));
    }

    // [+ New] and [ Edit ] buttons at the bottom row
    let btn_row_rect = Rect::new(content.x, content.y + (max_rows - 1) as f32 * row_h, content.w, row_h);
    let new_label = " [ + New ] ";
    let new_w = font.measure(new_label, text_px).0;
    let new_rect = Rect::new(btn_row_rect.x, btn_row_rect.y, new_w, row_h);
    draw_row_px(renderer, frame, font, WidgetId::PaletteNewBtn, new_rect, text_px, new_label, text_fg, selection);
    let edit_label = " [ Edit ] ";
    let edit_w = font.measure(edit_label, text_px).0;
    let edit_rect = Rect::new(btn_row_rect.x + btn_row_rect.w - edit_w, btn_row_rect.y, edit_w, row_h);
    draw_row_px(renderer, frame, font, WidgetId::PaletteEditBtn, edit_rect, text_px, edit_label, text_fg, panel_bg);
}

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
    renderer: &mut dyn DrawSurface,
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
        draw_text_row(renderer, font, &label, row_rect, text_px, def.fg, panel_bg);
    }
    let total_row = Rect::new(content.x, content.y + (max_rows - 1) as f32 * row_h, content.w, row_h);
    draw_text_row(renderer, font, &format!(" Total:{:>4}", total), total_row, text_px, text_fg, panel_bg);
}

pub fn draw_console(
    renderer: &mut dyn DrawSurface,
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
    let message_x = content.x + font.measure("[ERR] ", text_px).0;
    // Rough character budget for `truncate_chars`, sized from the font's
    // own average advance — not pixel-exact (no scissor clips this panel
    // either way, same as before this conversion), just enough to avoid
    // drawing wildly more text than could ever fit.
    let avg_char_w = (font.measure("MMMMMMMMMM", text_px).0 / 10.0).max(1.0);
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
        draw_text_row(renderer, font, prefix, row_rect, text_px, pfg, panel_bg);
        let message_rect = Rect::new(message_x, row_rect.y, content.w - (message_x - content.x), row_h);
        draw_text_row(renderer, font, &text, message_rect, text_px, text_fg, panel_bg);
    }
}

/// Draws the Inspector panel and registers every editable field's hit rect
/// in `frame` at the exact point it's drawn (Phase 7 Part 1d,
/// docs/ember2d-phase7-plan.md) — see `ui/frame.rs`'s header comment for
/// why, and `WidgetId::InspectorRow`'s own doc comment for why one field
/// set covers both the Player and tile editing modes this function draws.
///
/// One structural fix falls out of this: the old independent hit-test
/// (`input/panels.rs`, before this migration) matched `mouse.cell_y` against
/// the `INSP_LAYER_OFF`/`INSP_MASK_OFF`/`INSP_GRAPH_BTN` rows unconditionally,
/// with no check that this panel was even tall enough to have drawn them —
/// this function only ever drew those rows behind an `if cy + OFFSET < cy +
/// ch` guard. A short Inspector panel could therefore have a live, clickable
/// hitbox sitting on a row that had nothing drawn on it (or that scrolled
/// content from something else entirely occupied). Since a hit is now only
/// ever pushed from inside the exact same guard that drew it, that's no
/// longer possible.
pub fn draw_inspector(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    tile: Option<&TileRecord>,
    pos: Option<(i32, i32)>,
    mode_tag: &str,
    ix: usize,
    cy: usize,
    iw: usize,
    ch: usize,
    frame: &mut UiFrame,
) {
    use InspectorField::*;
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let input_bg = theme.role_color(PaletteRole::InputBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);

    renderer.draw_rect_filled(ix, cy, iw, ch, ' ', Color::White, panel_bg);
    let mode_line = format!(" {:<width$}", mode_tag, width = iw.saturating_sub(1));
    renderer.draw_str(ix, cy, &mode_line, Color::Black, accent);

    let sep: String =
        std::iter::once(' ').chain(std::iter::repeat_n('-', iw.saturating_sub(1))).collect();

    let Some(tile) = tile else {
        let hint = if pos.is_some() { "(empty cell)" } else { "hover a tile" };
        renderer.draw_str(ix + 1, cy + 2, hint, dim, panel_bg);
        return;
    };

    // ── Tile Edit Mode ───────────────────────────────────────────────────
    if let Some((gx, gy)) = pos {
        renderer.draw_str(ix, cy + 1, &format!(" ({},{})", gx, gy), accent, panel_bg);
    }
    let glyph_str = format!("  '{}' {:<width$}", tile.glyph, "glyph", width = iw.saturating_sub(6));
    renderer.draw_str(ix, cy + INSP_GLYPH_OFF, &glyph_str, tile.fg, input_bg);
    frame.push(
        WidgetId::InspectorRow(Glyph),
        UiRect::from_cells(ix as i32, (cy + INSP_GLYPH_OFF) as i32, iw, 1),
    );

    renderer.draw_str(ix, cy + 4, &sep, dim, panel_bg);
    renderer.draw_str(ix, cy + 5, " Tag:", dim, panel_bg);
    let tag_disp = if tile.tag.is_empty() { "(none)" } else { &tile.tag };
    let tag_line = format!("  {:<width$}", tag_disp, width = iw.saturating_sub(3));
    renderer.draw_str(ix, cy + INSP_TAG_OFF + 1, &tag_line, text_fg, input_bg);
    // The click target is the "Tag:" LABEL's own row (`INSP_TAG_OFF`), one
    // row above where `tag_line`'s value actually renders
    // (`INSP_TAG_OFF + 1`) — that's exactly where the pre-migration
    // `tag_row` hitbox already was, preserved as-is here rather than
    // "fixed" to also cover the value row, since that would be a UX change
    // this phase wasn't asked to make, not a draw/hit-test drift fix.
    frame.push(
        WidgetId::InspectorRow(Tag),
        UiRect::from_cells(ix as i32, (cy + INSP_TAG_OFF) as i32, iw, 1),
    );

    renderer.draw_str(ix, cy + 7, &sep, dim, panel_bg);
    renderer.draw_str(
        ix,
        cy + INSP_SOLID_OFF,
        &format!(" [{}] Solid", if tile.solid { 'x' } else { ' ' }),
        text_fg,
        input_bg,
    );
    frame.push(
        WidgetId::InspectorRow(Solid),
        UiRect::from_cells(ix as i32, (cy + INSP_SOLID_OFF) as i32, iw, 1),
    );
    renderer.draw_str(
        ix,
        cy + INSP_TRIG_OFF,
        &format!(" [{}] Trigger", if tile.trigger { 'x' } else { ' ' }),
        text_fg,
        input_bg,
    );
    frame.push(
        WidgetId::InspectorRow(Trigger),
        UiRect::from_cells(ix as i32, (cy + INSP_TRIG_OFF) as i32, iw, 1),
    );
    renderer.draw_str(
        ix,
        cy + INSP_CAM_OFF,
        &format!(" [{}] Camera follow", if tile.camera_follow { 'x' } else { ' ' }),
        text_fg,
        input_bg,
    );
    frame.push(
        WidgetId::InspectorRow(CameraFollow),
        UiRect::from_cells(ix as i32, (cy + INSP_CAM_OFF) as i32, iw, 1),
    );

    renderer.draw_str(ix, cy + 12, &sep, dim, panel_bg);
    renderer.draw_str(ix, cy + 13, " Script:", dim, panel_bg);
    let (script_disp, script_fg) = match &tile.script {
        Some(path) => {
            let short = path
                .rfind('/')
                .or_else(|| path.rfind('\\'))
                .map(|i| &path[i + 1..])
                .unwrap_or(path.as_str());
            (format!("  {:<width$}", short, width = iw.saturating_sub(3)), text_fg)
        }
        None => (format!("  {:<width$}", "(none)", width = iw.saturating_sub(3)), dim),
    };
    renderer.draw_str(ix, cy + INSP_SCRIPT_OFF, &script_disp, script_fg, input_bg);
    frame.push(
        WidgetId::InspectorRow(Script),
        UiRect::from_cells(ix as i32, (cy + INSP_SCRIPT_OFF) as i32, iw, 1),
    );
    let (exit_disp, exit_fg) = match &tile.next_level {
        Some(p) => (format!("  >{:<width$}", p, width = iw.saturating_sub(4)), accent),
        None => (format!("  {:<width$}", "(no exit)", width = iw.saturating_sub(3)), dim),
    };
    renderer.draw_str(ix, cy + INSP_EXIT_OFF, &exit_disp, exit_fg, input_bg);
    frame.push(
        WidgetId::InspectorRow(Exit),
        UiRect::from_cells(ix as i32, (cy + INSP_EXIT_OFF) as i32, iw, 1),
    );

    renderer.draw_str(ix, cy + 16, &sep, dim, panel_bg);
    if cy + 17 < cy + ch {
        renderer.draw_str(ix, cy + 17, " Scripting:", dim, panel_bg);
    }
    if cy + INSP_GRAPH_BTN < cy + ch {
        if tile.graph.is_some() {
            let n = tile.graph.as_ref().map(|g| g.nodes.len()).unwrap_or(0);
            let e = tile.graph.as_ref().map(|g| g.edges.len()).unwrap_or(0);
            let btn = "  [Edit Graph]".to_string();
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, accent);
            if cy + 19 < cy + ch {
                let info = format!("  {} nodes  {} edges", n, e);
                let info: String = info.chars().take(iw).collect();
                renderer.draw_str(ix, cy + 19, &info, dim, panel_bg);
            }
        } else {
            // A distinct "create" action, not decorative chrome — stays
            // literal green rather than a theme role, same reasoning as
            // `draw_console`'s own untouched log-level colors.
            let btn = "  [New Graph]".to_string();
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::DarkGreen);
        }
        frame.push(
            WidgetId::InspectorRow(GraphBtn),
            UiRect::from_cells(ix as i32, (cy + INSP_GRAPH_BTN) as i32, iw, 1),
        );
    }
    if cy + 20 < cy + ch {
        renderer.draw_str(ix, cy + 20, &sep, dim, panel_bg);
    }
    if cy + INSP_LAYER_OFF < cy + ch {
        renderer.draw_str(ix, cy + INSP_LAYER_OFF, " Layer:", dim, panel_bg);
        let layer_disp =
            if tile.collider_layer.is_empty() { "(any)" } else { &tile.collider_layer };
        let layer_line = format!("  {:<width$}", layer_disp, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + INSP_LAYER_OFF, &layer_line, accent, input_bg);
        frame.push(
            WidgetId::InspectorRow(Layer),
            UiRect::from_cells(ix as i32, (cy + INSP_LAYER_OFF) as i32, iw, 1),
        );
    }
    if cy + INSP_MASK_OFF < cy + ch {
        renderer.draw_str(ix, cy + INSP_MASK_OFF, " Mask:", dim, panel_bg);
        let mask_str = if tile.collider_mask.is_empty() {
            "(all layers)".to_string()
        } else {
            tile.collider_mask.join(",")
        };
        let mask_line = format!("  {:<width$}", mask_str, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + INSP_MASK_OFF, &mask_line, accent, input_bg);
        frame.push(
            WidgetId::InspectorRow(Mask),
            UiRect::from_cells(ix as i32, (cy + INSP_MASK_OFF) as i32, iw, 1),
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_hierarchy(
    renderer: &mut dyn DrawSurface,
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
    renderer.fill_rect_px(content, panel_bg);
    let sep_row = Rect::new(content.x, content.y, content.w, row_h);
    let sep = "-".repeat((content.w / font.measure("-", text_px).0.max(1.0)) as usize);
    draw_text_row(renderer, font, &sep, sep_row, text_px, dim, panel_bg);
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
            renderer,
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
    let avg_char_w = (font.measure("MMMMMMMMMM", text_px).0 / 10.0).max(1.0);
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
            renderer,
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

#[allow(clippy::too_many_arguments)]
pub fn draw_file_browser_panel(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    files: &[String],
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

    renderer.fill_rect_px(content, panel_bg);

    // Breadcrumbs / Current Path — no themed "text-on-accent" role exists
    // (same gap `chrome.rs`'s `draw_dock_tabs` comments on), so this
    // header stays plain black-on-accent.
    let path_label = format!(" Content > {}", current_folder.replace("./", "").replace("/", " > "));
    let header_rect = Rect::new(content.x, content.y, content.w, row_h);
    draw_text_row(renderer, font, &path_label, header_rect, text_px, Color::Black, accent);

    let list_top = content.y + row_h;
    let max_visible = ((content.h - row_h) / row_h).floor().max(0.0) as usize;

    if files.is_empty() {
        let empty_rect = Rect::new(content.x, list_top, content.w, row_h);
        draw_text_row(renderer, font, " (empty folder)", empty_rect, text_px, dim, panel_bg);
        return;
    }

    // Fixed pixel columns for the icon tag and the name that follows it —
    // every icon tag is the same 5-character width ("[DIR]"/"[LVL]"/
    // "[SCR]"/"[---]"), and Cascadia Mono is a real monospace face, so
    // measuring any one of them gives a stable name-column start (matches
    // the old cell math's fixed `cx + 7` exactly, derived from the real
    // font instead of assuming fixed 8px cells).
    let name_x = content.x + font.measure("[DIR] ", text_px).0;
    for (i, raw_line) in files.iter().enumerate().skip(scroll).take(max_visible) {
        let row_index = i - scroll + 1;
        let row_rect = Rect::new(content.x, content.y + row_index as f32 * row_h, content.w, row_h);

        let is_selected = i == cursor;
        let bg = if is_selected { selection } else { panel_bg };
        renderer.fill_rect_px(row_rect, bg);
        // 7C-1 (master plan §5.3): registers this row's rect at the exact
        // point it's drawn, replacing `handle_file_browser_click`'s own
        // independently-recomputed `row_idx` arithmetic (E5). Pushed once
        // here (rather than through `draw_row_px`) since a file browser
        // row draws more than one text run — an icon tag, then the name —
        // depending on which of the four content branches below runs.
        frame.push(WidgetId::FileBrowserRow(i), UiRect::new(row_rect.x, row_rect.y, row_rect.w, row_rect.h));

        if raw_line.contains("[UP]") {
            let baseline_y = row_rect.y + font.ascent(text_px);
            renderer.draw_text_px(font, " .. [PARENT FOLDER] ", Vec2::new(row_rect.x, baseline_y), text_px, accent);
            continue;
        }

        // File-kind colors (dir/level/script) are semantic icon tags, not
        // chrome — left literal, same reasoning as `draw_console`'s
        // log-level colors and `draw_hierarchy`'s entity-kind colors.
        let (icon, fg, skip) = if raw_line.starts_with("/ ") {
            ("DIR", Color::Yellow, 2)
        } else if raw_line.starts_with("[] ") {
            ("LVL", Color::Cyan, 3)
        } else if raw_line.starts_with("{} ") {
            ("SCR", Color::Green, 3)
        } else {
            ("---", dim, 3)
        };

        let name = if raw_line.len() > skip { &raw_line[skip..] } else { raw_line };
        let icon_tag = format!("[{}]", icon);
        let baseline_y = row_rect.y + font.ascent(text_px);
        renderer.draw_text_px(font, &icon_tag, Vec2::new(row_rect.x, baseline_y), text_px, fg);
        renderer.draw_text_px(font, name, Vec2::new(name_x, baseline_y), text_px, text_fg);
    }
}

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
