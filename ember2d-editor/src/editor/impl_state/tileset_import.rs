// editor/impl_state/tileset_import.rs — starting, finishing, and cancelling
// a tileset import: the OS image picker, writing `assets/tilesets/<name>.ron`
// plus a copy of the sheet, and adding a palette entry per named region.
//
// Step 8-2 (docs/ember2d-master-plan.md §5.7). A sibling of export.rs /
// graph_sidecars.rs / viewport.rs — another `impl EditorState` block in its
// own file, since impl_state/mod.rs is near CLAUDE.md's 750-line limit. The
// dialog's own state and arithmetic are in `editor/importer.rs`; this file
// is the part that touches the filesystem and the rest of `EditorState`.

use std::path::Path;

use ember2d::renderer::color::Color;
use ember2d::renderer::Texture;
use ember2d_sim::scripting::LogEntry;
use ember2d_sim::tileset::SpriteRef;

use super::super::commands::Command;
use super::super::importer::TilesetImport;
use super::super::palette::TileDefinition;
use super::super::sprites::SpriteAssets;
use super::super::ui::ToolKind;
use super::super::{EditorMode, EditorState};

impl EditorState {
    /// File > Import Tileset...: ask the OS for an image, then open the
    /// importer on it. Cancelling the picker does nothing.
    pub(in crate::editor) fn pick_and_begin_tileset_import(&mut self) {
        if self.project_folder.is_none() {
            self.console_log.push(LogEntry::error(
                "Import Tileset needs an open project (tilesets live in <project>/assets/tilesets/)",
            ));
            return;
        }
        let picked = rfd::FileDialog::new()
            .set_title("Import Tileset - Pick a Sprite Sheet")
            .add_filter("Images", &["png", "jpg", "jpeg", "bmp", "gif"])
            .pick_file();
        if let Some(path) = picked {
            if let Err(e) = self.begin_tileset_import(&path) {
                self.console_log.push(LogEntry::error(format!("Import Tileset: {e}")));
            }
        }
    }

    /// Open the importer on `image` — everything after the OS picker, `pub`
    /// so headless tests can drive the dialog without one. If the project
    /// already has a tileset with this image's name, its settings and
    /// region names are carried over (re-slicing an existing sheet).
    pub fn begin_tileset_import(&mut self, image: &Path) -> Result<(), String> {
        if self.project_folder.is_none() {
            return Err("no project is open".to_string());
        }
        let texture = Texture::load(image).map_err(|e| format!("{}: {e}", image.display()))?;
        let stem = image.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
        let existing = self.sprites.tilesets.get(stem).map(|t| &t.data);
        self.tileset_import = Some(TilesetImport::new(image.to_path_buf(), texture, existing));
        self.mode = EditorMode::TilesetImport;
        Ok(())
    }

    pub(in crate::editor) fn cancel_tileset_import(&mut self) {
        self.tileset_import = None;
        self.mode = EditorMode::Paint(ToolKind::Paint);
    }

    /// The importer's [ Import ] button. On success: the sheet is copied to
    /// `assets/tilesets/<name>.png`, `<name>.ron` is written beside it, the
    /// editor's tilesets are reloaded (every tile already painted from this
    /// tileset redraws with the new slicing at once), each named region not
    /// already in the palette gets an entry (one undoable palette change),
    /// and the dialog closes. On failure the dialog stays open showing why.
    pub(in crate::editor) fn finish_tileset_import(&mut self) {
        let Some(imp) = self.tileset_import.as_mut() else { return };
        let result = (|| -> Result<(String, Vec<String>), String> {
            let data = imp.to_tileset()?;
            let folder = self.project_folder.as_deref().ok_or("no project is open")?;
            let dir = SpriteAssets::dir(folder);
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let image_dest = dir.join(&data.image);
            // Re-importing the sheet that's already in place: nothing to copy
            // (and copying a file onto itself would truncate it on some OSes).
            let same = std::fs::canonicalize(&imp.source).ok()
                == std::fs::canonicalize(&image_dest).ok()
                && image_dest.exists();
            if !same {
                std::fs::copy(&imp.source, &image_dest)
                    .map_err(|e| format!("copying the image: {e}"))?;
            }
            let ron_path = dir.join(format!("{}.ron", data.name));
            let text =
                ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::new().depth_limit(3))
                    .map_err(|e| e.to_string())?;
            std::fs::write(&ron_path, text).map_err(|e| format!("{}: {e}", ron_path.display()))?;
            let regions = data.regions.iter().map(|r| r.name.clone()).collect();
            Ok((data.name, regions))
        })();

        let (name, regions) = match result {
            Ok(ok) => ok,
            Err(e) => {
                imp.error = Some(e);
                return;
            }
        };
        self.tileset_import = None;
        self.mode = EditorMode::Paint(ToolKind::Paint);

        for problem in self.sprites.reload(self.project_folder.as_deref()) {
            self.console_log.push(LogEntry::warn(problem));
        }

        let before = self.palette.clone();
        let mut added = 0;
        for region in &regions {
            let sprite = SpriteRef::new(name.clone(), region.clone());
            if self.palette.tiles.iter().any(|t| t.sprite.as_ref() == Some(&sprite)) {
                continue;
            }
            self.palette.tiles.push(TileDefinition {
                name: region.clone(),
                // The fallback glyph: the region name's first letter, so a
                // missing tileset still leaves a readable map.
                glyph: region.chars().next().unwrap_or('#'),
                fg: Color::White,
                bg: Color::Reset,
                solid: false,
                trigger: false,
                tag: String::new(),
                sprite: Some(sprite),
                clip: None,
            });
            added += 1;
        }
        if added > 0 {
            self.undo.push(Command::UpdatePalette { before, after: self.palette.clone() });
            self.save_palette();
        }
        self.refresh_project_files();
        self.console_log.push(LogEntry::info(format!(
            "Imported tileset '{name}': {} region(s), {added} new palette entr{}",
            regions.len(),
            if added == 1 { "y" } else { "ies" }
        )));
    }
}
