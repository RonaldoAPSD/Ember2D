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
// was found.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::level_source::LevelSource;
use crate::math::Rect;
use crate::tileset::{SpriteRef, TilesetData, TILESET_DIR};

/// How many directories above the level's own the search climbs. Generous
/// for any real project layout, and a hard stop so a level at a filesystem
/// root can't loop.
const MAX_ASCENT: usize = 8;

/// Loads each tileset at most once per level load, and reports each broken
/// reference at most once (a 40×40 wall of the same missing sprite is one
/// problem, not 1,600 log lines).
pub(super) struct TilesetResolver<'a> {
    level_path: &'a str,
    source: &'a dyn LevelSource,
    /// tileset name -> (data, directory its .ron was found in), or why not.
    cache: BTreeMap<String, Result<(TilesetData, String), String>>,
    warned: BTreeSet<(String, String)>,
}

impl<'a> TilesetResolver<'a> {
    pub(super) fn new(level_path: &'a str, source: &'a dyn LevelSource) -> Self {
        TilesetResolver { level_path, source, cache: BTreeMap::new(), warned: BTreeSet::new() }
    }

    /// The image path and pixel rect `sprite` names, or a one-line reason it
    /// can't be drawn.
    pub(super) fn resolve(&mut self, sprite: &SpriteRef) -> Result<(String, Rect), String> {
        if !self.cache.contains_key(&sprite.tileset) {
            let loaded = self.load(&sprite.tileset);
            self.cache.insert(sprite.tileset.clone(), loaded);
        }
        let (data, dir) = self.cache[&sprite.tileset].as_ref().map_err(|e| e.clone())?;
        let rect = data.region_rect(&sprite.region).ok_or_else(|| {
            format!("tileset '{}' has no region named '{}'", sprite.tileset, sprite.region)
        })?;
        let image = if Path::new(&data.image).is_absolute() || dir.is_empty() {
            data.image.clone()
        } else {
            Path::new(dir).join(&data.image).to_string_lossy().into_owned()
        };
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

    fn load(&self, name: &str) -> Result<(TilesetData, String), String> {
        let file = format!("{name}.ron");
        let mut candidates: Vec<std::path::PathBuf> = Vec::new();
        let mut dir = Path::new(self.level_path).parent();
        for _ in 0..MAX_ASCENT {
            let Some(d) = dir else { break };
            candidates.push(d.join(TILESET_DIR).join(&file));
            dir = d.parent();
        }
        candidates.push(Path::new(TILESET_DIR).join(&file));

        for path in candidates {
            let p = path.to_string_lossy().into_owned();
            if !self.source.exists(&p) {
                continue;
            }
            let text = self.source.read_to_string(&p)?;
            let data: TilesetData =
                ron::de::from_str(&text).map_err(|e| format!("{p} is not a valid tileset: {e}"))?;
            data.validate().map_err(|e| format!("{p}: {e}"))?;
            let dir = path.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
            return Ok((data, dir));
        }
        Err(format!("tileset '{name}' not found (looked for {TILESET_DIR}/{file} above the level)"))
    }
}
