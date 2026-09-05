// components/sprite.rs — Visual appearance component.
//
// A Sprite is "what this entity looks like." As of Phase 3
// (docs/ember2d-refactor-plan.md), that's a `SpriteSource` — a glyph from
// the font atlas, a loaded texture, or (Step 3c) a named animation clip —
// plus a tint, an optional explicit size, and a draw layer.
//
// SpriteSource::Texture stores the texture's *path*, not a `TextureId`:
// ids are runtime-assigned and don't survive save/load (see
// `renderer::texture::TextureId`'s own doc comment), so the path is what's
// actually persisted; resolving it to a handle is a render-time concern
// (`play.rs`'s render loop, via `AssetManager::load`).
//
// Z-ORDER (draw order, now called `layer`):
//   In a flat buffer, whoever draws last "wins" — they appear on top.
//   By sorting entities by `layer` before drawing, we get predictable
//   layering: 0 = floor tiles (drawn first), 1 = items, 2 = walls, etc.

use crate::math::{Rect, Vec2};
use crate::renderer::Color;
use serde::{Serialize, Deserialize};

/// What a Sprite draws. `tint`/`size`/`layer`/`visible` (on `Sprite` itself)
/// apply uniformly regardless of which variant this is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SpriteSource {
    /// A single glyph from the font atlas. `bg` is the cell's background
    /// color — `Color::Reset` (the existing "no override" sentinel) means
    /// transparent, same convention `Color` already uses everywhere else.
    /// The glyph's *foreground* color is `Sprite::tint`, not stored here.
    Glyph { ch: char, bg: Color },

    /// A loaded texture. `src` is an optional pixel-space sub-rect (for
    /// sprite sheets); `None` samples the whole texture.
    Texture { path: String, src: Option<Rect> },

    /// A named, script-registered animation clip (Step 3c —
    /// `ctx.register_clip` + `ctx.play_clip`/`play_clip_once`, which is what
    /// actually constructs this variant on an existing sprite).
    Clip { name: String },
}

/// The visual representation of an entity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sprite {
    pub source: SpriteSource,

    /// Foreground/tint color: a glyph's ink color, or a texture's
    /// multiplicative color tint (`Color::White` = no tint, matching how
    /// texture draws already treated an all-white `color_fg` before this).
    pub tint: Color,

    /// World-space size. `None` = natural size: a glyph is always exactly
    /// one world unit (matching the ASCII grid); a texture is its pixel
    /// dimensions divided by `ProjectData::pixels_per_unit`.
    pub size: Option<Vec2>,

    /// Draw order: lower = drawn first. Was `z_order`.
    pub layer: i32,

    /// When false, this entity is not drawn.
    pub visible: bool,
}

impl Sprite {
    /// A glyph sprite — the common case for ASCII tiles/entities.
    pub fn glyph(ch: char, tint: Color, bg: Color, layer: i32) -> Self {
        Sprite {
            source: SpriteSource::Glyph { ch, bg },
            tint,
            size: None,
            layer,
            visible: true,
        }
    }

    /// A texture sprite at natural size (see `size`'s doc comment).
    pub fn texture(path: impl Into<String>, layer: i32) -> Self {
        Sprite {
            source: SpriteSource::Texture { path: path.into(), src: None },
            tint: Color::White,
            size: None,
            layer,
            visible: true,
        }
    }

    /// Equivalent to `Sprite::glyph` — kept under the pre-Phase-3 name
    /// (and argument order) for the many existing positional call sites.
    pub fn new(glyph: char, fg: Color, bg: Color, z_order: i32) -> Self {
        Self::glyph(glyph, fg, bg, z_order)
    }

    /// Switch this sprite to a texture, builder-style.
    pub fn with_texture(mut self, path: impl Into<String>) -> Self {
        self.source = SpriteSource::Texture { path: path.into(), src: None };
        self
    }

    /// Shorthand: glyph + foreground color, transparent background, layer 0.
    pub fn simple(glyph: char, fg: Color) -> Self {
        Sprite::glyph(glyph, fg, Color::Reset, 0)
    }

    /// Set the draw layer using the builder pattern.
    pub fn with_z(mut self, z: i32) -> Self {
        self.layer = z;
        self
    }

    /// Set visibility using the builder pattern.
    pub fn hidden(mut self) -> Self {
        self.visible = false;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_constructor_produces_a_glyph_source_with_no_explicit_size() {
        let sp = Sprite::glyph('@', Color::Green, Color::Reset, 3);
        assert_eq!(sp.source, SpriteSource::Glyph { ch: '@', bg: Color::Reset });
        assert_eq!(sp.tint, Color::Green);
        assert_eq!(sp.layer, 3);
        assert_eq!(sp.size, None, "glyphs are always natural (1x1) size, not an explicit override");
        assert!(sp.visible);
    }

    #[test]
    fn texture_constructor_produces_a_texture_source_with_white_tint() {
        let sp = Sprite::texture("assets/coin.png", 5);
        assert_eq!(sp.source, SpriteSource::Texture { path: "assets/coin.png".to_string(), src: None });
        assert_eq!(sp.tint, Color::White, "white tint means \"no tint\" — a texture shows its own colors by default");
        assert_eq!(sp.layer, 5);
    }

    #[test]
    fn new_is_equivalent_to_glyph_for_the_many_existing_positional_call_sites() {
        assert_eq!(Sprite::new('x', Color::Red, Color::Black, 1).source, Sprite::glyph('x', Color::Red, Color::Black, 1).source);
    }

    #[test]
    fn with_texture_switches_an_existing_sprite_to_a_texture_source() {
        let sp = Sprite::glyph('#', Color::White, Color::Reset, 0).with_texture("wall.png");
        assert_eq!(sp.source, SpriteSource::Texture { path: "wall.png".to_string(), src: None });
    }
}
