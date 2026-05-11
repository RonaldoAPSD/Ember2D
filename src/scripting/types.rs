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
    pub globals:         HashMap<String, rhai::Dynamic>,
    pub persistent:      HashMap<String, rhai::Dynamic>,
    pub camera_override: Option<crate::math::Vec2>,
    pub shake_state:     Option<super::super::play::ShakeState>,
    pub clear_hud:       bool,
    pub particles:       Vec<ParticleRequest>,
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

// ── Color name → Color enum ───────────────────────────────────────────────────

pub fn parse_color(name: &str) -> Color {
    match name.trim() {
        "Black"               => Color::Black,
        "White"               => Color::White,
        "Red"                 => Color::Red,
        "Green"               => Color::Green,
        "Yellow"              => Color::Yellow,
        "Cyan"                => Color::Cyan,
        "DarkBlue"            => Color::DarkBlue,
        "DarkGrey"|"DarkGray" => Color::DarkGrey,
        "DarkGreen"           => Color::DarkGreen,
        "Grey"|"Gray"         => Color::Grey,
        _ => {
            eprintln!("[script] unknown color '{}', defaulting to Reset", name.trim());
            Color::Reset
        }
    }
}

// ── Key name snapshot ─────────────────────────────────────────────────────────

pub(super) fn snapshot_keys(input: &InputManager) -> (HashSet<String>, HashSet<String>) {
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
    ];

    let mut held         = HashSet::new();
    let mut just_pressed = HashSet::new();
    for (key, name) in KEY_MAP {
        if input.is_held(*key)      { held.insert(name.to_string()); }
        if input.just_pressed(*key) { just_pressed.insert(name.to_string()); }
    }
    (held, just_pressed)
}
