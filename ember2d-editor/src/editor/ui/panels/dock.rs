// editor/ui/panels/dock.rs — Drawing functions for the dockable content
// panels (Palette, Stats, Console, Inspector, Hierarchy, File Browser).
//
// Split out of the single `ui/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to stay under CLAUDE.md's 600-line
// hard limit — no behavioral change from being grouped this way. See
// `../panels/mod.rs` for the split's overall shape (`chrome`/`dock`/`modals`).

use std::collections::HashMap;
use ember2d::renderer::{color::Color, Font, Renderer};
use crate::editor::grid::LevelGrid;
use crate::editor::palette::TilePalette;
use ember2d_sim::level::TileRecord;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use super::super::types::*;
use super::super::rect::UiRect;
use super::super::frame::{UiFrame, WidgetId, InspectorField};

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
pub fn draw_palette_panel(renderer: &mut Renderer, font: &mut dyn Font, palette: &TilePalette, mode: Option<&str>, scroll: usize, cx: usize, cy: usize, cw: usize, ch: usize, frame: &mut UiFrame) {
    if let Some(m) = mode {
        // Rust's own `{:^width$}` centers by character count, same
        // "1 char = 1 cell" assumption as a hand-rolled `.len()` centering
        // formula would make (Phase 7 Part 2c, docs/ember2d-phase7-plan.md)
        // — replicated by hand here so the padding comes from `Font::measure`
        // instead. Byte-for-byte identical output under `BitmapFont`: same
        // left/right split Rust's own formatter uses (extra padding cell,
        // if any, goes on the right).
        let text_w = cells(font, m);
        let total_pad = cw.saturating_sub(text_w);
        let left_pad = total_pad / 2;
        let right_pad = total_pad - left_pad;
        let header = format!(" {}{}{} ", " ".repeat(left_pad), m, " ".repeat(right_pad));
        renderer.draw_str(cx, cy, &header, Color::Black, Color::Cyan);
    }

    // 1. Search Bar
    let search_row = cy + 1;
    let search_label = format!(" S: [{:<width$}]", if palette.search.is_empty() { "Search..." } else { &palette.search }, width = cw.saturating_sub(6));
    renderer.draw_str(cx, search_row, &search_label, if palette.search.is_empty() { Color::DarkGrey } else { Color::White }, Color::Black);
    frame.push(WidgetId::PaletteSearchBar, UiRect::from_cells(cx as i32, search_row as i32, cw, 1));

    use crate::editor::palette::PaletteRow;
    let layout = palette.build_layout();
    let visible_rows = ch.saturating_sub(3); // Room for header, search, and buttons
    let list_start = cy + 2;

    for (i, row_item) in layout.iter().enumerate().skip(scroll).take(visible_rows) {
        let row = list_start + (i - scroll);
        if row >= cy + ch - 1 { break; }

        match row_item {
            PaletteRow::Header(name) => {
                let is_collapsed = palette.collapsed.contains(name);
                let icon = if is_collapsed { "[+]" } else { "[-]" };
                let label = format!("{} {}", icon, name);
                let clipped: String = format!("{:<width$}", label, width = cw).chars().take(cw).collect();
                renderer.draw_str(cx, row, &clipped, Color::Yellow, Color::DarkGrey);
            }
            PaletteRow::Item(idx) => {
                let tile = &palette.tiles[*idx];
                let is_selected = *idx == palette.selected;
                let row_bg = if is_selected { Color::DarkBlue } else { Color::DarkGrey };

                // Row background
                renderer.draw_rect_filled(cx, row, cw, 1, ' ', Color::White, row_bg);

                // Indented glyph container [ # ]
                let gx = cx + 2;
                renderer.draw_str(gx, row, "[   ]", if is_selected { Color::Cyan } else { Color::White }, row_bg);
                renderer.draw_char(gx + 2, row, tile.glyph, tile.fg, tile.bg);

                // Indented name
                let name_col = gx + 5;
                let max_name_len = cw.saturating_sub(gx - cx + 7);
                let name_disp: String = tile.name.chars().take(max_name_len).collect();
                renderer.draw_str(name_col, row, &name_disp, if is_selected { Color::White } else { Color::Grey }, row_bg);

                // Shortcut mapping (4-9, 0)
                let shortcut = match *idx { 0..=5 => Some(format!("{}", *idx + 4)), 6 => Some("0".to_string()), _ => None };
                if let Some(num) = shortcut {
                    renderer.draw_str(cx + cw - 1, row, &num, Color::Yellow, row_bg);
                }
            }
        }
        frame.push(WidgetId::PaletteRow(i), UiRect::from_cells(cx as i32, row as i32, cw, 1));
    }

    // [+ New] and [ Edit ] buttons at the bottom row
    let btn_row = cy + ch - 1;
    renderer.draw_str(cx, btn_row, " [ + New ] ", Color::White, Color::DarkCyan);
    frame.push(WidgetId::PaletteNewBtn, UiRect::from_cells(cx as i32, btn_row as i32, 11, 1));
    renderer.draw_str(cx + cw - 10, btn_row, " [ Edit ] ", Color::White, Color::DarkBlue);
    frame.push(WidgetId::PaletteEditBtn, UiRect::from_cells((cx + cw - 10) as i32, btn_row as i32, 10, 1));
}

