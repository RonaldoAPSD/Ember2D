// editor/impl_state/export.rs — "Export Standalone Game" support.
//
// Split out of impl_state/mod.rs in Phase 7 Part 1f (docs/ember2d-phase7-plan.md)
// to keep that file under CLAUDE.md's 600-line hard limit — this is a
// self-contained feature (one `EditorState` method plus its own private
// recursive-copy helper) with no behavioral change from being its own file.

use super::super::panel::PanelId;
use super::EditorState;
use ember2d_sim::scripting::LogEntry;

impl EditorState {
    pub(super) fn export_game(&mut self) {
        let Some(project_path) = self.project_folder.clone() else {
            self.console_log.push(LogEntry::error("Cannot export: No project folder open."));
            return;
        };

        // Ensure current level is saved first
        self.save();

        #[cfg(debug_assertions)]
        self.console_log.push(LogEntry::warn(
            "Exporting a DEBUG build. For a release build run: cargo build --release, then export from target/release/ember2d."
        ));

        let picked = rfd::FileDialog::new()
            .set_title("Export Standalone Game - Pick Destination Folder")
            .pick_folder();

        if let Some(out_dir) = picked {
            let project_name = std::path::Path::new(&project_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "MyGame".to_string());

            let export_root = out_dir.join(format!("{}_Export", project_name));
            if let Err(e) = std::fs::create_dir_all(&export_root) {
                self.console_log
                    .push(LogEntry::error(format!("Export failed (create dir): {}", e)));
                return;
            }

            // 1. Copy executable
            if let Ok(exe_path) = std::env::current_exe() {
                let mut target_exe = export_root.join(&project_name);
                if cfg!(windows) {
                    target_exe.set_extension("exe");
                }
                if let Err(e) = std::fs::copy(&exe_path, &target_exe) {
                    self.console_log.push(LogEntry::warn(format!(
                        "Executable copy failed: {}. You may need to copy it manually.",
                        e
                    )));
                }
            }

            // 2–3. The project's own files — `copy_project_files` below
            // (Step 9-6 split it out so it's testable without the OS
            // folder picker above).
            let warnings = copy_project_files(std::path::Path::new(&project_path), &export_root);
            self.console_log.extend(warnings.into_iter().map(LogEntry::warn));

            // 4. Create .standalone marker
            if let Err(e) = std::fs::write(export_root.join(".standalone"), "") {
                self.console_log.push(LogEntry::error(format!("Failed to create marker: {}", e)));
            } else {
                self.console_log
                    .push(LogEntry::info(format!("SUCCESS: Game exported to {:?}", export_root)));
                self.panels.show(PanelId::Console);
            }
        }
    }
}

/// Copies everything an exported game needs from `project` into `out`,
/// returning a warning per folder it couldn't copy:
/// 2. the asset folders, recursively — `assets` since Step 8-2 (tilesets;
///    without it sprite tiles all fall back to their glyphs), and since
///    Step 9-6 `scenes` (9-1's scene scripts) and `art` (loose images 8-4
///    imports), which an exported game used to lose;
/// 3. `project.ron`, the palette, every level, and (Step 9-6) the
///    `.rhai` files beside them — node-graph sidecars
///    (`<level>_graph_x_y_l.rhai`) live there, so an exported game's graph
///    tiles used to lose their scripts.
pub(crate) fn copy_project_files(project: &std::path::Path, out: &std::path::Path) -> Vec<String> {
    let mut warnings = Vec::new();
    for folder in ["audio", "scripts", "assets", "scenes", "art"] {
        let src = project.join(folder);
        if src.exists() {
            if let Err(e) = copy_dir_all(&src, &out.join(folder)) {
                warnings.push(format!("Failed to copy {}: {}", folder, e));
            }
        }
    }
    if let Ok(entries) = std::fs::read_dir(project) {
        for entry in entries.flatten() {
            let p = entry.path();
            let Some(name) = p.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let wanted = name == "project.ron"
                || name.ends_with(".level")
                || name.ends_with(".palette.ron")
                || name.ends_with(".rhai");
            if p.is_file() && wanted {
                if let Err(e) = std::fs::copy(&p, out.join(&name)) {
                    warnings.push(format!("Failed to copy {}: {}", name, e));
                }
            }
        }
    }
    warnings
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}
