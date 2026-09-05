// scripting/types.rs — Log types, HUD drawing, and internal scripting types.

use std::collections::{HashMap, HashSet};
use crate::renderer::color::Color;
use crate::world::EntityId;
use crate::input::{InputManager, Key};

/// The scripting API's breaking-change generation, returned by
/// `ctx.api_version()`. See `docs/ember2d-scripting-api.md` §6's changelog
/// table: v1 is the pre-refactor baseline; v2 is Phase 2 (camera zoom added
/// — no scripted control yet); v3 is Phase 3's breaking renames
/// (`set_color`→`set_tint`, `set_z_order`→`set_layer_order`,
/// `set_animation` removed in favor of the clip API); v4 is Step 4g —
/// `get_mouse_world_y` stops subtracting a HUD row now that `HUD_TOP_ROWS`
/// is 0 (an earlier version of this comment claimed that shipped already in
/// v2; it hadn't — `HUD_TOP_ROWS` only got centralized into one constant
/// then, never actually zeroed). Bump this alongside the next "Yes" row in
/// that table.
pub const API_VERSION: i64 = 4;

// ── Console log types (used by editor console panel) ─────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LogLevel { Error, Warning, Info }

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub level: LogLevel,
    pub text:  String,
}

impl LogEntry {
    pub fn error(text: impl Into<String>) -> Self { LogEntry { level: LogLevel::Error,   text: text.into() } }
    pub fn warn(text:  impl Into<String>) -> Self { LogEntry { level: LogLevel::Warning, text: text.into() } }
    pub fn info(text:  impl Into<String>) -> Self { LogEntry { level: LogLevel::Info,    text: text.into() } }
}

pub struct ScriptUpdateResult {
    pub pending_level:   Option<String>,
    pub pending_save:    Option<String>,
    pub pending_load:    Option<String>,
    pub globals:         HashMap<String, rhai::Dynamic>,
    pub clips:           HashMap<String, crate::components::AnimationClip>,
    pub persistent:      HashMap<String, rhai::Dynamic>,
    pub camera_override: Option<crate::math::Vec2>,
    pub shake_state:     Option<super::super::play::ShakeState>,
    pub clear_hud:       bool,
    pub particles:       Vec<ParticleRequest>,
    pub trigger_turn:    bool,
}

// ── HudDraw ───────────────────────────────────────────────────────────────────

pub enum HudDraw {
    Text { x: usize, y: usize, text: String, fg: Color, bg: Color },
    Box  { x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color },
    Fill { x: usize, y: usize, w: usize, h: usize, ch: char, fg: Color, bg: Color },
    Menu { x: usize, y: usize, w: usize, options: Vec<String>, selected: usize, fg: Color, bg: Color, sel_fg: Color, sel_bg: Color },
    Panel { x: usize, y: usize, w: usize, h: usize, title: String, fg: Color, bg: Color },
}

// ── SpawnRequest ──────────────────────────────────────────────────────────────

pub(super) struct SpawnRequest {
    pub id:    EntityId,
    pub glyph: char,
    pub x:     f32,
    pub y:     f32,
    pub tag:   String,
    /// Defect D10: these used to be hardcoded (white, z=2, 1x1 non-solid
    /// trigger, no layer) in `apply_ctx` regardless of what the script asked
    /// for. `spawn_entity`'s default overload still fills in these exact
    /// same values, so existing scripts see no behavior change; the new
    /// extended overload lets a script set them at spawn time instead of
    /// having to `set_tint`/`set_layer_order`/etc. the id on some later frame.
    pub fg:    Color,
    pub bg:    Color,
    pub z:     i32,
    pub solid: bool,
    pub w:     f32,
    pub h:     f32,
    pub layer: String,
}

// ── ParticleRequest ───────────────────────────────────────────────────────────

pub struct ParticleRequest {
    pub x:     f32,
    pub y:     f32,
    pub glyph: char,
    pub fg:    Color,
}

// ── Color name ↔ Color enum ───────────────────────────────────────────────────

/// Step 3e: `parse_color` now also reads explicit `"#RRGGBB"` hex values —
/// `color_to_name` already emits that exact format for `Color::Rgb`, so a
/// value round-tripped out through `get_color`/etc. and back in through
/// `set_tint`/etc. survives unchanged. Purely additive: every existing
/// named-color script keeps working exactly as before.
pub fn parse_color(name: &str) -> Color {
    let trimmed = name.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return Color::Rgb(r, g, b);
            }
        }
        eprintln!("[script] malformed hex color '{}', defaulting to Reset", trimmed);
        return Color::Reset;
    }

    match trimmed {
        "Black"       => Color::Black,
        "DarkRed"     => Color::DarkRed,
        "DarkGreen"   => Color::DarkGreen,
        "DarkYellow"  => Color::DarkYellow,
        "DarkBlue"    => Color::DarkBlue,
        "DarkMagenta" => Color::DarkMagenta,
        "DarkCyan"    => Color::DarkCyan,
        "Grey"|"Gray" => Color::Grey,
        "DarkGrey"|"DarkGray" => Color::DarkGrey,
        "Red"         => Color::Red,
        "Green"       => Color::Green,
        "Yellow"      => Color::Yellow,
        "Blue"        => Color::Blue,
        "Magenta"     => Color::Magenta,
        "Cyan"        => Color::Cyan,
        "White"       => Color::White,
        "Reset"       => Color::Reset,
        _ => {
            eprintln!("[script] unknown color '{}', defaulting to Reset", trimmed);
            Color::Reset
        }
    }
}

