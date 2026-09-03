// renderer/color.rs — Color definitions, backed by real RGB values.
//
// We now also derive Serialize and Deserialize so that Color values can be
// written to and read from level files (.level RON format). This lets a
// tile record store `fg: Yellow, bg: Reset` as human-readable text on disk.

use serde::{Deserialize, Serialize};

/// The set of named colors available in ember2d.
///
/// These correspond to the classic 16 ANSI terminal colors.
/// Use `Color::Reset` for "no color override" — the engine substitutes
/// DEFAULT_FG (for foreground) or DEFAULT_BG (for background).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Color {
    Black,
    DarkRed,
    DarkGreen,
    DarkYellow,
    DarkBlue,
    DarkMagenta,
    DarkCyan,
    Grey,
    DarkGrey,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    /// Use the terminal's default color. Renderer resolves this to DEFAULT_FG or DEFAULT_BG.
    Reset,
    /// A custom 24-bit RGB color.
    Rgb(u8, u8, u8),
}

impl Color {
    /// Convert this color to a packed 32-bit RGB pixel value: 0x00RRGGBB.
    ///
    /// `default` is used when Color::Reset is encountered — pass DEFAULT_FG for
    /// foreground contexts and DEFAULT_BG for background contexts.
    pub fn to_rgb(self, default: u32) -> u32 {
        match self {
            Color::Black       => 0x000000,
            Color::DarkRed     => 0xAA0000,
            Color::DarkGreen   => 0x00AA00,
            Color::DarkYellow  => 0xAA5500,
            Color::DarkBlue    => 0x0000AA,
            Color::DarkMagenta => 0xAA00AA,
            Color::DarkCyan    => 0x00AAAA,
            Color::Grey        => 0xAAAAAA,
            Color::DarkGrey    => 0x555555,
            Color::Red         => 0xFF5555,
            Color::Green       => 0x55FF55,
            Color::Yellow      => 0xFFFF55,
            Color::Blue        => 0x5555FF,
            Color::Magenta     => 0xFF55FF,
            Color::Cyan        => 0x55FFFF,
            Color::White       => 0xFFFFFF,
            Color::Reset       => default,
            Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
        }
    }

    pub fn to_rgba(self, default: u32) -> [f32; 4] {
        let rgb = self.to_rgb(default);
        let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
        let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
        let b = (rgb & 0xFF) as f32 / 255.0;
        [r, g, b, 1.0]
    }

    pub fn from_hsv(h: f32, s: f32, v: f32) -> Self {
        let h = h.rem_euclid(360.0);
        let s = s.clamp(0.0, 1.0);
        let v = v.clamp(0.0, 1.0);

        let c = v * s;
        let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
        let m = v - c;

        let (r, g, b) = if h < 60.0 {
            (c, x, 0.0)
        } else if h < 120.0 {
            (x, c, 0.0)
        } else if h < 180.0 {
            (0.0, c, x)
        } else if h < 240.0 {
            (0.0, x, c)
        } else if h < 300.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };

        Color::Rgb(
            ((r + m) * 255.0).round() as u8,
            ((g + m) * 255.0).round() as u8,
            ((b + m) * 255.0).round() as u8,
        )
    }

    pub fn to_hsv(self, default: u32) -> (f32, f32, f32) {
        let rgb = self.to_rgb(default);
        let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
        let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
        let b = (rgb & 0xFF) as f32 / 255.0;

        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / delta).rem_euclid(6.0))
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let s = if max == 0.0 { 0.0 } else { delta / max };
        let v = max;

        (h, s, v)
    }
}

/// Default foreground (text) color: light grey.
pub const DEFAULT_FG: u32 = 0xCCCCCC;

/// Default background color: very dark grey.
pub const DEFAULT_BG: u32 = 0x111111;
