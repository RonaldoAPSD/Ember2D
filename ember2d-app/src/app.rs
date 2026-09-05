// app.rs — Top-level application loop: manages Editor ↔ Play mode transitions.

use std::io;

use ember2d::prelude::*;
use ember2d_editor::prelude::EditorState;

/// Run the editor, switching into play mode and back as the user requests.
///
/// `play_gameplay_loop` is the project's target loop model (RealTime or
/// TurnBased) for actual gameplay. The editor itself has no time model of
/// its own (D6 — it must not inherit the project's setting) and always runs
/// realtime; `play_gameplay_loop` only takes effect for the stretches where
/// a `PlayState` is on top of the stack. `pixels_per_unit` is the project's
/// natural-texture-sizing setting (Step 3b) — threaded through the same way
/// for the same reason: `PlayState::from_level` takes only a `LevelData`,
/// not a `ProjectData`.
pub fn run_editor_app(engine: &mut Engine, editor: EditorState, play_gameplay_loop: GameplayLoop, pixels_per_unit: f32) -> io::Result<bool> {
    engine.gameplay_loop = GameplayLoop::RealTime;
    engine.push_state(Box::new(editor));

    while let Some(transition) = engine.run()? {
        match transition {
            Transition::ToPlay(mut level_data) => {
                // ── Switch to play mode ────────────────────────────────────
                engine.gameplay_loop = play_gameplay_loop;
                let mut pending_save: Option<SaveState> = None;
                loop {
                    engine.reset_world();
                    let mut play = if let Some(save) = pending_save.take() {
                        // globals/clips restored directly from the save,
                        // not rebuilt via on_start — defect D17 fix (Step
                        // 5c, docs/ember2d-phase5-plan.md); see
                        // PlayState::from_save's own doc comment for why.
                        let (globals, clips) = (save.globals, save.clips);
                        engine.world = save.world;
                        engine.persistent = save.persistent;
                        level_data = LevelData::load(&save.level_path)
                            .map_err(|e| { eprintln!("Error loading level: {}", e); io::Error::new(io::ErrorKind::Other, "Level load failed") })?;
                        PlayState::from_save(level_data.clone(), engine.persistent.clone(), globals, clips)
                    } else {
                        PlayState::from_level(level_data.clone(), engine.persistent.clone())
                    };
                    play.set_pixels_per_unit(pixels_per_unit);
                    engine.push_state(Box::new(play));
                    
                    match engine.run()? {
                        Some(Transition::ToEditor) => {
                            while engine.state_stack_len() > 1 {
                                engine.pop_state();
                            }
                            engine.reset_world();
                            break; // Back to editor loop
                        }
                        Some(Transition::ToPlay(next_data)) => {
                            engine.pop_state();
                            level_data = next_data;
                            // Loop continues to run next level
                        }
                        Some(Transition::LoadGame(save_state)) => {
                            engine.pop_state();
                            pending_save = Some(save_state);
                            // Loop continues and restores world at top
                        }
                        Some(Transition::ToStart) => {
                            while engine.state_stack_len() > 1 {
                                engine.pop_state();
                            }
                            return Ok(true);
                        }
                        Some(Transition::Quit) => {
                            return Ok(false);
                        }
                        _ => {
                            while engine.state_stack_len() > 1 {
                                engine.pop_state();
                            }
                            break;
                        }
                    }
                }
                // Back under editor control — restore realtime (D6).
                engine.gameplay_loop = GameplayLoop::RealTime;
            }
            Transition::ToStart => return Ok(true),
            Transition::Quit => break,
            _ => {}
        }
    }

    Ok(false)
}

pub fn run_play_app(engine: &mut Engine, mut data: LevelData, pixels_per_unit: f32) -> io::Result<()> {
    let mut pending_save: Option<SaveState> = None;
    loop {
        engine.reset_world();
        let mut play = if let Some(save) = pending_save.take() {
            // globals/clips restored directly from the save, not rebuilt
            // via on_start — defect D17 fix (Step 5c,
            // docs/ember2d-phase5-plan.md); see PlayState::from_save's own
            // doc comment for why.
            let (globals, clips) = (save.globals, save.clips);
            engine.world = save.world;
            engine.persistent = save.persistent;
            data = LevelData::load(&save.level_path)
                .map_err(|e| { eprintln!("Error loading level: {}", e); io::Error::new(io::ErrorKind::Other, "Level load failed") })?;
            PlayState::from_save(data.clone(), engine.persistent.clone(), globals, clips)
        } else {
            PlayState::from_level(data.clone(), engine.persistent.clone())
        };
        play.set_pixels_per_unit(pixels_per_unit);
        engine.push_state(Box::new(play));
        
        match engine.run()? {
            Some(Transition::ToPlay(next)) => {
                engine.pop_state();
                data = next;
            }
            Some(Transition::LoadGame(save_state)) => {
                engine.pop_state();
                pending_save = Some(save_state);
            }
            _ => {
                engine.pop_state();
                return Ok(());
            }
        }
    }
}
