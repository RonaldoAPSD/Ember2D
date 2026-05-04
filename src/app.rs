// app.rs — Top-level application loop: manages Editor ↔ Play mode transitions.
//
// ── WHY THIS FILE EXISTS ──────────────────────────────────────────────────────
//
// The Engine's `run()` method handles ONE GameState until quit. But we want to
// switch between two GameStates at runtime (editor ↔ play). That switching logic
// lives here so neither GameState nor the Engine needs to know about the other.
//
// ── THE LOOP ─────────────────────────────────────────────────────────────────
//
//   run_editor_app():
//     loop {
//         run editor until F5 or quit
//         if F5 → reset world, run play until Escape or quit
//         if Escape → reset world, loop back to editor
//         if quit from either → exit
//     }
//
// The editor's state (grid, palette, undo history) is kept alive on the stack
// across the play session. When the player presses Escape in play mode and
// we loop back, the editor picks up exactly where it left off.
//
// ── DIRECT PLAY ──────────────────────────────────────────────────────────────
//
// run_play_app() skips the editor entirely — used when launched with a
// .level file argument: `cargo run -- mymap.level`

use std::io;

use crate::editor::EditorState;
use crate::engine::{Engine, Transition};
use crate::level::LevelData;
use crate::play::PlayState;
use crate::scripting::LogEntry;

// ── Editor ↔ Play loop ────────────────────────────────────────────────────────

/// Run the editor with full editor ↔ play-mode switching.
///
/// - F5 in the editor: transitions to play mode with the current grid.
/// - Escape in play mode: returns to the editor (state preserved).
/// - Escape in the editor (or closing the window): exits the app.
pub fn run_editor_app(engine: &mut Engine, mut editor: EditorState) -> io::Result<()> {
    loop {
        // ── Run the editor ─────────────────────────────────────────────────
        match engine.run_until_transition(&mut editor)? {
            None => {
                // Editor exited normally (Escape or window close). Done.
                break;
            }
            Some(Transition::ToPlay(mut level_data)) => {
                // ── F5 pressed — switch to play mode ───────────────────────
                // Loop so that level-exit transitions stay in play mode
                // instead of bouncing back through the editor.
                let mut accumulated_log: Vec<LogEntry> = Vec::new();
                let window_closed = loop {
                    engine.reset_world();
                    let mut play = PlayState::from_level(level_data);
                    match engine.run_until_transition(&mut play)? {
                        None => break true,
                        Some(Transition::ToEditor) => {
                            accumulated_log.extend(play.take_log());
                            break false;
                        }
                        Some(Transition::ToPlay(next_data)) => {
                            accumulated_log.extend(play.take_log());
                            level_data = next_data;
                        }
                    }
                };
                engine.reset_world();
                if !accumulated_log.is_empty() { editor.receive_log(accumulated_log); }
                if window_closed { break; }
                // outer loop continues back to editor
            }
            _ => break, // unexpected transition from editor
        }
    }

    Ok(())
}

// ── Direct play (no editor) ───────────────────────────────────────────────────

/// Run play mode directly from a LevelData without opening the editor.
///
/// Used when the user launches with a .level file:
///   `cargo run -- mymap.level`
///
/// In this mode, Escape quits the app (there is no editor to return to).
/// PlayState::update() sets Transition::ToEditor on Escape; we treat that as
/// a normal exit here since there is no editor to go back to.
pub fn run_play_app(engine: &mut Engine, mut data: LevelData) -> io::Result<()> {
    loop {
        let mut play = PlayState::from_level(data);
        match engine.run_until_transition(&mut play)? {
            Some(Transition::ToPlay(next)) => {
                engine.reset_world();
                data = next;
            }
            _ => return Ok(()),
        }
    }
}
