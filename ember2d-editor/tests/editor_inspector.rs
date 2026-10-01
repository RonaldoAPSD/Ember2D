// ember2d-editor/tests/editor_inspector.rs — Step 9-6 (docs/ember2d-master-
// plan.md §5.8): the Inspector fields that used to be settable only by a
// Rust level generator — a tile's colours, its Actor section (speed,
// physics, tints, stats), and the player's colours and collider size —
// driven by real clicks and typing through `EditorHarness`.

mod common;

use common::{canvas_pixel_for_grid, click_menu_item, open_menu, EditorHarness};
use ember2d::input::Key;
use ember2d::renderer::color::Color;
use ember2d_editor::editor::ui::{
    HierarchySelection, InspectorField, MenuKind, ToolbarAction, WidgetId,
};
use ember2d_editor::editor::{EditorMode, TextInputPurpose};

/// Clicks the Inspector row for `field`, scrolling the Inspector down
/// (the mouse wheel over it) until the row is on screen.
fn click_field(h: &mut EditorHarness, field: InspectorField) {
    for _ in 0..30 {
        if let Some(r) = h.state.ui_frame().rect_of(WidgetId::InspectorRow(field)) {
            h.click(r.x + 2.0, r.y + 2.0);
            return;
        }
        let any = h
            .state
            .ui_frame()
            .rect_of(WidgetId::InspectorRow(InspectorField::Glyph))
            .or_else(|| h.state.ui_frame().rect_of(WidgetId::InspectorRow(InspectorField::Layer)))
            .expect("the Inspector shows something to scroll");
        h.wheel(any.x + 2.0, any.y + 2.0, -1.0);
    }
    panic!("Inspector row {field:?} never came into view");
}

/// Types `text` into the open prompt and presses Enter.
fn answer(h: &mut EditorHarness, text: &str) {
    assert!(
        matches!(h.state.mode(), EditorMode::Prompt(TextInputPurpose::Inspector { .. })),
        "an Inspector prompt is open, not {:?}",
        h.state.mode()
    );
    h.type_text(text);
    h.key(Key::Enter);
}

/// A painted, selected tile at (5, 5).
fn selected_tile() -> EditorHarness {
    let mut h = EditorHarness::new();
    let (cx, cy) = canvas_pixel_for_grid(&h, 5, 5);
    h.click(cx, cy);
    open_menu(&mut h, MenuKind::Tools);
    click_menu_item(&mut h, MenuKind::Tools, |a| matches!(a, ToolbarAction::EnterInspect));
    h.click(cx, cy);
    h.frame();
    h
}

#[test]
fn a_tiles_actor_section_is_authored_entirely_in_the_inspector() {
    let mut h = selected_tile();
    let tile = |h: &EditorHarness| h.state.grid().get(5, 5, 1).cloned().unwrap();
    assert!(tile(&h).actor.is_none());

    click_field(&mut h, InspectorField::ActorToggle);
    assert!(tile(&h).actor.is_some(), "the toggle makes the tile an actor");
    h.frame();

    click_field(&mut h, InspectorField::ActorSpeed);
    // The prompt starts with the current value; clear it first.
    for _ in 0..4 {
        h.key(Key::Backspace);
    }
    answer(&mut h, "150");
    h.frame();
    click_field(&mut h, InspectorField::ActorStatAdd);
    answer(&mut h, "hp=6");
    h.frame();
    click_field(&mut h, InspectorField::ActorStatAdd);
    answer(&mut h, "atk = 2");
    h.frame();
    click_field(&mut h, InspectorField::TintAware);
    for _ in 0..12 {
        h.key(Key::Backspace);
    }
    answer(&mut h, "Red");
    h.frame();

    let a = tile(&h).actor.unwrap();
    assert_eq!(a.speed, 150);
    assert_eq!(a.stats.get("hp"), Some(&6.0));
    assert_eq!(a.stats.get("atk"), Some(&2.0));
    assert_eq!(a.tint_aware, Color::Red);

    // Stat 0 in key order is `atk`: rename it and change it in one go.
    click_field(&mut h, InspectorField::ActorStat(0));
    for _ in 0..12 {
        h.key(Key::Backspace);
    }
    answer(&mut h, "power=3");
    h.frame();
    let a = tile(&h).actor.unwrap();
    assert_eq!((a.stats.get("atk"), a.stats.get("power")), (None, Some(&3.0)));

    // Nonsense changes nothing and says so.
    let undo_before = h.state.undo_len();
    click_field(&mut h, InspectorField::ActorStatAdd);
    answer(&mut h, "hp=lots");
    assert_eq!(h.state.undo_len(), undo_before, "a rejected value records no undo step");
    assert_eq!(tile(&h).actor.unwrap().stats.get("hp"), Some(&6.0));
}

#[test]
fn a_tiles_colour_change_is_one_undo_step() {
    let mut h = selected_tile();
    let before = h.state.grid().get(5, 5, 1).unwrap().fg;
    let undo_before = h.state.undo_len();
    click_field(&mut h, InspectorField::Fg);
    for _ in 0..12 {
        h.key(Key::Backspace);
    }
    answer(&mut h, "#ff8800");
    assert_eq!(h.state.grid().get(5, 5, 1).unwrap().fg, Color::Rgb(255, 136, 0));
    assert_eq!(h.state.undo_len(), undo_before + 1);
    h.key(Key::U);
    assert_eq!(h.state.grid().get(5, 5, 1).unwrap().fg, before, "undo restores the colour");
}

#[test]
fn the_players_colours_and_collider_size_are_editable() {
    let mut h = EditorHarness::new();
    h.frame();
    let row = h
        .state
        .ui_frame()
        .rect_of(WidgetId::HierarchyRow(HierarchySelection::Player))
        .expect("the Hierarchy lists the player");
    h.click(row.x + 2.0, row.y + 2.0);
    h.frame();

    click_field(&mut h, InspectorField::ColliderSize);
    for _ in 0..12 {
        h.key(Key::Backspace);
    }
    answer(&mut h, "0.7,0.7");
    h.frame();
    click_field(&mut h, InspectorField::Fg);
    for _ in 0..12 {
        h.key(Key::Backspace);
    }
    answer(&mut h, "Cyan");

    let p = &h.state.grid().player;
    assert_eq!((p.collider_w, p.collider_h), (0.7, 0.7));
    assert_eq!(p.fg, Color::Cyan);
}
