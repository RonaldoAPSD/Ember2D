// ember2d-editor/tests/editor_undo.rs — 7C-6 (docs/ember2d-master-plan.md
// §5.3, D18): LevelGrid determinism and undo batching regression tests,
// driven through `EditorHarness` (tests/common/mod.rs). Split out of
// tests/editor_input.rs (7C-5's own suite) purely to keep both files under
// CLAUDE.md's 750-line hard limit — no behavioral change, and both share
// the same `common` module (menu/canvas helpers included).

mod common;

use common::{
    canvas_pixel_for_grid, click_menu_item, ensure_workspace_root_cwd, open_menu, select_dock_tab,
    EditorHarness,
};
use ember2d::input::Key;
use ember2d_editor::editor::ui::{MenuKind, ToolbarAction, ToolKind, WidgetId};
use ember2d_editor::editor::EditorMode;

// ── 7C-6: LevelGrid determinism and undo batching (D18) ─────────────────────

#[test]
fn a_50_cell_freehand_paint_stroke_batches_into_one_undo_step() {
    let mut h = EditorHarness::new();
    let mut path = Vec::new();
    for x in 1..26 {
        path.push(canvas_pixel_for_grid(&h, x, 5));
    }
    for x in 1..26 {
        path.push(canvas_pixel_for_grid(&h, x, 6));
    }
    assert_eq!(path.len(), 50);

    h.drag_through(&path);

    assert_eq!(h.state.grid().tiles.len(), 50, "the whole stroke must have painted all 50 cells");
    assert_eq!(h.state.undo_len(), 1, "a whole freehand stroke must be exactly one undo step");

    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(h.state.grid().tiles.len(), 0, "one undo must restore all 50 cells to empty");
}

#[test]
fn drag_erase_batches_into_one_undo_step() {
    // 7C-6 (master plan §5.3, D18): a right-held drag-erase (brush size 1)
    // used to push one `EraseTile` per cell touched, same class of bug as
    // freehand paint above.
    let mut h = EditorHarness::new();
    let path: Vec<(f32, f32)> = (1..11).map(|x| canvas_pixel_for_grid(&h, x, 5)).collect();
    h.drag_through(&path);
    assert_eq!(h.state.grid().tiles.len(), 10);
    assert_eq!(h.state.undo_len(), 1, "the paint stroke that set this up must be one undo step");

    h.drag_button_through(ember2d::mouse::MouseButton::Right, &path);

    assert_eq!(h.state.grid().tiles.len(), 0, "the drag-erase must have erased all 10 cells");
    assert_eq!(
        h.state.undo_len(),
        2,
        "the whole erase drag must be exactly one MORE undo step (paint + erase = 2 total)"
    );

    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(h.state.grid().tiles.len(), 10, "one undo must restore all 10 erased cells at once");
}

#[test]
fn scatter_paint_batches_into_at_most_one_undo_step() {
    // 7C-6 (master plan §5.3, D18): scatter's per-cell coin flip (this
    // step's own replacement for the old, accidentally-undo-length-keyed
    // "randomness" — see canvas.rs's own comment) makes exactly how many
    // of these 30 cells get painted non-deterministic, but the whole
    // Alt+drag must still collapse into at most one undo step, never one
    // per painted cell.
    let mut h = EditorHarness::new();
    let path: Vec<(f32, f32)> = (1..31).map(|x| canvas_pixel_for_grid(&h, x, 5)).collect();

    h.press_key(Key::LeftAlt);
    h.drag_through(&path);
    h.release_key(Key::LeftAlt);

    assert!(h.state.undo_len() <= 1, "a whole scatter drag must be AT MOST one undo step, got {}", h.state.undo_len());
}

