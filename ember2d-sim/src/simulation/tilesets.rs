// simulation/tilesets.rs — resolving a tile's `SpriteRef` (tileset + region
// name) to the image path + pixel rect the renderer draws, at level load.
//
// Step 8-2 (docs/ember2d-master-plan.md §5.7). A child module of
// simulation.rs, like spawn.rs and step.rs, used only by `do_on_start`.
//
// ── WHERE A TILESET IS FOUND ─────────────────────────────────────────────────
//
// A tileset lives at `<project>/assets/tilesets/<name>.ron`, but a level only
// knows its own path, and a level isn't always at the project root. So the
// search walks UP from the level's own directory — `dir/assets/tilesets/`,
// then the parent's, and so on — and finally tries the working directory
// (how the demos are run: `cargo run -- demos/...` from the repo root, the
// same CWD fallback `resolve_exit_path` gives every other level-authored
// path). The first one that exists wins. Every check and read goes through
// the `LevelSource` the simulation was given — this crate never touches the
// filesystem itself (CLAUDE.md, determinism rules); `std::path` here only
// joins strings.
//
// The sheet image is named relative to the tileset file's own directory (the
// importer copies it there), so it resolves against wherever the `.ron`
// was found. Step 8-3 added animation clips (`assets/clips/<name>.ron`,
// `crate::clip_asset`): found by the same upward search, their frames
// resolved through the same tileset cache.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::clip_asset::{ClipData, CLIP_DIR};
use crate::components::AnimationClip;
use crate::level_source::LevelSource;
use crate::math::Rect;
use crate::tileset::{SpriteRef, TilesetData, TILESET_DIR};

/// How many directories above the level's own the search climbs. Generous
/// for any real project layout, and a hard stop so a level at a filesystem
/// root can't loop.
const MAX_ASCENT: usize = 8;

/// Loads each tileset (and, since Step 8-3, each animation clip) at most
/// once per level load, and reports each broken reference at most once (a
/// 40×40 wall of the same missing sprite is one problem, not 1,600 log
/// lines).
pub(super) struct TilesetResolver<'a> {
    level_path: &'a str,
    source: &'a dyn LevelSource,
    /// tileset name -> (data, directory its .ron was found in), or why not.
    cache: BTreeMap<String, Result<(TilesetData, String), String>>,
    /// Step 8-3: clip name -> the runtime clip it resolved to, or why not.
    clips: BTreeMap<String, Result<AnimationClip, String>>,
    warned: BTreeSet<(String, String)>,
    /// Step 8-3: clips already warned about, by name.
    warned_clips: BTreeSet<String>,
}

impl<'a> TilesetResolver<'a> {
    pub(super) fn new(level_path: &'a str, source: &'a dyn LevelSource) -> Self {
        TilesetResolver {
            level_path,
            source,
            cache: BTreeMap::new(),
            clips: BTreeMap::new(),
            warned: BTreeSet::new(),
            warned_clips: BTreeSet::new(),
        }
    }

    /// Tileset `name` (loaded and cached) and its sheet image's resolved
    /// path.
    fn tileset(&mut self, name: &str) -> Result<(&TilesetData, String), String> {
        if !self.cache.contains_key(name) {
            let loaded = self.load_tileset(name);
            self.cache.insert(name.to_string(), loaded);
        }
        let (data, dir) = self.cache[name].as_ref().map_err(|e| e.clone())?;
        let image = if Path::new(&data.image).is_absolute() || dir.is_empty() {
            data.image.clone()
        } else {
            Path::new(dir).join(&data.image).to_string_lossy().into_owned()
        };
        Ok((data, image))
    }

    /// The image path and pixel rect `sprite` names, or a one-line reason it
    /// can't be drawn.
    pub(super) fn resolve(&mut self, sprite: &SpriteRef) -> Result<(String, Rect), String> {
        let (data, image) = self.tileset(&sprite.tileset)?;
        let rect = data.region_rect(&sprite.region).ok_or_else(|| {
            format!("tileset '{}' has no region named '{}'", sprite.tileset, sprite.region)
        })?;
        Ok((image, rect))
    }

