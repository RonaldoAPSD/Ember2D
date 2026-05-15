// app.rs — Top-level application loop: manages Editor ↔ Play mode transitions.

use std::collections::HashMap;
use std::io;

use crate::editor::EditorState;
use crate::engine::{Engine, Transition};
use crate::level::LevelData;
use crate::play::PlayState;
use crate::save::SaveState;

pub struct AppState {
    pub persistent: HashMap<String, rhai::Dynamic>,
}

impl AppState {
    pub fn new() -> Self {
        Self { persistent: HashMap::new() }
    }
}

pub fn run_editor_app(engine: &mut Engine, editor: EditorState) -> io::Result<bool> {
    engine.push_state(Box::new(editor));

    while let Some(transition) = engine.run()? {
        match transition {
            Transition::ToPlay(mut level_data) => {
                // ── Switch to play mode ────────────────────────────────────
                let mut pending_save: Option<SaveState> = None;
                loop {
                    engine.reset_world();
                    let play = if let Some(save) = pending_save.take() {
                        engine.world = save.world;
                        engine.persistent = save.persistent;
                        level_data = LevelData::load(&save.level_path)
                            .map_err(|e| { eprintln!("Error loading level: {}", e); io::Error::new(io::ErrorKind::Other, "Level load failed") })?;
                        PlayState::from_save(level_data.clone(), engine.persistent.clone())
                    } else {
                        PlayState::from_level(level_data.clone(), engine.persistent.clone())
                    };
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
            }
            Transition::ToStart => return Ok(true),
            Transition::Quit => break,
            _ => {}
        }
    }

    Ok(false)
}

pub fn run_play_app(engine: &mut Engine, mut data: LevelData) -> io::Result<()> {
    let mut pending_save: Option<SaveState> = None;
    loop {
        engine.reset_world();
        let play = if let Some(save) = pending_save.take() {
            engine.world = save.world;
            engine.persistent = save.persistent;
            data = LevelData::load(&save.level_path)
                .map_err(|e| { eprintln!("Error loading level: {}", e); io::Error::new(io::ErrorKind::Other, "Level load failed") })?;
            PlayState::from_save(data.clone(), engine.persistent.clone())
        } else {
            PlayState::from_level(data.clone(), engine.persistent.clone())
        };
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
