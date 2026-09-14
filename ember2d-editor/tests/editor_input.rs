// ember2d-editor/tests/editor_input.rs — headless editor input regression
// tests (7C-5, docs/ember2d-master-plan.md §5.3): the baseline harness
// check, menu bar/dropdown, docked panels/focus, text capture (R11/R12),
// and other `EditorMode` transitions. Driven through `EditorHarness`
// (tests/common/mod.rs) — see that module's own header comment for how one
// simulated frame works, and each test's own comment for which defect (if
// any) it pins. The two "repro" file-on-disk areas moved to
// editor_input_files.rs, and 7D-3's right-click/wheel-scroll area to
// editor_input_panels.rs, at R87 (docs/ember2d-master-plan.md §3.2 — this
// file was 761 real lines, over CLAUDE.md's 750-line limit, pushed there
// by the one-time `cargo fmt --all` pass) — pure file splits, no test
// content changed; see those files' own header comments.

mod common;

use common::{canvas_center, canvas_pixel_for_grid, click_menu_item, open_menu, EditorHarness};
use ember2d::input::Key;
use ember2d_editor::editor::ui::{ChromeMetrics, MenuKind, ToolKind, ToolbarAction, WidgetId};
use ember2d_editor::editor::{EditorMode, TextInputPurpose};

// ── Baseline: the harness itself behaves like a fresh editor ────────────────

#[test]
fn a_fresh_harness_starts_in_paint_mode_with_an_empty_grid() {
    let h = EditorHarness::new();
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)));
    assert_eq!(h.state.grid().tiles.len(), 0);
}

#[test]
fn clicking_on_canvas_with_the_paint_tool_places_a_tile() {
    let mut h = EditorHarness::new();
    let (cx, cy) = canvas_pixel_for_grid(&h, 5, 5);
    h.click(cx, cy);
    assert_eq!(
        h.state.grid().tiles.len(),
        1,
        "a left-click on canvas with Paint active must place exactly one tile"
    );
    assert!(h.state.unsaved());
}

// ── Menu bar / dropdown ──────────────────────────────────────────────────────

#[test]
fn a_menu_label_click_opens_its_dropdown_and_a_second_click_on_it_closes_it() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::View);
    // Clicking the same label again toggles it closed (handle_menu_bar_click).
    let rect = h.state.ui_frame().rect_of(WidgetId::MenuLabel(MenuKind::View)).unwrap();
    h.click(rect.x + 1.0, rect.y + 1.0);
    assert_eq!(h.state.active_menu(), None);
}

#[test]
fn escape_closes_an_open_dropdown() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::File);
    h.key(Key::Escape);
    assert_eq!(h.state.active_menu(), None, "Escape must dismiss an open dropdown");
}

#[test]
fn a_menu_click_does_not_also_paint_the_canvas_underneath_it() {
    let mut h = EditorHarness::new();
    // Open a dropdown, then click on-canvas — this click is entirely
    // consumed by dismissing the dropdown (`handle_menu_dropdown_click`
    // claims every left-click while a menu is open, wherever it lands),
    // and must not ALSO reach the Paint tool's own click handling
    // underneath it (the painting guard CLAUDE.md's own "Painting guard"
    // rule requires).
    open_menu(&mut h, MenuKind::File);
    let (cx, cy) = canvas_center(&h);
    h.click(cx, cy);
    assert_eq!(h.state.active_menu(), None, "the click should have dismissed the dropdown");
    assert_eq!(h.state.grid().tiles.len(), 0, "the same click must not also have painted a tile");
}

#[test]
fn selecting_rect_from_the_tools_menu_switches_the_active_tool() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Tools);
    click_menu_item(&mut h, MenuKind::Tools, |a| {
        matches!(a, ToolbarAction::SetTool(ToolKind::Rect))
    });
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Rect)));
}

#[test]
fn r13_dismissing_a_dropdown_over_the_canvas_does_not_also_arm_a_rect_anchor() {
    // R13 (7A-2, docs/ember2d-master-plan.md): Rect/Line/Fill used to skip
    // the `ignore_drag` guard every other paint path already had, so a
    // click that dismissed a menu (setting `ignore_drag` for exactly this
    // reason) could also drop a rect anchor on the canvas underneath it.
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Tools);
    click_menu_item(&mut h, MenuKind::Tools, |a| {
        matches!(a, ToolbarAction::SetTool(ToolKind::Rect))
    });
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Rect)));

    open_menu(&mut h, MenuKind::File);
    let (cx, cy) = canvas_center(&h);
    h.click(cx, cy);

    assert_eq!(h.state.active_menu(), None, "the click should have dismissed the dropdown");
    assert!(
        h.state.rect_anchor().is_none(),
        "a click that only dismissed a dropdown must not also arm a Rect-tool anchor on the canvas underneath it"
    );
    assert_eq!(h.state.grid().tiles.len(), 0, "no rect was ever stamped, so no tile should exist");
}

