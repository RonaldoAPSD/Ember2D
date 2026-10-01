// play/ui_draw.rs — drawing the script menus and dialogue box that are open
// in the simulation's `UiModel`.
//
// Step 9-3 (docs/ember2d-master-plan.md §5.8): the first script-facing UI
// drawn on the PIXEL path with a real TTF font (the bundled Cascadia Mono,
// `renderer::font::bundled_ui_font`) rather than the cell-grid bitmap glyphs
// `draw_hud`/`draw_menu` use. Layout is still in cells (a menu's `x`/`y`/
// `width` are cell counts, matching every other HUD call) and converted to
// logical pixels here; the text size is chosen so the monospace font fits
// one character per cell — which is what lets the simulation page dialogue
// by character count (`ember2d_sim::ui::wrap_text`) without measuring a
// font it can't see.
//
// Everything open is drawn above the level's and the scenes' HUD: menus in
// the order they were opened (the newest — the one the keyboard moves — on
// top), then the dialogue box.

use ember2d_sim::math::{Rect, Vec2};
use ember2d_sim::ui::{Dialogue, Menu, UiModel, DIALOGUE_LINES};

use crate::renderer::color::Color;
use crate::renderer::{DrawSurface, Font, CELL_H, CELL_W};

/// Text size in logical pixels: Cascadia Mono advances 0.6 em, so 13 px is
/// 7.8 px per character — inside one 8 px cell.
pub(super) const UI_TEXT_PX: f32 = 13.0;

const PANEL_BG: Color = Color::Rgb(24, 28, 35);
const BORDER: Color = Color::Rgb(232, 163, 61);
const TEXT: Color = Color::Rgb(230, 233, 237);
const DIM: Color = Color::Rgb(154, 164, 175);
const SEL_BG: Color = Color::Rgb(232, 163, 61);
const SEL_FG: Color = Color::Rgb(24, 28, 35);

/// A widget's box, in cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CellRect {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

/// Where `menu` sits on a `vw`×`vh`-cell screen: its own `x`/`y`/`width`
/// when given, else centred and sized to its longest line — always kept on
/// screen, and never underflowing on a screen smaller than the menu (the
/// R86 rule the old Rust pause menu needed).
pub(super) fn menu_rect(menu: &Menu, vw: usize, vh: usize) -> CellRect {
    let longest =
        menu.items.iter().map(|i| i.chars().count() + 2).chain([menu.title.chars().count()]).max();
    let w = menu.width.map(|w| w.max(1) as usize).unwrap_or(longest.unwrap_or(0) + 4);
    let h = menu.items.len() + 2 + if menu.title.is_empty() { 0 } else { 2 };
    let place = |pos: Option<i64>, size: usize, screen: usize| -> usize {
        let max = screen.saturating_sub(size);
        match pos {
            Some(p) => (p.max(0) as usize).min(max),
            None => max / 2,
        }
    };
    CellRect { x: place(menu.x, w, vw), y: place(menu.y, h, vh), w, h }
}

/// The dialogue box: full width less one cell each side, at the bottom.
pub(super) fn dialogue_rect(d: &Dialogue, vw: usize, vh: usize) -> CellRect {
    let h = DIALOGUE_LINES + 2 + usize::from(!d.speaker.is_empty());
    let w = vw.saturating_sub(2).max(1);
    CellRect { x: 1.min(vw.saturating_sub(w)), y: vh.saturating_sub(h + 1), w, h }
}

fn px(r: CellRect) -> Rect {
    Rect::new(
        (r.x * CELL_W) as f32,
        (r.y * CELL_H) as f32,
        (r.w * CELL_W) as f32,
        (r.h * CELL_H) as f32,
    )
}

fn frame(surface: &mut dyn DrawSurface, r: Rect) {
    surface.fill_rect_px(r, PANEL_BG);
    let t = 1.0;
    surface.fill_rect_px(Rect::new(r.x, r.y, r.w, t), BORDER);
    surface.fill_rect_px(Rect::new(r.x, r.y + r.h - t, r.w, t), BORDER);
    surface.fill_rect_px(Rect::new(r.x, r.y, t, r.h), BORDER);
    surface.fill_rect_px(Rect::new(r.x + r.w - t, r.y, t, r.h), BORDER);
}

/// Text on cell row `row`, starting at cell column `col`.
fn text(
    surface: &mut dyn DrawSurface,
    font: &mut dyn Font,
    s: &str,
    col: usize,
    row: usize,
    c: Color,
) {
    let baseline = (row * CELL_H) as f32 + CELL_H as f32 * 0.75;
    surface.draw_text_px(font, s, Vec2::new((col * CELL_W) as f32, baseline), UI_TEXT_PX, c);
}

fn draw_menu(
    surface: &mut dyn DrawSurface,
    font: &mut dyn Font,
    m: &Menu,
    active: bool,
    vw: usize,
    vh: usize,
) {
    let r = menu_rect(m, vw, vh);
    frame(surface, px(r));
    let mut row = r.y + 1;
    if !m.title.is_empty() {
        text(surface, font, &m.title, r.x + 2, row, BORDER);
        row += 2;
    }
    for (i, item) in m.items.iter().enumerate() {
        let selected = i == m.selected;
        if selected {
            let band = CellRect { x: r.x + 1, y: row, w: r.w.saturating_sub(2), h: 1 };
            surface.fill_rect_px(px(band), if active { SEL_BG } else { DIM });
        }
        let label = format!("{}{}", if selected { "> " } else { "  " }, item);
        text(surface, font, &label, r.x + 1, row, if selected { SEL_FG } else { TEXT });
        row += 1;
    }
}

