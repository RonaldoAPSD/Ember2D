// renderer/texture.rs — Texture management for Sprites2D mode.

use std::path::Path;
use image::GenericImageView;

/// A simple RGBA texture stored in CPU memory as a flat Vec<u32>.
/// Colors are stored as 0xAARRGGBB for compatibility with minifb's buffer.
#[derive(Clone)]
pub struct Texture {
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
                // Convert [r, g, b, a] to 0xAARRGGBB
                let argb = ((p[3] as u32) << 24) |
                           ((p[0] as u32) << 16) |
                           ((p[1] as u32) << 8)  |
                            (p[2] as u32);
                pixels.push(argb);
            }
        }

        Ok(Texture { width, height, pixels })
    }

    /// Create a 1x1 solid color texture.
    pub fn solid(color: u32) -> Self {
        Texture {
            width: 1,
            height: 1,
            pixels: vec![color],
        }
    }
}
