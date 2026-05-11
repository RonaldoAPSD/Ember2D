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
            match Texture::load(path) {
                Ok(tex) => {
                    self.textures.insert(path.to_string(), tex);
                }
                Err(e) => {
                    eprintln!("Failed to load texture '{}': {}", path, e);
                    // Insert a 1x1 magenta placeholder so we don't spam errors
                    let fallback = Texture {
                        width: 1, height: 1, pixels: vec![0xFFFF00FF] // Magenta
                    };
                    self.textures.insert(path.to_string(), fallback);
                }
            }
        }
        Ok(self.textures.get(path).unwrap())
    }


    /// Clear all cached assets.
    pub fn clear(&mut self) {
        self.textures.clear();
    }
}
