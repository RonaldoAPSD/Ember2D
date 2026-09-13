// ember2d-editor/tests/editor_theme.rs — 7D-4 (docs/ember2d-master-plan.md
// §5.4): runtime theme listing/switching regression tests, driven through
// `EditorHarness` (tests/common/mod.rs). New file rather than folded into
// `editor_input.rs` — a distinct feature area, same split rationale as
// `editor_script.rs`/`editor_undo.rs` before it.

mod common;

use common::{
    click_theme_menu_item, ensure_workspace_root_cwd, open_menu, select_dock_tab, EditorHarness,
};
use ember2d::renderer::DisplayScale;
use ember2d::theme::SliceRole;
use ember2d_editor::editor::panel::PanelId;
use ember2d_editor::editor::prefs::{EditorPrefs, PrefsStore, UiScaleChoice};
use ember2d_editor::editor::ui::{theme_menu_entries, MenuEntry, MenuKind, WidgetId};
use ember2d_editor::editor::EditorState;

// ── Regression: a test-constructed EditorState silently got Theme::fallback ─

#[test]
fn a_fresh_editor_loads_the_real_shipped_theme_not_the_fallback() {
    // `cargo test` runs this binary with CWD set to `ember2d-editor/`, not
    // the repo root where `themes/ember-clean/` actually lives — before
    // `EditorHarness::new`'s own `ensure_workspace_root_cwd` call was
    // added (7D-4), every test-built `EditorState` silently loaded
    // `Theme::fallback()` (magenta chrome, no slices, the built-in bitmap
    // font) instead, since `Theme::load` never fails outward. This test
    // pins the fix by checking real theme content — the fallback would
    // fail every one of these.
    let h = EditorHarness::new();
    assert_eq!(
        h.state.theme().name,
        "ember-clean",
        "the real shipped theme must load, not the fallback"
    );
    assert!(
        h.state.theme().slice(SliceRole::Panel).is_some(),
        "ember-clean must resolve real 9-slice geometry, not an empty fallback theme"
    );
}

#[test]
fn available_themes_is_never_empty_and_includes_the_shipped_ember_clean_theme() {
    let h = EditorHarness::new();
    assert!(
        h.state.available_themes().iter().any(|n| n == "ember-clean"),
        "the shipped theme must be discovered on disk: {:?}",
        h.state.available_themes()
    );
}

// `list_available_themes`'s own missing-directory fallback is a
// `#[cfg(test)]` unit test inside `theme_loader.rs` instead of here — it's
// `pub(super)`, not reachable from this external integration-test crate,
// and testing it properly needs CWD manipulation that's safer to do as an
// isolated unit test than shared with every other test in this binary.

// ── The View > Theme menu itself ────────────────────────────────────────────

#[test]
fn the_theme_menu_lists_one_entry_per_available_theme() {
    let h = EditorHarness::new();
    let available = h.state.available_themes();
    let entries = theme_menu_entries(available);

    // 7D-3 checkpoint 7 (master plan §5.4): the theme list is followed by a
    // separator and the UI Scale picker (`UiScaleChoice::ALL`), not just
    // one entry per theme.
    assert_eq!(entries.len(), available.len() + 1 + UiScaleChoice::ALL.len());
    for name in available {
        assert!(
            entries
                .iter()
                .any(|e| matches!(e, MenuEntry::DynamicItem { label, .. } if label == name)),
            "theme menu must list {name:?}"
        );
    }
}

#[test]
fn selecting_the_current_theme_from_its_own_menu_round_trips_without_crashing() {
    // Only one theme ships today (`ember-clean`) — this can't yet prove
    // switching between two DIFFERENT themes' content, but it exercises
    // the entire click -> hit-test -> action -> reload pipeline
    // end-to-end: `MenuKind::Theme`'s dynamic entries, `WidgetId::MenuItem`
    // hit resolution against them, `ToolbarAction::SetTheme` extraction,
    // and `EditorState::switch_theme`'s reload. Any break in that chain
    // (wrong entries function, index mismatch, unhandled action) fails
    // this test, not just a checkmark's cosmetic position.
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Theme);
    click_theme_menu_item(&mut h, "ember-clean");

    assert_eq!(h.state.active_menu(), None, "picking a theme must close its dropdown");
    assert_eq!(
        h.state.theme().name,
        "ember-clean",
        "the theme must still resolve after the round trip"
    );
    assert!(
        h.state.theme().slice(SliceRole::Panel).is_some(),
        "the reloaded theme must still be the real one"
    );
}

// ── 7D-3 (docs/ember2d-master-plan.md §5.4): preferences and UI scale ──────