#[test]
fn selecting_fill_from_the_tools_menu_then_dismissing_a_dropdown_does_not_flood_fill() {
    // Same R13 class of bug as the Rect test above, for the Fill tool.
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Tools);
    click_menu_item(&mut h, MenuKind::Tools, |a| {
        matches!(a, ToolbarAction::SetTool(ToolKind::Fill))
    });
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Fill)));

    open_menu(&mut h, MenuKind::Edit);
    let (cx, cy) = canvas_center(&h);
    h.click(cx, cy);

    assert_eq!(h.state.active_menu(), None);
    assert_eq!(
        h.state.grid().tiles.len(),
        0,
        "dismissing a dropdown over the canvas must not trigger a flood fill"
    );
}

// ── Docked panels and focus ──────────────────────────────────────────────────

#[test]
fn hiding_a_docked_panel_grows_the_viewport_to_fill_the_remainder() {
    let mut h = EditorHarness::new();
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let before = h.state.panels().viewport().content_rect(&metrics);
    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::ToggleHierarchy));
    let after = h.state.panels().viewport().content_rect(&metrics);
    assert!(
        after.w > before.w,
        "hiding the (left-docked) Hierarchy panel must widen the viewport into the space it left, got {before:?} -> {after:?}"
    );
}

#[test]
fn r14_pressing_a_shortcut_key_while_the_docked_script_panel_is_focused_does_not_fire_it() {
    // R14 (7A-2, docs/ember2d-master-plan.md): the docked script panel
    // having focus must swallow global shortcuts the same way fullscreen
    // `EditorMode::Script` does — before the fix, typing while the panel
    // merely had focus fired shortcuts instead (S saved, G toggled the
    // physics overlay, etc.).
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::ToggleScriptEditor));
    assert!(h.state.panels().visible(ember2d_editor::editor::panel::PanelId::ScriptEditor));

    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let script_panel_rect = h
        .state
        .panels()
        .get(ember2d_editor::editor::panel::PanelId::ScriptEditor)
        .content_rect(&metrics);
    h.click(script_panel_rect.x + 1.0, script_panel_rect.y + 1.0);
    assert_eq!(
        h.state.focused_panel(),
        Some(ember2d_editor::editor::panel::PanelId::ScriptEditor),
        "clicking inside the docked script panel must focus it"
    );
    assert!(!h.state.focus_is_canvas(), "focus must have left the canvas");
    assert!(
        matches!(h.state.mode(), EditorMode::Paint(_)),
        "docked focus must not itself change `mode`"
    );

    let physics_before = h.state.show_physics();
    h.key(Key::G); // View > Physics's shortcut when focus IS on canvas
    assert_eq!(
        h.state.show_physics(),
        physics_before,
        "a global shortcut must not fire while the docked script panel has focus"
    );
}

#[test]
fn clicking_outside_the_focused_docked_script_panel_returns_focus_to_the_canvas() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::View);
    click_menu_item(&mut h, MenuKind::View, |a| matches!(a, ToolbarAction::ToggleScriptEditor));
    let metrics = ChromeMetrics::from_theme(h.state.theme());
    let script_panel_rect = h
        .state
        .panels()
        .get(ember2d_editor::editor::panel::PanelId::ScriptEditor)
        .content_rect(&metrics);
    h.click(script_panel_rect.x + 1.0, script_panel_rect.y + 1.0);
    assert!(!h.state.focus_is_canvas());

    let (cx, cy) = canvas_center(&h);
    h.click(cx, cy);
    assert!(h.state.focus_is_canvas(), "a click outside the docked panel's own bounds must fall through and return focus to the canvas");
}

// ── Text capture (R11/R12) ───────────────────────────────────────────────────

#[test]
fn r12_stray_keystrokes_with_nothing_focused_do_not_flood_the_next_prompt() {
    // R12 (7A-2, docs/ember2d-master-plan.md): before `begin_text_capture`/
    // `finish_frame_text_capture` existed, every printable key typed with
    // no widget capturing it silently piled up in `text_buffer` until
    // some prompt next opened, which then received the whole backlog in
    // one `take_text()` call.
    let mut h = EditorHarness::new();
    h.inject_raw_text("stray");
    h.frame(); // nothing this frame requested capture — the buffer must be wiped

    h.press_key(Key::LeftShift);
    h.press_key(Key::S); // Shift+S: Save As prompt
    h.release_key(Key::S);
    h.release_key(Key::LeftShift);
    assert!(matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::SaveAs)));

    // One settling frame so the prompt's own field can arm text capture
    // for the first time (see `EditorHarness::type_text`'s doc comment).
    h.frame();
    assert!(
        !h.state.prompt_buffer().contains("stray"),
        "keystrokes typed before the prompt ever opened must not leak into it: got {:?}",
        h.state.prompt_buffer()
    );
}