#[test]
fn adding_a_graph_node_then_undoing_removes_it() {
    // 7C-6 (master plan §5.3, D18): graph edits had zero undo support at
    // all before this step.
    use ember2d_editor::editor::ui::InspectorField;

    let mut h = EditorHarness::new();
    let (cx, cy) = canvas_pixel_for_grid(&h, 5, 5);
    h.click(cx, cy); // paint a tile to attach a graph to
    assert_eq!(h.state.grid().tiles.len(), 1);

    open_menu(&mut h, MenuKind::Tools);
    click_menu_item(&mut h, MenuKind::Tools, |a| matches!(a, ToolbarAction::EnterInspect));
    h.click(cx, cy); // select the tile in Inspect mode
    assert!(matches!(h.state.mode(), EditorMode::Inspect));

    let graph_btn_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::InspectorRow(InspectorField::GraphBtn))
        .expect("Inspector's GraphBtn row was not drawn for the selected tile");
    h.click(graph_btn_rect.x + 1.0, graph_btn_rect.y + 1.0);
    assert!(matches!(h.state.mode(), EditorMode::Graph { .. }), "clicking GraphBtn must open the graph editor");

    fn node_count(h: &EditorHarness) -> Option<usize> {
        h.state.grid().get(5, 5, 1).and_then(|t| t.graph.as_ref()).map(|g| g.nodes.len())
    }
    assert_eq!(node_count(&h), Some(0), "a freshly attached graph must start empty");

    let undo_before = h.state.undo_len();
    h.right_click(cx, cy); // open the node palette (cursor defaults to the first selectable entry, "OnStart")
    h.key(Key::Enter); // commit it
    assert_eq!(node_count(&h), Some(1), "adding a node from the palette must add exactly one node");
    assert_eq!(h.state.undo_len(), undo_before + 1, "adding a node must be exactly one undo step");

    // Ctrl+Z is a global shortcut — like every other hard-exclusive mode,
    // `EditorMode::Graph` owns the keyboard while open (`handle_update`
    // never reaches `handle_shortcuts` for it), so it must be closed first.
    h.key(Key::Escape);
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)));

    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(node_count(&h), Some(0), "undoing the add must remove the node");
}

#[test]
fn deleting_a_named_spawn_via_the_context_menu_is_undoable() {
    // 7C-6 (master plan §5.3, D18): hierarchy Duplicate/Delete had no undo
    // support at all before this step.
    use ember2d_editor::editor::ui::HierarchySelection;

    let mut h = EditorHarness::new();

    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::AddNamedSpawn));
    h.type_text("Enemy");
    h.key(Key::Enter);
    assert!(matches!(h.state.mode(), EditorMode::PlaceSpawn(Some(_))));

    let (cx, cy) = canvas_pixel_for_grid(&h, 5, 5);
    h.click(cx, cy);
    assert_eq!(h.state.grid().extra_spawns.len(), 1, "clicking must have placed the named spawn");

    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::HierarchyRow(HierarchySelection::Spawn(0)))
        .expect("the named spawn's Hierarchy row was not drawn");
    h.right_click(row_rect.x + 1.0, row_rect.y + 1.0);
    assert!(matches!(h.state.mode(), EditorMode::ContextMenu(_)), "right-clicking a hierarchy row must open its context menu");

    // A Spawn selection's context menu is built (`context_menu_trigger.rs`)
    // as ["Focus Camera", "Duplicate", "Delete"] — index 2 is "Delete".
    let delete_idx = 2;
    let item_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::ContextMenuRow(delete_idx))
        .expect("context menu row 2 (\"Delete\") was not drawn");
    h.click(item_rect.x + 1.0, item_rect.y + 1.0);

    assert_eq!(h.state.grid().extra_spawns.len(), 0, "Delete must remove the named spawn");

    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(h.state.grid().extra_spawns.len(), 1, "undo must restore the deleted spawn");
}

#[test]
fn adding_a_palette_item_via_the_new_button_is_undoable() {
    // 7C-6 (master plan §5.3, D18): palette edits had no undo support at
    // all before this step.
    let mut h = EditorHarness::new();

    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::TogglePalette));

    assert!(
        h.state.panels().visible(ember2d_editor::editor::panel::PanelId::Palette),
        "the Palette panel must be visible after toggling it on"
    );

    let before_count = h.state.palette_tile_count();
    let new_btn_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::PaletteNewBtn)
        .expect("the Palette panel's [+ New] button was not drawn");
    h.click(new_btn_rect.x + 1.0, new_btn_rect.y + 1.0);

    assert_eq!(
        h.state.palette_tile_count(),
        before_count + 1,
        "clicking [+ New] must add exactly one palette item"
    );

    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(h.state.palette_tile_count(), before_count, "undo must remove the new palette item again");
}

