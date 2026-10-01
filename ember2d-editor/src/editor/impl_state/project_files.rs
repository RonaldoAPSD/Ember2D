// editor/impl_state/project_files.rs — listing the File Browser's current
// folder.
//
// Moved here from impl_state/mod.rs unchanged at Step 8-4 (docs/ember2d-
// master-plan.md §5.7), whose own additions — asset rows and their image
// thumbnails — would otherwise have pushed that file past CLAUDE.md's
// 750-line limit.

use super::super::EditorState;

impl EditorState {
    pub(in crate::editor) fn refresh_project_files(&mut self) {
        if let Some(ref root) = self.project_folder {
            let mut files = Vec::new();
            let current_path = if self.current_folder == "." {
                std::path::PathBuf::from(root)
            } else {
                std::path::Path::new(root).join(&self.current_folder)
            };

            // Add ".." if not at root
            if self.current_folder != "." {
                files.push(".. [UP]".to_string());
            }

            if let Ok(entries) = std::fs::read_dir(&current_path) {
                let mut dirs = Vec::new();
                let mut other = Vec::new();

                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

                    if path.is_dir() {
                        dirs.push(format!("/ {} ", name));
                    } else if name.ends_with(".level") {
                        other.push(format!("[] {} ", name));
                    } else if name.ends_with(".rhai") {
                        other.push(format!("{{}} {} ", name));
                    } else if name == "project.ron" || name.ends_with(".palette.ron") {
                        other.push(format!(":: {} ", name));
                    } else if let Some(prefix) =
                        super::super::assets::asset_prefix(&self.current_folder, &name)
                    {
                        // Step 8-4: images anywhere, tileset/clip `.ron`s in
                        // their asset folders — see `editor/assets.rs`.
                        other.push(format!("{prefix}{name} "));
                    }
                }

                dirs.sort();
                other.sort();
                files.extend(dirs);
                files.extend(other);
            }

            self.file_browser_files = files;
            // Step 8-4: this folder's image thumbnails, loaded here so the
            // File Browser never reads the disk while drawing. Rebuilt from
            // scratch each refresh — a folder's handful of images is cheap,
            // and an image re-saved outside the editor shows up current.
            self.file_thumbs.clear();
            for raw in &self.file_browser_files {
                if let Some(super::super::assets::AssetRef::Image(rel)) =
                    super::super::assets::classify(&self.current_folder, raw)
                {
                    if let Ok(tex) =
                        ember2d::renderer::Texture::load(std::path::Path::new(root).join(&rel))
                    {
                        self.file_thumbs.insert(rel, tex);
                    }
                }
            }
            // Clamp cursor
            if self.file_browser_cursor >= self.file_browser_files.len() {
                self.file_browser_cursor = self.file_browser_files.len().saturating_sub(1);
            }
        }
    }
}
