// ember2d-editor/tests/editor_script.rs — script editor regression tests
// (7D-3 checkpoint 5, docs/ember2d-master-plan.md §5.4), driven through
// `EditorHarness` (tests/common/mod.rs). See each test's own comment for
// which defect it pins.

mod common;

use common::{click_menu_item, open_menu, select_dock_tab, EditorHarness};
use ember2d::input::Key;
use ember2d::renderer::draw_log::DrawOp;
use ember2d_editor::editor::panel::PanelId;
use ember2d_editor::editor::ui::{ChromeMetrics, MenuKind, ScriptLayout, ToolbarAction};
use ember2d_editor::editor::EditorMode;

/// Opens `content` as `<dir>/script.rhai` through the real File Browser
/// click path (fullscreen), then leaves fullscreen and docks+selects the
/// Script Editor panel — the "loaded, visible, but not yet focused" state
/// `handle_script_editor_click`'s own first-click path (R67) exists for.
fn open_docked_unfocused_script(h: &mut EditorHarness, dir: &std::path::Path, content: &str) {
    std::fs::write(dir.join("script.rhai"), content).expect("test script file must be writable");
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    select_dock_tab(h, PanelId::FileBrowser);
    let row_rect = h
        .state
        .ui_frame()
        .rect_of(ember2d_editor::editor::ui::WidgetId::FileBrowserRow(0))
        .expect("script.rhai's row was not drawn");
    h.click(row_rect.x + 1.0, row_rect.y + 1.0);
    assert!(
        matches!(h.state.mode(), EditorMode::Script),
        "opening a .rhai file must enter fullscreen script mode"
    );
    h.key(Key::Escape);
    assert!(
        matches!(h.state.mode(), EditorMode::Paint(_)),
        "Escape must leave fullscreen script mode"
    );

    open_menu(h, MenuKind::View);
    click_menu_item(h, MenuKind::View, |a| matches!(a, ToolbarAction::ToggleScriptEditor));
    select_dock_tab(h, PanelId::ScriptEditor);
}

fn script_layout(h: &mut EditorHarness) -> ScriptLayout {
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let content: ember2d_sim::math::Rect =
        h.state.panels().get(PanelId::ScriptEditor).content_rect(&metrics).into();
    let line_count = h.state.script_buffer().len();
    let theme = h.state.theme().clone();
    // Mirrors `draw_script_editor`'s own call exactly (same theme, same
    // content rect, same line count, no error/find bar reserved) — the
    // test's own oracle for where a click SHOULD land.
    ScriptLayout::compute(&theme, h.state.code_font(), content, line_count, false, false)
}

/// R67 (§3 in the master plan): the docked panel's FIRST click (before it
/// has focus, `handle_script_editor_click` in
/// `input/panels/file_and_script.rs`) used its own independent formula —
/// a fixed `gutter_w = 4` (cells) with no `hscroll` term at all — instead
/// of the real, dynamic gutter `draw_script_editor`/`ScriptLayout` use. A
/// short (single-digit-line-count) file's real gutter is narrower than
/// that fixed literal, so the old formula placed the cursor in the wrong
/// column on the very first click. Both paths now read the same
/// `ScriptLayout::hit`.
#[test]
fn r67_the_first_click_on_a_docked_unfocused_script_editor_lands_in_the_exact_column_clicked() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_r67_first_click_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = EditorHarness::new();
    open_docked_unfocused_script(&mut h, &dir, "ab\ncd\n");

    let layout = script_layout(&mut h);
    // Column 1 of line 0 ("ab") — deliberately not column 0, so a gutter-
    // width error of even one character would move the click to the
    // wrong side of a character boundary.
    let px = layout.line_x + 1.0 * layout.char_w + 1.0;
    let py = layout.text.y + 1.0;
    h.click(px, py);

    assert_eq!(
        h.state.script_cursor(),
        (1, 0),
        "the first click into the docked, unfocused script editor must land exactly where `ScriptLayout` (and the draw side) say column 1 of line 0 is"
    );
}

/// R67: the docked panel's first click also had no lower bound excluding
/// the reserved error row — a click there could still resolve to a
/// (nonexistent) buffer line. `ScriptLayout::hit` returns `None` outside
/// its own `text` rect, which already excludes the error row.
#[test]
fn r67_clicking_the_reserved_error_row_does_not_move_the_cursor() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_r67_error_row_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = EditorHarness::new();
    // A script `rhai::Engine::compile` rejects — guarantees a live error
    // row is reserved at the bottom of the panel.
    open_docked_unfocused_script(&mut h, &dir, "fn on_update(id, ctx) {\n");

    let before = h.state.script_cursor();
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let content_rect = h.state.panels().get(PanelId::ScriptEditor).content_rect(&metrics);
    // Just above the panel's own bottom edge — inside the reserved error
    // row once one is showing.
    h.click(content_rect.x + 1.0, content_rect.y + content_rect.h - 1.0);

    assert_eq!(
        before,
        h.state.script_cursor(),
        "clicking the error row must never move the cursor"
    );
}

