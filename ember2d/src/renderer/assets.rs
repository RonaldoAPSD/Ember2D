// renderer/assets.rs — Asset management and texture caching.

use crate::renderer::texture::{Texture, TextureId};
use std::collections::HashMap;

/// R26 (7B-3, docs/ember2d-master-plan.md §5.2): lets `AssetManager::clear`
/// free every GPU-resident texture it knows about without depending on
/// `Renderer` (renderer/mod.rs) directly — constructing a real `Renderer`
/// needs a live GPU device, which a headless unit test can't do, so
/// `clear`'s own test uses a trivial recording mock instead. `Renderer` is
/// the only real implementor.
pub trait TextureEvictor {
    fn evict_texture(&mut self, id: u64);
}

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
        AssetManager { textures: HashMap::new(), path_to_id: HashMap::new() }
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

    /// Clear all cached assets, including their GPU-resident copies via
    /// `evictor` (R26, 7B-3, docs/ember2d-master-plan.md §5.2). This used
    /// to only clear the CPU-side maps here, leaving every
    /// previously-uploaded GPU texture resident forever — worse, a later
    /// reload of the "same" path got a fresh id (since `path_to_id` was
    /// wiped too), so the leak compounded on every call rather than at
    /// least reusing the old GPU copy.
    pub fn clear(&mut self, evictor: &mut dyn TextureEvictor) {
        for &id in self.textures.keys() {
            evictor.evict_texture(id);
        }
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
        assert_eq!(
            (tex.width, tex.height),
            (1, 1),
            "a failed load should cache the 1x1 placeholder"
        );
    }

    #[test]
    fn loading_the_same_path_twice_returns_the_same_id() {
        let mut assets = AssetManager::new();
        let a = assets.load("__missing_a__.png");
        let b = assets.load("__missing_a__.png");
        assert_eq!(
            a, b,
            "the same path must dedupe to the same handle, not allocate a new texture"
        );
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

    /// R26 (7B-3, docs/ember2d-master-plan.md §5.2): records every id it's
    /// asked to evict — stands in for `Renderer` (the real
    /// `TextureEvictor`), which needs a live GPU device to construct.
    #[derive(Default)]
    struct RecordingEvictor {
        evicted: Vec<u64>,
    }
    impl TextureEvictor for RecordingEvictor {
        fn evict_texture(&mut self, id: u64) {
            self.evicted.push(id);
        }
    }

    #[test]
    fn clear_invalidates_previously_loaded_handles() {
        let mut assets = AssetManager::new();
        let id = assets.load("__missing__.png");
        let mut evictor = RecordingEvictor::default();
        assets.clear(&mut evictor);
        assert!(assets.get(id).is_none());
    }

    #[test]
    fn clear_evicts_the_gpu_texture_for_every_loaded_id() {
        let mut assets = AssetManager::new();
        let a = assets.load("__missing_a__.png");
        let b = assets.load("__missing_b__.png");
        let mut evictor = RecordingEvictor::default();
        assets.clear(&mut evictor);

        let mut evicted = evictor.evicted;
        evicted.sort();
        let mut expected = vec![a.0, b.0];
        expected.sort();
        assert_eq!(
            evicted, expected,
            "every id clear() forgets on the CPU side must also be evicted on the GPU side (R26)"
        );
    }
}