pub fn draw_stats_panel(renderer: &mut Renderer, grid: &LevelGrid, palette: &TilePalette, cx: usize, cy: usize, cw: usize, ch: usize) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for (_, tile) in grid.iter() { *counts.entry(tile.tag.clone()).or_insert(0) += 1; }
    let total = grid.tiles.len();
    for (i, def) in palette.tiles.iter().enumerate() {
        let row = cy + i;
        if row >= cy + ch - 1 { break; }
        let count = counts.get(&def.tag).copied().unwrap_or(0);
        let label = format!(" {}: {:>4}", def.name, count);
        let clipped: String = label.chars().take(cw).collect();
        renderer.draw_str(cx, row, &clipped, def.fg, Color::DarkGrey);
    }
    renderer.draw_str(cx, cy + ch - 1, &format!(" Total:{:>4}", total), Color::White, Color::DarkGrey);
}

pub fn draw_console(renderer: &mut Renderer, log: &[LogEntry], cx: usize, cy: usize, cw: usize, ch: usize) {
    let visible = ch;
    let start   = log.len().saturating_sub(visible);
    for (i, entry) in log.iter().skip(start).enumerate() {
        let row = cy + i;
        if row >= cy + ch { break; }
        let (prefix, pfg, tbg) = match entry.level {
            LogLevel::Error   => ("[ERR]", Color::Red,       Color::Black),
            LogLevel::Warning => ("[WRN]", Color::Yellow,    Color::Black),
            LogLevel::Info    => ("[OK] ", Color::DarkGreen, Color::Black),
        };
        let max_text = cw.saturating_sub(7);
        let text = truncate_chars(&entry.text, max_text);
        renderer.draw_str(cx,     row, prefix, pfg,         tbg);
        renderer.draw_str(cx + 6, row, &text,  Color::White, tbg);
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
pub fn draw_inspector(renderer: &mut Renderer, tile: Option<&TileRecord>, pos: Option<(i32, i32)>, mode_tag: &str, ix: usize, cy: usize, iw: usize, ch: usize, frame: &mut UiFrame) {
    use InspectorField::*;
    renderer.draw_rect_filled(ix, cy, iw, ch, ' ', Color::White, Color::DarkGrey);
    let mode_line = format!(" {:<width$}", mode_tag, width = iw.saturating_sub(1));
    renderer.draw_str(ix, cy, &mode_line, Color::Black, Color::Cyan);

    let sep: String = std::iter::once(' ').chain(std::iter::repeat('-').take(iw.saturating_sub(1))).collect();

    let Some(tile) = tile else {
        let hint = if pos.is_some() { "(empty cell)" } else { "hover a tile" };
        renderer.draw_str(ix + 1, cy + 2, hint, Color::DarkGrey, Color::DarkGrey);
        return;
    };

    // ── Tile Edit Mode ───────────────────────────────────────────────────
    if let Some((gx, gy)) = pos { renderer.draw_str(ix, cy + 1, &format!(" ({},{})", gx, gy), Color::Cyan, Color::DarkGrey); }
    let glyph_str = format!("  '{}' {:<width$}", tile.glyph, "glyph", width = iw.saturating_sub(6));
    renderer.draw_str(ix, cy + INSP_GLYPH_OFF, &glyph_str, tile.fg, Color::DarkBlue);
    frame.push(WidgetId::InspectorRow(Glyph), UiRect::from_cells(ix as i32, (cy + INSP_GLYPH_OFF) as i32, iw, 1));

    renderer.draw_str(ix, cy + 4, &sep, Color::DarkGrey, Color::DarkGrey);
    renderer.draw_str(ix, cy + 5, " Tag:", Color::DarkGrey, Color::DarkGrey);
    let tag_disp = if tile.tag.is_empty() { "(none)" } else { &tile.tag };
    let tag_line = format!("  {:<width$}", tag_disp, width = iw.saturating_sub(3));
    renderer.draw_str(ix, cy + INSP_TAG_OFF + 1, &tag_line, Color::White, Color::DarkBlue);
    // The click target is the "Tag:" LABEL's own row (`INSP_TAG_OFF`), one
    // row above where `tag_line`'s value actually renders
    // (`INSP_TAG_OFF + 1`) — that's exactly where the pre-migration
    // `tag_row` hitbox already was, preserved as-is here rather than
    // "fixed" to also cover the value row, since that would be a UX change
    // this phase wasn't asked to make, not a draw/hit-test drift fix.
    frame.push(WidgetId::InspectorRow(Tag), UiRect::from_cells(ix as i32, (cy + INSP_TAG_OFF) as i32, iw, 1));

    renderer.draw_str(ix, cy + 7, &sep, Color::DarkGrey, Color::DarkGrey);
    renderer.draw_str(ix, cy + INSP_SOLID_OFF, &format!(" [{}] Solid", if tile.solid { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    frame.push(WidgetId::InspectorRow(Solid), UiRect::from_cells(ix as i32, (cy + INSP_SOLID_OFF) as i32, iw, 1));
    renderer.draw_str(ix, cy + INSP_TRIG_OFF, &format!(" [{}] Trigger", if tile.trigger { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    frame.push(WidgetId::InspectorRow(Trigger), UiRect::from_cells(ix as i32, (cy + INSP_TRIG_OFF) as i32, iw, 1));
    renderer.draw_str(ix, cy + INSP_CAM_OFF, &format!(" [{}] Camera follow", if tile.camera_follow { 'x' } else { ' ' }), Color::White, Color::DarkBlue);
    frame.push(WidgetId::InspectorRow(CameraFollow), UiRect::from_cells(ix as i32, (cy + INSP_CAM_OFF) as i32, iw, 1));

    renderer.draw_str(ix, cy + 12, &sep, Color::DarkGrey, Color::DarkGrey);
    renderer.draw_str(ix, cy + 13, " Script:", Color::DarkGrey, Color::DarkGrey);
    let (script_disp, script_fg) = match &tile.script {
        Some(path) => {
            let short = path.rfind('/').or_else(|| path.rfind('\\')).map(|i| &path[i+1..]).unwrap_or(path.as_str());
            (format!("  {:<width$}", short, width = iw.saturating_sub(3)), Color::White)
        }
        None => (format!("  {:<width$}", "(none)", width = iw.saturating_sub(3)), Color::DarkGrey),
    };
    renderer.draw_str(ix, cy + INSP_SCRIPT_OFF, &script_disp, script_fg, Color::DarkBlue);
    frame.push(WidgetId::InspectorRow(Script), UiRect::from_cells(ix as i32, (cy + INSP_SCRIPT_OFF) as i32, iw, 1));
    let (exit_disp, exit_fg) = match &tile.next_level {
        Some(p) => (format!("  >{:<width$}", p, width = iw.saturating_sub(4)), Color::Cyan),
        None    => (format!("  {:<width$}", "(no exit)", width = iw.saturating_sub(3)), Color::DarkGrey),
    };
    renderer.draw_str(ix, cy + INSP_EXIT_OFF, &exit_disp, exit_fg, Color::DarkBlue);
    frame.push(WidgetId::InspectorRow(Exit), UiRect::from_cells(ix as i32, (cy + INSP_EXIT_OFF) as i32, iw, 1));

    renderer.draw_str(ix, cy + 16, &sep, Color::DarkGrey, Color::DarkGrey);
    if cy + 17 < cy + ch { renderer.draw_str(ix, cy + 17, " Scripting:", Color::DarkGrey, Color::DarkGrey); }
    if cy + INSP_GRAPH_BTN < cy + ch {
        if tile.graph.is_some() {
            let n = tile.graph.as_ref().map(|g| g.nodes.len()).unwrap_or(0);
            let e = tile.graph.as_ref().map(|g| g.edges.len()).unwrap_or(0);
            let btn = format!("  [Edit Graph]");
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::Cyan);
            if cy + 19 < cy + ch {
                let info = format!("  {} nodes  {} edges", n, e);
                let info: String = info.chars().take(iw).collect();
                renderer.draw_str(ix, cy + 19, &info, Color::DarkGrey, Color::DarkGrey);
            }
        } else {
            let btn = format!("  [New Graph]");
            let btn: String = format!("{:<width$}", btn, width = iw).chars().take(iw).collect();
            renderer.draw_str(ix, cy + INSP_GRAPH_BTN, &btn, Color::Black, Color::DarkGreen);
        }
        frame.push(WidgetId::InspectorRow(GraphBtn), UiRect::from_cells(ix as i32, (cy + INSP_GRAPH_BTN) as i32, iw, 1));
    }
    if cy + 20 < cy + ch { renderer.draw_str(ix, cy + 20, &sep, Color::DarkGrey, Color::DarkGrey); }
    if cy + INSP_LAYER_OFF < cy + ch {
        renderer.draw_str(ix, cy + INSP_LAYER_OFF, " Layer:", Color::DarkGrey, Color::DarkGrey);
        let layer_disp = if tile.collider_layer.is_empty() { "(any)" } else { &tile.collider_layer };
        let layer_line = format!("  {:<width$}", layer_disp, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + INSP_LAYER_OFF, &layer_line, Color::Cyan, Color::DarkBlue);
        frame.push(WidgetId::InspectorRow(Layer), UiRect::from_cells(ix as i32, (cy + INSP_LAYER_OFF) as i32, iw, 1));
    }
    if cy + INSP_MASK_OFF < cy + ch {
        renderer.draw_str(ix, cy + INSP_MASK_OFF, " Mask:", Color::DarkGrey, Color::DarkGrey);
        let mask_str = if tile.collider_mask.is_empty() { "(all layers)".to_string() } else { tile.collider_mask.join(",") };
        let mask_line = format!("  {:<width$}", mask_str, width = iw.saturating_sub(3));
        renderer.draw_str(ix, cy + INSP_MASK_OFF, &mask_line, Color::Cyan, Color::DarkBlue);
        frame.push(WidgetId::InspectorRow(Mask), UiRect::from_cells(ix as i32, (cy + INSP_MASK_OFF) as i32, iw, 1));
    }
}

pub fn draw_hierarchy(renderer: &mut Renderer, grid: &LevelGrid, hier_sel: Option<HierarchySelection>, hx: usize, hy: usize, hw: usize, hh: usize) {
    renderer.draw_rect_filled(hx, hy, hw, hh, ' ', Color::White, Color::DarkGrey);
    let sep: String = std::iter::repeat('-').take(hw).collect();
    renderer.draw_str(hx, hy, &sep, Color::DarkGrey, Color::DarkGrey);
    if hh > 1 {
        let player_sel = hier_sel == Some(HierarchySelection::Player);
        let label = format!(" {} Player{}", grid.player.glyph, " ".repeat(hw.saturating_sub(9)));
        if player_sel { renderer.draw_str(hx, hy + 1, &label, Color::Black, Color::Cyan); }
        else { renderer.draw_str(hx, hy + 1, &label, Color::Green, Color::DarkGrey); }
    }
    for (i, (name, _, _)) in grid.extra_spawns.iter().enumerate() {
        let row = hy + 2 + i;
        if row >= hy + hh { break; }
        let spawn_sel = hier_sel == Some(HierarchySelection::Spawn(i));
        let max_name = hw.saturating_sub(3);
        let short: String = name.chars().take(max_name).collect();
        let label = format!(" ! {:<width$}", short, width = max_name);
        if spawn_sel { renderer.draw_str(hx, row, &label, Color::Black, Color::Cyan); }
        else { renderer.draw_str(hx, row, &label, Color::Yellow, Color::DarkGrey); }
    }
}

pub fn draw_file_browser_panel(renderer: &mut Renderer, files: &[String], cursor: usize, scroll: usize, current_folder: &str, cx: usize, cy: usize, cw: usize, ch: usize) {
    renderer.draw_rect_filled(cx, cy, cw, ch, ' ', Color::White, Color::DarkGrey);

    // Breadcrumbs / Current Path
    let path_label = format!(" Content > {}", current_folder.replace("./", "").replace("/", " > "));
    let header = format!(" {:<width$}", path_label, width = cw.saturating_sub(1));
    renderer.draw_str(cx, cy, &header, Color::Black, Color::Cyan);

    let list_start = cy + 1;
    let max_visible = ch.saturating_sub(1);

    if files.is_empty() {
        renderer.draw_str(cx + 1, list_start, "(empty folder)", Color::Grey, Color::DarkGrey);
        return;
    }

    for (i, raw_line) in files.iter().enumerate().skip(scroll).take(max_visible) {
        let row = list_start + (i - scroll);
        if row >= cy + ch { break; }

        let is_selected = i == cursor;
        let bg = if is_selected { Color::DarkBlue } else { Color::DarkGrey };

        renderer.draw_rect_filled(cx, row, cw, 1, ' ', Color::White, bg);

        if raw_line.contains("[UP]") {
            renderer.draw_str(cx + 1, row, " .. [PARENT FOLDER] ", Color::Yellow, bg);
            continue;
        }

        let (icon, fg, skip) = if raw_line.starts_with("/ ") {
            ("DIR", Color::Yellow, 2)
        } else if raw_line.starts_with("[] ") {
            ("LVL", Color::Cyan, 3)
        } else if raw_line.starts_with("{} ") {
            ("SCR", Color::Green, 3)
        } else {
            ("---", Color::Grey, 3)
        };

        let name = if raw_line.len() > skip { &raw_line[skip..] } else { raw_line };
        let icon_tag = format!("[{}]", icon);
        renderer.draw_str(cx + 1, row, &icon_tag, fg, bg);

        let name_x = cx + 7;
        let max_name_w = cw.saturating_sub(8);
        let clipped_name: String = name.chars().take(max_name_w).collect();
        renderer.draw_str(name_x, row, &clipped_name, if is_selected { Color::White } else { Color::White }, bg);
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
