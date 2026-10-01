// editor/assets.rs — what a File Browser row IS as an asset (an image, a
// tileset, an animation clip), the thumbnail it shows, the one-line
// description its preview shows, and the drag the user can start from it.
//
// ── WHY (Step 8-4, docs/ember2d-master-plan.md §5.7) ─────────────────────────
//
// Until 8-4 the File Browser listed only levels, scripts, `project.ron` and
// palettes — the images and `.ron` assets 8-2/8-3 write into
// `assets/tilesets/` and `assets/clips/` were invisible in it, and nothing
// could be dragged anywhere. 8-4 lists them with a thumbnail (a clip's
// thumbnail plays), previews the selected one, and lets one be dragged onto
// the palette (adds palette entries) or onto a canvas tile (paints it).
//
// Kept apart from the drawing (`ui/panels/dock.rs`) and from the drop
// actions (`impl_state/asset_drop.rs`): this file is the pure part — which
// row is which asset, and what to show for it — so it has plain unit tests.
//
// A row's KIND rides in the 3-character prefix `refresh_project_files`
// gives every listed name (the File Browser's existing convention: `/ ` a
// folder, `[] ` a level, `{} ` a script, `:: ` a project file). 8-4 adds
// `<> ` an image, `## ` a tileset file and `~~ ` a clip file.

use std::collections::BTreeMap;
use std::path::Path;

use ember2d::renderer::Texture;
use ember2d_sim::clip_asset::CLIP_DIR;
use ember2d_sim::math::Rect;
use ember2d_sim::tileset::TILESET_DIR;

use super::sprites::SpriteAssets;

/// Row prefixes for the three asset kinds (see the header comment).
pub const IMAGE_PREFIX: &str = "<> ";
pub const TILESET_PREFIX: &str = "## ";
pub const CLIP_PREFIX: &str = "~~ ";

/// The image formats the File Browser lists and thumbnails — the same set
/// File > Import Tileset... offers in its OS picker.
pub const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "bmp", "gif"];

/// One draggable asset.
#[derive(Debug, Clone, PartialEq)]
pub enum AssetRef {
    /// An image file, by its path relative to the project root (forward
    /// slashes) — what the File Browser thumbnail cache is keyed by.
    Image(String),
    /// A tileset, by name (its `assets/tilesets/<name>.ron` file stem).
    Tileset(String),
    /// An animation clip, by name (`assets/clips/<name>.ron`).
    Clip(String),
}

impl AssetRef {
    /// The label the drag ghost and the preview show.
    pub fn label(&self) -> String {
        match self {
            AssetRef::Image(rel) => {
                Path::new(rel).file_name().and_then(|n| n.to_str()).unwrap_or(rel).to_string()
            }
            AssetRef::Tileset(name) => format!("tileset {name}"),
            AssetRef::Clip(name) => format!("clip {name}"),
        }
    }
}

/// A drag that started on a File Browser asset row. It only becomes a real
/// drag (`active`) once the mouse moves `DRAG_THRESHOLD_PT` away from where
/// it was pressed, so a plain click on an asset row just selects it.
#[derive(Debug, Clone, PartialEq)]
pub struct AssetDrag {
    pub asset: AssetRef,
    /// Where the press happened, in points.
    pub start_pt: (f32, f32),
    pub active: bool,
}

/// How far (points) the mouse must travel before a press becomes a drag.
pub const DRAG_THRESHOLD_PT: f32 = 6.0;

/// `folder` relative to the project root, normalised: forward slashes, no
/// leading `./`, and `.` for the root itself.
fn normalise(folder: &str) -> String {
    let f = folder.replace('\\', "/");
    let f = f.trim_start_matches("./").trim_end_matches('/');
    if f.is_empty() {
        ".".to_string()
    } else {
        f.to_string()
    }
}

/// The prefix a file named `name` in project folder `folder` gets in the
/// File Browser, if it's one of 8-4's asset kinds. A `.ron` counts as a
/// tileset/clip only where 8-2/8-3 keep those (`assets/tilesets/`,
/// `assets/clips/`) — `.ron` elsewhere is some other data file.
pub fn asset_prefix(folder: &str, name: &str) -> Option<&'static str> {
    let ext = Path::new(name).extension()?.to_str()?.to_ascii_lowercase();
    if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        return Some(IMAGE_PREFIX);
    }
    if ext == "ron" {
        let folder = normalise(folder);
        if folder == TILESET_DIR {
            return Some(TILESET_PREFIX);
        }
        if folder == CLIP_DIR {
            return Some(CLIP_PREFIX);
        }
    }
    None
}

/// The asset a File Browser row (`raw` as `refresh_project_files` listed
/// it, in project folder `folder`) stands for, if any.
pub fn classify(folder: &str, raw: &str) -> Option<AssetRef> {
    let prefix = raw.get(..3)?;
    let name = raw.get(3..)?.trim();
    if name.is_empty() {
        return None;
    }
    let folder = normalise(folder);
    let rel = if folder == "." { name.to_string() } else { format!("{folder}/{name}") };
    let stem = Path::new(name).file_stem()?.to_str()?.to_string();
    match prefix {
        IMAGE_PREFIX => Some(AssetRef::Image(rel)),
        TILESET_PREFIX => Some(AssetRef::Tileset(stem)),
        CLIP_PREFIX => Some(AssetRef::Clip(stem)),
        _ => None,
    }
}

