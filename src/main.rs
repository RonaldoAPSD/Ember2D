// main.rs — Entry point for the ember2d demo game and level editor.
//
// ── TWO MODES ─────────────────────────────────────────────────────────────────
//
//   cargo run                          → opens a blank level editor (default)
//   cargo run -- --editor              → opens a blank level editor
//   cargo run -- --editor level.level  → opens the editor with a level file
//   cargo run -- level.level           → plays the level directly (no editor UI)
//
// The editor launches by default when no args are given.
//
// ── DEMO GAME OVERVIEW ────────────────────────────────────────────────────────
//
//   - Implementing the GameState trait
//   - Spawning entities with multiple components
//   - Smooth keyboard-driven player movement (delta-time scaled)
//   - Solid collision: walls stop the player (position rollback in late_update)
//   - Trigger collision: walking over items collects them
//   - Z-ordered rendering: floor → items → walls → player
//   - HUD: FPS counter, score, player position
//
// DEMO CONTROLS:
//   WASD or Arrow keys — move the player (@)
//   Escape             — quit

use std::path::Path;
use ember2d::prelude::*;
use ember2d::editor::start_screen::StartScreen;
use ember2d::renderer::SCALE;

// ─────────────────────────────────────────────────────────────────────────────
//  Screen size detection
// ─────────────────────────────────────────────────────────────────────────────

/// Detect the primary monitor's character-cell dimensions.
/// On Windows we call GetSystemMetrics directly (no extra crate needed).
/// Other platforms fall back to a comfortable 80×24 default.
#[cfg(target_os = "windows")]
fn screen_cells() -> (usize, usize) {
    extern "system" { fn GetSystemMetrics(nIndex: i32) -> i32; }
    // SM_CXSCREEN = 0 (screen width in pixels), SM_CYSCREEN = 1 (height in pixels).
    let sw = unsafe { GetSystemMetrics(0) }.max(0) as usize;
    let sh = unsafe { GetSystemMetrics(1) }.max(0) as usize;
    // Each cell is 8px wide × 16px tall in the pixel buffer, then doubled by Scale::X2.
    let cell_w = 8 * SCALE;   // 16 px/cell on screen
    let cell_h = 16 * SCALE;  // 32 px/cell on screen
    let cols = (sw / cell_w).max(80);
    let rows = (sh / cell_h).max(24);
    (cols, rows)
}

#[cfg(not(target_os = "windows"))]
fn screen_cells() -> (usize, usize) { (80, 24) }

// ─────────────────────────────────────────────────────────────────────────────
//  Entry point
// ─────────────────────────────────────────────────────────────────────────────

