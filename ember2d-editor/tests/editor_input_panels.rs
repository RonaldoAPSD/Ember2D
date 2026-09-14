// ember2d-editor/tests/editor_input_panels.rs — headless editor input
// regression tests for 7D-3's own area: right-click row targeting and wheel
// scroll ceilings tracking the theme's real `row_h`, not a fixed cell grid
// (split out of editor_input.rs at R87, docs/ember2d-master-plan.md §3.2 —
// that file was 761 real lines, over CLAUDE.md's 750-line limit, pushed
// there by the one-time `cargo fmt --all` pass; this is purely a file
// split, no test content changed). Driven through `EditorHarness`
// (tests/common/mod.rs) — see that module's own header comment for how one
// simulated frame works, and each test's own comment for which defect it
// pins.

mod common;

use common::{canvas_pixel_for_grid, click_menu_item, open_menu, select_dock_tab, EditorHarness};
use ember2d::input::Key;
use ember2d_editor::editor::ui::{
    ChromeMetrics, ContextMenuAction, HierarchySelection, MenuKind, ToolbarAction, WidgetId,
};
use ember2d_editor::editor::{EditorMode, PaletteField};

/// R64 (§3 in the master plan): `handle_panel_context_menu_trigger` used to
/// recompute a right-clicked row from `mouse.cell_y` (fixed 16px cells)
/// minus a cell-rounded `content_y()`, independently of the exact `row_h`
/// (20px in `ember-clean`) rows are actually drawn and hit-registered at —
/// a deep, scrolled-to row could target an entirely different file than the
/// one under the cursor. Fixed by reading the same `UiFrame` hit the
/// left-click handler already trusts.
#[test]
fn r64_right_clicking_a_deep_scrolled_file_browser_row_targets_that_exact_file() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_r64_file_browser_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    for i in 0..10 {
        std::fs::write(dir.join(format!("file{:02}.rhai", i)), "fn on_update(id, ctx) {}\n")
            .expect("victim file must be writable");
    }

    let mut h = EditorHarness::new();
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);

    // Scroll all the way to the bottom — the panel's own default height
    // only shows a handful of rows at once, so this brings the LAST file
    // ("file09.rhai") to the last visible row, the deepest, most-scrolled
    // case the old cell math above got wrong (verified by hand: at this
    // panel's real pixel position, its 16px-cell round trip landed one row
    // short of the real target here).
    let panel = h.state.panels().get(ember2d_editor::editor::panel::PanelId::FileBrowser);
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let content = panel.content_rect(&metrics);
    h.wheel(content.x + 1.0, content.y + 1.0, -100.0);

    let target_idx = h
        .state
        .file_browser_files()
        .iter()
        .position(|f| f.contains("file09.rhai"))
        .expect("file09.rhai must be listed");
    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(target_idx))
        .expect("file09.rhai's row was not drawn after scrolling to it");
    h.right_click(row_rect.x + 1.0, row_rect.y + 1.0);

    match h.state.mode() {
        EditorMode::ContextMenu(cm) => {
            let delete = cm.items.iter().find_map(|(_, action)| match action {
                ContextMenuAction::DeleteFile(path) => Some(path.clone()),
                _ => None,
            });
            assert_eq!(
                delete.as_deref().map(|p| p.contains("file09.rhai")),
                Some(true),
                "right-clicking file09.rhai's own row must offer to delete file09.rhai, not a neighboring row: {:?}",
                cm.items
            );
        }
        other => panic!("right-clicking a file row must open a context menu, got {:?}", other),
    }
}

/// R64 (§3 in the master plan): same bug as the File Browser test above, in
/// the Hierarchy panel's own `row - cy` arithmetic.
#[test]
fn r64_right_clicking_a_deep_hierarchy_row_targets_that_exact_spawn() {
    let mut h = EditorHarness::new();

    for i in 0..8 {
        open_menu(&mut h, MenuKind::Level);
        click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::AddNamedSpawn));
        h.type_text(&format!("Enemy{}", i));
        h.key(Key::Enter);
        assert!(matches!(h.state.mode(), EditorMode::PlaceSpawn(Some(_))));
        let (cx, cy) = canvas_pixel_for_grid(&h, 5 + i, 5);
        h.click(cx, cy);
    }
    assert_eq!(h.state.grid().extra_spawns.len(), 8, "all 8 named spawns must have been placed");

    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::HierarchyRow(HierarchySelection::Spawn(6)))
        .expect("the 7th spawn's Hierarchy row was not drawn");
    h.right_click(row_rect.x + 1.0, row_rect.y + 1.0);

    match h.state.mode() {
        EditorMode::ContextMenu(cm) => {
            let target = cm.items.iter().find_map(|(_, action)| match action {
                ContextMenuAction::DeleteEntity(sel) => Some(*sel),
                _ => None,
            });
            assert_eq!(
                target,
                Some(HierarchySelection::Spawn(6)),
                "right-clicking the 7th spawn's own row must target spawn index 6, not a neighboring one: {:?}",
                cm.items
            );
        }
        other => panic!("right-clicking a hierarchy row must open a context menu, got {:?}", other),
    }
}

