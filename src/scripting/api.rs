// scripting/api.rs — ScriptCtx implementation (the Rhai API).

use std::sync::{Arc, Mutex};
use rhai::{Array, Dynamic};
use super::engine::ScriptState;
use super::types::*;

#[derive(Clone)]
pub struct ScriptCtx {
    pub(super) inner: Arc<Mutex<ScriptState>>,
}

impl ScriptCtx {
    pub(super) fn new(state: ScriptState) -> Self {
        ScriptCtx { inner: Arc::new(Mutex::new(state)) }
    }

    pub fn get_x(&mut self, id: i64) -> f64 { self.inner.lock().unwrap().positions.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0) }
    pub fn get_y(&mut self, id: i64) -> f64 { self.inner.lock().unwrap().positions.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0) }
    pub fn get_vel_x(&mut self, id: i64) -> f64 { self.inner.lock().unwrap().velocities.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0) }
    pub fn get_vel_y(&mut self, id: i64) -> f64 { self.inner.lock().unwrap().velocities.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0) }

    pub fn get_tag(&mut self, id: i64) -> String { self.inner.lock().unwrap().tags.get(&id).cloned().unwrap_or_default() }
    pub fn has_tag(&mut self, id: i64, name: String) -> bool { self.inner.lock().unwrap().tags.get(&id).map(|t| t == &name).unwrap_or(false) }
    pub fn find_by_tag(&mut self, tag: String) -> i64 { self.inner.lock().unwrap().tag_to_id.get(&tag).copied().unwrap_or(-1) }
    pub fn find_all_by_tag(&mut self, tag: String) -> Array {
        self.inner.lock().unwrap().tag_to_ids.get(&tag).cloned().unwrap_or_default().into_iter().map(Dynamic::from).collect()
    }

    pub fn is_held(&mut self, key: String) -> bool { self.inner.lock().unwrap().held_keys.contains(&key) }
    pub fn just_pressed(&mut self, key: String) -> bool { self.inner.lock().unwrap().just_pressed_keys.contains(&key) }

    pub fn get_spawn_point(&mut self, name: String) -> Array {
        self.inner.lock().unwrap().extra_spawns.get(&name).map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)]).unwrap_or_default()
    }

    pub fn get_delta(&mut self) -> f64 { self.inner.lock().unwrap().delta_time as f64 }
    pub fn get_elapsed(&mut self) -> f64 { self.inner.lock().unwrap().elapsed as f64 }

    pub fn set_velocity(&mut self, id: i64, vx: f64, vy: f64) { self.inner.lock().unwrap().pending_velocities.push((id, vx as f32, vy as f32)); }
    pub fn set_position(&mut self, id: i64, x: f64, y: f64) { self.inner.lock().unwrap().pending_positions.push((id, x as f32, y as f32)); }
    pub fn set_glyph(&mut self, id: i64, glyph_str: String) {
        if let Some(ch) = glyph_str.chars().next() { self.inner.lock().unwrap().pending_glyphs.push((id, ch)); }
    }
    pub fn set_color(&mut self, id: i64, fg: String, bg: String) { self.inner.lock().unwrap().pending_colors.push((id, fg, bg)); }

    pub fn despawn(&mut self, id: i64) { self.inner.lock().unwrap().despawn_queue.push(id); }
    pub fn spawn(&mut self, glyph_str: String, x: f64, y: f64, tag: String) -> i64 {
        let glyph = glyph_str.chars().next().unwrap_or('?');
        let mut s = self.inner.lock().unwrap();
        let id = s.next_spawn_id;
        s.next_spawn_id += 1;
        s.spawn_queue.push(SpawnRequest { id, glyph, x: x as f32, y: y as f32, tag });
        id as i64
    }

    pub fn load_level(&mut self, path: String) {
        let mut s = self.inner.lock().unwrap();
        if s.pending_level.is_none() { s.pending_level = Some(path); }
    }
    pub fn log(&mut self, msg: String) { self.inner.lock().unwrap().pending_logs.push(msg); }
    pub fn draw_hud(&mut self, x: i64, y: i64, text: String, fg: String, bg: String) {
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw { x: x as usize, y: y as usize, text, fg: parse_color(&fg), bg: parse_color(&bg) });
    }

    pub fn play_sound(&mut self, path: String) { self.inner.lock().unwrap().pending_sounds.push(path); }
    pub fn play_music(&mut self, path: String) { self.inner.lock().unwrap().pending_music = Some(path); }
    pub fn stop_music(&mut self) { self.inner.lock().unwrap().stop_music = true; }
}
