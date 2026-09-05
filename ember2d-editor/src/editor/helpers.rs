// editor/helpers.rs — Standalone helper functions for the level editor.

use ember2d::input::Key;
use ember2d::renderer::color::Color;
use crate::editor::grid::LevelGrid;
use ember2d_sim::level::TileRecord;
use ember2d_sim::graph::NodeKind;

pub fn key_to_char(key: Key, shift: bool) -> Option<char> {
    match key {
        Key::A=>Some(if shift { 'A' } else { 'a' }),
        Key::B=>Some(if shift { 'B' } else { 'b' }),
        Key::C=>Some(if shift { 'C' } else { 'c' }),
        Key::D=>Some(if shift { 'D' } else { 'd' }),
        Key::E=>Some(if shift { 'E' } else { 'e' }),
        Key::F=>Some(if shift { 'F' } else { 'f' }),
        Key::G=>Some(if shift { 'G' } else { 'g' }),
        Key::H=>Some(if shift { 'H' } else { 'h' }),
        Key::I=>Some(if shift { 'I' } else { 'i' }),
        Key::J=>Some(if shift { 'J' } else { 'j' }),
        Key::K=>Some(if shift { 'K' } else { 'k' }),
        Key::L=>Some(if shift { 'L' } else { 'l' }),
        Key::M=>Some(if shift { 'M' } else { 'm' }),
        Key::N=>Some(if shift { 'N' } else { 'n' }),
        Key::O=>Some(if shift { 'O' } else { 'o' }),
        Key::P=>Some(if shift { 'P' } else { 'p' }),
        Key::Q=>Some(if shift { 'Q' } else { 'q' }),
        Key::R=>Some(if shift { 'R' } else { 'r' }),
        Key::S=>Some(if shift { 'S' } else { 's' }),
        Key::T=>Some(if shift { 'T' } else { 't' }),
        Key::U=>Some(if shift { 'U' } else { 'u' }),
        Key::V=>Some(if shift { 'V' } else { 'v' }),
        Key::W=>Some(if shift { 'W' } else { 'w' }),
        Key::X=>Some(if shift { 'X' } else { 'x' }),
        Key::Y=>Some(if shift { 'Y' } else { 'y' }),
        Key::Z=>Some(if shift { 'Z' } else { 'z' }),

        Key::Key0 => Some(if shift { ')' } else { '0' }),
        Key::Key1 => Some(if shift { '!' } else { '1' }),
        Key::Key2 => Some(if shift { '@' } else { '2' }),
        Key::Key3 => Some(if shift { '#' } else { '3' }),
        Key::Key4 => Some(if shift { '$' } else { '4' }),
        Key::Key5 => Some(if shift { '%' } else { '5' }),
        Key::Key6 => Some(if shift { '^' } else { '6' }),
        Key::Key7 => Some(if shift { '&' } else { '7' }),
        Key::Key8 => Some(if shift { '*' } else { '8' }),
        Key::Key9 => Some(if shift { '(' } else { '9' }),

        Key::Period      => Some(if shift { '>' } else { '.' }),
        Key::Comma       => Some(if shift { '<' } else { ',' }),
        Key::Slash       => Some(if shift { '?' } else { '/' }),
        Key::Semicolon   => Some(if shift { ':' } else { ';' }),
        Key::Apostrophe  => Some(if shift { '"' } else { '\'' }),
        Key::LeftBracket  => Some(if shift { '{' } else { '[' }),
        Key::RightBracket => Some(if shift { '}' } else { ']' }),
        Key::Backslash   => Some(if shift { '|' } else { '\\' }),
        Key::Minus       => Some(if shift { '_' } else { '-' }),
        Key::Equals      => Some(if shift { '+' } else { '=' }),
        Key::Backquote   => Some(if shift { '~' } else { '`' }),
        Key::Space       => Some(' '),
        _ => None,
    }
}

pub const TEXT_INPUT_KEYS: &[Key] = &[
    Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
    Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P,
    Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X,
    Key::Y, Key::Z,
    Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4,
    Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9,
    Key::Period, Key::Comma, Key::Slash, Key::Semicolon, Key::Apostrophe,
    Key::LeftBracket, Key::RightBracket, Key::Backslash, Key::Minus,
    Key::Equals, Key::Backquote, Key::Space,
];

