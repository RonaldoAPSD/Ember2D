// ember2d-editor/tests/editor_ui_scale.rs — 7D-3 checkpoint 7 (docs/
// ember2d-master-plan.md §5.4): regression tests for the LIVE UI scale —
// `effective_ui_scale` un-pinned from `render_scale`, the `Theme > UI Scale`
// menu, and the points<->logical conversion this checkpoint wired into every
// chrome draw AND input call site. Earlier checkpoints' tests (`editor_theme
// .rs`, `editor_input.rs`, etc.) already cover the individual draw/input
// fixes at whatever scale their own harness happens to construct; this file
// is the one place that deliberately drives `ui_scale` away from
// `render_scale` and checks the whole stack still agrees with itself.

mod common;

use common::{
    canvas_pixel_for_grid, click_theme_menu_item, ensure_workspace_root_cwd, open_menu,
    EditorHarness,
};
use ember2d::renderer::draw_log::DrawOp;
use ember2d::renderer::DisplayScale;
use ember2d_editor::editor::prefs::{EditorPrefs, PrefsStore, UiScaleChoice};
use ember2d_editor::editor::ui::MenuKind;
use ember2d_editor::editor::EditorState;

/// A harness at a specific `(render_scale, ui_scale)` pair, `ui_scale` forced
/// via an in-memory `Fixed` preference rather than `Auto` — every test below
/// wants to pin the exact ratio under test, not depend on `Auto`'s own
/// `os_scale_factor` resolution (that gets its own dedicated test).
fn harness_at(render_scale: u32, ui_scale: u8) -> EditorHarness {
    ensure_workspace_root_cwd();
    let prefs =
        EditorPrefs { ui_scale: UiScaleChoice::Fixed(ui_scale), theme: "ember-clean".to_string() };
    let state = EditorState::new("harness.level").with_prefs(PrefsStore::InMemory(prefs));
    EditorHarness::with_state_and_display(
        state,
        DisplayScale { render_scale, os_scale_factor: 1.0 },
    )
}

#[test]
fn setting_ui_scale_via_the_theme_menu_takes_effect_on_the_next_frame_and_persists() {
    // The harness default (`render_scale: 2`) means `ui_scale` starts at
    // `Auto`'s resolved `2` — picking "UI Scale: 3x" from the menu must
    // both persist the preference AND change what `ui_space()` reports on
    // the very next render, without a restart.
    let mut h = EditorHarness::new();
    assert_eq!(h.state.ui_space().ui_scale(), 2, "Auto at 100% OS scale must start at ui_scale 2");

    open_menu(&mut h, MenuKind::Theme);
    click_theme_menu_item(&mut h, "UI Scale: 3x");

    assert_eq!(
        h.state.prefs().ui_scale,
        UiScaleChoice::Fixed(3),
        "picking a UI Scale entry must persist it to prefs"
    );
    assert_eq!(
        h.state.ui_space().ui_scale(),
        3,
        "the new scale must take effect on the very next draw, not require a restart"
    );
    assert_eq!(h.state.active_menu(), None, "picking a UI Scale entry must close its dropdown");
}

#[test]
fn auto_ui_scale_resolves_from_the_displays_own_os_scale_factor() {
    // `UiScaleChoice::resolve`'s own unit tests (`editor/prefs.rs`) already
    // pin the 100/125/150/200% -> 2/3/3/4 table in isolation; this exercises
    // the same resolution through a real `EditorState`'s draw path, at the
    // default `Auto` preference no test here ever changes.
    for (os_scale_factor, expected_ui_scale) in [(1.0_f32, 2_u32), (1.5, 3), (2.0, 4)] {
        ensure_workspace_root_cwd();
        let h = EditorHarness::with_display(DisplayScale { render_scale: 2, os_scale_factor });
        assert_eq!(
            h.state.ui_space().ui_scale(),
            expected_ui_scale,
            "Auto at {os_scale_factor}x OS scale must resolve to ui_scale {expected_ui_scale}"
        );
    }
}