    /// `resolve`, plus a warning into `logs` the first time a given
    /// reference fails — what `do_on_start` calls. `None` means "draw the
    /// tile's glyph instead".
    pub(super) fn resolve_or_warn(
        &mut self,
        sprite: &SpriteRef,
        logs: &mut Vec<crate::scripting::LogEntry>,
    ) -> Option<(String, Rect)> {
        match self.resolve(sprite) {
            Ok(found) => Some(found),
            Err(why) => {
                if self.warned.insert((sprite.tileset.clone(), sprite.region.clone())) {
                    logs.push(crate::scripting::LogEntry::warn(format!(
                        "Tile sprite {}/{}: {} — drawing its glyph instead",
                        sprite.tileset, sprite.region, why
                    )));
                }
                None
            }
        }
    }

    /// Step 8-3: clip `name` (`assets/clips/<name>.ron`) as a runtime
    /// `AnimationClip` — its frames' regions looked up in its tileset, the
    /// same way a tile sprite's are.
    pub(super) fn resolve_clip(&mut self, name: &str) -> Result<AnimationClip, String> {
        if let Some(done) = self.clips.get(name) {
            return done.clone();
        }
        let result = self.load_clip(name);
        self.clips.insert(name.to_string(), result.clone());
        result
    }

    /// `resolve_clip`, plus a warning the first time a clip fails. `None`
    /// means "draw the tile without animation".
    pub(super) fn resolve_clip_or_warn(
        &mut self,
        name: &str,
        logs: &mut Vec<crate::scripting::LogEntry>,
    ) -> Option<AnimationClip> {
        match self.resolve_clip(name) {
            Ok(clip) => Some(clip),
            Err(why) => {
                if self.warned_clips.insert(name.to_string()) {
                    logs.push(crate::scripting::LogEntry::warn(format!(
                        "Tile animation '{name}': {why} — drawing the tile unanimated"
                    )));
                }
                None
            }
        }
    }

    fn load_clip(&mut self, name: &str) -> Result<AnimationClip, String> {
        let path = self.find(CLIP_DIR, name).ok_or_else(|| {
            format!("clip '{name}' not found (looked for {CLIP_DIR}/{name}.ron above the level)")
        })?;
        let text = self.source.read_to_string(&path)?;
        let data: ClipData =
            ron::de::from_str(&text).map_err(|e| format!("{path} is not a valid clip: {e}"))?;
        data.validate().map_err(|e| format!("{path}: {e}"))?;
        let (tileset, image) = self.tileset(&data.tileset)?;
        data.to_animation_clip(tileset, &image)
    }

    fn load_tileset(&self, name: &str) -> Result<(TilesetData, String), String> {
        let p = self.find(TILESET_DIR, name).ok_or_else(|| {
            format!(
                "tileset '{name}' not found (looked for {TILESET_DIR}/{name}.ron above the level)"
            )
        })?;
        let text = self.source.read_to_string(&p)?;
        let data: TilesetData =
            ron::de::from_str(&text).map_err(|e| format!("{p} is not a valid tileset: {e}"))?;
        data.validate().map_err(|e| format!("{p}: {e}"))?;
        let dir =
            Path::new(&p).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
        Ok((data, dir))
    }

    /// `<asset_dir>/<name>.ron`, searched upward from the level's own
    /// directory, then from the working directory (see this file's header);
    /// the first that exists.
    fn find(&self, asset_dir: &str, name: &str) -> Option<String> {
        let file = format!("{name}.ron");
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        let mut dir = Path::new(self.level_path).parent();
        for _ in 0..MAX_ASCENT {
            let Some(d) = dir else { break };
            candidates.push(d.join(asset_dir).join(&file));
            dir = d.parent();
        }
        candidates.push(Path::new(asset_dir).join(&file));
        candidates
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .find(|p| self.source.exists(p))
    }
}
