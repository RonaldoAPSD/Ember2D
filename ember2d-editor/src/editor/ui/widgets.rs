// editor/ui/widgets.rs — Shared draw-and-register widget primitives
// (master plan §5.3, step 7C-1).
//
// `UiFrame::push`'s whole point (see `frame.rs`'s own header comment) is
// that a widget's hit rect is registered at the exact point it's drawn.
// Before this step, that discipline only covered panel chrome, tabs, the
// palette panel, and Inspector rows — eight other sites (confirm modal,
// advanced color picker, palette editor color grids, context menu, graph
// palette, hierarchy rows, file browser rows, and StartScreen) each still
// hit-tested by recomputing the same layout math a second time in their
// input handler, the exact drift risk defect E5 names. These four
// functions are the shared shape every one of those sites turned out to
// need: draw one thing, and push its rect in the same call, so the two
// can never again go out of sync.
//
// `draw_row`/`draw_menu_item` cover the widgets built from more than one
// `draw_str` call (a hierarchy row is one call; a StartScreen menu item is
// two, label plus description); sites whose visual shape is bespoke enough
// that none of these fit (the file browser's icon+name row, StartScreen's
// folder/project browsers, the template cards) still push their own rect
// directly, exactly as `ui/panels/dock.rs`'s Inspector rows already did
// before this step — see that file's own `draw_inspector` for the
// precedent. `draw_button`/`draw_swatch`, this pair's own former CELL_W/
// CELL_H-addressed siblings, were deleted in 7D-3's chrome audit
// (docs/ember2d-master-plan.md §5.4, checkpoint 6) once their own last
// caller converted to `draw_button_px`/`draw_swatch_px` — `draw_row` stays
// (still `graph_ui.rs`'s own cell-grid, per the 7C-9 decision gate), as
// does `draw_menu_item` (`start_screen/`, deliberately excluded).

use super::frame::{UiFrame, WidgetId};
use super::rect::UiRect;
use ember2d::renderer::{color::Color, DrawSurface, Font, CELL_H, CELL_W};
use ember2d_sim::math::{Rect, Vec2};

/// One row of pixel-positioned text: a background fill sized to `rect`,
/// then `text` baseline-positioned inside it at `px` through `font` — the
/// pixel-space replacement for `renderer.draw_str(cell_x, cell_y, text,
/// fg, bg)` (docs/ember2d-master-plan.md §5.4, the `UiRect::from_cells`
/// removal). `rect.y` is the row's TOP, matching `UiRect`/
/// `Panel::content_rect`'s own convention — the baseline is computed from
/// `font.ascent(px)`, the same math `Renderer::draw_str`'s own `Ttf`
/// branch already uses for its "cell-snapped baseline," just without the
/// cell-snap rounding, since `rect.y` is already a real pixel position,
/// not a cell index waiting to be multiplied out. Returns the text's own
/// measured advance, so a caller can position whatever comes next on the
/// same row (mirrors `DrawSurface::draw_text_px`'s own return value).
pub fn draw_text_row(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    text: &str,
    rect: Rect,
    px: f32,
    fg: Color,
    bg: Color,
) -> f32 {
    renderer.fill_rect_px(rect, bg);
    let baseline_y = rect.y + font.ascent(px);
    renderer.draw_text_px(font, text, Vec2::new(rect.x, baseline_y), px, fg)
}

/// Same shape as `draw_row_px`, kept as its own named function for the
/// same reason `draw_row`/`draw_swatch_px` are (this file's own header
/// comment): each documents a different KIND of call site (a modal
/// action) even though the bodies are identical.
#[allow(clippy::too_many_arguments)]
pub fn draw_button_px(
    renderer: &mut dyn DrawSurface,
    frame: &mut UiFrame,
    font: &mut dyn Font,
    id: WidgetId,
    rect: Rect,
    px: f32,
    label: &str,
    fg: Color,
    bg: Color,
) {
    draw_text_row(renderer, font, label, rect, px, fg, bg);
    frame.push(id, UiRect::new(rect.x, rect.y, rect.w, rect.h));
}

/// Same reasoning as `draw_button_px`'s own doc comment for why this
/// duplicates `draw_row_px`'s body under its own name.
#[allow(clippy::too_many_arguments)]
pub fn draw_swatch_px(
    renderer: &mut dyn DrawSurface,
    frame: &mut UiFrame,
    font: &mut dyn Font,
    id: WidgetId,
    rect: Rect,
    px: f32,
    label: &str,
    fg: Color,
    bg: Color,
) {
    draw_text_row(renderer, font, label, rect, px, fg, bg);
    frame.push(id, UiRect::new(rect.x, rect.y, rect.w, rect.h));
}

