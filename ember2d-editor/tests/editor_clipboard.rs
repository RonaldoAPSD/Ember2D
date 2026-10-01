// ember2d-editor/tests/editor_clipboard.rs — the level grid's copy / cut /
// paste (checklist §5), headless: a two-tile pattern (a wall then an item,
// left to right) is copied and stamped plain, flipped (H) and rotated (]),
// and cut. Added in the Phase 8 gate pass, which found R103 here: in Paste
// mode H flipped the stamp AND hid the Hierarchy panel.

mod common;

use common::{canvas_pixel_for_grid, EditorHarness};
use ember2d::input::Key;
use ember2d_editor::editor::panel::PanelId;

const LAYER: u8 = 1; // the editor's default active layer (Main)

fn click_cell(h: &mut EditorHarness, gx: i32, gy: i32) {
    let (x, y) = canvas_pixel_for_grid(h, gx, gy);
    h.click(x, y);
    h.frame();
}

fn glyph(h: &EditorHarness, gx: i32, gy: i32) -> Option<char> {
    h.state.grid().get(gx, gy, LAYER).map(|t| t.glyph)
}

/// A fresh editor with wall '#' at (1,1) and item '*' at (2,1), both
/// copied (C, drag over them) — ready for V.
fn copied_pair() -> EditorHarness {
    let mut h = EditorHarness::new();
    h.key(Key::Key4); // Wall
    click_cell(&mut h, 1, 1);
    h.key(Key::Key6); // Item
    click_cell(&mut h, 2, 1);
    assert_eq!((glyph(&h, 1, 1), glyph(&h, 2, 1)), (Some('#'), Some('*')));
    h.key(Key::C);
    let from = canvas_pixel_for_grid(&h, 1, 1);
    let to = canvas_pixel_for_grid(&h, 2, 1);
    h.drag(from, to);
    h.frame();
    h
}

#[test]
fn a_copied_pattern_pastes_unchanged() {
    let mut h = copied_pair();
    h.key(Key::V);
    click_cell(&mut h, 5, 5);
    assert_eq!((glyph(&h, 5, 5), glyph(&h, 6, 5)), (Some('#'), Some('*')));
    assert_eq!((glyph(&h, 1, 1), glyph(&h, 2, 1)), (Some('#'), Some('*')), "copy keeps the source");
}

/// R103: H in Paste mode is "flip X" — it must not also toggle the
/// Hierarchy panel (the global H shortcut used to run as well).
#[test]
fn r103_flipping_a_paste_with_h_does_not_hide_the_hierarchy() {
    let mut h = copied_pair();
    assert!(h.state.panels().visible(PanelId::Hierarchy));
    h.key(Key::V);
    h.key(Key::H);
    assert!(h.state.panels().visible(PanelId::Hierarchy), "H flipped the paste only");
    click_cell(&mut h, 5, 5);
    assert_eq!((glyph(&h, 5, 5), glyph(&h, 6, 5)), (Some('*'), Some('#')), "mirrored");
}

#[test]
fn a_rotated_paste_turns_a_row_into_a_column() {
    let mut h = copied_pair();
    h.key(Key::V);
    h.key(Key::RightBracket);
    click_cell(&mut h, 8, 8);
    let stamped: Vec<(i32, i32)> = h
        .state
        .grid()
        .tiles
        .keys()
        .filter(|&&(x, y, l)| l == LAYER && (x, y) != (1, 1) && (x, y) != (2, 1))
        .map(|&(x, y, _)| (x, y))
        .collect();
    assert_eq!(stamped.len(), 2, "{stamped:?}");
    assert_eq!(stamped[0].0, stamped[1].0, "rotated 90°: both in one column, {stamped:?}");
}

#[test]
fn a_cut_removes_the_source_and_pastes_elsewhere_as_one_undo_step_each() {
    let mut h = EditorHarness::new();
    h.key(Key::Key4);
    click_cell(&mut h, 1, 1);
    h.key(Key::Key6);
    click_cell(&mut h, 2, 1);
    let undo_before = h.state.undo_len();
    h.key(Key::X);
    let from = canvas_pixel_for_grid(&h, 1, 1);
    let to = canvas_pixel_for_grid(&h, 2, 1);
    h.drag(from, to);
    h.frame();
    assert_eq!((glyph(&h, 1, 1), glyph(&h, 2, 1)), (None, None), "cut removes the source");
    assert_eq!(h.state.undo_len(), undo_before + 1, "the cut is one undo step");
    h.key(Key::V);
    click_cell(&mut h, 4, 4);
    assert_eq!((glyph(&h, 4, 4), glyph(&h, 5, 4)), (Some('#'), Some('*')));
    h.key(Key::U); // undo the paste
    h.key(Key::U); // undo the cut
    h.frame();
    assert_eq!((glyph(&h, 4, 4), glyph(&h, 1, 1)), (None, Some('#')));
}
