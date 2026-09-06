// editor/impl_state/export.rs — "Export Standalone Game" support.
//
// Split out of impl_state/mod.rs in Phase 7 Part 1f (docs/ember2d-phase7-plan.md)
// to keep that file under CLAUDE.md's 600-line hard limit — this is a
// self-contained feature (one `EditorState` method plus its own private
// recursive-copy helper) with no behavioral change from being its own file.

use ember2d_sim::scripting::LogEntry;
use super::EditorState;
use super::super::panel::PanelId;

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
                self.console_log.push(LogEntry::error(format!("Export failed (create dir): {}", e)));
                return;
            }

            // 1. Copy executable
            if let Ok(exe_path) = std::env::current_exe() {
                let mut target_exe = export_root.join(&project_name);
                if cfg!(windows) { target_exe.set_extension("exe"); }
                if let Err(e) = std::fs::copy(&exe_path, &target_exe) {
                    self.console_log.push(LogEntry::warn(format!("Executable copy failed: {}. You may need to copy it manually.", e)));
                }
            }

            // 2. Copy assets (recursive)
            let asset_folders = ["audio", "scripts"];
            for folder in asset_folders {
                let src = std::path::Path::new(&project_path).join(folder);
                if src.exists() {
                    let dst = export_root.join(folder);
                    if let Err(e) = copy_dir_all(&src, &dst) {
                        self.console_log.push(LogEntry::warn(format!("Failed to copy {}: {}", folder, e)));
                    }
                }
            }

            // 3. Copy project.ron, palette, and all levels
            if let Ok(entries) = std::fs::read_dir(&project_path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        let name = p.file_name().unwrap().to_string_lossy();
                        if name == "project.ron" || name.ends_with(".level") || name.ends_with(".palette.ron") {
                            let _ = std::fs::copy(&p, export_root.join(&*name));
                        }
                    }
                }
            }

            // 4. Create .standalone marker
            if let Err(e) = std::fs::write(export_root.join(".standalone"), "") {
                self.console_log.push(LogEntry::error(format!("Failed to create marker: {}", e)));
            } else {
                self.console_log.push(LogEntry::info(format!("SUCCESS: Game exported to {:?}", export_root)));
                self.panels.show(PanelId::Console);
            }
        }
    }
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
