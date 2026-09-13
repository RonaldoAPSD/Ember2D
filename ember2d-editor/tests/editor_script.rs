// ember2d-editor/tests/editor_script.rs — 7C-7 (docs/ember2d-master-plan.md
// §5.3, R18): script errors reaching the editor. Two halves: the live
// syntax check (compile-on-save, compile-on-idle, inline error display)
// and the F5-preview log hand-off (`GameState::receive_script_log`) —
// the play-state side of that hand-off (`take_script_log`) is pinned in
// ember2d's own `tests/take_script_log.rs`; the full round trip through a
// real F5 press needs `ember2d-app`'s live `Engine`/window, which no test
// in this repo drives headlessly (see `run_editor_app`'s own comment on
// the `Transition::ToEditor` arm this step added).

mod common;

use common::{click_menu_item, open_menu, EditorHarness};
use ember2d::engine::GameState;
use ember2d::input::Key;
use ember2d_editor::editor::ui::{MenuKind, ToolbarAction, WidgetId};
use ember2d_sim::scripting::LogEntry;

// ── receive_script_log (the editor side of R18's fix) ───────────────────────

#[test]
fn receive_script_log_appends_to_the_console() {
    let mut h = EditorHarness::new();
    assert!(h.state.console_log().is_empty());

    h.state.receive_script_log(vec![LogEntry::error("boom")]);

    assert_eq!(h.state.console_log().len(), 1);
    assert!(h.state.console_log()[0].text.contains("boom"));
}

// ── Live syntax check ────────────────────────────────────────────────────────

/// Writes `name` with `initial` content into a fresh project folder, opens
/// the File Browser, and clicks it — the same real path a user takes
/// (`clicking_a_rhai_file_in_the_file_browser_opens_the_fullscreen_script_editor`,
/// editor_input.rs), not a direct call to the crate-private `load_script`.
fn open_script_via_file_browser(dir: &std::path::Path, name: &str, initial: &str) -> EditorHarness {
    std::fs::write(dir.join(name), initial).expect("test script file must be writable");

    let mut h = EditorHarness::new();
    h.state.open_project_folder(dir.to_string_lossy().into_owned());

    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::ToggleFileBrowser));
    h.frame();

    let row = h
        .state
        .file_browser_files()
        .iter()
        .position(|f| f.contains(name))
        .unwrap_or_else(|| panic!("{name} must be listed in the File Browser"));
    let rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(row))
        .unwrap_or_else(|| panic!("{name}'s row was not drawn"));
    h.click(rect.x + 1.0, rect.y + 1.0);
    h
}

fn ctrl_s(h: &mut EditorHarness) {
    h.press_key(Key::LeftCtrl);
    h.press_key(Key::S);
    h.release_key(Key::S);
    h.release_key(Key::LeftCtrl);
}

#[test]
fn saving_an_unclosed_function_shows_a_parse_error_at_the_right_line() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_save_error_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_script_via_file_browser(&dir, "player.rhai", "fn on_update(id, ctx) {}\n");
    assert_eq!(h.state.script_error(), None, "a script that compiles cleanly starts with no error");

    // The plan's own repro (7C-6, master plan §5.3): break the one line by
    // deleting its closing brace, then save — Ctrl+S must compile and
    // report the parse error on the right (only) line.
    h.key(Key::End);
    h.key(Key::Backspace);
    ctrl_s(&mut h);

    let err = h.state.script_error();
    assert!(err.is_some(), "an unclosed function must fail to compile");
    let (line, msg) = err.unwrap();
    assert_eq!(line, 0, "the only line in the buffer is where the error must be reported, got line {line} ({msg})");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fixing_the_error_and_saving_again_clears_it() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_fix_error_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_script_via_file_browser(&dir, "player.rhai", "fn on_update(id, ctx) {\n");
    ctrl_s(&mut h);
    assert!(h.state.script_error().is_some(), "the seed script is deliberately broken");

    h.key(Key::End);
    h.type_text("}");
    ctrl_s(&mut h);

    assert_eq!(h.state.script_error(), None, "a fixed script must clear the error on the next save");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_idle_timer_triggers_a_check_without_an_explicit_save() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_idle_check_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_script_via_file_browser(&dir, "player.rhai", "fn on_update(id, ctx) {}\n");
    assert_eq!(h.state.script_error(), None);

    // Break it in place (no save) — move to the end of the one line and
    // delete the closing brace.
    h.key(Key::End);
    h.key(Key::Backspace);
    assert_eq!(h.state.script_error(), None, "no check has run yet — only ~500ms of idle triggers one");

    // `note_script_edit` just reset the idle timer to 0 (the Backspace
    // above); 30 frames (SCRIPT_IDLE_CHECK_FRAMES, script_editor.rs) at
    // the editor's fixed 60Hz step is ~500ms.
    for _ in 0..30 {
        h.frame();
    }

    assert!(h.state.script_error().is_some(), "500ms idle with no further edits must trigger a live syntax check");

    let _ = std::fs::remove_dir_all(&dir);
}

// ── 7C-8: text editor completeness ──────────────────────────────────────────

fn open_blank_script(dir: &std::path::Path) -> EditorHarness {
    open_script_via_file_browser(dir, "player.rhai", "\n")
}

