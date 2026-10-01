// ember2d-editor/tests/editor_input_files.rs — headless editor input
// regression tests for the two "repro" areas that touch real files on disk:
// opening a script from the File Browser, and the File Browser refreshing
// after a save/new-level/rename (split out of editor_input.rs at R87,
// docs/ember2d-master-plan.md §3.2 — that file was 761 real lines, over
// CLAUDE.md's 750-line limit, pushed there by the one-time `cargo fmt --all`
// pass; this is purely a file split, no test content changed). Driven
// through `EditorHarness` (tests/common/mod.rs) — see that module's own
// header comment for how one simulated frame works, and each test's own
// comment for which defect (if any) it pins.

mod common;

use common::{click_menu_item, ensure_workspace_root_cwd, open_menu, select_dock_tab, EditorHarness};
use ember2d::input::Key;
use ember2d_editor::editor::ui::{MenuKind, ToolKind, ToolbarAction, WidgetId};
use ember2d_editor::editor::{EditorMode, TextInputPurpose};

// ── Repro: user report, opening a .rhai file from the File Browser ─────────

#[test]
fn clicking_a_rhai_file_in_the_file_browser_opens_the_fullscreen_script_editor() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_file_browser_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    std::fs::write(dir.join("player.rhai"), "fn on_update(id, ctx) {}\n")
        .expect("test script file must be writable");

    let mut h = EditorHarness::new();
    h.state.open_project_folder(dir.to_string_lossy().into_owned());

    // The File Browser is visible by default (7D layout default: tabbed
    // with Console at the bottom) — no toggle needed, but Console is the
    // initially active bottom tab, so FileBrowser's own tab must still be
    // selected before its rows render (see `select_dock_tab`'s own doc
    // comment). Only one file exists at the project root, so it's row 0.
    assert!(h.state.panels().visible(ember2d_editor::editor::panel::PanelId::FileBrowser));
    select_dock_tab(&mut h, ember2d_editor::editor::panel::PanelId::FileBrowser);
    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(0))
        .expect("player.rhai's row was not drawn in the File Browser panel");
    h.click(row_rect.x + 1.0, row_rect.y + 1.0);

    // The bug this pins (found live by the user, 2026-09-12): `handle_update`'s
    // `match std::mem::take(&mut self.mode) { EditorMode::Script => { ... } }`
    // (7C-4, master plan §5.3) takes `EditorMode::Script` out of `self.mode`
    // (leaving `EditorMode::default()`, i.e. `Paint(Paint)`) before calling
    // `handle_script_mode_input` — every OTHER hard-exclusive arm restores
    // its own mode as that handler's first action; this one never did, so
    // the very next frame (the release half of this `click`) silently
    // reverted `self.mode` to `Paint` and the fullscreen editor never
    // rendered again — it "flashed" open for a single frame, invisible at
    // 60fps, and looked to the user like clicking the file did nothing.
    assert!(
        matches!(h.state.mode(), EditorMode::Script),
        "clicking player.rhai in the File Browser must open (and stay in) the fullscreen script editor, got {:?}",
        h.state.mode()
    );
}

// ── Repro: user report, a new project's level files never showing up ───────

#[test]
fn saving_a_brand_new_level_for_the_first_time_refreshes_the_file_browser() {
    // Found live by the user (2026-09-12): a fresh project's first save
    // (nothing existed at `save_path` yet) wrote the level file to disk
    // but never called `refresh_project_files`, so it never appeared in
    // the File Browser until something else (a folder navigation)
    // happened to refresh it.
    ensure_workspace_root_cwd(); // before EditorState::new, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_save_refresh_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let level_path = dir.join("ManualPass.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&level_path));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    assert!(h.state.file_browser_files().is_empty(), "nothing saved yet");

    h.key(Key::S); // plain S: save (shortcuts.rs) — not Shift+S (save-as)

    assert!(
        h.state.file_browser_files().iter().any(|f| f.contains("ManualPass.level")),
        "saving a level for the first time must refresh the File Browser: {:?}",
        h.state.file_browser_files()
    );
}

#[test]
fn creating_a_new_level_via_the_level_menu_refreshes_the_file_browser() {
    ensure_workspace_root_cwd(); // before EditorState::new, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_new_level_refresh_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    // `with_state`, not `new()`: `NewLevelName`'s own commit handler
    // saves the CURRENT level before switching (`text.rs`) — a
    // default harness's relative "harness.level" save path would
    // otherwise write a stray file next to the test binary's CWD
    // instead of into this test's own temp dir.
    let level_path = dir.join("current.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&level_path));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());

    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::NewLevel));
    // 7C-6 (master plan §5.3): New Level now confirms first.
    assert!(
        matches!(h.state.mode(), EditorMode::Modal(_)),
        "New Level must confirm before opening the name prompt"
    );
    h.key(Key::Y);
    assert!(matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::NewLevelName)));

    h.type_text("Test");
    h.key(Key::Enter);

    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)));
    assert!(
        h.state.file_browser_files().iter().any(|f| f.contains("Test.level")),
        "creating a new level must refresh the File Browser: {:?}",
        h.state.file_browser_files()
    );
}

#[test]
fn renaming_a_level_also_renames_its_file_on_disk() {
    // Found live by the user (2026-09-12): "Rename Level" only ever
    // updated `grid.name` (the title-bar display name) — the file on
    // disk, and `save_path`, kept the old name forever, so the File
    // Browser and the level's own displayed name silently drifted apart.
    ensure_workspace_root_cwd(); // before EditorState::load, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_input_rename_level_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let old_path = dir.join("Test.level");
    let mut seed = ember2d_editor::editor::grid::LevelGrid::new(4, 4);
    seed.name = "Test".to_string();
    seed.to_level_data().save(old_path.to_str().unwrap()).expect("seed level must save");

    let mut h = EditorHarness::with_state(
        ember2d_editor::editor::EditorState::load(old_path.to_str().unwrap()).unwrap(),
    );
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    assert!(h.state.file_browser_files().iter().any(|f| f.contains("Test.level")));

    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::RenameLevel));
    assert!(matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::LevelName)));

    // RenameLevel seeds the prompt buffer with the current name — clear
    // it before typing the real new name.
    for _ in 0..h.state.prompt_buffer().chars().count() {
        h.key(Key::Backspace);
    }
    h.type_text("Test4");
    h.key(Key::Enter);

    assert_eq!(h.state.grid().name, "Test4");
    let new_path = dir.join("Test4.level");
    assert!(new_path.exists(), "the renamed file must exist at the new name");
    assert!(!old_path.exists(), "the old file must not be left behind under its old name");
    assert!(
        h.state.file_browser_files().iter().any(|f| f.contains("Test4.level")),
        "the File Browser must show the renamed file: {:?}",
        h.state.file_browser_files()
    );
    assert!(
        !h.state
            .file_browser_files()
            .iter()
            .any(|f| f.contains("Test.level") && !f.contains("Test4")),
        "the File Browser must not still show the old filename: {:?}",
        h.state.file_browser_files()
    );
}

/// R107's Enter half (fixed in the Phase 9 gate pass): Enter answers a
/// confirm modal the way its highlighted `[ YES ]` button promises — it
/// used to do nothing.
#[test]
fn r107_enter_confirms_a_modal() {
    ensure_workspace_root_cwd();
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join("r107");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let level_path = dir.join("current.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&level_path));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::NewLevel));
    assert!(matches!(h.state.mode(), EditorMode::Modal(_)));
    h.key(Key::Enter);
    assert!(matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::NewLevelName)));
}
