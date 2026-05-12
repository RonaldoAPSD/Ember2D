// main.rs — Entry point for the ember2d demo game and level editor.

use std::env;
use std::io;
use std::path::Path;

use ember2d::app::{AppState, run_editor_app, run_play_app};
use ember2d::editor::EditorState;
use ember2d::editor::start_screen::StartScreen;
use ember2d::engine::{Engine, Transition};
use ember2d::level::LevelData;
use ember2d::project::{ProjectData, VisualStyle};

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().collect();
    let mut engine = Engine::new(80, 24, "Ember2D")?;
    let _app_state = AppState::new();

    if args.len() > 1 {
        let mut editor_mode = false;
        let mut path = String::new();
        for arg in &args[1..] {
            if arg == "--editor" { editor_mode = true; }
            else if !arg.starts_with("--") { path = arg.clone(); }
        }

        if editor_mode {
            let editor = if path.is_empty() { EditorState::new("") } 
            else { match EditorState::load(&path) { Ok(e) => e, Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); } } };
            let project_dir = Path::new(&path).parent().unwrap_or(Path::new("."));
            if let Ok(proj) = ProjectData::load(&project_dir.to_string_lossy()) {
                if proj.visual_style == VisualStyle::Sprites2D { engine.renderer.set_sprite_mode(true); } 
                else { engine.renderer.set_sprite_mode(false); }
                engine.gameplay_loop = proj.gameplay_loop;
            }
            run_editor_app(&mut engine, editor)?;
        } else if !path.is_empty() {
            let data = match LevelData::load(&path) { Ok(d) => d, Err(e) => { eprintln!("Error: {}", e); std::process::exit(1); } };
            let project_dir = Path::new(&path).parent().unwrap_or(Path::new("."));
            if let Ok(proj) = ProjectData::load(&project_dir.to_string_lossy()) {
                if proj.visual_style == VisualStyle::Sprites2D { engine.renderer.set_sprite_mode(true); } 
                else { engine.renderer.set_sprite_mode(false); }
                engine.gameplay_loop = proj.gameplay_loop;
            }
            run_play_app(&mut engine, data)?;
        } else {
            print_usage();
        }
    } else {
        loop {
            engine.reset_world();
            engine.push_state(Box::new(StartScreen::new()));

            match engine.run()? {
                Some(Transition::ToEditorWithResult(res)) => {
                    engine.pop_state(); // Pop start screen
                    if let Ok(editor) = EditorState::new_from_result(res) {
                        let folder = editor.project_folder.clone().unwrap_or_else(|| ".".to_string());
                        if let Ok(proj) = ProjectData::load(&folder) {
                            if proj.visual_style == VisualStyle::Sprites2D { engine.renderer.set_sprite_mode(true); } 
                            else { engine.renderer.set_sprite_mode(false); }
                            engine.gameplay_loop = proj.gameplay_loop;
                        }
                        if !run_editor_app(&mut engine, editor)? { break; } // Quit from editor
                    }
                }
                Some(Transition::Quit) | None => break,
                _ => { engine.pop_state(); }
            }
        }
    }
    Ok(())
}

fn print_usage() {
    println!("Ember2D Engine v0.5");
    println!("Usage:");
    println!("  ember2d --editor             (Launch the editor start screen)");
    println!("  ember2d --editor file.level  (Open a specific level in the editor)");
    println!("  ember2d file.level           (Play a level directly)");
}