#[test]
fn a_fresh_editor_uses_an_in_memory_prefs_store_with_defaults() {
    // Every `EditorState` built directly (as every test's is) stays on
    // `PrefsStore::InMemory(EditorPrefs::default())` — only
    // `ember2d-app/src/main.rs`'s `.with_prefs(PrefsStore::user())` ever
    // touches the real per-user file. This can't inspect the `PrefsStore`
    // variant directly (no `Debug`/accessor for it), but a mismatch here
    // would mean either the default changed unexpectedly or something
    // outside this process's control leaked in — both worth failing on.
    let h = EditorHarness::new();
    assert_eq!(h.state.prefs(), &EditorPrefs::default());
}

#[test]
fn selecting_a_theme_from_its_menu_persists_it_to_prefs() {
    let mut h = EditorHarness::new();
    open_menu(&mut h, MenuKind::Theme);
    click_theme_menu_item(&mut h, "ember-clean");
    assert_eq!(
        h.state.prefs().theme,
        "ember-clean",
        "picking a theme from the menu must persist it to prefs"
    );
}

#[test]
fn menu_clicks_round_trip_when_ui_scale_genuinely_diverges_from_render_scale() {
    // 7D-3 checkpoint 7 (master plan §5.4): `effective_ui_scale` is un-pinned
    // as of this checkpoint — `UiScaleChoice::Auto.resolve(os_scale_factor)`
    // (`os_scale_factor: 2.0` here resolves to `ui_scale == 4`) no longer
    // has any reason to equal `render_scale` (`2` here). This was
    // `the_harness_at_render_scale_2_still_round_trips_menu_clicks` through
    // checkpoints 2-6, when the two were still deliberately pinned equal —
    // renamed and re-pointed at the real S=4/R=2 divergence this checkpoint
    // makes possible, since `open_menu`/`click_theme_menu_item` (tests/
    // common/mod.rs) converting `rect_of`'s points-space rects to logical
    // before clicking is exactly what this test exists to prove works.
    let mut h = EditorHarness::with_display(DisplayScale { render_scale: 2, os_scale_factor: 2.0 });
    assert_eq!(h.state.ui_space().render_scale(), 2);
    assert_eq!(
        h.state.ui_space().ui_scale(),
        4,
        "Auto at os_scale_factor 2.0 must resolve to ui_scale 4, independent of render_scale"
    );
    open_menu(&mut h, MenuKind::Theme);
    click_theme_menu_item(&mut h, "ember-clean");
    assert_eq!(
        h.state.active_menu(),
        None,
        "picking a theme must close its dropdown even with ui_scale != render_scale"
    );
}

#[test]
fn r70_switching_levels_keeps_the_active_theme_fonts_and_prefs() {
    // R70 (§3 in the master plan): `switch_to_level`'s `*self = ns` used to
    // silently reset the active theme/font AND (once they existed)
    // preferences back to fresh-construction defaults on every level
    // switch. Seeds a non-default `ui_scale` via a real prefs file (only
    // one theme ships, so switching theme itself can't be used to prove
    // this — `ui_scale` can), then confirms it survives a level switch.
    ensure_workspace_root_cwd(); // before EditorState::new, below — see its own doc comment
    let dir = std::env::temp_dir()
        .join(format!("ember2d-{}", std::process::id()))
        .join("r70_switch_level_keeps_prefs");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let other_path = dir.join("other.level");
    ember2d_editor::editor::grid::LevelGrid::new(4, 4)
        .to_level_data()
        .save(other_path.to_str().unwrap())
        .expect("seed level must save");

    let mut store = PrefsStore::File(dir.join("editor_prefs.ron"));
    store
        .save(&EditorPrefs { ui_scale: UiScaleChoice::Fixed(3), theme: "ember-clean".to_string() });

    let current_path = dir.join("current.level").to_string_lossy().into_owned();
    let state = EditorState::new(&current_path).with_prefs(store);
    let mut h = EditorHarness::with_state(state);
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    assert_eq!(
        h.state.prefs().ui_scale,
        UiScaleChoice::Fixed(3),
        "with_prefs must have applied the saved scale before the switch even happens"
    );

    select_dock_tab(&mut h, PanelId::FileBrowser);
    let row_rect = h
        .state
        .ui_frame()
        .rect_of(WidgetId::FileBrowserRow(0))
        .expect("other.level's row was not drawn");
    // 7D-3 checkpoint 7 (master plan §5.4): `rect_of` is points-space, but
    // `click` takes LOGICAL pixels — this test deliberately runs at
    // `ui_scale` (Fixed(3)) != `render_scale` (2, the harness default), so
    // unlike most callers of `rect_of` this one can't skip the conversion.
    let k = h.state.ui_space().pt_to_logical();
    h.click((row_rect.x + 1.0) * k, (row_rect.y + 1.0) * k);

    assert_eq!(h.state.grid().width, 4, "the level switch must have actually happened");
    assert_eq!(
        h.state.prefs().ui_scale,
        UiScaleChoice::Fixed(3),
        "R70: switching levels must not silently reset editor preferences back to defaults"
    );
    assert_eq!(
        h.state.theme().name,
        "ember-clean",
        "R70: the active theme must survive a level switch"
    );
}
