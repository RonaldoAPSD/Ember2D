// components/sprite.rs — Visual appearance component.
//
// A Sprite is "what this entity looks like." In ASCII rendering, that means:
//   - A single character (the glyph): '@', '#', '.', '*', '~', etc.
//   - A foreground color (the glyph's color)
//   - A background color (the cell behind the glyph)
//   - A z_order (draw order: lower = drawn first = appears "behind")
//   - A visibility flag (hidden entities still exist but aren't drawn)
//
// MULTI-CHARACTER SPRITES:
//   This Sprite stores one character. For ASCII art spanning multiple cells,
//   you'd spawn multiple entities and group them — or extend this struct with
//   a Vec<Vec<char>> for a grid of characters. For now, one char keeps it simple.
//
// Z-ORDER (draw order):
//   In a flat buffer, whoever draws last "wins" — they appear on top.
//   By sorting entities by z_order before drawing, we get predictable layering:
//     0 = floor tiles (drawn first, everything appears on top of them)
//     1 = items, pickups
//     2 = enemies, walls
//     3 = player
//     4 = UI / overlays

use crate::renderer::Color;
use serde::{Serialize, Deserialize};

/// The visual representation of an entity in the fake terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprite {
    /// The ASCII character used to represent this entity.
    pub glyph: char,

    /// The foreground (text/glyph) color.
    pub fg: Color,

    /// The background color behind the glyph.
    pub bg: Color,

    /// Draw order: lower = drawn first.
    pub z_order: i32,

    /// When false, this entity is not drawn.
    pub visible: bool,

    /// Optional animation frames. If not empty, the `glyph` is ignored
    /// and frames are cycled based on `frame_timer`.
    pub frames: Vec<char>,

    /// How many seconds each frame lasts.
    pub frame_rate: f32,

    /// Internal timer for cycling frames.
    pub frame_timer: f32,

    /// Optional path to a texture file (for Sprites2D mode).
    pub texture: Option<String>,
}

impl Sprite {
    /// Create a fully specified sprite.
    pub fn new(glyph: char, fg: Color, bg: Color, z_order: i32) -> Self {
        Sprite {
            glyph,
            fg,
            bg,
            z_order,
            visible: true,
            frames: Vec::new(),
            frame_rate: 0.1,
            frame_timer: 0.0,
            texture: None,
        }
    }

    /// Add a texture to this sprite.
    pub fn with_texture(mut self, path: impl Into<String>) -> Self {
        self.texture = Some(path.into());
        self
    }

    /// Shorthand: glyph + foreground color, transparent background, z_order 0.
    pub fn simple(glyph: char, fg: Color) -> Self {
        Sprite::new(glyph, fg, Color::Reset, 0)
    }

    /// Add animation frames to this sprite.
    pub fn with_animation(mut self, frames: Vec<char>, rate: f32) -> Self {
        self.frames = frames;
        self.frame_rate = rate;
        self
    }

    /// Set the z_order using the builder pattern.
    pub fn with_z(mut self, z: i32) -> Self {
        self.z_order = z;
        self
    }

    /// Set visibility using the builder pattern.
    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
}
