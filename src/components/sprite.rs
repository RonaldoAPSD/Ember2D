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

/// The visual representation of an entity in the fake terminal.
#[derive(Debug, Clone)]
pub struct Sprite {
    /// The ASCII character used to represent this entity.
    ///
    /// Classic ASCII game glyphs:
    ///   '@' = player        '#' = wall           '.' = floor
    ///   '*' = item/star     '~' = water          '^' = spike
    ///   '!' = exclamation   '+' = cross/health   'o' = barrel
    ///   'D' = dragon        'g' = goblin         'E' = exit
    pub glyph: char,

    /// The foreground (text/glyph) color.
    pub fg: Color,

    /// The background color behind the glyph.
    /// `Color::Reset` uses the terminal's default background (usually black).
    pub bg: Color,

    /// Draw order: entities with lower z_order are drawn first (appear behind).
    /// Entities with the same z_order are drawn in an unspecified order.
    ///
    /// Suggested conventions:
    ///   0 = background / floor tiles
    ///   1 = items and pickups
    ///   2 = walls and obstacles
    ///   3 = characters (player, enemies)
    ///   4 = effects and UI overlays
    pub z_order: i32,

    /// When false, this entity is not drawn even if it exists in the world.
    /// Use this to temporarily hide entities (e.g., invisible platforms,
    /// off-screen enemies, or entities mid-animation).
    pub visible: bool,
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
        }
    }

    /// Shorthand: glyph + foreground color, transparent background, z_order 0.
    /// The most common case for simple game objects.
    pub fn simple(glyph: char, fg: Color) -> Self {
        Sprite::new(glyph, fg, Color::Reset, 0)
    }

    /// Set the z_order using the builder pattern — returns Self for chaining.
    ///
    /// Example: `Sprite::simple('@', Color::Green).with_z(3)`
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