pub fn apply_basic_room(grid: &mut LevelGrid) {
    let w = grid.width as i32;
    let h = grid.height as i32;
    for gy in 0..h {
        for gx in 0..w {
            let on_edge = gx == 0 || gx == w - 1 || gy == 0 || gy == h - 1;
            let tile = if on_edge {
                TileRecord::new(gx, gy, 1, '#', Color::Grey, Color::Reset, true, false, "wall")
            } else {
                TileRecord::new(gx, gy, 1, '.', Color::DarkGrey, Color::Reset, false, false, "floor")
            };
            grid.place(gx, gy, 1, tile);
        }
    }
    grid.spawn_point = (w as f32 / 2.0, h as f32 / 2.0);
}

pub fn apply_param_edit(kind: &mut NodeKind, buf: &str) {
    match kind {
        NodeKind::OnKeyHeld   { key }    => *key = buf.to_string(),
        NodeKind::OnKeyPress  { key }    => *key = buf.to_string(),
        NodeKind::OnCollide   { tag_filter } => *tag_filter = buf.to_string(),
        NodeKind::LoadLevel   { path }   => *path = buf.to_string(),
        NodeKind::PlaySound   { path }   => *path = buf.to_string(),
        NodeKind::FloatLit    { value }  => *value = buf.parse().unwrap_or(0.0),
        NodeKind::StringLit   { value }  => *value = buf.to_string(),
        NodeKind::GetVar      { name }   => *name = buf.to_string(),
        NodeKind::SetVar      { name }   => *name = buf.to_string(),
        NodeKind::Sequence { outputs }   => *outputs = buf.parse().unwrap_or(3).clamp(2, 8),
        
        NodeKind::GetGlobal { name } => *name = buf.to_string(),
        NodeKind::SetGlobal { name } => *name = buf.to_string(),
        NodeKind::GetPersistent { name } => *name = buf.to_string(),
        NodeKind::SetPersistent { name } => *name = buf.to_string(),
        NodeKind::StartTimer { name } => *name = buf.to_string(),
        NodeKind::TimerDone { name } => *name = buf.to_string(),
        NodeKind::CancelTimer { name } => *name = buf.to_string(),
        NodeKind::PlayMusic { path } => *path = buf.to_string(),
        _ => {}
    }
}

pub fn param_default_for(kind: &NodeKind) -> String {
    match kind {
        NodeKind::OnKeyHeld   { key }    => key.clone(),
        NodeKind::OnKeyPress  { key }    => key.clone(),
        NodeKind::OnCollide   { tag_filter } => tag_filter.clone(),
        NodeKind::LoadLevel   { path }   => path.clone(),
        NodeKind::PlaySound   { path }   => path.clone(),
        NodeKind::FloatLit    { value }  => format!("{}", value),
        NodeKind::StringLit   { value }  => value.clone(),
        NodeKind::GetVar      { name }   => name.clone(),
        NodeKind::SetVar      { name }   => name.clone(),
        NodeKind::Sequence { outputs }   => format!("{}", outputs),

        NodeKind::GetGlobal { name } => name.clone(),
        NodeKind::SetGlobal { name } => name.clone(),
        NodeKind::GetPersistent { name } => name.clone(),
        NodeKind::SetPersistent { name } => name.clone(),
        NodeKind::StartTimer { name } => name.clone(),
        NodeKind::TimerDone { name } => name.clone(),
        NodeKind::CancelTimer { name } => name.clone(),
        NodeKind::PlayMusic { path } => path.clone(),
        _ => String::new(),
    }
}

pub fn next_color(c: Color) -> Color {
    match c {
        Color::Black       => Color::White,
        Color::White       => Color::Red,
        Color::Red         => Color::Green,
        Color::Green       => Color::Yellow,
        Color::Yellow      => Color::Blue,
        Color::Blue        => Color::Cyan,
        Color::Cyan        => Color::Magenta,
        Color::Magenta     => Color::DarkGrey,
        Color::DarkGrey    => Color::Grey,
        Color::Grey        => Color::DarkRed,
        Color::DarkRed     => Color::DarkGreen,
        Color::DarkGreen   => Color::DarkBlue,
        Color::DarkBlue    => Color::DarkYellow,
        Color::DarkYellow  => Color::DarkCyan,
        Color::DarkCyan    => Color::DarkMagenta,
        Color::DarkMagenta => Color::Reset,
        Color::Reset       => Color::Black,
        Color::Rgb(_, _, _) => Color::Reset,
    }
}