fn main() {
    // ── Mode detection ────────────────────────────────────────────────────────
    //
    // CLI argument matrix:
    //
    //   (no args)                   → open level editor (blank canvas)
    //   --editor                    → open level editor (blank canvas)
    //   --editor path/to/file.level → open level editor with a saved level
    //   path/to/file.level          → play the level directly (no editor UI)
    //
    // We detect mode by scanning args:
    //   - "--editor" flag OR no args → editor mode
    //   - a *.level filename without "--editor" → direct play
    let args: Vec<String> = std::env::args().collect();
    let has_level_file = args.iter().skip(1).any(|a| a.ends_with(".level") && !a.starts_with('-'));
    let mut editor_mode = args.iter().any(|a| a == "--editor") || (!has_level_file && args.len() == 1);

    // ── Standalone Mode Detection ─────────────────────────────────────────────
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let is_standalone = exe_dir.as_ref().map(|d| d.join(".standalone").exists()).unwrap_or(false);

    let mut standalone_level = String::new();
    if is_standalone {
        editor_mode = false;
        if let Some(ref d) = exe_dir {
            // Anchor cwd to the exe directory so all relative asset paths resolve correctly.
            let _ = std::env::set_current_dir(d);

            // Try to load start level from project.ron
            let proj_path = d.join("project.ron");
            if let Ok(content) = std::fs::read_to_string(proj_path) {
                if let Ok(proj) = ron::de::from_str::<ProjectData>(&content) {
                    standalone_level = proj.start_level.unwrap_or_default();
                }
            }
            // Fallback: find any .level, sorted so the pick is deterministic
            if standalone_level.is_empty() {
                if let Ok(entries) = std::fs::read_dir(d) {
                    let mut levels: Vec<_> = entries
                        .flatten()
                        .filter(|e| e.path().extension().map_or(false, |x| x == "level"))
                        .collect();
                    levels.sort_by_key(|e| e.file_name());
                    if let Some(entry) = levels.first() {
                        standalone_level = entry.file_name().to_string_lossy().into_owned();
                    }
                }
            }
        }
    }

    // Find a .level file arg that isn't the editor flag itself.
    let mut level_file: Option<String> = args.iter()
        .skip(1)
        .find(|a| a.ends_with(".level") && !a.starts_with('-'))
        .cloned();
    
    if is_standalone && !standalone_level.is_empty() {
        level_file = Some(standalone_level);
    }

    let (screen_w, screen_h) = screen_cells();

    if editor_mode {
        // ── Level editor with Editor ↔ Play switching ────────────────────────
        //
        // run_editor_app handles F5 (go to play) and Escape (return to editor)
        // in a loop — so we don't need `engine.run()` directly here.

        let level_path = args.iter()
            .skip_while(|a| *a != "--editor")
            .nth(1)
            .map(String::as_str)
            .unwrap_or("");

        let mut engine = Engine::new(screen_w, screen_h, "Ember2D Level Editor")
            .expect("Failed to open window.");

        let mut current_level_path = level_path.to_string();
        loop {
        let editor = if current_level_path.is_empty() {
            // Show the start screen so the user can name/configure the project.
            let mut start = StartScreen::new();
            if let Err(e) = engine.run(&mut start) {
                eprintln!("Start screen error: {}", e);
                std::process::exit(1);
            }
            engine.reset_world();
            match start.result {
                None => break,  // user quit from the start screen
                Some(result) => {
                    let style = result.visual_style;
                    let state = EditorState::new_from_result(result).unwrap_or_else(|e| {
                        eprintln!("Warning: {}", e);
                        EditorState::new("")
                    });
                    
                    if style == VisualStyle::Sprites2D {
                        engine.renderer.set_backend(Box::new(SpriteBackend::new(engine.width, engine.height)));
                    } else {
                        engine.renderer.set_backend(Box::new(AsciiBackend::new(engine.width, engine.height)));
                    }
                    state
                }
            }
        } else {
            let editor = EditorState::load(&current_level_path).unwrap_or_else(|e| {
                eprintln!("Warning: {}", e);
                EditorState::new(&current_level_path)
            });

            // If we're loading a level in a project, sync the backend to the project's style.
            if let Some(ref folder) = editor.project_folder {
                if let Ok(proj) = ProjectData::load(folder) {
                    if proj.visual_style == VisualStyle::Sprites2D {
                        engine.renderer.set_backend(Box::new(SpriteBackend::new(engine.width, engine.height)));
                    } else {
                        engine.renderer.set_backend(Box::new(AsciiBackend::new(engine.width, engine.height)));
                    }
                }
            }
            editor
        };

            match run_editor_app(&mut engine, editor) {
                Ok(true) => {
                    current_level_path = String::new();
                    continue;
                }
                Ok(false) => break,
                Err(e) => {
                    eprintln!("Editor error: {}", e);
                    std::process::exit(1);
                }
            }
        }

    } else if let Some(ref path) = level_file {
        // ── Direct play: load a .level file and run it ───────────────────────

        let data = LevelData::load(path).unwrap_or_else(|e| {
            eprintln!("Failed to load level '{}': {}", path, e);
            std::process::exit(1);
        });

        let mut engine = Engine::new(screen_w, screen_h, &format!("Ember2D — {}", path))
            .expect("Failed to open window.");

        // Check project style for standalone/direct play
        let project_dir = Path::new(path).parent().unwrap_or(Path::new("."));
        if let Ok(proj) = ProjectData::load(&project_dir.to_string_lossy()) {
            if proj.visual_style == VisualStyle::Sprites2D {
                engine.renderer.set_backend(Box::new(SpriteBackend::new(engine.width, engine.height)));
            }
        }

        if let Err(e) = run_play_app(&mut engine, data) {
            eprintln!("Play error: {}", e);
            std::process::exit(1);
        }

    } else {
        println!("Usage:");
        println!("  ember2d                      (Launch the editor start screen)");
        println!("  ember2d --editor             (Launch the editor start screen)");
        println!("  ember2d --editor file.level  (Open a specific level in the editor)");
        println!("  ember2d file.level           (Play a level directly)");
    }
}