#[test]
fn editing_a_palette_item_in_the_modal_editor_undoes_as_one_session() {
    // 7C-6 (master plan §5.3, D18): the modal palette editor's own path —
    // covers `palette_edit_before`/`close_palette_editor`, distinct from
    // the docked panel's standalone [+ New] button tested above. The
    // editor's title/field rows have no `WidgetId` (7C-1's own "Landed
    // as" note: only its two color grids were migrated), so this clicks
    // by raw cell position, mirroring the modal's own `mx`/`my`/`cx` math
    // (`input/mod.rs`'s `handle_palette_editor_input`).
    fn cell_click(h: &mut EditorHarness, col: usize, row: usize) {
        h.click(
            col as f32 * ember2d::renderer::CELL_W as f32 + 1.0,
            row as f32 * ember2d::renderer::CELL_H as f32 + 1.0,
        );
    }

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

    let (sw, sh) = h.state.ui_space().screen_cells();
    let (mw, mh) = (36usize, 18usize);
    let mx = (sw.saturating_sub(mw)) / 2;
    let my = (sh.saturating_sub(mh)) / 2;
    let cx = mx + 2;

    let undo_before = h.state.undo_len();

    // Toggle "solid" (row my+4, the "solid" checkbox starts at cx).
    cell_click(&mut h, cx, my + 4);
    // [ Save & Close ] — row my+mh-2, x in [mx+2, mx+20).
    cell_click(&mut h, mx + 5, my + mh - 2);

    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)), "Save & Close must exit the editor");
    assert_eq!(h.state.undo_len(), undo_before + 1, "the whole open-edit-close session must be one undo step");

    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(h.state.undo_len(), undo_before, "undo must remove that one session's worth of edits");
}

// ── 7C-6: confirm before destructive actions ─────────────────────────────────

/// Right-clicks `victim`'s File Browser row and clicks "Delete" — items =
/// `["New Level", "New Script", "New Folder", "Delete"]` for a file row
/// (`context_menu_trigger.rs`), so "Delete" is index 3. Leaves the confirm
/// modal open; the caller decides Y or N.
fn open_delete_file_confirm(h: &mut EditorHarness) {
    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(0))
        .expect("the victim file's row was not drawn");
    h.right_click(row_rect.x + 1.0, row_rect.y + 1.0);
    assert!(matches!(h.state.mode(), EditorMode::ContextMenu(_)));

    let delete_row = h
        .state
        .ui_frame()
        .rect_of(WidgetId::ContextMenuRow(3))
        .expect("context menu row 3 (\"Delete\") was not drawn");
    h.click(delete_row.x + 1.0, delete_row.y + 1.0);
    assert!(
        matches!(h.state.mode(), EditorMode::Modal(_)),
        "clicking Delete must open a confirm modal, not delete immediately"
    );
}

fn setup_delete_file_harness(label: &str) -> (EditorHarness, std::path::PathBuf) {
    // `label` keeps each caller's temp dir distinct — `cargo test` runs
    // tests concurrently in threads sharing one process, so two tests
    // both keyed only by `process::id()` would race on the same path.
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join(format!("editor_input_delete_file_repro_{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let victim = dir.join("victim.rhai");
    std::fs::write(&victim, "fn on_update(id, ctx) {}\n").expect("victim file must be writable");

    let mut h = EditorHarness::new();
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    // The File Browser is visible by default (7D layout default), but
    // Console is the initially active bottom tab — select FileBrowser's
    // own tab so its rows actually render.
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);
    (h, victim)
}

#[test]
fn deleting_a_file_from_the_browser_confirms_first_and_declining_keeps_it() {
    // Found live (7C-6, master plan §5.3): `ContextMenuAction::DeleteFile`
    // used to delete immediately, with no confirmation at all.
    let (mut h, victim) = setup_delete_file_harness("decline");
    open_delete_file_confirm(&mut h);
    assert!(victim.exists(), "the file must still exist until the modal is confirmed");

    h.key(Key::N);
    assert!(victim.exists(), "declining the confirm must leave the file alone");
}

#[test]
fn confirming_a_file_delete_actually_deletes_it() {
    let (mut h, victim) = setup_delete_file_harness("confirm");
    open_delete_file_confirm(&mut h);
    h.key(Key::Y);
    assert!(!victim.exists(), "confirming must actually delete the file");
}

#[test]
fn switching_levels_with_nothing_unsaved_does_not_confirm() {
    // 7C-6 (master plan §5.3): used to always show the "Switch Level?"
    // modal, even with nothing to lose.
    ensure_workspace_root_cwd(); // before EditorState::new, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_switch_level_no_confirm_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let other_path = dir.join("other.level");
    ember2d_editor::editor::grid::LevelGrid::new(4, 4)
        .to_level_data()
        .save(other_path.to_str().unwrap())
        .expect("seed level must save");

    let current_path = dir.join("current.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&current_path));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    assert!(!h.state.unsaved(), "a freshly opened level starts unmodified");

    // The File Browser is visible by default (7D layout default), but
    // Console is the initially active bottom tab — select FileBrowser's
    // own tab so its rows actually render.
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);

    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(0))
        .expect("other.level's row was not drawn");
    h.click(row_rect.x + 1.0, row_rect.y + 1.0);

    assert!(
        !matches!(h.state.mode(), EditorMode::Modal(_)),
        "switching with nothing unsaved must not show a confirm modal"
    );
    assert_eq!(h.state.grid().width, 4, "the switch must have happened directly, with no confirm step");
}