fn draw_dialogue(
    surface: &mut dyn DrawSurface,
    font: &mut dyn Font,
    d: &Dialogue,
    vw: usize,
    vh: usize,
) {
    let r = dialogue_rect(d, vw, vh);
    frame(surface, px(r));
    let mut row = r.y + 1;
    if !d.speaker.is_empty() {
        text(surface, font, &d.speaker, r.x + 2, row, BORDER);
        row += 1;
    }
    if let Some(page) = d.pages.get(d.page) {
        for line in page {
            text(surface, font, line, r.x + 2, row, TEXT);
            row += 1;
        }
    }
    // More to read, or the last page.
    let marker = if d.page + 1 < d.pages.len() { "v" } else { "*" };
    text(surface, font, marker, r.x + r.w.saturating_sub(3), r.y + r.h - 2, DIM);
}

/// Draws every open menu (oldest first) and the dialogue box.
pub(super) fn draw_ui(
    surface: &mut dyn DrawSurface,
    font: &mut dyn Font,
    ui: &UiModel,
    vw: usize,
    vh: usize,
) {
    let active = ui.active_menu().map(|(id, _)| id);
    let dialogue = ui.open_dialogue();
    for (id, m) in ui.open_menus() {
        draw_menu(surface, font, m, dialogue.is_none() && Some(id) == active, vw, vh);
    }
    if let Some(d) = dialogue {
        draw_dialogue(surface, font, d, vw, vh);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember2d_sim::ui::MenuState;

    fn menu(items: &[&str], title: &str) -> Menu {
        Menu {
            items: items.iter().map(|s| s.to_string()).collect(),
            selected: 0,
            state: MenuState::Open,
            title: title.into(),
            x: None,
            y: None,
            width: None,
            cancelable: true,
            owner: 0,
        }
    }

    #[test]
    fn a_menu_is_centred_and_sized_to_its_longest_line() {
        let r = menu_rect(&menu(&["Resume", "Quit Game"], "PAUSED"), 80, 24);
        assert_eq!((r.w, r.h), (9 + 2 + 4, 2 + 2 + 2));
        assert_eq!((r.x, r.y), ((80 - r.w) / 2, (24 - r.h) / 2));
    }

    #[test]
    fn r86_a_menu_never_underflows_on_a_screen_smaller_than_itself() {
        let r = menu_rect(&menu(&["a very long menu row indeed"], "T"), 10, 3);
        assert_eq!((r.x, r.y), (0, 0));
    }

    #[test]
    fn a_placed_menu_is_kept_on_screen() {
        let mut m = menu(&["a"], "");
        m.x = Some(78);
        m.y = Some(-4);
        let r = menu_rect(&m, 80, 24);
        assert_eq!(r.x, 80 - r.w);
        assert_eq!(r.y, 0);
    }

    #[test]
    fn the_dialogue_box_spans_the_bottom_and_its_text_fits() {
        let d = Dialogue {
            id: 0,
            speaker: "Old man".into(),
            text: String::new(),
            pages: vec![vec![]],
            page: 0,
            done: false,
            owner: 0,
        };
        let r = dialogue_rect(&d, 80, 24);
        assert_eq!((r.x, r.w), (1, 78));
        assert_eq!(r.y + r.h, 23, "one row above the bottom edge");
        // Text starts two cells in; the sim wraps at width - 4.
        assert!(r.x + 2 + (80 - ember2d_sim::scripting::DIALOGUE_MARGIN_COLS) <= r.x + r.w + 1);
        assert!(UI_TEXT_PX * 0.6 <= CELL_W as f32, "one character per cell");
    }

    #[test]
    fn draw_ui_draws_the_open_menu_and_dialogue_text_with_the_bundled_font() {
        use crate::renderer::draw_log::DrawOp;
        use crate::renderer::NullRenderer;
        use ember2d_sim::ui::UiOp;

        let mut font = crate::renderer::font::bundled_ui_font().expect("bundled font parses");
        let mut ui = UiModel::default();
        let dialogue = Dialogue {
            id: 1,
            speaker: "Old man".into(),
            text: String::new(),
            pages: vec![vec!["Hello there.".into()]],
            page: 0,
            done: false,
            owner: 0,
        };
        ui.apply(vec![
            UiOp::OpenMenu(0, menu(&["Attack", "Run"], "Battle")),
            UiOp::OpenDialogue(dialogue),
        ]);
        let mut r = NullRenderer::new(80 * CELL_W, 24 * CELL_H);
        r.start_recording();
        draw_ui(&mut r, &mut font, &ui, 80, 24);
        let texts: Vec<String> = r
            .ops()
            .iter()
            .filter_map(|op| match op {
                DrawOp::Text { text, raster_px, .. } => {
                    assert_eq!(*raster_px, UI_TEXT_PX);
                    Some(text.clone())
                }
                _ => None,
            })
            .collect();
        for want in ["Battle", "> Attack", "  Run", "Old man", "Hello there."] {
            assert!(texts.iter().any(|t| t == want), "{want:?} not drawn in {texts:?}");
        }
        assert!(r.ops().iter().any(|op| matches!(op, DrawOp::Fill(_))), "panels are filled");
    }
}
