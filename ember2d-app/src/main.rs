// main.rs — Entry point for the ember2d demo game and level editor.

use std::env;
use std::io;
use std::path::Path;

mod app;
use app::{load_project, run_editor_app, run_play_app};

use ember2d::prelude::*;
use ember2d_editor::prelude::{EditorState, PrefsStore, StartScreen};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut engine = Engine::new(80, 24, "Ember2D")?;

    if args.len() > 1 {
        let mut editor_mode = false;
        let mut path = String::new();
        for arg in &args[1..] {
            if arg == "--editor" {
                editor_mode = true;
            } else if !arg.starts_with("--") {
                path = arg.clone();
            }
        }

        if editor_mode {
            // `.with_prefs(PrefsStore::user())` (7D-3, docs/ember2d-master-plan.md
            // §5.4): the ONE place any real (file-backed) preferences load
            // happens — every other `EditorState` construction (tests,
            // `EditorHarness`) stays on the in-memory default, which is
            // what keeps tests off the real per-user prefs file.
            let mut editor = if path.is_empty() {
                EditorState::new("")
            } else {
                match EditorState::load(&path) {
                    Ok(e) => e,
                    Err(e) => {
                        eprintln!("Error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            .with_prefs(PrefsStore::user());
            let project_dir = Path::new(&path).parent().unwrap_or(Path::new("."));
            // The editor has no time model of its own (D6) — only the project's
            // sprite mode applies here. `gameplay_loop` (in `settings`) only
            // takes effect once the editor actually enters play mode.
            let (settings, name) = load_project(&mut engine, &project_dir.to_string_lossy());
            if name.is_some() {
                editor.project_name = name;
            }
            // R45 (7A-12, docs/ember2d-master-plan.md §5.1): without this,
            // a level opened via `--editor path/to.level` rendered fine but
            // the Files panel showed "(empty folder)" and New Script did
            // nothing — see `open_project_folder`'s own doc comment.
            if !path.is_empty() {
                editor.open_project_folder(project_dir.to_string_lossy().into_owned());
            }
            let back_to_start = run_editor_app(&mut engine, editor, settings)?;
            // R106 (§3 in the master plan): this return value used to be
            // dropped, so File > Close Project quit the whole app when the
            // editor had been launched as `--editor path/to.level` (it only
            // reached the start screen when launched with no arguments).
            if after_editor(back_to_start) == AfterEditor::StartScreen {
                run_start_screen(&mut engine)?;
            }
        } else if !path.is_empty() {
            let data = match LevelData::load(&path) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            };
            let project_dir = Path::new(&path).parent().unwrap_or(Path::new("."));
            let (settings, _) = load_project(&mut engine, &project_dir.to_string_lossy());
            run_play_app(&mut engine, data, settings)?;
        } else {
            print_usage();
        }
    } else {
        run_start_screen(&mut engine)?;
    }
    Ok(())
}

/// The start screen, then whatever it opens, round and round until the user
/// quits. Its own function since R106 (master plan §3): both the no-argument
/// launch and an editor launched with `--editor path/to.level` end up here —
/// the latter once the user picks File > Close Project.
fn run_start_screen(engine: &mut Engine) -> io::Result<()> {
    loop {
        engine.reset_world();
        // The start screen is chrome, not gameplay — it always runs
        // realtime (D6), same as the editor. This also undoes whatever
        // `run_editor_app` last set while play mode was active.
        engine.gameplay_loop = GameplayLoop::RealTime;
        engine.push_state(Box::new(StartScreen::new()));

        match engine.run()? {
            Some(Transition::ToEditorWithResult(res)) => {
                engine.pop_state(); // Pop start screen
                if let Ok(editor) =
                    EditorState::new_from_result(res).map(|e| e.with_prefs(PrefsStore::user()))
                {
                    let folder = editor.project_folder.clone().unwrap_or_else(|| ".".to_string());
                    let (settings, _) = load_project(engine, &folder);
                    let back_to_start = run_editor_app(engine, editor, settings)?;
                    if after_editor(back_to_start) == AfterEditor::Exit {
                        break; // Quit from editor
                    }
                }
            }
            Some(Transition::Quit) | None => break,
            _ => {
                engine.pop_state();
            }
        }
    }
    Ok(())
}

/// What the app does once the editor returns.
#[derive(Debug, PartialEq)]
enum AfterEditor {
    /// File > Close Project: back to the start screen.
    StartScreen,
    /// Quit (window closed, or the editor's own Quit).
    Exit,
}

/// R106: `run_editor_app`'s `true` ("back to start", Close Project) means
/// the start screen however the editor was launched — the one rule both
/// call sites now share instead of each reading the bool its own way.
fn after_editor(back_to_start: bool) -> AfterEditor {
    if back_to_start {
        AfterEditor::StartScreen
    } else {
        AfterEditor::Exit
    }
}

fn print_usage() {
    println!("Ember2D Engine v0.5");
    println!("Usage:");
    println!("  ember2d --editor             (Launch the editor start screen)");
    println!("  ember2d --editor file.level  (Open a specific level in the editor)");
    println!("  ember2d file.level           (Play a level directly)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r106_close_project_goes_to_the_start_screen_and_quit_exits() {
        assert_eq!(after_editor(true), AfterEditor::StartScreen);
        assert_eq!(after_editor(false), AfterEditor::Exit);
    }
}
