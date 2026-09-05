// renderer/texture.rs — Texture management for Sprites2D mode.

use std::path::Path;
use image::GenericImageView;

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A stable runtime handle for a loaded texture, backed by `Texture::id`.
/// Resolving one (`AssetManager::get`) is a plain integer `HashMap` lookup —
/// the point (Phase 3, docs/ember2d-refactor-plan.md) is a cleaner, typed
/// alternative to passing raw path strings around at every draw call.
///
/// Deliberately NOT `Serialize`/`Deserialize`: ids are assigned at runtime
/// (`NEXT_ID`) and aren't stable across process restarts or save/load, so a
/// persisted `Sprite` must keep storing the texture's *path* (portable,
/// re-resolvable) — a `TextureId` is a render-time convenience derived from
/// that path, never the thing on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

/// A simple RGBA texture stored in CPU memory as a flat Vec<u32>.
/// Colors are stored as 0xRRGGBBAA for easier GPU upload.
#[derive(Clone)]
pub struct Texture {
    pub id: u64,
    pub width:  u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Texture {
    /// Load a texture from a file (PNG, JPG, etc).
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let img = image::open(path).map_err(|e| e.to_string())?;
        let (width, height) = img.dimensions();
        let mut pixels = Vec::with_capacity((width * height) as usize);

        for y in 0..height {
            for x in 0..width {
                let p = img.get_pixel(x, y);
                // Store as 0xAABBGGRR so little-endian bytes are [R, G, B, A]
                // This matches wgpu::TextureFormat::Rgba8Unorm expectations.
                let rgba = ((p[3] as u32) << 24) |
                           ((p[2] as u32) << 16) |
                           ((p[1] as u32) << 8)  |
                            (p[0] as u32);
                pixels.push(rgba);
            }
        }

        Ok(Texture { 
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width, 
            height, 
            pixels 
        })
    }

    /// Create a 1x1 solid color texture.
    pub fn solid(color: u32) -> Self {
        Texture {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width: 1,
            height: 1,
            pixels: vec![color],
        }
    }
}