/// R72 (§3 in the master plan): the File Browser's wheel-scroll ceiling
/// used to be computed from `p.content_h()` (a cell-rounded row count)
/// minus a literal `1` — an approximation of `draw_file_browser_panel`'s
/// own `max_visible` that only agreed with it by coincidence at the old
/// fixed 16px `CELL_H`, so the wheel could stop short of ever showing the
/// last file(s) in a long list. Fixed by computing `max_scroll` from the
/// same pixel `content_rect`/`row_h` the draw side uses.
#[test]
fn r72_the_file_browser_wheel_can_scroll_all_the_way_to_the_last_file() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_r72_file_browser_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    for i in 0..30 {
        std::fs::write(dir.join(format!("file{:02}.rhai", i)), "fn on_update(id, ctx) {}\n")
            .expect("victim file must be writable");
    }

    let mut h = EditorHarness::new();
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);

    let panel = h.state.panels().get(ember2d_editor::editor::panel::PanelId::FileBrowser);
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let content = panel.content_rect(&metrics);
    // Scroll far past the real ceiling — `handle_file_browser_click` must
    // clamp to it, not to a smaller, cell-rounded approximation of it.
    h.wheel(content.x + 1.0, content.y + 1.0, -100.0);

    let last_idx = h.state.file_browser_files().len() - 1;
    assert!(
        h.state.ui_frame().rect_of(WidgetId::FileBrowserRow(last_idx)).is_some(),
        "scrolling far past the end must still bring the last file into view"
    );
}

/// R72 (§3 in the master plan): same bug as the File Browser test above, in
/// the Palette panel's own `layout.len() - (ch - 2)` arithmetic.
#[test]
fn r72_the_palette_wheel_can_scroll_all_the_way_to_the_last_item() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::TogglePalette));

    let before_count = h.state.palette_tile_count();
    let new_btn_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::PaletteNewBtn)
        .expect("the Palette panel's [+ New] button was not drawn");
    for _ in 0..20 {
        h.click(new_btn_rect.x + 1.0, new_btn_rect.y + 1.0);
    }
    assert_eq!(
        h.state.palette_tile_count(),
        before_count + 20,
        "all 20 New clicks must have added a palette item each"
    );

    let panel = h.state.panels().get(ember2d_editor::editor::panel::PanelId::Palette);
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let content = panel.content_rect(&metrics);
    h.wheel(content.x + 1.0, content.y + 1.0, -100.0);

    let last_row_idx = h.state.palette_layout_len() - 1;
    assert!(
        h.state.ui_frame().rect_of(WidgetId::PaletteRow(last_row_idx)).is_some(),
        "scrolling far past the end must still bring the palette's last row into view"
    );
}

/// R65 (§3 in the master plan): the palette editor modal used to draw
/// centered in real px (`(screen_w - mw) / 2.0`) while its input handler
/// centered in ROUNDED integer cells (`(sw_cells - mw) / 2`) and compared
/// `mouse.cell_y == my + N` — at the default 1280×720 window (an odd
/// leftover cell remainder), the drawn rows sat half a row off from what
/// the input handler expected, so a click near a row's edge could land on
/// the wrong field. Fixed by giving every field/toggle/button its own
/// `WidgetId`, read back via `UiFrame::hit` — this test clicks at the far
/// edge of the Tag field's own drawn rect (not its top-left corner, where
/// the old half-row drift would most easily go unnoticed) and confirms
/// typing lands in the Tag field, not a neighboring one.
#[test]
fn r65_palette_editor_fields_hit_where_drawn_with_an_odd_cell_remainder() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::TogglePalette));

    let edit_btn_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::PaletteEditBtn)
        .expect("the Palette panel's [Edit] button was not drawn");
    h.click(edit_btn_rect.x + 1.0, edit_btn_rect.y + 1.0);
    assert!(matches!(h.state.mode(), EditorMode::PaletteEditor));

    let tag_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::PaletteEditorField(PaletteField::Tag))
        .expect("the palette editor's Tag row was not drawn");
    // The far edge of the row, not the top-left corner — a half-row drift
    // (the old bug) would still land inside a rect clicked at its origin.
    h.click(tag_rect.x + tag_rect.w - 1.0, tag_rect.y + tag_rect.h - 1.0);
    h.type_text("boss");

    let idx = h.state.palette_editing_idx();
    assert_eq!(
        h.state.palette_tile(idx).tag,
        "boss",
        "clicking the Tag row's own drawn rect must focus the Tag field, not a neighboring one"
    );
}
