// app.rs — Top-level application loop: manages Editor ↔ Play mode transitions.

use std::io;

use ember2d::prelude::*;
use ember2d::project::PlaySettings;
use ember2d_editor::prelude::EditorState;

/// Run the editor, switching into play mode and back as the user requests.
///
/// `settings.gameplay_loop` is the project's target loop model (RealTime
/// or TurnBased) for actual gameplay. The editor itself has no time model
/// of its own (D6 — it must not inherit the project's setting) and always
/// runs realtime; the loop only takes effect for the stretches where a
/// `PlayState` is on top of the stack. The rest of `settings` (pixels per
/// unit, turn model, world cell — Step 9-5 bundled them into one
/// `PlaySettings`) is handed to each `PlayState` as it's built, since
/// `PlayState::from_level` takes only a `LevelData`, not a `ProjectData`.
pub fn run_editor_app(
    engine: &mut Engine,
    editor: EditorState,
    settings: PlaySettings,
) -> io::Result<bool> {
    engine.gameplay_loop = GameplayLoop::RealTime;
    engine.push_state(Box::new(editor));

    while let Some(transition) = engine.run()? {
        match transition {
            Transition::ToPlay(mut level_data) => {
                // ── Switch to play mode ────────────────────────────────────
                engine.gameplay_loop = settings.gameplay_loop;
                // R96 (docs/ember2d-master-plan.md §3.2): this arm is a
                // FRESH run (F5 / File > Play from the editor) — the inner
                // loop's own `ToPlay(next_data)` arm below is a level
                // transition WITHIN a run, which must keep it. `persistent`
                // is the engine's cross-level store and outlives every
                // `PlayState`, so without this a second F5 inherited the
                // last run's hp/gold/depth — masked until R96 itself, since
                // the roguelike's player script used to re-seed the run on
                // every level load anyway (the bug R96 fixes). A loaded
                // save replaces the whole map regardless (`pending_save`).
                engine.persistent.clear();
                let mut pending_save: Option<SaveState> = None;
                loop {
                    engine.reset_world();
                    let mut loaded_scenes: Option<(Vec<ember2d::play::SceneFrame>, ember2d::play::UiModel)> = None;
                    let mut play = if let Some(save) = pending_save.take() {
                        // globals/clips restored directly from the save,
                        // not rebuilt via on_start — defect D17 fix (Step
                        // 5c, docs/ember2d-phase5-plan.md); see
                        // PlayState::from_save's own doc comment for why.
                        let (globals, clips) = (save.globals, save.clips);
                        // R7 (7A-3, docs/ember2d-master-plan.md): the
                        // scheduler/turn_number half of a faithful save
                        // round trip, same treatment as globals/clips above.
                        let (turn_number, scheduler) = (save.turn_number, save.scheduler);
                        loaded_scenes = Some((save.scenes, save.ui)); // Steps 9-1, 9-3
                        engine.world = save.world;
                        engine.persistent = save.persistent;
                        level_data = LevelData::load(&save.level_path).map_err(|e| {
                            eprintln!("Error loading level: {}", e);
                            io::Error::new(io::ErrorKind::Other, "Level load failed")
                        })?;
                        PlayState::from_save(
                            level_data.clone(),
                            engine.persistent.clone(),
                            globals,
                            clips,
                            turn_number,
                            scheduler,
                        )
                    } else {
                        PlayState::from_level(level_data.clone(), engine.persistent.clone())
                    };
                    play.apply_play_settings(&settings);
                    // Step 9-1 (docs/ember2d-master-plan.md §5.8): an F5 run
                    // offers the pause scene's Back to Editor row.
                    play.set_editor_preview(true);
                    if let Some((scenes, ui)) = loaded_scenes.take() {
                        play.set_saved_scenes(scenes);
                        play.set_saved_ui(ui);
                    }
                    engine.push_state(Box::new(play));

                    match engine.run()? {
                        Some(Transition::ToEditor) => {
                            // 7C-7 (master plan §5.3, R18): a `PlayState`
                            // popped here (or an overlay state above it)
                            // is gone the moment its `Box<dyn GameState>`
                            // drops — draining each one's script log via
                            // the trait (`take_script_log`, default no-op
                            // for anything that doesn't override it) is
                            // the only way to keep a script error the
                            // player triggered from vanishing with the
                            // state that logged it, since neither state is
                            // reachable as its concrete type once pushed.
                            let mut script_log = Vec::new();
                            while engine.state_stack_len() > 1 {
                                if let Some(mut popped) = engine.pop_state() {
                                    script_log.extend(popped.take_script_log());
                                }
                            }
                            if let Some(editor) = engine.top_state_mut() {
                                editor.receive_script_log(script_log);
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
                            // R19 (7A-2, docs/ember2d-master-plan.md): this
                            // used to stop at len() > 1, leaving the
                            // EditorState (pushed once, at the top of this
                            // function) sitting on the stack under nothing.
                            // `main.rs`'s own loop then pushes a fresh
                            // StartScreen on top of that orphan instead of
                            // onto an empty stack — popping to 0 here is
                            // what `Engine::run`'s own "stack empty -> Ok(None)"
                            // contract expects the caller to leave behind
                            // before pushing a new top-level state.
                            while engine.state_stack_len() > 0 {
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
            // R19 (7A-2, docs/ember2d-master-plan.md): same orphan as the
            // inner loop's `ToStart` arm above — this is the editor itself
            // (top of stack, no `PlayState` pushed over it) requesting the
            // start screen, e.g. via File > Start Screen. Popping it before
            // returning is what keeps `main.rs`'s next `push_state` landing
            // on an empty stack instead of stacking a second EditorState.
            Transition::ToStart => {
                engine.pop_state();
                return Ok(true);
            }
            Transition::Quit => break,
            _ => {}
        }
    }

    Ok(false)
}

/// Play `data` directly (`ember2d level.level`), following level changes
/// and loaded saves until the game ends. `settings` as in `run_editor_app`.
pub fn run_play_app(
    engine: &mut Engine,
    mut data: LevelData,
    settings: PlaySettings,
) -> io::Result<()> {
    engine.gameplay_loop = settings.gameplay_loop;
    let mut pending_save: Option<SaveState> = None;
    loop {
        engine.reset_world();
        let mut loaded_scenes: Option<(Vec<ember2d::play::SceneFrame>, ember2d::play::UiModel)> = None;
        let mut play = if let Some(save) = pending_save.take() {
            // globals/clips restored directly from the save, not rebuilt
            // via on_start — defect D17 fix (Step 5c,
            // docs/ember2d-phase5-plan.md); see PlayState::from_save's own
            // doc comment for why.
            let (globals, clips) = (save.globals, save.clips);
            // R7 (7A-3, docs/ember2d-master-plan.md): see the matching
            // comment in `run_editor_app` above.
            let (turn_number, scheduler) = (save.turn_number, save.scheduler);
                        loaded_scenes = Some((save.scenes, save.ui)); // Steps 9-1, 9-3
            engine.world = save.world;
            engine.persistent = save.persistent;
            data = LevelData::load(&save.level_path).map_err(|e| {
                eprintln!("Error loading level: {}", e);
                io::Error::new(io::ErrorKind::Other, "Level load failed")
            })?;
            PlayState::from_save(
                data.clone(),
                engine.persistent.clone(),
                globals,
                clips,
                turn_number,
                scheduler,
            )
        } else {
            PlayState::from_level(data.clone(), engine.persistent.clone())
        };
        play.apply_play_settings(&settings);
        if let Some((scenes, ui)) = loaded_scenes.take() {
            play.set_saved_scenes(scenes); // Step 9-1
            play.set_saved_ui(ui); // Step 9-3
        }
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
