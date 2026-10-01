// editor/impl_state/clip_edit.rs — opening and closing the clip editor,
// saving a clip to `assets/clips/<name>.ron`, and adding a palette entry
// that paints it.
//
// Step 8-3 (docs/ember2d-master-plan.md §5.7). Another `impl EditorState`
// sibling (see tileset_import.rs, the 8-2 twin this follows); the dialog's
// pure state is `editor/clip_editor.rs`.

use ember2d::renderer::color::Color;
use ember2d_sim::scripting::LogEntry;
use ember2d_sim::tileset::SpriteRef;

use super::super::clip_editor::ClipEditor;
use super::super::commands::Command;
use super::super::palette::TileDefinition;
use super::super::sprites::SpriteAssets;
use super::super::ui::ToolKind;
use super::super::{EditorMode, EditorState};

impl EditorState {
    /// File > Animation Clips...: a new, empty clip drawing from the
    /// project's first tileset. `pub` so headless tests open it directly.
    pub fn open_clip_editor(&mut self) {
        if self.project_folder.is_none() {
            self.console_log.push(LogEntry::error(
                "Animation Clips needs an open project (clips live in <project>/assets/clips/)",
            ));
            return;
        }
        let first = self.sprites.tilesets.keys().next().cloned();
        self.clip_editor = Some(ClipEditor::new(first));
        self.mode = EditorMode::ClipEditor;
    }

    pub(in crate::editor) fn close_clip_editor(&mut self) {
        self.clip_editor = None;
        self.mode = EditorMode::Paint(ToolKind::Paint);
    }

    /// Replace the dialog's contents with saved clip `name`.
    pub(in crate::editor) fn load_clip_into_editor(&mut self, name: &str) {
        if let Some(clip) = self.sprites.clips.get(name) {
            self.clip_editor = Some(ClipEditor::open(clip));
        }
    }

    /// The next tileset (by name, wrapping) for frames to come from — only
    /// while the clip has no frames yet: every frame names a region of ONE
    /// tileset, so switching under existing frames would silently point
    /// them at regions that may not exist.
    pub(in crate::editor) fn cycle_clip_tileset(&mut self) {
        let names: Vec<String> = self.sprites.tilesets.keys().cloned().collect();
        let Some(ce) = self.clip_editor.as_mut() else { return };
        if !ce.frames.is_empty() {
            ce.status =
                Some((false, "remove this clip's frames before switching tileset".to_string()));
            return;
        }
        if names.is_empty() {
            return;
        }
        let next = match &ce.tileset {
            Some(cur) => {
                names.iter().position(|n| n == cur).map(|i| (i + 1) % names.len()).unwrap_or(0)
            }
            None => 0,
        };
        ce.tileset = Some(names[next].clone());
    }

    /// [ Save ]: write the clip, reload the project's clips (so the canvas
    /// and palette pick up the change at once), and report it in the dialog.
    /// Returns the saved clip's name.
    pub(in crate::editor) fn save_clip(&mut self) -> Option<String> {
        let folder = self.project_folder.clone()?;
        let ce = self.clip_editor.as_mut()?;
        let result = (|| -> Result<String, String> {
            let clip = ce.to_clip()?;
            let dir = SpriteAssets::clip_dir(&folder);
            std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            let path = dir.join(format!("{}.ron", clip.name));
            let text = ron::ser::to_string_pretty(&clip, ron::ser::PrettyConfig::new())
                .map_err(|e| e.to_string())?;
            std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(clip.name)
        })();
        match result {
            Ok(name) => {
                ce.status = Some((true, format!("Saved assets/clips/{name}.ron")));
                for problem in self.sprites.reload(Some(&folder)) {
                    self.console_log.push(LogEntry::warn(problem));
                }
                Some(name)
            }
            Err(e) => {
                ce.status = Some((false, e));
                None
            }
        }
    }

    /// [ Add to Palette ]: save, then add a palette entry that paints this
    /// clip (one undoable palette change; skipped if an entry for this clip
    /// already exists). Its still `sprite` is the clip's first frame — what
    /// a tile shows if the clip itself can't be found.
    pub(in crate::editor) fn add_clip_to_palette(&mut self) {
        let Some(name) = self.save_clip() else { return };
        let Some(clip) = self.sprites.clips.get(&name).cloned() else { return };
        if self.palette.tiles.iter().any(|t| t.clip.as_deref() == Some(name.as_str())) {
            if let Some(ce) = self.clip_editor.as_mut() {
                ce.status = Some((true, format!("Saved; '{name}' is already in the palette")));
            }
            return;
        }
        let before = self.palette.clone();
        self.palette.tiles.push(TileDefinition {
            name: name.clone(),
            glyph: name.chars().next().unwrap_or('*'),
            fg: Color::White,
            bg: Color::Reset,
            solid: false,
            trigger: false,
            tag: String::new(),
            sprite: clip.frames.first().map(|r| SpriteRef::new(clip.tileset.clone(), r.clone())),
            clip: Some(name.clone()),
        });
        self.undo.push(Command::UpdatePalette { before, after: self.palette.clone() });
        self.save_palette();
        if let Some(ce) = self.clip_editor.as_mut() {
            ce.status = Some((true, format!("Saved and added '{name}' to the palette")));
        }
    }
}