fn ctrl_key(h: &mut EditorHarness, key: Key) {
    h.press_key(Key::LeftCtrl);
    h.press_key(key);
    h.release_key(key);
    h.release_key(Key::LeftCtrl);
}

/// The plan's own repro (7C-8, master plan §5.3): select-all, cut, paste
/// round-trips content including non-ASCII.
#[test]
fn select_all_cut_paste_round_trips_non_ascii_content() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_cut_paste_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("café 🎮 hello");

    ctrl_key(&mut h, Key::A);
    let sel = h.state.script_selection();
    assert!(sel.is_some(), "Ctrl+A must select the whole buffer");

    ctrl_key(&mut h, Key::X);
    assert_eq!(h.state.script_buffer(), &[String::new()], "cut must remove the selected text");
    assert_eq!(h.state.script_clipboard(), "café 🎮 hello");

    ctrl_key(&mut h, Key::V);
    assert_eq!(
        h.state.script_buffer(),
        &["café 🎮 hello".to_string()],
        "pasting the cut text back must restore it exactly, non-ASCII included"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn copy_leaves_the_original_text_in_place() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_copy_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("hello");
    ctrl_key(&mut h, Key::A);
    ctrl_key(&mut h, Key::C);

    assert_eq!(h.state.script_clipboard(), "hello");
    assert_eq!(h.state.script_buffer(), &["hello".to_string()], "copy must not remove anything");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn shift_right_extends_a_selection_one_character_at_a_time() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_shift_select_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("hello");
    h.key(Key::Home);
    assert_eq!(h.state.script_selection(), None);

    h.press_key(Key::LeftShift);
    h.press_key(Key::Right);
    h.release_key(Key::Right);
    h.press_key(Key::Right);
    h.release_key(Key::Right);
    h.release_key(Key::LeftShift);

    assert_eq!(h.state.script_selection(), Some(((0, 0), (2, 0))), "Shift+Right twice must select the first 2 characters");

    // A plain (non-Shift) move collapses the selection (7C-8's own
    // documented simplification — see `update_selection_anchor`).
    h.key(Key::Right);
    assert_eq!(h.state.script_selection(), None);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typing_replaces_an_active_selection() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_replace_selection_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("hello");
    ctrl_key(&mut h, Key::A);
    h.type_text("bye");

    assert_eq!(h.state.script_buffer(), &["bye".to_string()]);
    assert_eq!(h.state.script_selection(), None, "typing must consume the selection, not leave it dangling");

    let _ = std::fs::remove_dir_all(&dir);
}

fn ctrl_z(h: &mut EditorHarness) {
    ctrl_key(h, Key::Z);
}
fn ctrl_y(h: &mut EditorHarness) {
    ctrl_key(h, Key::Y);
}

#[test]
fn a_typing_burst_undoes_as_one_step() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_undo_coalesce_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("hello");
    assert_eq!(h.state.script_buffer(), &["hello".to_string()]);

    ctrl_z(&mut h);
    assert_eq!(h.state.script_buffer(), &[String::new()], "one undo must remove the whole typed word");

    ctrl_y(&mut h);
    assert_eq!(h.state.script_buffer(), &["hello".to_string()], "redo must restore it");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn switching_edit_kinds_starts_a_new_undo_step() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_undo_group_switch_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("hello");
    h.key(Key::Backspace);
    h.key(Key::Backspace);
    assert_eq!(h.state.script_buffer(), &["hel".to_string()]);

    // Typing then backspacing is two separate coalescing groups — undoing
    // once must only reverse the backspaces, not the typing too.
    ctrl_z(&mut h);
    assert_eq!(h.state.script_buffer(), &["hello".to_string()], "undo must first restore just the 2 backspaces");
    ctrl_z(&mut h);
    assert_eq!(h.state.script_buffer(), &[String::new()], "a second undo removes the typed word");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ctrl_f_finds_and_selects_the_next_match() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_find_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    h.type_text("fn on_start(id, ctx) {}");
    h.key(Key::Enter);
    h.type_text("fn on_update(id, ctx) {}");

    ctrl_key(&mut h, Key::F);
    assert!(h.state.script_find_active());

    h.type_text("on_");
    assert_eq!(h.state.script_find_query(), "on_");
    // The FIRST match (in "on_start") is at line 0, columns 3..6.
    assert_eq!(h.state.script_selection(), Some(((3, 0), (6, 0))), "typing the query must jump to the first match");

    h.key(Key::Enter);
    assert_eq!(h.state.script_selection(), Some(((3, 1), (6, 1))), "Enter must advance to the next match (\"on_update\")");

    h.key(Key::Escape);
    assert!(!h.state.script_find_active(), "Escape must close the find bar");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn typing_a_long_line_scrolls_it_horizontally() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_hscroll_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = open_blank_script(&dir);
    assert_eq!(h.state.script_hscroll(), 0);

    // Comfortably wider than any realistic script panel — must push the
    // view horizontally once the cursor runs past the visible width.
    h.type_text(&"x".repeat(300));

    assert!(h.state.script_hscroll() > 0, "typing past the visible width must scroll the view horizontally");

    let _ = std::fs::remove_dir_all(&dir);
}