pub fn color_to_name(color: Color) -> String {
    match color {
        Color::Black       => "Black".to_string(),
        Color::DarkRed     => "DarkRed".to_string(),
        Color::DarkGreen   => "DarkGreen".to_string(),
        Color::DarkYellow  => "DarkYellow".to_string(),
        Color::DarkBlue    => "DarkBlue".to_string(),
        Color::DarkMagenta => "DarkMagenta".to_string(),
        Color::DarkCyan    => "DarkCyan".to_string(),
        Color::Grey        => "Grey".to_string(),
        Color::DarkGrey    => "DarkGrey".to_string(),
        Color::Red         => "Red".to_string(),
        Color::Green       => "Green".to_string(),
        Color::Yellow      => "Yellow".to_string(),
        Color::Blue        => "Blue".to_string(),
        Color::Magenta     => "Magenta".to_string(),
        Color::Cyan        => "Cyan".to_string(),
        Color::White       => "White".to_string(),
        Color::Reset       => "Reset".to_string(),
        Color::Rgb(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
    }
}

// ── Key name snapshot ─────────────────────────────────────────────────────────

pub(super) fn snapshot_keys(input: &InputManager) -> (HashSet<String>, HashSet<String>) {
    // Lowercase to match the documented script API contract
    // (docs/ember2d-scripting-api.md §3: `"w"`, `"space"`, `"escape"`, `"left"`, …).
    const KEY_MAP: &[(Key, &str)] = &[
        (Key::W, "w"), (Key::A, "a"), (Key::S, "s"), (Key::D, "d"),
        (Key::Q, "q"), (Key::E, "e"), (Key::R, "r"), (Key::F, "f"),
        (Key::Z, "z"), (Key::X, "x"), (Key::C, "c"), (Key::V, "v"),
        (Key::Up, "up"), (Key::Down, "down"), (Key::Left, "left"), (Key::Right, "right"),
        (Key::Space, "space"), (Key::Enter, "enter"), (Key::Escape, "escape"),
        (Key::LeftShift, "shift"), (Key::RightShift, "shift"),
        (Key::LeftCtrl, "ctrl"),  (Key::RightCtrl, "ctrl"),
        (Key::Key1, "1"), (Key::Key2, "2"), (Key::Key3, "3"),
        (Key::Key4, "4"), (Key::Key5, "5"), (Key::Key6, "6"),
        (Key::Key7, "7"), (Key::Key8, "8"), (Key::Key9, "9"), (Key::Key0, "0"),
        (Key::Tab, "tab"),
        (Key::Backspace, "backspace"),
        (Key::F1, "f1"), (Key::F2, "f2"), (Key::F3, "f3"), (Key::F4, "f4"),
        (Key::F5, "f5"), (Key::F6, "f6"), (Key::F7, "f7"), (Key::F8, "f8"),
        (Key::F9, "f9"), (Key::F10, "f10"), (Key::F11, "f11"), (Key::F12, "f12"),
    ];

    let mut held         = HashSet::new();
    let mut just_pressed = HashSet::new();
    for (key, name) in KEY_MAP {
        if input.is_held(*key)      { held.insert(name.to_string()); }
        if input.just_pressed(*key) { just_pressed.insert(name.to_string()); }
    }
    (held, just_pressed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_keys_uses_lowercase_names() {
        // Regression test: KEY_MAP previously emitted "W"/"Up"/"Enter" while
        // both the documented API (ember2d-scripting-api.md §3) and every
        // demo script call ctx.is_held("w") / ctx.just_pressed("enter") in
        // lowercase. The mismatch meant scripts gating on movement/menu keys
        // silently never matched — e.g. the original demo's player script
        // (docs/archive/demo/scripts/player.rhai) had a tutorial gate that
        // never dismissed, which zeroed player velocity every frame.
        let mut input = InputManager::new();
        input.handle_pressed(Key::W);
        input.handle_pressed(Key::Enter);
        input.consume_step();

        let (held, just_pressed) = snapshot_keys(&input);
        assert!(held.contains("w"), "held set should use lowercase key names");
        assert!(just_pressed.contains("enter"), "just_pressed set should use lowercase key names");
        assert!(!held.contains("W") && !just_pressed.contains("Enter"), "no capitalized names should leak through");
    }

    // ── Tests: Step 3e hex color support (ember2d-scripting-api.md §3) ─────────

    #[test]
    fn parse_color_still_reads_every_named_color_unchanged() {
        assert_eq!(parse_color("Red"), Color::Red);
        assert_eq!(parse_color("Reset"), Color::Reset);
        assert_eq!(parse_color("DarkGray"), Color::DarkGrey, "the Gray/Grey American-spelling alias must still work");
    }

    #[test]
    fn parse_color_reads_explicit_hex_values() {
        assert_eq!(parse_color("#FF8000"), Color::Rgb(0xFF, 0x80, 0x00));
        assert_eq!(parse_color("#00ff00"), Color::Rgb(0, 255, 0), "hex digits should be case-insensitive");
        assert_eq!(parse_color("  #112233  "), Color::Rgb(0x11, 0x22, 0x33), "surrounding whitespace must still be trimmed");
    }

    #[test]
    fn parse_color_round_trips_through_color_to_name() {
        let original = Color::Rgb(0x4A, 0x90, 0xE2);
        assert_eq!(parse_color(&color_to_name(original)), original, "a tint read back out via get_color/color_to_name and back in via set_tint must survive unchanged");
    }

    #[test]
    fn parse_color_falls_back_to_reset_on_malformed_hex() {
        assert_eq!(parse_color("#ZZZZZZ"), Color::Reset);
        assert_eq!(parse_color("#FFF"), Color::Reset, "only 6-digit RRGGBB is accepted, not the 3-digit shorthand");
    }
}