/// The pixel-space twin of `draw_row` below — a full-row list entry
/// (hierarchy rows, file browser rows) drawn through `draw_text_row` at a
/// real pixel rect, with its hit rect pushed at that EXACT same rect
/// (`UiRect::new`, not re-derived from a cell count via `from_cells`) —
/// same "draw and push together" discipline 7C-1 established, just in
/// pixel space (docs/ember2d-master-plan.md §5.4, the `UiRect::from_cells`
/// removal).
#[allow(clippy::too_many_arguments)]
pub fn draw_row_px(
    renderer: &mut dyn DrawSurface,
    frame: &mut UiFrame,
    font: &mut dyn Font,
    id: WidgetId,
    rect: Rect,
    px: f32,
    label: &str,
    fg: Color,
    bg: Color,
) {
    draw_text_row(renderer, font, label, rect, px, fg, bg);
    frame.push(id, UiRect::new(rect.x, rect.y, rect.w, rect.h));
}

/// A single-line, full-row list entry — context menu items, hierarchy
/// rows, and graph-palette rows are all this shape.
#[allow(clippy::too_many_arguments)]
pub fn draw_row(
    renderer: &mut dyn DrawSurface,
    frame: &mut UiFrame,
    id: WidgetId,
    x: usize,
    y: usize,
    w: usize,
    label: &str,
    fg: Color,
    bg: Color,
) {
    renderer.draw_str(x, y, label, fg, bg);
    frame.push(
        id,
        UiRect::new(x as f32 * CELL_W as f32, y as f32 * CELL_H as f32, w as f32 * CELL_W as f32, CELL_H as f32),
    );
}

/// One entry of `StartScreen`'s main menu — a two-row item (a label row,
/// then an indented description row) whose highlight state changes which
/// row gets a filled background. The click target always spans both rows
/// (`h = 2`), so clicking the description line selects the item too,
/// matching the removed `menu_item_hit`'s own `hit_test(.., 2)` height.
/// Drawing logic copied verbatim from the removed `draw_main_menu` loop
/// body — this function IS that loop body, extracted so its rect can be
/// pushed at the exact point it's drawn.
#[allow(clippy::too_many_arguments)]
pub fn draw_menu_item(
    renderer: &mut dyn DrawSurface,
    frame: &mut UiFrame,
    id: WidgetId,
    x: usize,
    row: usize,
    w: usize,
    index: usize,
    label: &str,
    desc: &str,
    selected: bool,
) {
    if selected {
        // 44 is copied verbatim from the removed `draw_main_menu` loop body
        // — not derived from `w` (56 there today); preserved exactly so
        // this extraction changes no pixel.
        let line = format!("  >  {}. {:<44}", index + 1, label);
        renderer.draw_str(x, row, &line, Color::Black, Color::Cyan);
        renderer.draw_str(
            x,
            row + 1,
            &format!("{:width$}", "", width = w),
            Color::Black,
            Color::DarkBlue,
        );
        renderer.draw_str(x + 9, row + 1, desc, Color::Cyan, Color::DarkBlue);
    } else {
        renderer.draw_str(x, row, &format!("     {}. {}", index + 1, label), Color::White, Color::Black);
        renderer.draw_str(x + 9, row + 1, desc, Color::DarkGrey, Color::Black);
    }
    frame.push(
        id,
        UiRect::new(x as f32 * CELL_W as f32, row as f32 * CELL_H as f32, w as f32 * CELL_W as f32, 2.0 * CELL_H as f32),
    );
}

/// The 16-color palette shared by the palette editor's foreground and
/// background grids, and by the (unused but still compiled) standalone
/// `draw_color_picker` swatch strip — deduplicates four independent copies
/// of this exact array (two draw-side, two click-side; E5, master plan
/// §5.3 7C-1's own Change list calls this out by name).
pub const PALETTE_COLORS: [Color; 16] = [
    Color::Black,
    Color::White,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Cyan,
    Color::Magenta,
    Color::DarkGrey,
    Color::Grey,
    Color::DarkRed,
    Color::DarkGreen,
    Color::DarkBlue,
    Color::DarkYellow,
    Color::DarkCyan,
    Color::DarkMagenta,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_colors_has_no_duplicate_entries() {
        // A silent copy/paste duplicate here would make two grid cells
        // select the same color and leave one palette color unreachable —
        // cheap to pin since the four sites this dedupes used to drift
        // independently.
        for (i, a) in PALETTE_COLORS.iter().enumerate() {
            for (j, b) in PALETTE_COLORS.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "duplicate color at indices {i} and {j}");
                }
            }
        }
    }
}
