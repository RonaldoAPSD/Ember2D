// ember2d-editor/tests/editor_palette.rs — R95 (docs/ember2d-master-plan.md
// §3.2): the palette panel's tile glyph previews, and the advanced color
// picker's hue bar / saturation-value map, used to be painted on the CELL
// grid from a layout computed in UI POINTS (a points value divided by
// `CELL_W`/`CELL_H`). That only lines up at the one UI scale where a point
// happens to equal a logical pixel; at any other (the user's live 1.5x), the
// previews landed in the wrong cells — several palette rows' glyphs stacked
// over the panel's `[ Edit ]` button, and the color picker's swatches spilled
// out of its dialog onto the Inspector. Each test here drives `ui_scale` away
// from `render_scale` and checks the recorded draws land where the widgets
// they belong to were registered.

mod common;

use common::{ensure_workspace_root_cwd, EditorHarness};
use ember2d::input::Key;
use ember2d::renderer::draw_log::DrawOp;
use ember2d::renderer::DisplayScale;
use ember2d_editor::editor::prefs::{EditorPrefs, PrefsStore, UiScaleChoice};
use ember2d_editor::editor::ui::WidgetId;
use ember2d_editor::editor::EditorState;
use ember2d_sim::math::Rect;

/// Same shape as `editor_ui_scale.rs`'s own helper: `ui_scale` pinned via an
/// in-memory preference so the exact points:logical ratio is the one under
/// test.
fn harness_at(render_scale: u32, ui_scale: UiScaleChoice) -> EditorHarness {
    ensure_workspace_root_cwd();
    let prefs = EditorPrefs { ui_scale, theme: "ember-clean".to_string() };
    let state = EditorState::new("harness.level").with_prefs(PrefsStore::InMemory(prefs));
    EditorHarness::with_state_and_display(
        state,
        DisplayScale { render_scale, os_scale_factor: 1.0 },
    )
}

/// Every scale the Theme menu offers, against the harness's realistic
/// `render_scale: 2` — 1.5x is the one the live bug was seen at.
fn scales() -> Vec<UiScaleChoice> {
    vec![
        UiScaleChoice::Fixed(1),
        UiScaleChoice::OnePointFive,
        UiScaleChoice::Fixed(2),
        UiScaleChoice::Fixed(3),
        UiScaleChoice::Fixed(4),
    ]
}

fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
}

fn open_palette(h: &mut EditorHarness) {
    h.frame();
    h.key(Key::B);
    h.frame();
}

#[test]
fn r95_every_palette_rows_glyph_preview_is_drawn_inside_that_row_at_every_ui_scale() {
    for scale in scales() {
        let mut h = harness_at(2, scale);
        open_palette(&mut h);
        h.start_recording();
        h.frame();

        let space = h.state.ui_space();
        let glyph_centers: Vec<(f32, f32)> = h
            .draw_ops()
            .iter()
            .filter_map(|op| match op {
                // `draw_char_px` at `scale` draws a CELL_W x CELL_H glyph
                // scaled — its center is half that size in from `pos`.
                DrawOp::Char { pos, scale, .. } => Some((
                    pos.x + ember2d::renderer::CELL_W as f32 * scale * 0.5,
                    pos.y + ember2d::renderer::CELL_H as f32 * scale * 0.5,
                )),
                _ => None,
            })
            .collect();

        let mut rows_with_preview = 0;
        for i in 0..h.state.palette_layout_len() {
            let Some(r) = h.state.ui_frame().rect_of(WidgetId::PaletteRow(i)) else { continue };
            let logical = space.rect_to_logical(Rect::new(r.x, r.y, r.w, r.h));
            let inside = glyph_centers.iter().filter(|&&(x, y)| contains(logical, x, y)).count();
            assert!(inside <= 1, "{scale:?}: row {i} has {inside} glyph previews drawn over it");
            rows_with_preview += inside;
        }
        assert_eq!(
            rows_with_preview,
            h.state.palette_tile_count(),
            "{scale:?}: every palette tile's glyph preview must be drawn inside its own row"
        );
    }
}

#[test]
fn r95_the_color_pickers_hue_bar_and_map_are_painted_where_they_are_hit_tested_at_every_ui_scale() {
    for scale in scales() {
        let mut h = harness_at(2, scale);
        open_palette(&mut h);
        let space = h.state.ui_space();
        let click = |h: &mut EditorHarness, id: WidgetId| {
            let r = h.state.ui_frame().rect_of(id).unwrap_or_else(|| panic!("{id:?} not drawn"));
            let (x, y) = h.state.ui_space().to_logical(r.x + r.w * 0.5, r.y + r.h * 0.5);
            h.click(x, y);
            h.frame();
        };
        click(&mut h, WidgetId::PaletteEditBtn);
        click(&mut h, WidgetId::PaletteEditorCustomColor { is_fg: true });
        h.start_recording();
        h.frame();

        let fills: Vec<Rect> = h
            .draw_ops()
            .iter()
            .filter_map(|op| match op {
                DrawOp::Fill(r) => Some(*r),
                _ => None,
            })
            .collect();
        for id in [WidgetId::ColorPickerHueBar, WidgetId::ColorPickerSvMap] {
            let r = h.state.ui_frame().rect_of(id).unwrap_or_else(|| panic!("{scale:?}: {id:?}"));
            let area = space.rect_to_logical(Rect::new(r.x, r.y, r.w, r.h));
            // Its swatches are fills that tile the registered rect exactly:
            // one at each corner. Before the fix they were cell-addressed
            // `draw_char`s nowhere near it (and never recorded as fills).
            for (cx, cy) in
                [(area.x + 1.0, area.y + 1.0), (area.x + area.w - 1.0, area.y + area.h - 1.0)]
            {
                assert!(
                    fills.iter().any(|f| contains(*f, cx, cy) && f.w <= area.w * 0.5),
                    "{scale:?}: {id:?}'s swatches must be painted inside its own hit rect"
                );
            }
            // ...and the whole widget stays inside the dialog, whose close
            // button sits at its top-right corner.
            let close = h.state.ui_frame().rect_of(WidgetId::ColorPickerClose).unwrap();
            assert!(
                r.x + r.w <= close.x + close.w + 0.5,
                "{scale:?}: {id:?} must not run past the dialog's right edge"
            );
        }
    }
}
