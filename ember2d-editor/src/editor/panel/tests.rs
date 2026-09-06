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
    let insp_x   = (screen_w as i32 - INSP_W as i32).max(0);
    let pal_x    = (insp_x - PAL_W as i32 - 2).max(0);
    let con_y    = screen_h as i32 - CON_H as i32 - 1;

    let expected: [(PanelId, i32, i32, usize, usize); 8] = [
        (PanelId::Viewport,     0,      canvas_y, screen_w, canvas_h as usize),
        (PanelId::Hierarchy,    0,      canvas_y, HIER_W,   canvas_h as usize),
        (PanelId::Inspector,    insp_x, canvas_y, INSP_W,   canvas_h as usize),
        (PanelId::Palette,      pal_x,  canvas_y, PAL_W,    canvas_h as usize),
        (PanelId::Console,      0,      con_y,    screen_w, CON_H),
        (PanelId::Stats,        pal_x,  canvas_y, PAL_W,    canvas_h as usize),
        (PanelId::FileBrowser,  0,      canvas_y, BROW_W,   canvas_h as usize),
        (PanelId::ScriptEditor, 0,      con_y,    screen_w, EDIT_H),
    ];

    for (id, cx, cy, cw, ch) in expected {
        let p = pm.get(id);
        assert_eq!(p.rect, UiRect::from_cells(cx, cy, cw, ch),
            "{:?}'s constructed rect must equal UiRect::from_cells of its documented cell geometry", id);
        assert_eq!((p.cell_x(), p.cell_y(), p.cell_w(), p.cell_h()), (cx, cy, cw, ch),
            "{:?}'s cell_x/y/w/h bridge must invert from_cells exactly", id);
    }
}

#[test]
fn apply_layout_fills_the_viewport_gap_for_every_docked_panel_combination() {
    // Pixel-space screen matching `PanelManager::new(80, 24)`'s own cell
    // dimensions (8x16 px/cell), so the docked panels' sizes are the real
    // HIER_W/INSP_W/CON_H ones and not an arbitrary test size.
    let (screen_w, screen_h) = (640.0f32, 384.0f32);
    let canvas_top    = 2.0 * CELL_H;
    let canvas_bottom = screen_h - CELL_H;

    let mut pm = PanelManager::new(80, 24);

    // (a) Default construction: only Hierarchy (Left) and Inspector
    // (Right) are visible+docked; Console is Bottom-docked but hidden.
    pm.apply_layout(screen_w as usize, screen_h as usize);
    let hier_w = HIER_W as f32 * CELL_W;
    let insp_w = INSP_W as f32 * CELL_W;
    assert_eq!(pm.get(PanelId::Viewport).rect, UiRect::new(
        hier_w, canvas_top, screen_w - hier_w - insp_w, canvas_bottom - canvas_top,
    ), "default: viewport must fill the gap between the docked Hierarchy and Inspector");

    // (b) Hide Hierarchy with no other Left-docked panel visible: the left
    // gap closes to zero and the viewport expands to fill it.
    pm.hide(PanelId::Hierarchy);
    pm.apply_layout(screen_w as usize, screen_h as usize);
    assert_eq!(pm.get(PanelId::Viewport).rect, UiRect::new(
        0.0, canvas_top, screen_w - insp_w, canvas_bottom - canvas_top,
    ), "hiding the only Left-docked panel must give its space back to the viewport");

    // (c) Show FileBrowser (also Left-docked) while Hierarchy stays
    // hidden: `validate_active_panels` must promote it to active_left, and
    // the viewport must shrink back by FileBrowser's (different) width —
    // this doubles as the "validate_active_panels recovers" property,
    // exercised through the layout path a real render frame takes.
    pm.show(PanelId::FileBrowser);
    pm.apply_layout(screen_w as usize, screen_h as usize);
    let brow_w = BROW_W as f32 * CELL_W;
    assert_eq!(pm.active_left, Some(PanelId::FileBrowser));
    assert_eq!(pm.get(PanelId::Viewport).rect, UiRect::new(
        brow_w, canvas_top, screen_w - brow_w - insp_w, canvas_bottom - canvas_top,
    ), "the newly active Left-docked panel's width must be what the viewport gives up");

    // (d) Show Console (Bottom-docked): the viewport's height shrinks by
    // Console's height, independent of the Left/Right gap already tested.
    pm.show(PanelId::Console);
    pm.apply_layout(screen_w as usize, screen_h as usize);
    let con_h = CON_H as f32 * CELL_H;
    assert_eq!(pm.active_bottom, Some(PanelId::Console));
    assert_eq!(pm.get(PanelId::Viewport).rect, UiRect::new(
        brow_w, canvas_top, screen_w - brow_w - insp_w, canvas_bottom - canvas_top - con_h,
    ), "a docked Bottom panel must shrink the viewport's height, not its x/width");

    // (e) Hide everything: the viewport must reclaim the entire canvas.
    pm.hide(PanelId::FileBrowser);
    pm.hide(PanelId::Inspector);
    pm.hide(PanelId::Console);
    pm.apply_layout(screen_w as usize, screen_h as usize);
    assert_eq!(pm.get(PanelId::Viewport).rect, UiRect::new(
        0.0, canvas_top, screen_w, canvas_bottom - canvas_top,
    ), "with no docked panel visible on any side, the viewport must fill the whole canvas");
}

#[test]
fn validate_active_panels_recovers_to_another_visible_docked_panel_or_clears_to_none() {
    let mut pm = PanelManager::new(80, 24);
    assert_eq!(pm.active_left, Some(PanelId::Hierarchy));

    // Hiding the only visible Left-docked panel must clear the stale
    // marker to None, not leave it dangling on a now-hidden panel.
    pm.hide(PanelId::Hierarchy);
    pm.validate_active_panels();
    assert_eq!(pm.active_left, None, "no visible Left-docked panel remains, so active_left must clear to None");

    // Make another Left-docked panel visible directly (`Panel.visible` is
    // a plain field — bypassing `show()`'s own `active_left = Some(id)`
    // side effect) so this isolates `validate_active_panels`'s OWN
    // recovery rule: a side with a visible docked panel but no active
    // marker promotes the first one it finds.
    pm.get_mut(PanelId::FileBrowser).visible = true;
    assert_eq!(pm.active_left, None, "making a panel visible directly must not itself set the active marker");
    pm.validate_active_panels();
    assert_eq!(pm.active_left, Some(PanelId::FileBrowser),
        "validate_active_panels must promote the first visible Left-docked panel when none is active");
}