#[test]
fn chrome_geometry_scales_with_ui_scale_at_a_fixed_render_scale() {
    // The title bar's own background (`ui/panels/chrome.rs`'s
    // `draw_title_bar`: `painter.fill(Rect::new(0.0, 0.0, pixel_w, row_h),
    // bg)`) is the one chrome rect whose HEIGHT is a pure `ChromeMetrics`
    // constant (`row_h` == `metrics.bar_h`, in points) with no dependence
    // on how much of the window's own points BUDGET the docked side panels
    // are currently eating — unlike that budget itself (`pixel_w` above,
    // `UiSpace::screen_pt()`), which shrinks as `ui_scale` grows (a bigger
    // `ui_scale` means each point covers more physical pixels, so FEWER
    // points fit in the same physical window — this is also why the
    // viewport's own logical width shrinks, not grows, as `ui_scale`
    // increases: fixed-point-width side panels eat a bigger logical
    // share). The title bar's fixed-points HEIGHT has no such interaction,
    // so its recorded LOGICAL height must scale with `ui_scale` exactly —
    // checked against the real recorded draw op, not a second call to
    // `UiSpace::rect_to_logical` in the test.
    let mut small = harness_at(2, 2);
    let mut big = harness_at(2, 4);
    small.start_recording();
    big.start_recording();
    small.frame();
    big.frame();

    let title_bar_height = |ops: &[DrawOp]| -> f32 {
        ops.iter()
            .find_map(|op| match op {
                DrawOp::Fill(r) if r.x == 0.0 && r.y == 0.0 => Some(r.h),
                _ => None,
            })
            .expect(
                "the title bar's own background fill must be drawn at the screen's top-left corner",
            )
    };
    let h_small = title_bar_height(small.draw_ops());
    let h_big = title_bar_height(big.draw_ops());

    // ui_scale doubled (2 -> 4) at the same render_scale, so the title
    // bar's fixed-points height must double in logical pixels too.
    let eps = 0.5; // sub-pixel rounding from `UiSpace::snap`'s own point-rounding
    assert!(
        (h_big - h_small * 2.0).abs() < eps,
        "the title bar's logical height must double with ui_scale: {h_small} vs {h_big}"
    );
}

#[test]
fn canvas_painting_is_unaffected_by_ui_scale() {
    // The 7C-9 decision gate (master plan §7.1): the level canvas stays on
    // the engine's fixed logical `CELL_W`/`CELL_H` grid forever, regardless
    // of chrome's own `ui_scale` — painting the same grid cell at two very
    // different `ui_scale`s (same `render_scale`, so the viewport's own
    // on-screen LOGICAL position is identical) must place a tile at the
    // exact same grid coordinates both times.
    let mut low = harness_at(2, 1);
    let mut high = harness_at(2, 4);

    let (lx, ly) = canvas_pixel_for_grid(&low, 5, 5);
    low.click(lx, ly);
    let (hx, hy) = canvas_pixel_for_grid(&high, 5, 5);
    high.click(hx, hy);

    assert_eq!(
        low.state.grid().tiles.len(),
        1,
        "the low-ui_scale click must place exactly one tile"
    );
    assert_eq!(
        high.state.grid().tiles.len(),
        1,
        "the high-ui_scale click must place exactly one tile"
    );
    assert!(
        low.state.grid().get(5, 5, low.state.active_layer()).is_some(),
        "must land on grid cell (5,5) at ui_scale 1"
    );
    assert!(
        high.state.grid().get(5, 5, high.state.active_layer()).is_some(),
        "must land on grid cell (5,5) at ui_scale 4"
    );
}

#[test]
fn menu_and_theme_dropdown_clicks_round_trip_when_ui_scale_is_smaller_than_render_scale() {
    // Every other test in this file has `ui_scale >= render_scale` (S/R >=
    // 1.0) — this is the step's own "S/R = 0.5" case (master plan §5.4's
    // commit-sequence note for this checkpoint), `render_scale: 4` (a
    // hypothetical high-DPI display) with an explicit `Fixed(2)` — a point
    // covers HALF as many logical pixels as at S == R, so a hit-test bug
    // that only showed up when points are LARGER than logical pixels
    // (mixing up a multiply and a divide, say) would invert here instead
    // of just being a different multiplier.
    let mut h = harness_at(4, 2);
    assert_eq!(h.state.ui_space().ui_scale(), 2);
    assert_eq!(h.state.ui_space().render_scale(), 4);

    open_menu(&mut h, MenuKind::Theme);
    click_theme_menu_item(&mut h, "ember-clean");
    assert_eq!(
        h.state.active_menu(),
        None,
        "picking a theme must close its dropdown even when ui_scale < render_scale"
    );
    assert_eq!(h.state.theme().name, "ember-clean");
}
