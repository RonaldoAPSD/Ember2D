// ember2d-editor/tests/editor_theme.rs — 7D-4 (docs/ember2d-master-plan.md
// §5.4): runtime theme listing/switching regression tests, driven through
// `EditorHarness` (tests/common/mod.rs). New file rather than folded into
// `editor_input.rs` — a distinct feature area, same split rationale as
// `editor_script.rs`/`editor_undo.rs` before it.

mod common;

use common::{click_theme_menu_item, open_menu, EditorHarness};
use ember2d::theme::SliceRole;
use ember2d_editor::editor::ui::{theme_menu_entries, MenuEntry, MenuKind};

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
    assert_eq!(h.state.theme().name, "ember-clean", "the real shipped theme must load, not the fallback");
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

    assert_eq!(entries.len(), available.len());
    for name in available {
        assert!(
            entries.iter().any(|e| matches!(e, MenuEntry::DynamicItem { label, .. } if label == name)),
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
    assert_eq!(h.state.theme().name, "ember-clean", "the theme must still resolve after the round trip");
    assert!(h.state.theme().slice(SliceRole::Panel).is_some(), "the reloaded theme must still be the real one");
}
