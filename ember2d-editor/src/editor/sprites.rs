// editor/sprites.rs — the editor's own view of a project's tilesets: every
// `assets/tilesets/*.ron` it found, and the sheet image each one slices,
// loaded once and kept for drawing.
//
// Step 8-2 (docs/ember2d-master-plan.md §5.7). Before 8-2 the editor never
// drew an image (its canvas was glyphs only, and the one texture it held was
// the theme's chrome atlas); with tileset sprites it has to draw them on the
// canvas, as palette thumbnails, and in the importer. Play mode resolves the
// same `SpriteRef`s through `ember2d-sim`'s `TilesetResolver` at level load —
// this is the editor-side twin, kept separate because the editor reads
// tilesets straight from the project folder (it owns filesystem access the
// sim doesn't have) and re-reads them the moment the importer writes one,
// without reloading the level.
//
// Textures are owned here, one per sheet image, the same way
// `EditorState::theme_chrome_tex` owns the chrome atlas: `Texture` is a CPU
// pixel buffer with a stable `id`, and the renderer uploads it to the GPU on
// first draw — no `AssetManager` needed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ember2d::renderer::Texture;
use ember2d_sim::clip_asset::{ClipData, CLIP_DIR};
use ember2d_sim::math::Rect;
use ember2d_sim::tileset::{SpriteRef, TilesetData, TILESET_DIR};

/// One tileset the editor can draw from.
pub struct LoadedTileset {
    pub data: TilesetData,
    /// The sheet, or `None` if the image failed to load (the tileset still
    /// lists its regions; tiles using it fall back to their glyph).
    pub texture: Option<Texture>,
}

#[derive(Default)]
pub struct SpriteAssets {
    /// By tileset name — a `BTreeMap` so anything listing them (the
    /// palette's auto-added entries) sees a stable order.
    pub tilesets: BTreeMap<String, LoadedTileset>,
    /// Step 8-3: the project's animation clips (`assets/clips/*.ron`), by
    /// name — what animated tiles and palette entries play, and what the
    /// clip editor lists.
    pub clips: BTreeMap<String, ClipData>,
}

impl SpriteAssets {
    /// `<project>/assets/tilesets/` for `project_folder`.
    pub fn dir(project_folder: &str) -> PathBuf {
        Path::new(project_folder).join(TILESET_DIR)
    }

    /// Step 8-3: `<project>/assets/clips/` for `project_folder`.
    pub fn clip_dir(project_folder: &str) -> PathBuf {
        Path::new(project_folder).join(CLIP_DIR)
    }

    /// Re-scan `project_folder`'s tileset directory from scratch. Returns a
    /// message per file that couldn't be used (bad RON, failed validation,
    /// an image that won't load), for the console. No folder, or no
    /// tileset directory yet, is just "no tilesets" — not an error.
    pub fn reload(&mut self, project_folder: Option<&str>) -> Vec<String> {
        self.tilesets.clear();
        self.clips.clear();
        let mut problems = Vec::new();
        let Some(folder) = project_folder else { return problems };
        // Step 8-3: clips too — each only needs to parse and validate here;
        // a frame naming a region its tileset lacks just draws nothing for
        // that frame (and the clip editor shows which).
        for path in ron_files(&Self::clip_dir(folder)) {
            let loaded = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|t| ron::de::from_str::<ClipData>(&t).map_err(|e| e.to_string()))
                .and_then(|c| c.validate().map(|_| c));
            match loaded {
                Ok(c) => {
                    self.clips.insert(c.name.clone(), c);
                }
                Err(e) => problems.push(format!("Clip {}: {e}", path.display())),
            }
        }
        for path in ron_files(&Self::dir(folder)) {
            match Self::load_one(&path) {
                Ok(t) => {
                    if t.texture.is_none() {
                        problems.push(format!(
                            "Tileset '{}': image '{}' could not be loaded",
                            t.data.name, t.data.image
                        ));
                    }
                    self.tilesets.insert(t.data.name.clone(), t);
                }
                Err(e) => problems.push(e),
            }
        }
        problems
    }

    fn load_one(path: &Path) -> Result<LoadedTileset, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Tileset {}: {e}", path.display()))?;
        let data: TilesetData =
            ron::de::from_str(&text).map_err(|e| format!("Tileset {}: {e}", path.display()))?;
        data.validate().map_err(|e| format!("Tileset {}: {e}", path.display()))?;
        let image = path.parent().unwrap_or(Path::new("")).join(&data.image);
        let texture = Texture::load(&image).ok();
        Ok(LoadedTileset { data, texture })
    }

    /// The texture and pixel rect `sprite` names, if both its tileset (with
    /// a loaded image) and its region exist.
    pub fn resolve(&self, sprite: &SpriteRef) -> Option<(&Texture, Rect)> {
        let t = self.tilesets.get(&sprite.tileset)?;
        let rect = t.data.region_rect(&sprite.region)?;
        Some((t.texture.as_ref()?, rect))
    }

    /// Step 8-3: which frame of clip `name` is showing `t` seconds into
    /// playback — `floor(t * fps)`, wrapped for a looping clip, held on the
    /// last frame for a one-shot — the same arithmetic as the game's own
    /// `Animator`, so the canvas plays a clip at the speed the game will.
    pub fn clip_frame_index(clip: &ClipData, t: f32) -> usize {
        let n = clip.frames.len().max(1);
        let i = (t.max(0.0) * clip.fps).floor() as usize;
        if clip.looping {
            i % n
        } else {
            i.min(n - 1)
        }
    }

    /// Step 8-3: the texture and rect clip `name` shows `t` seconds into
    /// playback, if the clip, its tileset, and that frame's region exist.
    pub fn clip_frame(&self, name: &str, t: f32) -> Option<(&Texture, Rect)> {
        let clip = self.clips.get(name)?;
        let region = clip.frames.get(Self::clip_frame_index(clip, t))?;
        self.resolve(&SpriteRef::new(clip.tileset.clone(), region.clone()))
    }
}

/// Every `*.ron` directly in `dir`, sorted (a missing dir is just empty).
fn ron_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("ron"))
        .collect();
    files.sort();
    files
}

/// The largest rect of aspect `src_w`:`src_h` that fits inside `slot`,
/// centered — so a thumbnail never stretches a sprite out of shape.
pub fn fit_inside(slot: Rect, src_w: f32, src_h: f32) -> Rect {
    if src_w <= 0.0 || src_h <= 0.0 {
        return slot;
    }
    let scale = (slot.w / src_w).min(slot.h / src_h);
    let (w, h) = (src_w * scale, src_h * scale);
    Rect::new(slot.x + (slot.w - w) * 0.5, slot.y + (slot.h - h) * 0.5, w, h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_inside_keeps_aspect_and_centers() {
        let r = fit_inside(Rect::new(0.0, 0.0, 30.0, 20.0), 16.0, 16.0);
        assert_eq!(r, Rect::new(5.0, 0.0, 20.0, 20.0));
        let r = fit_inside(Rect::new(10.0, 10.0, 20.0, 40.0), 32.0, 16.0);
        assert_eq!(r, Rect::new(10.0, 25.0, 20.0, 10.0));
    }

    #[test]
    fn reload_without_a_project_or_a_tileset_dir_is_silent() {
        let mut s = SpriteAssets::default();
        assert!(s.reload(None).is_empty());
        let dir = std::env::temp_dir()
            .join(format!("ember2d-{}", std::process::id()))
            .join("no_tilesets");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(s.reload(dir.to_str()).is_empty());
        assert!(s.tilesets.is_empty());
    }
}