#[test]
fn switching_levels_with_unsaved_edits_confirms_first() {
    ensure_workspace_root_cwd(); // before EditorState::new, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_switch_level_confirm_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let other_path = dir.join("other.level");
    ember2d_editor::editor::grid::LevelGrid::new(4, 4)
        .to_level_data()
        .save(other_path.to_str().unwrap())
        .expect("seed level must save");

    let current_path = dir.join("current.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&current_path));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());

    let (cx, cy) = canvas_pixel_for_grid(&h, 5, 5);
    h.click(cx, cy); // paint something so there's a real unsaved edit
    assert!(h.state.unsaved());

    // The File Browser is visible by default (7D layout default), but
    // Console is the initially active bottom tab — select FileBrowser's
    // own tab so its rows actually render.
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);

    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(0))
        .expect("other.level's row was not drawn");
    h.click(row_rect.x + 1.0, row_rect.y + 1.0);

    assert!(matches!(h.state.mode(), EditorMode::Modal(_)), "switching with unsaved edits must confirm first");
    h.key(Key::Y);
    assert_eq!(h.state.grid().width, 4, "confirming must actually load the other (4-wide) level");
}

// ── Repro: user report, 2026-09-12 — switching levels with an unsaved
// script edit (no grid edit at all) silently discards it ──────────────────

#[test]
fn switching_levels_with_only_an_unsaved_script_edit_still_confirms_first() {
    ensure_workspace_root_cwd(); // before EditorState::new, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_switch_level_script_confirm_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let other_path = dir.join("other.level");
    ember2d_editor::editor::grid::LevelGrid::new(4, 4)
        .to_level_data()
        .save(other_path.to_str().unwrap())
        .expect("seed level must save");
    std::fs::write(dir.join("player.rhai"), "fn on_update(id, ctx) {}\n")
        .expect("test script file must be writable");

    let current_path = dir.join("current.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&current_path));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());

    // The File Browser is visible by default (7D layout default), but
    // Console is the initially active bottom tab — select FileBrowser's
    // own tab so its rows actually render.
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);

    let script_row = h
        .state
        .file_browser_files()
        .iter()
        .position(|f| f.contains("player.rhai"))
        .expect("player.rhai must be listed");
    let script_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(script_row))
        .expect("player.rhai's row was not drawn");
    h.click(script_rect.x + 1.0, script_rect.y + 1.0);
    assert!(matches!(h.state.mode(), EditorMode::Script), "clicking player.rhai must open the script editor");

    h.type_text("// edited");
    assert!(h.state.script_unsaved(), "typing into the script buffer must mark it dirty");
    h.key(Key::Escape); // back to Paint — the level itself was never touched
    assert!(!h.state.unsaved(), "no grid edit was made");
    assert!(h.state.script_unsaved(), "the script edit is still unsaved after leaving fullscreen");

    // File Browser is already visible from the toggle above — leaving it
    // fullscreen (Escape) doesn't hide it again, unlike the toggle menu
    // item, which would close it if clicked a second time here.
    h.frame();

    let level_row = h
        .state
        .file_browser_files()
        .iter()
        .position(|f| f.contains("other.level"))
        .expect("other.level must be listed");
    let level_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(level_row))
        .expect("other.level's row was not drawn");
    h.click(level_rect.x + 1.0, level_rect.y + 1.0);

    assert!(
        matches!(h.state.mode(), EditorMode::Modal(_)),
        "switching levels with an unsaved script edit (even with no grid edit) must confirm first, got {:?}",
        h.state.mode()
    );
}
