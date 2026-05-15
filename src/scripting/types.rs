// scripting/types.rs — Log types, HUD drawing, and internal scripting types.

use std::collections::{HashMap, HashSet};
use crate::renderer::color::Color;
use crate::world::EntityId;
use crate::input::{InputManager, Key};

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
}

// ── ParticleRequest ───────────────────────────────────────────────────────────

pub struct ParticleRequest {
    pub x:     f32,
    pub y:     f32,
    pub glyph: char,
    pub fg:    Color,
}

// ── Color name ↔ Color enum ───────────────────────────────────────────────────

pub fn parse_color(name: &str) -> Color {
    match name.trim() {
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
            eprintln!("[script] unknown color '{}', defaulting to Reset", name.trim());
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
    const KEY_MAP: &[(Key, &str)] = &[
        (Key::W, "W"), (Key::A, "A"), (Key::S, "S"), (Key::D, "D"),
        (Key::Q, "Q"), (Key::E, "E"), (Key::R, "R"), (Key::F, "F"),
        (Key::Z, "Z"), (Key::X, "X"), (Key::C, "C"), (Key::V, "V"),
        (Key::Up, "Up"), (Key::Down, "Down"), (Key::Left, "Left"), (Key::Right, "Right"),
        (Key::Space, "Space"), (Key::Enter, "Enter"), (Key::Escape, "Escape"),
        (Key::LeftShift, "Shift"), (Key::RightShift, "Shift"),
        (Key::LeftCtrl, "Ctrl"),  (Key::RightCtrl, "Ctrl"),
        (Key::Key1, "1"), (Key::Key2, "2"), (Key::Key3, "3"),
        (Key::Key4, "4"), (Key::Key5, "5"), (Key::Key6, "6"),
        (Key::Key7, "7"), (Key::Key8, "8"), (Key::Key9, "9"), (Key::Key0, "0"),
        (Key::Tab, "Tab"),
        (Key::Backspace, "Backspace"),
        (Key::F1, "F1"), (Key::F2, "F2"), (Key::F3, "F3"), (Key::F4, "F4"),
        (Key::F5, "F5"), (Key::F6, "F6"), (Key::F7, "F7"), (Key::F8, "F8"),
        (Key::F9, "F9"), (Key::F10, "F10"), (Key::F11, "F11"), (Key::F12, "F12"),
    ];

    let mut held         = HashSet::new();
    let mut just_pressed = HashSet::new();
    for (key, name) in KEY_MAP {
        if input.is_held(*key)      { held.insert(name.to_string()); }
        if input.just_pressed(*key) { just_pressed.insert(name.to_string()); }
    }
    (held, just_pressed)
}
