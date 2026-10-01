// ember2d-editor/tests/editor_project.rs — Step 9-6 (docs/ember2d-master-
// plan.md §5.8): what a project needs that used to mean leaving the editor
// — File > Project Settings (writes `project.ron`), File > New Scene
// (`scenes/<name>.rhai` from a template) and New Script into a folder that
// doesn't exist yet — driven through `EditorHarness`.

mod common;

use common::{click_menu_item, open_menu, EditorHarness};
use ember2d::input::Key;
use ember2d::project::{GameplayLoop, ProjectData, VisualStyle};
use ember2d_editor::editor::project_settings::ProjectField;
use ember2d_editor::editor::ui::{MenuKind, ToolbarAction, WidgetId};
use ember2d_editor::editor::EditorMode;
use std::path::PathBuf;

/// A fresh project folder with a `project.ron` and one level, open in the
/// editor.
fn project(tag: &str) -> (EditorHarness, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    ProjectData::new("Proj", VisualStyle::ClassicASCII, GameplayLoop::RealTime)
        .save(&dir.to_string_lossy())
        .unwrap();
    let level = dir.join("main.level").to_string_lossy().into_owned();
    let mut h = EditorHarness::with_state(ember2d_editor::editor::EditorState::new(&level));
    h.state.open_project_folder(dir.to_string_lossy().into_owned());
    h.frame();
    (h, dir)
}

fn saved(dir: &std::path::Path) -> ProjectData {
    ProjectData::load(&dir.to_string_lossy()).expect("project.ron is still readable")
}

#[test]
fn project_settings_are_edited_and_saved_from_the_file_menu() {
    let (mut h, dir) = project("settings96");
    open_menu(&mut h, MenuKind::File);
    click_menu_item(&mut h, MenuKind::File, |a| matches!(a, ToolbarAction::ProjectSettings));
    assert!(matches!(h.state.mode(), EditorMode::ProjectSettings));
    h.frame();

    let row = |h: &EditorHarness, f: ProjectField| {
        h.state.ui_frame().rect_of(WidgetId::ProjectSettingsRow(f)).expect("row drawn")
    };
    let r = row(&h, ProjectField::GameplayLoop);
    h.click(r.x + 2.0, r.y + 2.0);
    assert_eq!(saved(&dir).gameplay_loop, GameplayLoop::TurnBased, "a choice row cycles and saves");

    h.frame();
    let r = row(&h, ProjectField::WorldCell);
    h.click(r.x + 2.0, r.y + 2.0);
    assert!(matches!(h.state.mode(), EditorMode::Prompt(_)));
    for _ in 0..10 {
        h.key(Key::Backspace);
    }
    h.type_text("16,16");
    h.key(Key::Enter);
    assert!(matches!(h.state.mode(), EditorMode::ProjectSettings), "a prompt returns to the dialog");
    assert_eq!(saved(&dir).world_cell, (16, 16));

    h.key(Key::Escape);
    assert!(matches!(h.state.mode(), EditorMode::Paint(_)));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn new_scene_writes_a_working_template_into_scenes() {
    let (mut h, dir) = project("scene96");
    open_menu(&mut h, MenuKind::File);
    click_menu_item(&mut h, MenuKind::File, |a| matches!(a, ToolbarAction::NewScene));
    h.type_text("inventory");
    h.key(Key::Enter);
    let path = dir.join("scenes").join("inventory.rhai");
    let text = std::fs::read_to_string(&path).expect("scenes/inventory.rhai was created");
    assert!(text.contains("pop_scene"), "{text}");
    rhai::Engine::new().compile(&text).expect("the template compiles");

    // It opened in the script editor; leave it.
    h.key(Key::Escape);
    h.frame();
    // A second New Scene with the same name never overwrites.
    std::fs::write(&path, "// mine").unwrap();
    open_menu(&mut h, MenuKind::File);
    click_menu_item(&mut h, MenuKind::File, |a| matches!(a, ToolbarAction::NewScene));
    h.type_text("inventory");
    h.key(Key::Enter);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "// mine");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn r114_new_script_creates_the_folder_it_names() {
    let (mut h, dir) = project("script96");
    open_menu(&mut h, MenuKind::File);
    click_menu_item(&mut h, MenuKind::File, |a| matches!(a, ToolbarAction::NewScript));
    h.type_text("scripts/ai");
    h.key(Key::Enter);
    assert!(dir.join("scripts").join("ai.rhai").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
