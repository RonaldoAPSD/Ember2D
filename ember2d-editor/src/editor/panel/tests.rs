// editor/panel/tests.rs — Phase 7 Part 1f (docs/ember2d-phase7-plan.md):
// `Panel::new`/`apply_layout`/`validate_active_panels` tests, split into
// their own file purely to keep `mod.rs` under CLAUDE.md's 750-line hard
// limit. Rewritten in POINTS (7D-3, docs/ember2d-master-plan.md §5.4) —
// every size here now comes from a `ChromeMetrics`, not `CELL_W`/`CELL_H`.

use super::*;

/// A fixed `ChromeMetrics` matching the real shipped `ember-clean` theme's
/// own `Metrics` values (`row_h: 20, border: 6, padding: 6`,
/// `themes/ember-clean/theme.ron`) — not synthetic numbers, but pinned
/// directly here (rather than loading the real theme file) so these tests
/// don't depend on it staying byte-for-byte unchanged, and stay exact and
/// fast without touching disk.
fn test_metrics() -> ChromeMetrics {
    ChromeMetrics {
        bar_h: 20.0,
        row_h: 20.0,
        border: 6.0,
        padding: 6.0,
        close_w: 20.0,
        grip: 12.0,
        min_w: 80.0,
        min_h: 64.0,
        undock_max_w: 320.0,
        undock_max_h: 320.0,
        dock_threshold_x: 24.0,
        dock_threshold_y: 48.0,
        hier_w: 112.0,
        insp_w: 240.0,
        pal_w: 192.0,
        con_h: 144.0,
        edit_h: 192.0,
    }
}

#[test]
fn every_panel_pm_new_constructs_matches_intended_point_geometry() {
    // Part 1's "appearance must not change" property, pinned directly
    // against every real panel `PanelManager::new` constructs — this pins
    // that `Panel::new` builds the exact documented point geometry
    // (`ChromeMetrics::from_theme`'s own formula, `PanelManager::new`) for
    // every one of the eight real panels.
    let metrics = test_metrics();
    let (screen_w, screen_h) = (640.0f32, 384.0f32);
    let pm = PanelManager::new(screen_w, screen_h, &metrics);

    let canvas_y = metrics.chrome_top();
    let canvas_h = (screen_h - metrics.chrome_top() - metrics.bar_h).max(metrics.min_h);
    let insp_x = (screen_w - metrics.insp_w).max(0.0);
    let pal_x = (insp_x - metrics.pal_w - metrics.padding * 2.0).max(0.0);
    let con_y = screen_h - metrics.con_h - metrics.bar_h;

    let expected: [(PanelId, f32, f32, f32, f32); 8] = [
        (PanelId::Viewport, 0.0, canvas_y, screen_w, canvas_h),
        (PanelId::Hierarchy, 0.0, canvas_y, metrics.hier_w, canvas_h),
        (PanelId::Inspector, insp_x, canvas_y, metrics.insp_w, canvas_h),
        (PanelId::Palette, pal_x, canvas_y, metrics.pal_w, canvas_h),
        (PanelId::Console, 0.0, con_y, screen_w, metrics.con_h),
        (PanelId::Stats, pal_x, canvas_y, metrics.pal_w, canvas_h),
        (PanelId::FileBrowser, 0.0, con_y, screen_w, metrics.con_h),
        (PanelId::ScriptEditor, 0.0, con_y, screen_w, metrics.edit_h),
    ];

    for (id, x, y, w, h) in expected {
        let p = pm.get(id);
        assert_eq!(
            p.rect,
            UiRect::new(x, y, w, h),
            "{:?}'s constructed rect must equal its documented point geometry",
            id
        );
    }
}

