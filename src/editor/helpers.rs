// editor/helpers.rs — Standalone helper functions for the level editor.

use crate::input::Key;
use crate::renderer::color::Color;
use crate::editor::grid::LevelGrid;
use crate::level::TileRecord;
use super::node_graph;

pub fn key_to_char(key: Key, shift: bool) -> Option<char> {
    let ch = match key {
        Key::A=>'a', Key::B=>'b', Key::C=>'c', Key::D=>'d', Key::E=>'e',
        Key::F=>'f', Key::G=>'g', Key::H=>'h', Key::I=>'i', Key::J=>'j',
        Key::K=>'k', Key::L=>'l', Key::M=>'m', Key::N=>'n', Key::O=>'o',
        Key::P=>'p', Key::Q=>'q', Key::R=>'r', Key::S=>'s', Key::T=>'t',
        Key::U=>'u', Key::V=>'v', Key::W=>'w', Key::X=>'x', Key::Y=>'y',
        Key::Z=>'z',
        Key::Key0=>'0', Key::Key1=>'1', Key::Key2=>'2', Key::Key3=>'3',
        Key::Key4=>'4', Key::Key5=>'5', Key::Key6=>'6', Key::Key7=>'7',
        Key::Key8=>'8', Key::Key9=>'9',
        Key::Period    => '.',
        Key::Slash     => '/',
        Key::Backslash => '\\',
        Key::Minus     => if shift { '_' } else { '-' },
        Key::Space     => ' ',
        _ => return None,
    };
    Some(if shift && ch.is_ascii_alphabetic() { ch.to_ascii_uppercase() } else { ch })
}

pub const TEXT_INPUT_KEYS: &[Key] = &[
    Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
    Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P,
    Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X,
    Key::Y, Key::Z,
    Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4,
    Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9,
    Key::Period, Key::Slash, Key::Backslash, Key::Minus, Key::Space,
];

pub fn apply_basic_room(grid: &mut LevelGrid) {
    let w = grid.width as i32;
    let h = grid.height as i32;
    for gy in 0..h {
        for gx in 0..w {
            let on_edge = gx == 0 || gx == w - 1 || gy == 0 || gy == h - 1;
            let tile = if on_edge {
                TileRecord::new(gx, gy, '#', Color::Grey, Color::Reset, true, false, "wall")
            } else {
                TileRecord::new(gx, gy, '.', Color::DarkGrey, Color::Reset, false, false, "floor")
            };
            grid.place(gx, gy, tile);
        }
    }
    grid.spawn_point = (w as f32 / 2.0, h as f32 / 2.0);
}

pub fn apply_param_edit(kind: &mut node_graph::NodeKind, buf: &str) {
    use node_graph::NodeKind;
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
        _ => {}
    }
}

pub fn param_default_for(kind: &node_graph::NodeKind) -> String {
    use node_graph::NodeKind;
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
        _ => String::new(),
    }
}