#[test]
fn typing_into_the_save_as_prompt_appends_to_its_buffer() {
    let mut h = EditorHarness::new();
    h.press_key(Key::LeftShift);
    h.press_key(Key::S);
    h.release_key(Key::S);
    h.release_key(Key::LeftShift);
    assert!(matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::SaveAs)));

    let before = h.state.prompt_buffer().to_string();
    h.type_text("_v2");
    assert_eq!(h.state.prompt_buffer(), format!("{before}_v2"));
}

#[test]
fn r11_non_ascii_text_in_a_prompt_does_not_panic_and_round_trips() {
    // R11 (7A-2, docs/ember2d-master-plan.md): non-ASCII text used to
    // panic editor-side text handling via byte-indexed cursor math. This
    // test passing at all (no panic unwinds the test) is most of the
    // assertion; the content check confirms it wasn't silently mangled.
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::RenameLevel));
    assert!(matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::LevelName)));

    h.type_text("café 🎮");
    assert!(h.state.prompt_buffer().ends_with("café 🎮"));
}

#[test]
fn backspace_removes_one_character_from_a_prompt() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::RenameLevel));
    let seeded = h.state.prompt_buffer().to_string();
    h.type_text("X");
    h.key(Key::Backspace);
    assert_eq!(h.state.prompt_buffer(), seeded);
}

#[test]
fn escape_cancels_a_prompt_without_committing_it() {
    let mut h = EditorHarness::new();
    let name_before = h.state.grid().name.clone();
    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::RenameLevel));
    h.type_text("Renamed");
    h.key(Key::Escape);
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)));
    assert_eq!(h.state.grid().name, name_before, "Escape must not commit the typed rename");
}

#[test]
fn enter_commits_a_level_rename_prompt() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Level);
    click_menu_item(&mut h, MenuKind::Level, |a| matches!(a, ToolbarAction::RenameLevel));
    // RenameLevel seeds `prompt_buffer` with the level's current name
    // (`self.prompt_buffer = self.grid.name.clone()`), so typed text
    // appends rather than replacing it.
    let seeded = h.state.prompt_buffer().to_string();
    h.type_text("Renamed");
    h.key(Key::Enter);
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)));
    assert_eq!(h.state.grid().name, format!("{seeded}Renamed"));
}

// ── Other EditorMode transitions ─────────────────────────────────────────────

#[test]
fn q_toggles_inspect_mode() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Tools);
    click_menu_item(&mut h, MenuKind::Tools, |a| matches!(a, ToolbarAction::EnterInspect));
    assert!(matches!(h.state.mode(), EditorMode::Inspect));
}

#[test]
fn entering_copy_select_then_escape_returns_to_paint() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Edit);
    click_menu_item(&mut h, MenuKind::Edit, |a| matches!(a, ToolbarAction::EnterCopy));
    assert!(matches!(h.state.mode(), EditorMode::Select { cutting: false, .. }));
    h.key(Key::Escape);
    assert!(matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)));
}

#[test]
fn a_copy_select_drag_selects_a_region_and_returns_to_paint_on_release() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Edit);
    click_menu_item(&mut h, MenuKind::Edit, |a| matches!(a, ToolbarAction::EnterCopy));
    let (cx, cy) = canvas_center(&h);
    h.drag((cx, cy), (cx + 16.0, cy + 16.0));
    assert!(
        matches!(h.state.mode(), EditorMode::Paint(ToolKind::Paint)),
        "releasing a copy-select drag must return to Paint"
    );
}

#[test]
fn tab_toggles_the_grid_overlay() {
    let mut h = EditorHarness::new();
    let before = h.state.show_grid();
    h.key(Key::Tab);
    assert_eq!(h.state.show_grid(), !before);
}

#[test]
fn layer_number_keys_switch_the_active_layer() {
    let mut h = EditorHarness::new();
    h.key(Key::Key1);
    assert_eq!(h.state.active_layer(), 0);
    h.key(Key::Key3);
    assert_eq!(h.state.active_layer(), 2);
    h.key(Key::Key2);
    assert_eq!(h.state.active_layer(), 1);
}

#[test]
fn undo_reverts_a_placed_tile() {
    let mut h = EditorHarness::new();
    let (cx, cy) = canvas_pixel_for_grid(&h, 5, 5);
    h.click(cx, cy);
    assert_eq!(h.state.grid().tiles.len(), 1);
    h.press_key(Key::LeftCtrl);
    h.press_key(Key::Z);
    h.release_key(Key::Z);
    h.release_key(Key::LeftCtrl);
    assert_eq!(h.state.grid().tiles.len(), 0, "Ctrl+Z must undo the placed tile");
}