#[test]
fn apply_layout_fills_the_viewport_gap_for_every_docked_panel_combination() {
    let metrics = test_metrics();
    let (screen_w, screen_h) = (640.0f32, 384.0f32);
    let canvas_top = metrics.chrome_top();
    let canvas_bottom = metrics.chrome_bottom(screen_h);

    let mut pm = PanelManager::new(screen_w, screen_h, &metrics);

    // (a) Default construction (7D layout default): Hierarchy left,
    // Inspector right, Console AND FileBrowser both Bottom-docked and
    // VISIBLE out of the box (tabbed together, Console the initially
    // active tab) — Files moved off Left, where it used to be the
    // alternate to Hierarchy. The viewport's height is already reduced
    // by the bottom dock from the very first layout pass.
    pm.apply_layout(screen_w, screen_h, &metrics);
    let hier_w = metrics.hier_w;
    let insp_w = metrics.insp_w;
    let con_h = metrics.con_h;
    assert_eq!(pm.active_bottom, Some(PanelId::Console));
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(
            hier_w,
            canvas_top,
            screen_w - hier_w - insp_w,
            canvas_bottom - canvas_top - con_h,
        ),
        "default: viewport must fill the gap between Hierarchy/Inspector, minus the bottom dock"
    );

    // (b) Hide Hierarchy with no other Left-docked panel visible: the left
    // gap closes to zero and the viewport expands to fill it; the bottom
    // dock is untouched.
    pm.hide(PanelId::Hierarchy);
    pm.apply_layout(screen_w, screen_h, &metrics);
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(0.0, canvas_top, screen_w - insp_w, canvas_bottom - canvas_top - con_h,),
        "hiding the only Left-docked panel must give its space back to the viewport"
    );

    // (c) Hide Console while FileBrowser (also Bottom-docked, same
    // height) stays visible: `apply_layout`'s own `validate_active_panels`
    // call must recover `active_bottom` to FileBrowser rather than
    // clearing it — since the two share a height, the viewport's rect is
    // unchanged from (b).
    pm.hide(PanelId::Console);
    pm.apply_layout(screen_w, screen_h, &metrics);
    assert_eq!(
        pm.active_bottom,
        Some(PanelId::FileBrowser),
        "hiding the active Bottom tab must recover to the other visible Bottom-docked panel"
    );
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(0.0, canvas_top, screen_w - insp_w, canvas_bottom - canvas_top - con_h,),
        "recovering to an equal-height alternate must not itself resize the viewport"
    );

    // (d) Hide FileBrowser too: nothing left on Bottom, so its height
    // reservation disappears entirely.
    pm.hide(PanelId::FileBrowser);
    pm.apply_layout(screen_w, screen_h, &metrics);
    assert_eq!(pm.active_bottom, None);
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(0.0, canvas_top, screen_w - insp_w, canvas_bottom - canvas_top,),
        "hiding every Bottom-docked panel must give the bottom dock's space back to the viewport"
    );

    // (e) Hide everything: the viewport must reclaim the entire canvas.
    pm.hide(PanelId::Inspector);
    pm.apply_layout(screen_w, screen_h, &metrics);
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(0.0, canvas_top, screen_w, canvas_bottom - canvas_top,),
        "with no docked panel visible on any side, the viewport must fill the whole canvas"
    );
}

#[test]
fn validate_active_panels_recovers_to_another_visible_docked_panel_or_clears_to_none() {
    let metrics = test_metrics();
    let mut pm = PanelManager::new(640.0, 384.0, &metrics);
    assert_eq!(pm.active_left, Some(PanelId::Hierarchy));

    // Hiding the only visible Left-docked panel must clear the stale
    // marker to None, not leave it dangling on a now-hidden panel.
    pm.hide(PanelId::Hierarchy);
    pm.validate_active_panels();
    assert_eq!(
        pm.active_left, None,
        "no visible Left-docked panel remains, so active_left must clear to None"
    );

    // Bottom (Console/FileBrowser, 7D layout default) is where this repo
    // actually has two panels sharing a dock side, both visible from
    // construction — this half of the test exercises Bottom instead of
    // Left. Hiding just the active one (Console) must recover to the
    // other visible Bottom panel (FileBrowser), not clear to None —
    // that's the same "recovers to an alternate" property
    // `apply_layout_fills_the_viewport_gap_...`'s own scenario (c)
    // already pins through the render-frame path; hiding BOTH is what
    // actually reaches "nothing active."
    pm.hide(PanelId::Console);
    pm.validate_active_panels();
    assert_eq!(
        pm.active_bottom,
        Some(PanelId::FileBrowser),
        "hiding the active Bottom tab while another Bottom panel is still visible must recover to it"
    );

    pm.hide(PanelId::FileBrowser);
    pm.validate_active_panels();
    assert_eq!(
        pm.active_bottom, None,
        "no visible Bottom-docked panel remains, so active_bottom must clear to None"
    );

    // Make FileBrowser visible directly (`Panel.visible` is a plain field
    // — bypassing `show()`'s own `active_bottom = Some(id)` side effect)
    // so this isolates `validate_active_panels`'s OWN recovery rule: a
    // side with a visible docked panel but no active marker promotes the
    // first one it finds.
    pm.get_mut(PanelId::FileBrowser).visible = true;
    assert_eq!(
        pm.active_bottom, None,
        "making a panel visible directly must not itself set the active marker"
    );
    pm.validate_active_panels();
    assert_eq!(pm.active_bottom, Some(PanelId::FileBrowser),
        "validate_active_panels must promote the first visible Bottom-docked panel when none is active");
}

/// R74 (§3 in the master plan): the resize grip is exactly `2 * border` on
/// a side — its own 9-slice corners fit precisely without overlapping,
/// unlike the old fixed 8px grip with a 6+6px border.
#[test]
fn chrome_metrics_resize_grip_fits_its_nine_slice_borders() {
    let metrics = test_metrics();
    assert_eq!(metrics.grip, 2.0 * metrics.border);
}

/// A docked panel is never squeezed so small the viewport disappears
/// entirely — `min_w`/`min_h` (carried from the old 10x4-cell floor) are
/// real constants regardless of the active theme's own row height.
#[test]
fn docked_panels_never_shrink_below_the_minimum_size() {
    let metrics = test_metrics();
    let mut pm = PanelManager::new(640.0, 384.0, &metrics);
    pm.apply_layout(640.0, 384.0, &metrics);
    pm.start_resize(PanelId::Hierarchy, 200.0, 100.0);
    // Drag far past zero width — must clamp at `min_w`, not go negative or
    // collapse to zero.
    pm.update_resize(-10000.0, 100.0, &metrics);
    assert!(pm.get(PanelId::Hierarchy).rect.w >= metrics.min_w);
}
