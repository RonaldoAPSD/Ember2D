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
// `draw_button`/`draw_row`/`draw_swatch` are intentionally near-identical
// (one `draw_str` call, then one `push`) — kept as three separate,
// semantically-named functions rather than one generic helper because
// each documents a different kind of call site (a modal action, a list
// entry, a color cell), matching this step's own Change list. `draw_row`
// and `draw_menu_item` cover the widgets built from more than one
// `draw_str` call (a hierarchy row is one call; a StartScreen menu item is
// two, label plus description); sites whose visual shape is bespoke enough
// that none of the four fit (the file browser's icon+name row, StartScreen's
// folder/project browsers, the template cards) still push their own rect
// directly, exactly as `ui/panels/dock.rs`'s Inspector rows already did
// before this step — see that file's own `draw_inspector` for the
// precedent.

use super::frame::{UiFrame, WidgetId};
use super::rect::UiRect;
use ember2d::renderer::{color::Color, Renderer};

/// A single clickable line of text — every modal "button" (confirm
/// Yes/No, the advanced color picker's Apply/Cancel/title-close) is
/// exactly this shape: one `draw_str` call whose own rect IS the click
/// target.
#[allow(clippy::too_many_arguments)]
pub fn draw_button(
    renderer: &mut Renderer,
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
    frame.push(id, UiRect::from_cells(x as i32, y as i32, w, 1));
}

/// A single-line, full-row list entry — context menu items, hierarchy
/// rows, and graph-palette rows are all this shape.
#[allow(clippy::too_many_arguments)]
pub fn draw_row(
    renderer: &mut Renderer,
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
    frame.push(id, UiRect::from_cells(x as i32, y as i32, w, 1));
}

/// One color swatch cell of a color grid (the palette editor's foreground
/// and background tables) — `label` is the pre-formatted `"[#]"`/`"[*]"`
/// glyph, `w` its cell width (3, to match the grid's own spacing).
#[allow(clippy::too_many_arguments)]
pub fn draw_swatch(
    renderer: &mut Renderer,
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
    frame.push(id, UiRect::from_cells(x as i32, y as i32, w, 1));
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
    renderer: &mut Renderer,
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
    frame.push(id, UiRect::from_cells(x as i32, row as i32, w, 2));
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