/// The tileset (if any) whose sheet is the project image at `rel` — so an
/// image that's already been imported drags as its tileset, not as a raw
/// picture to import a second time.
pub fn tileset_for_image(sprites: &SpriteAssets, rel: &str) -> Option<String> {
    let rel = normalise(rel);
    sprites
        .tilesets
        .values()
        .find(|t| normalise(&format!("{TILESET_DIR}/{}", t.data.image)) == rel)
        .map(|t| t.data.name.clone())
}

/// The picture to draw for `asset`: a whole image, a tileset's whole sheet,
/// or the frame a clip shows `t` seconds into playback.
pub fn thumbnail<'a>(
    asset: &AssetRef,
    sprites: &'a SpriteAssets,
    images: &'a BTreeMap<String, Texture>,
    t: f32,
) -> Option<(&'a Texture, Rect)> {
    let whole = |tex: &'a Texture| (tex, Rect::new(0.0, 0.0, tex.width as f32, tex.height as f32));
    match asset {
        AssetRef::Image(rel) => images.get(rel).map(whole),
        AssetRef::Tileset(name) => sprites.tilesets.get(name)?.texture.as_ref().map(whole),
        AssetRef::Clip(name) => sprites.clip_frame(name, t),
    }
}

/// The preview's one-line description of `asset`.
pub fn describe(
    asset: &AssetRef,
    sprites: &SpriteAssets,
    images: &BTreeMap<String, Texture>,
) -> String {
    match asset {
        AssetRef::Image(rel) => {
            let size = match images.get(rel) {
                Some(t) => format!("{}x{} px", t.width, t.height),
                None => "image (could not be loaded)".to_string(),
            };
            // Kept short: the preview pane is narrow, and the drag ghost
            // already says what a drop will do.
            let stem = Path::new(rel).file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            match tileset_for_image(sprites, rel) {
                Some(ts) => format!("{size}, sheet of tileset '{ts}'"),
                None if sprites.tilesets.contains_key(stem) => {
                    format!("{size}, tileset '{stem}' has its name")
                }
                None => format!("{size}, not imported"),
            }
        }
        AssetRef::Tileset(name) => match sprites.tilesets.get(name) {
            Some(t) => {
                let d = &t.data;
                format!("{} regions, {}x{} px cells", d.regions.len(), d.cell_w, d.cell_h)
            }
            None => "not loaded - see the console".to_string(),
        },
        AssetRef::Clip(name) => match sprites.clips.get(name) {
            Some(c) => format!(
                "{} frames, {} fps, {}",
                c.frames.len(),
                c.fps,
                if c.looping { "loops" } else { "once" }
            ),
            None => "not loaded - see the console".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_prefix_lists_images_anywhere_and_ron_only_in_asset_folders() {
        assert_eq!(asset_prefix(".", "hero.PNG"), Some(IMAGE_PREFIX));
        assert_eq!(asset_prefix("art/chars", "hero.jpg"), Some(IMAGE_PREFIX));
        assert_eq!(asset_prefix("assets/tilesets", "fx.ron"), Some(TILESET_PREFIX));
        assert_eq!(asset_prefix("./assets/clips/", "pulse.ron"), Some(CLIP_PREFIX));
        assert_eq!(asset_prefix("assets\\clips", "pulse.ron"), Some(CLIP_PREFIX));
        assert_eq!(asset_prefix(".", "project.ron"), None);
        assert_eq!(asset_prefix("assets", "notes.txt"), None);
        assert_eq!(asset_prefix(".", "README"), None);
    }

    #[test]
    fn classify_reads_the_row_prefix_and_builds_project_relative_paths() {
        assert_eq!(classify(".", "<> hero.png "), Some(AssetRef::Image("hero.png".to_string())));
        assert_eq!(
            classify("assets/tilesets", "<> fx.png "),
            Some(AssetRef::Image("assets/tilesets/fx.png".to_string()))
        );
        assert_eq!(
            classify("assets/tilesets", "## fx.ron "),
            Some(AssetRef::Tileset("fx".to_string()))
        );
        assert_eq!(
            classify("assets/clips", "~~ pulse.ron "),
            Some(AssetRef::Clip("pulse".to_string()))
        );
        assert_eq!(classify(".", "[] floor1.level "), None);
        assert_eq!(classify(".", "/ assets "), None);
        assert_eq!(classify(".", ".. [UP]"), None);
        assert_eq!(classify(".", "<>"), None);
    }

    #[test]
    fn labels_name_the_kind() {
        assert_eq!(AssetRef::Image("a/b/hero.png".into()).label(), "hero.png");
        assert_eq!(AssetRef::Tileset("fx".into()).label(), "tileset fx");
        assert_eq!(AssetRef::Clip("pulse".into()).label(), "clip pulse");
    }
}
