// renderer/assets.rs — Asset management and texture caching.

use std::collections::HashMap;
use crate::renderer::texture::{Texture, TextureId};

/// Manages loaded textures to avoid redundant disk I/O and memory usage.
///
/// Id-primary as of Phase 3 (docs/ember2d-refactor-plan.md): `textures` is
/// keyed by the numeric id every `Texture` already carries, with
/// `path_to_id` as a secondary index purely for `load`'s dedup-by-path
/// check. `get(TextureId)` is the lookup the render path uses every frame;
/// `load(path)` is the one place path strings still matter.
pub struct AssetManager {
    textures: HashMap<u64, Texture>,
    path_to_id: HashMap<String, u64>,
}

impl AssetManager {
    pub fn new() -> Self {
        AssetManager {
            textures: HashMap::new(),
            path_to_id: HashMap::new(),
        }
    }

    /// Resolve `path` to a stable `TextureId`, loading from disk on first
    /// access and caching thereafter under both maps. A failed load caches a
    /// 1x1 magenta placeholder under that same path, so a missing/bad
    /// texture logs once and then just renders as an obvious placeholder —
    /// never a different failure (or a repeated log) on every later call.
    pub fn load(&mut self, path: &str) -> TextureId {
        if let Some(&id) = self.path_to_id.get(path) {
            return TextureId(id);
        }

        let texture = match Texture::load(path) {
            Ok(tex) => tex,
            Err(e) => {
                eprintln!("Failed to load texture '{}': {}", path, e);
                Texture::solid(0xFFFF00FF)
            }
        };

        let id = texture.id;
        self.path_to_id.insert(path.to_string(), id);
        self.textures.insert(id, texture);
        TextureId(id)
    }

    /// Resolve a handle back to its texture data. `None` only means `id`
    /// didn't come from this `AssetManager` instance — every id `load` hands
    /// out is guaranteed present here afterward (`clear` aside).
    pub fn get(&self, id: TextureId) -> Option<&Texture> {
        self.textures.get(&id.0)
    }

    /// Path-keyed convenience wrapper matching the pre-Phase-3 API shape.
    /// Kept only until Step 3b migrates play.rs's one remaining caller onto
    /// `load`/`get` directly — new code should prefer those.
    pub fn load_texture(&mut self, path: &str) -> Result<&Texture, String> {
        let id = self.load(path);
        Ok(self.textures.get(&id.0).expect("load() always inserts before returning an id"))
    }

    /// Clear all cached assets.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.path_to_id.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_a_resolvable_id() {
        let mut assets = AssetManager::new();
        // A path that can't possibly exist — exercises the placeholder
        // fallback path, which is the only one testable without real image
        // fixtures on disk.
        let id = assets.load("__no_such_texture__.png");
        let tex = assets.get(id).expect("load() must insert before returning");
        assert_eq!((tex.width, tex.height), (1, 1), "a failed load should cache the 1x1 placeholder");
    }

    #[test]
    fn loading_the_same_path_twice_returns_the_same_id() {
        let mut assets = AssetManager::new();
        let a = assets.load("__missing_a__.png");
        let b = assets.load("__missing_a__.png");
        assert_eq!(a, b, "the same path must dedupe to the same handle, not allocate a new texture");
    }

    #[test]
    fn different_paths_get_different_ids() {
        let mut assets = AssetManager::new();
        let a = assets.load("__missing_a__.png");
        let b = assets.load("__missing_b__.png");
        assert_ne!(a, b);
    }

    #[test]
    fn get_returns_none_for_an_unknown_id() {
        let assets = AssetManager::new();
        assert!(assets.get(TextureId(999_999)).is_none());
    }

    #[test]
    fn clear_invalidates_previously_loaded_handles() {
        let mut assets = AssetManager::new();
        let id = assets.load("__missing__.png");
        assets.clear();
        assert!(assets.get(id).is_none());
    }
}