/// R68 (§3 in the master plan): the gutter is now sized from the buffer's
/// own real line count (`ScriptLayout::compute`), not a fixed `4`-column
/// budget that silently ran out past line 999 — a 1000+ line file's own
/// wheel-scroll ceiling (which depends on how many ROWS actually fit,
/// which in turn depends on the gutter eating into... no, the gutter only
/// affects columns, not rows; this instead pins that `visible_rows`
/// itself — computed once by `ScriptLayout` and shared by draw, the wheel
/// handler, and keep-in-view — lets the wheel reach the buffer's true
/// last line in a file large enough to need the widened gutter).
#[test]
fn r68_the_focused_script_editor_wheel_can_scroll_to_the_last_line_of_a_1000_plus_line_file() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_r68_wide_gutter_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut lines = String::new();
    for i in 0..1005 {
        lines.push_str(&format!("let x{} = {};\n", i, i));
    }
    let mut h = EditorHarness::new();
    open_docked_unfocused_script(&mut h, &dir, &lines);

    let layout = script_layout(&mut h);
    assert_eq!(layout.gutter_digits, 4, "1005 lines must get a 4-digit gutter");

    // Focus the panel first (any click inside it), then scroll far past
    // the end — `handle_script_mode_input`'s own wheel handling, once
    // focused.
    h.click(layout.line_x + 1.0, layout.text.y + 1.0);
    // Each wheel notch moves a fixed 2 rows regardless of magnitude
    // (`handle_script_mode_input`'s own step) — enough iterations to
    // clear the buffer's real `max_scroll` (999) comfortably.
    for _ in 0..600 {
        h.wheel(layout.line_x + 1.0, layout.text.y + 1.0, -2.0);
    }

    let last_line_visible_row = h.state.script_buffer().len() - h.state.script_scroll() - 1;
    assert!(
        last_line_visible_row < layout.visible_rows,
        "scrolling far past the end of a 1005-line file must still bring its last line into view (scroll={}, visible_rows={})",
        h.state.script_scroll(),
        layout.visible_rows
    );
}

/// R73 (§3 in the master plan): selection highlighting used to be a
/// per-TOKEN background decision the syntax highlighter made while
/// drawing each token (checked once, at the token's first character) —
/// a selection boundary landing mid-token highlighted the whole token
/// instead of just the selected characters. Fixed by painting the
/// selection as its own exact per-character background FILL before any
/// text draws. This selects half of the identifier "identifier" (chars
/// 6..10, "enti") and confirms the recorded `Fill` is exactly 4 characters
/// wide — not the whole 10-character token.
#[test]
fn r73_a_selection_starting_mid_token_highlights_only_the_selected_characters() {
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("editor_script_r73_mid_token_selection_repro");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");

    let mut h = EditorHarness::new();
    // "let identifier = 5;" — "identifier" spans chars 4..14; this selects
    // chars 6..10 ("enti"), squarely inside that one token.
    open_docked_unfocused_script(&mut h, &dir, "let identifier = 5;\n");

    let layout = script_layout(&mut h);
    // First click (unfocused): places the cursor at char 6, also focuses
    // the panel so the Shift+Right presses below go through the normal
    // focused selection path.
    h.click(layout.line_x + 6.0 * layout.char_w + 1.0, layout.text.y + 1.0);
    assert_eq!(h.state.script_cursor(), (6, 0));

    h.press_key(Key::LeftShift);
    for _ in 0..4 {
        h.press_key(Key::Right);
        h.release_key(Key::Right);
    }
    h.release_key(Key::LeftShift);
    assert_eq!(h.state.script_cursor(), (10, 0), "Shift+Right x4 must select exactly 4 characters");

    h.start_recording();
    h.frame();

    let expected = ember2d_sim::math::Rect::new(
        layout.line_x + 6.0 * layout.char_w,
        layout.text.y,
        4.0 * layout.char_w,
        layout.row_h,
    );
    let found = h.draw_ops().iter().any(|op| match op {
        DrawOp::Fill(r) => {
            (r.x - expected.x).abs() < 0.5
                && (r.y - expected.y).abs() < 0.5
                && (r.w - expected.w).abs() < 0.5
                && (r.h - expected.h).abs() < 0.5
        }
        _ => false,
    });
    assert!(
        found,
        "expected an exact 4-character-wide selection Fill at {:?}, got: {:?}",
        expected,
        h.draw_ops().iter().filter(|op| matches!(op, DrawOp::Fill(_))).collect::<Vec<_>>()
    );
}
