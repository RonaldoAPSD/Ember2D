// editor/panel/tests.rs — Phase 7 Part 1f (docs/ember2d-phase7-plan.md):
// `UiRect::from_cells`/`apply_layout`/`validate_active_panels` tests, split
// into their own file purely to keep `mod.rs` under CLAUDE.md's 600-line
// hard limit.

use super::*;

#[test]
fn every_panel_pm_new_constructs_matches_intended_cell_geometry_via_from_cells() {
    // Part 1's "appearance must not change" property, pinned directly
    // against every real panel `PanelManager::new` constructs — `ui/rect.rs`'s
    // own test pins `from_cells`'s pure math against a few sample shapes;
    // this one pins that `Panel::new` actually calls it with the exact
    // documented cell geometry for every one of the eight real panels, and
    // that `cell_x`/`cell_y`/`cell_w`/`cell_h` invert it back exactly.
    let (screen_w, screen_h) = (80usize, 24usize);
    let pm = PanelManager::new(screen_w, screen_h);

    let canvas_y = 2i32;
    let canvas_h = screen_h.saturating_sub(3).max(4) as i32;
    let insp_x = (screen_w as i32 - INSP_W as i32).max(0);
    let pal_x = (insp_x - PAL_W as i32 - 2).max(0);
    let con_y = screen_h as i32 - CON_H as i32 - 1;

    let expected: [(PanelId, i32, i32, usize, usize); 8] = [
        (PanelId::Viewport, 0, canvas_y, screen_w, canvas_h as usize),
        (PanelId::Hierarchy, 0, canvas_y, HIER_W, canvas_h as usize),
        (PanelId::Inspector, insp_x, canvas_y, INSP_W, canvas_h as usize),
        (PanelId::Palette, pal_x, canvas_y, PAL_W, canvas_h as usize),
        (PanelId::Console, 0, con_y, screen_w, CON_H),
        (PanelId::Stats, pal_x, canvas_y, PAL_W, canvas_h as usize),
        (PanelId::FileBrowser, 0, con_y, screen_w, CON_H),
        (PanelId::ScriptEditor, 0, con_y, screen_w, EDIT_H),
    ];

    for (id, cx, cy, cw, ch) in expected {
        let p = pm.get(id);
        assert_eq!(
            p.rect,
            UiRect::from_cells(cx, cy, cw, ch),
            "{:?}'s constructed rect must equal UiRect::from_cells of its documented cell geometry",
            id
        );
        assert_eq!(
            (p.cell_x(), p.cell_y(), p.cell_w(), p.cell_h()),
            (cx, cy, cw, ch),
            "{:?}'s cell_x/y/w/h bridge must invert from_cells exactly",
            id
        );
    }
}

#[test]
fn apply_layout_fills_the_viewport_gap_for_every_docked_panel_combination() {
    // Pixel-space screen matching `PanelManager::new(80, 24)`'s own cell
    // dimensions (8x16 px/cell), so the docked panels' sizes are the real
    // HIER_W/INSP_W/CON_H ones and not an arbitrary test size.
    let (screen_w, screen_h) = (640.0f32, 384.0f32);
    let canvas_top = 2.0 * CELL_H;
    let canvas_bottom = screen_h - CELL_H;

    let mut pm = PanelManager::new(80, 24);

    // (a) Default construction (7D layout default): Hierarchy left,
    // Inspector right, Console AND FileBrowser both Bottom-docked and
    // VISIBLE out of the box (tabbed together, Console the initially
    // active tab) — Files moved off Left, where it used to be the
    // alternate to Hierarchy. The viewport's height is already reduced
    // by the bottom dock from the very first layout pass.
    pm.apply_layout(screen_w as usize, screen_h as usize);
    let hier_w = HIER_W as f32 * CELL_W;
    let insp_w = INSP_W as f32 * CELL_W;
    let con_h = CON_H as f32 * CELL_H;
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
    pm.apply_layout(screen_w as usize, screen_h as usize);
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
    pm.apply_layout(screen_w as usize, screen_h as usize);
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
    pm.apply_layout(screen_w as usize, screen_h as usize);
    assert_eq!(pm.active_bottom, None);
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(0.0, canvas_top, screen_w - insp_w, canvas_bottom - canvas_top,),
        "hiding every Bottom-docked panel must give the bottom dock's space back to the viewport"
    );

    // (e) Hide everything: the viewport must reclaim the entire canvas.
    pm.hide(PanelId::Inspector);
    pm.apply_layout(screen_w as usize, screen_h as usize);
    assert_eq!(
        pm.get(PanelId::Viewport).rect,
        UiRect::new(0.0, canvas_top, screen_w, canvas_bottom - canvas_top,),
        "with no docked panel visible on any side, the viewport must fill the whole canvas"
    );
}

#[test]
fn validate_active_panels_recovers_to_another_visible_docked_panel_or_clears_to_none() {
    let mut pm = PanelManager::new(80, 24);
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
