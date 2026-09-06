// editor/impl_state/tests.rs — split out of impl_state/mod.rs in Phase 7
// Part 1f (docs/ember2d-phase7-plan.md) purely to keep mod.rs under
// CLAUDE.md's 600-line hard limit; no behavioral change from being its own
// file. `super::*` reaches `impl_state/mod.rs`'s own imports (`EditorState`,
// `Command`, `node_graph`, etc.) exactly as it did as a nested `mod tests`.

use super::*;
use ember2d_sim::level::TileRecord;
use ember2d::renderer::color::Color;

/// Step 3d's "done when": a level round-trip where a tile carries a live
/// node-graph confirms `graph` never reaches the saved `.level` output
/// and the sidecar `.rhai` file actually contains the generated code.
#[test]
fn graph_migrates_to_a_sidecar_script_and_never_appears_in_saved_output() {
    let dir = std::env::temp_dir().join("ember2d_test_graph_sidecar");
    std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
    let level_path = dir.join("test_level.level");
    let sidecar_path = dir.join("test_level_graph_5_5_1.rhai");
    let _ = std::fs::remove_file(&level_path);
    let _ = std::fs::remove_file(&sidecar_path);

    let mut editor = EditorState::new(level_path.to_str().unwrap());

    let mut graph = node_graph::NodeGraph::default();
    let on_start = graph.add_node(node_graph::NodeKind::OnStart, 0, 0);
    let log_node = graph.add_node(node_graph::NodeKind::Log, 100, 0);
    let str_lit  = graph.add_node(node_graph::NodeKind::StringLit { value: "hello from graph".to_string() }, 100, 50);
    graph.add_edge(on_start, 0, log_node, 0); // OnStart.Out -> Log.In
    graph.add_edge(str_lit, 0, log_node, 1);  // StringLit.Value -> Log.Msg

    let mut tile = TileRecord::new(5, 5, 1, '#', Color::White, Color::Reset, false, false, "npc");
    tile.graph = Some(graph);
    editor.grid.place(5, 5, 1, tile);

    editor.save();

    let saved = std::fs::read_to_string(&level_path).expect("level file must be written");
    assert!(!saved.contains("graph:"), "a saved level must never carry a `graph` field");
    assert!(saved.contains("test_level_graph_5_5_1.rhai"), "the tile's script field must point at the sidecar");

    let sidecar = std::fs::read_to_string(&sidecar_path).expect("sidecar script must be written");
    assert!(sidecar.contains("hello from graph"), "the sidecar must contain the generated Rhai source");

    // Mutating `to_level_data()`'s own clone must never touch the live
    // grid — the node-graph editor keeps working on the real graph.
    assert!(editor.grid.get(5, 5, 1).unwrap().graph.is_some());

    let _ = std::fs::remove_file(&level_path);
    let _ = std::fs::remove_file(&sidecar_path);
}

// ── Undo batching (Phase 7 Part 1f, docs/ember2d-phase7-plan.md) ───────
//
// Every multi-cell editor operation must collapse into exactly one
// `Command::Batch` push, not one push per cell — otherwise a single
// rect fill would take N presses of Ctrl+Z to undo instead of one.
// These pin that property directly against the `EditorState` methods
// the input layer calls (`stamp_rect`/`stamp_line`/`flood_fill`/
// `stamp_paste`/`erase_brush`), without needing real mouse input.

#[test]
fn rect_fill_batches_every_stamped_cell_into_one_undo_step() {
    let mut editor = EditorState::new("unused.level");
    let lyr = editor.active_layer;

    editor.stamp_rect((0, 0), (2, 2)); // 3x3 = 9 cells
    assert_eq!(editor.undo.len(), 1, "a rect fill must be exactly one undo step regardless of cell count");
    for gy in 0..=2 {
        for gx in 0..=2 {
            assert!(editor.grid.get(gx, gy, lyr).is_some());
        }
    }

    let cmd = editor.undo.pop_undo().expect("the rect fill must have pushed a command");
    match cmd {
        Command::Batch { cells } => assert_eq!(cells.len(), 9, "the batch must record all 9 stamped cells"),
        other => panic!("expected Command::Batch, got {:?}", other),
    }
}

#[test]
fn line_draw_batches_every_stamped_cell_into_one_undo_step() {
    let mut editor = EditorState::new("unused.level");
    let lyr = editor.active_layer;

    editor.stamp_line((0, 0), (3, 3)); // bresenham diagonal, 4 cells
    assert_eq!(editor.undo.len(), 1, "a line draw must be exactly one undo step regardless of length");
    for i in 0..=3 {
        assert!(editor.grid.get(i, i, lyr).is_some());
    }
}

