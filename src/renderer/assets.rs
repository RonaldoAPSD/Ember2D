// renderer/assets.rs — Asset management and texture caching.

use std::collections::HashMap;
use crate::renderer::texture::Texture;

/// Manages loaded textures to avoid redundant disk I/O and memory usage.
pub struct AssetManager {
    textures: HashMap<String, Texture>,
}

impl AssetManager {
    pub fn new() -> Self {
        AssetManager {
            textures: HashMap::new(),
        }
    }

    /// Get a reference to a cached texture, loading from disk on first access.
    pub fn load_texture(&mut self, path: &str) -> Result<&Texture, String> {
        if !self.textures.contains_key(path) {
            let tex = Texture::load(path)?;
            self.textures.insert(path.to_string(), tex);
        }
        Ok(self.textures.get(path).expect("just inserted"))
    }

    /// Clear all cached assets.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}