#[test]
fn flood_fill_batches_the_whole_enclosed_region_into_one_undo_step() {
    let mut editor = EditorState::new("unused.level");
    let lyr = editor.active_layer;

    // A 5x5 wall ring enclosing a 3x3 empty interior — flood_fill's own
    // BFS must stop at the walls, so this is a small, deterministic
    // region rather than flooding the entire (mostly empty) default
    // grid.
    let wall = editor.palette.current().to_tile_record(0, 0); // "Wall"
    for gy in 0..=4 {
        for gx in 0..=4 {
            if gx == 0 || gx == 4 || gy == 0 || gy == 4 {
                editor.grid.place(gx, gy, lyr, wall.clone());
            }
        }
    }
    assert_eq!(editor.undo.len(), 0, "placing the wall ring directly through LevelGrid must not itself touch the undo stack");

    editor.flood_fill(2, 2); // center of the enclosed interior
    assert_eq!(editor.undo.len(), 1, "a flood fill must be exactly one undo step regardless of area");

    let cmd = editor.undo.pop_undo().expect("the flood fill must have pushed a command");
    match cmd {
        Command::Batch { cells } => assert_eq!(cells.len(), 9, "must fill exactly the 3x3 enclosed interior, no more"),
        other => panic!("expected Command::Batch, got {:?}", other),
    }
}

#[test]
fn paste_batches_the_whole_clipboard_into_one_undo_step() {
    let mut editor = EditorState::new("unused.level");
    let lyr = editor.active_layer;

    let wall = editor.palette.current().to_tile_record(0, 0);
    for gy in 0..=1 {
        for gx in 0..=1 {
            editor.grid.place(gx, gy, lyr, wall.clone());
        }
    }
    editor.copy_selection((0, 0), (1, 1)); // 2x2 = 4 tiles into the clipboard
    assert_eq!(editor.clipboard.len(), 4);
    assert_eq!(editor.undo.len(), 0, "copying a selection must not itself touch the undo stack");

    editor.stamp_paste((10, 10));
    assert_eq!(editor.undo.len(), 1, "a paste must be exactly one undo step regardless of clipboard size");
    for gy in 10..=11 {
        for gx in 10..=11 {
            assert!(editor.grid.get(gx, gy, lyr).is_some());
        }
    }
}

#[test]
fn multi_erase_batches_every_erased_cell_into_one_undo_step() {
    let mut editor = EditorState::new("unused.level");
    let lyr = editor.active_layer;

    let wall = editor.palette.current().to_tile_record(0, 0);
    for gy in 5..=7 {
        for gx in 5..=7 {
            editor.grid.place(gx, gy, lyr, wall.clone());
        }
    }
    editor.erase_size = 3; // half=1 -> a 3x3 brush

    editor.erase_brush(6, 6);
    assert_eq!(editor.undo.len(), 1, "a multi-cell erase (erase_size > 1) must be exactly one undo step");
    for gy in 5..=7 {
        for gx in 5..=7 {
            assert!(editor.grid.get(gx, gy, lyr).is_none());
        }
    }

    let cmd = editor.undo.pop_undo().expect("the erase brush must have pushed a command");
    match cmd {
        Command::Batch { cells } => assert_eq!(cells.len(), 9, "the batch must record all 9 erased cells"),
        other => panic!("expected Command::Batch (erase_size > 1 never pushes a single EraseTile), got {:?}", other),
    }
}

#[test]
fn a_single_cell_erase_still_pushes_the_older_single_tile_command() {
    // erase_size == 1 is the pre-existing single-tile path (`Command::
    // EraseTile`), kept distinct from the `erase_size > 1` batch path
    // above — pinning that the size-1 case wasn't accidentally folded
    // into a one-cell `Batch` (a behavior change `Command::EraseTile`'s
    // own callers don't expect).
    let mut editor = EditorState::new("unused.level");
    let lyr = editor.active_layer;
    let wall = editor.palette.current().to_tile_record(5, 5);
    editor.grid.place(5, 5, lyr, wall);
    editor.erase_size = 1;

    editor.erase_brush(5, 5);
    assert_eq!(editor.undo.len(), 1);
    match editor.undo.pop_undo().unwrap() {
        Command::EraseTile { .. } => {}
        other => panic!("expected Command::EraseTile for erase_size == 1, got {:?}", other),
    }
}

#[test]
fn redo_stack_clears_after_a_new_edit() {
    let mut editor = EditorState::new("unused.level");

    editor.stamp_rect((0, 0), (1, 1));
    let cmd = editor.undo.pop_undo().unwrap();
    editor.reverse_command(&cmd);
    assert_eq!(editor.undo.redo_len(), 1, "undoing must move the command onto the redo stack");

    editor.stamp_rect((5, 5), (6, 6)); // a new edit
    assert_eq!(editor.undo.redo_len(), 0, "a new edit must clear the redo stack, matching standard linear undo history");
}
