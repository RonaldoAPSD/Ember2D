// scripting/api.rs — ScriptCtx implementation (the Rhai API).

use std::sync::{Arc, Mutex};
use rhai::{Array, Dynamic};
use super::engine::ScriptState;
use super::types::*;
use rand::rngs::SmallRng;
use rand::Rng;

#[derive(Clone)]
pub struct ScriptCtx {
    pub(super) inner: Arc<Mutex<ScriptState>>,
    pub(super) rng:   Arc<Mutex<SmallRng>>,
    pub(super) entity_id: i64,
}

impl ScriptCtx {
    pub(super) fn new(state: ScriptState, rng: Arc<Mutex<SmallRng>>) -> Self {
        ScriptCtx { inner: Arc::new(Mutex::new(state)), rng, entity_id: -1 }
    }

    pub(super) fn with_entity(&self, id: i64) -> Self {
        let mut cloned = self.clone();
        cloned.entity_id = id;
        cloned
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
    pub fn set_animation(&mut self, id: i64, frames_str: String, rate: f64) {
        let frames: Vec<char> = frames_str.chars().collect();
        self.inner.lock().unwrap().pending_animations.push((id, frames, rate as f32));
    }

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
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw::Text { x: x as usize, y: y as usize, text, fg: parse_color(&fg), bg: parse_color(&bg) });
    }

    pub fn draw_menu(&mut self, x: i64, y: i64, w: i64, options: Array, selected: i64, fg: String, bg: String, sel_fg: String, sel_bg: String) {
        let opts: Vec<String> = options.into_iter().map(|d| d.to_string()).collect();
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw::Menu { 
            x: x as usize, y: y as usize, w: w as usize, options: opts, selected: selected as usize, 
            fg: parse_color(&fg), bg: parse_color(&bg), sel_fg: parse_color(&sel_fg), sel_bg: parse_color(&sel_bg) 
        });
    }

    pub fn draw_panel(&mut self, x: i64, y: i64, w: i64, h: i64, title: String, fg: String, bg: String) {
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw::Panel { 
            x: x as usize, y: y as usize, w: w as usize, h: h as usize, title, 
            fg: parse_color(&fg), bg: parse_color(&bg) 
        });
    }

    pub fn play_sound(&mut self, path: String) { self.inner.lock().unwrap().pending_sounds.push(path); }
    pub fn play_sound_at(&mut self, path: String, x: f64, y: f64) {
        self.inner.lock().unwrap().pending_spatial_sounds.push((path, x as f32, y as f32));
    }
    pub fn play_music(&mut self, path: String) { self.inner.lock().unwrap().pending_music = Some(path); }
    pub fn stop_music(&mut self) { self.inner.lock().unwrap().stop_music = true; }

    pub fn emit_particles(&mut self, x: f64, y: f64, glyph_str: String, fg: String) {
        let glyph = glyph_str.chars().next().unwrap_or('*');
        let fg_col = parse_color(&fg);
        self.inner.lock().unwrap().pending_particles.push(ParticleRequest { x: x as f32, y: y as f32, glyph, fg: fg_col });
    }

    // ── V0.4 Extensions ───────────────────────────────────────────────────────

    // 1. Shared Global State
    pub fn set_global(&mut self, key: String, value: Dynamic) { self.inner.lock().unwrap().pending_globals.insert(key, value); }
    pub fn get_global(&mut self, key: String) -> Dynamic { self.inner.lock().unwrap().globals.get(&key).cloned().unwrap_or(Dynamic::UNIT) }
    pub fn has_global(&mut self, key: String) -> bool { self.inner.lock().unwrap().globals.contains_key(&key) }
    pub fn remove_global(&mut self, key: String) { self.inner.lock().unwrap().pending_globals.insert(key, Dynamic::UNIT); }

    // 2. Randomness
    pub fn random_int(&mut self, min: i64, max: i64) -> i64 { self.rng.lock().unwrap().gen_range(min..=max) }
    pub fn random_float(&mut self) -> f64 { self.rng.lock().unwrap().gen() }
    pub fn random_bool(&mut self, chance: f64) -> bool { self.rng.lock().unwrap().gen_bool(chance.clamp(0.0, 1.0)) }
    pub fn random_choice(&mut self, arr: Array) -> Dynamic {
        if arr.is_empty() { Dynamic::UNIT }
        else { arr[self.rng.lock().unwrap().gen_range(0..arr.len())].clone() }
    }

    // 3. Spatial Queries
    pub fn get_entity_at(&mut self, x: f64, y: f64) -> i64 {
        let s = self.inner.lock().unwrap();
        for (&id, &(w, h, _, _, _)) in &s.colliders {
            if let Some(&(px, py)) = s.positions.get(&id) {
                if x >= px as f64 && x < (px + w) as f64 && y >= py as f64 && y < (py + h) as f64 { return id; }
            }
        }
        -1
    }
    pub fn is_solid_at(&mut self, x: f64, y: f64) -> bool {
        let s = self.inner.lock().unwrap();
        for (&id, &(w, h, solid, _, _)) in &s.colliders {
            if !solid { continue; }
            if let Some(&(px, py)) = s.positions.get(&id) {
                if x >= px as f64 && x < (px + w) as f64 && y >= py as f64 && y < (py + h) as f64 { return true; }
            }
        }
        false
    }
    pub fn find_entities_in_rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> Array {
        let s = self.inner.lock().unwrap();
        let mut found = Vec::new();
        let r1 = crate::math::Rect::new(x as f32, y as f32, w as f32, h as f32);
        for (&id, &(cw, ch, _, _, _)) in &s.colliders {
            if let Some(&(px, py)) = s.positions.get(&id) {
                let r2 = crate::math::Rect::new(px, py, cw, ch);
                if r1.intersects(r2) { found.push(Dynamic::from(id)); }
            }
        }
        found.into()
    }
    pub fn get_distance(&mut self, id_a: i64, id_b: i64) -> f64 {
        let s = self.inner.lock().unwrap();
        match (s.positions.get(&id_a), s.positions.get(&id_b)) {
            (Some(&(x1, y1)), Some(&(x2, y2))) => (((x2-x1).powi(2) + (y2-y1).powi(2)) as f64).sqrt(),
            _ => 0.0
        }
    }
    pub fn get_angle_to(&mut self, from_id: i64, to_id: i64) -> f64 {
        let s = self.inner.lock().unwrap();
        match (s.positions.get(&from_id), s.positions.get(&to_id)) {
            (Some(&(x1, y1)), Some(&(x2, y2))) => ((y2-y1) as f64).atan2((x2-x1) as f64),
            _ => 0.0
        }
    }

    // 4. Entity Utility Queries
    pub fn entity_exists(&mut self, id: i64) -> bool { self.inner.lock().unwrap().positions.contains_key(&id) }
    pub fn count_by_tag(&mut self, tag: String) -> i64 { self.inner.lock().unwrap().tag_to_ids.get(&tag).map(|v| v.len()).unwrap_or(0) as i64 }
    pub fn get_collider_w(&mut self, id: i64) -> f64 { self.inner.lock().unwrap().colliders.get(&id).map(|c| c.0 as f64).unwrap_or(0.0) }
    pub fn get_collider_h(&mut self, id: i64) -> f64 { self.inner.lock().unwrap().colliders.get(&id).map(|c| c.1 as f64).unwrap_or(0.0) }
    pub fn is_visible(&mut self, id: i64) -> bool { self.inner.lock().unwrap().visibility.get(&id).copied().unwrap_or(false) }
    pub fn set_visible(&mut self, id: i64, visible: bool) { self.inner.lock().unwrap().pending_visibility.push((id, visible)); }
    pub fn get_z_order(&mut self, id: i64) -> i64 { self.inner.lock().unwrap().z_orders.get(&id).copied().unwrap_or(0) as i64 }
    pub fn set_z_order(&mut self, id: i64, z: i64) { self.inner.lock().unwrap().pending_z_order.push((id, z as i32)); }

    // 5. Mouse Input
    pub fn get_mouse_x(&mut self) -> f64 { self.inner.lock().unwrap().mouse_pos.0 as f64 }
    pub fn get_mouse_y(&mut self) -> f64 { self.inner.lock().unwrap().mouse_pos.1 as f64 }
    
    pub fn get_mouse_world_x(&mut self) -> f64 {
        let s = self.inner.lock().unwrap();
        (s.mouse_pos.0 + s.camera_pos.0) as f64
    }
    
    pub fn get_mouse_world_y(&mut self) -> f64 {
        let s = self.inner.lock().unwrap();
        // Screen row Y (1..H-1) -> World row (Y - 1 + cam_y)
        (s.mouse_pos.1 + s.camera_pos.1 - 1.0) as f64
    }

    pub fn mouse_left_pressed(&mut self) -> bool { self.inner.lock().unwrap().mouse_pressed.0 }
    pub fn mouse_right_pressed(&mut self) -> bool { self.inner.lock().unwrap().mouse_pressed.1 }
    pub fn mouse_left_held(&mut self) -> bool { self.inner.lock().unwrap().mouse_held.0 }
    pub fn mouse_right_held(&mut self) -> bool { self.inner.lock().unwrap().mouse_held.1 }

    // 6. Camera Control
    pub fn get_camera_x(&mut self) -> f64 { self.inner.lock().unwrap().camera_pos.0 as f64 }
    pub fn get_camera_y(&mut self) -> f64 { self.inner.lock().unwrap().camera_pos.1 as f64 }
    pub fn set_camera(&mut self, x: f64, y: f64) { self.inner.lock().unwrap().pending_camera = Some(crate::math::Vec2::new(x as f32, y as f32)); }
    pub fn shake_camera(&mut self, intensity: f64, duration: f64) {
        self.inner.lock().unwrap().pending_shake = Some(crate::play::ShakeState { intensity: intensity as f32, duration: duration as f32 });
    }

    // 7. Cross-Level Persistence
    pub fn set_persistent(&mut self, key: String, value: Dynamic) { self.inner.lock().unwrap().pending_persistent.insert(key, value); }
    pub fn get_persistent(&mut self, key: String) -> Dynamic { self.inner.lock().unwrap().persistent.get(&key).cloned().unwrap_or(Dynamic::UNIT) }
    pub fn has_persistent(&mut self, key: String) -> bool { self.inner.lock().unwrap().persistent.contains_key(&key) }
    pub fn clear_persistent(&mut self, key: String) { self.inner.lock().unwrap().pending_persistent.insert(key, Dynamic::UNIT); }
    pub fn clear_all_persistent(&mut self) { self.inner.lock().unwrap().pending_persistent.clear(); }

    // 8. HUD / Draw Utilities
    pub fn draw_box(&mut self, x: i64, y: i64, w: i64, h: i64, fg: String, bg: String) {
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw::Box { x: x as usize, y: y as usize, w: w as usize, h: h as usize, fg: parse_color(&fg), bg: parse_color(&bg) });
    }
    pub fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, ch_str: String, fg: String, bg: String) {
        let ch = ch_str.chars().next().unwrap_or(' ');
        self.inner.lock().unwrap().pending_hud_draws.push(HudDraw::Fill { x: x as usize, y: y as usize, w: w as usize, h: h as usize, ch, fg: parse_color(&fg), bg: parse_color(&bg) });
    }
    pub fn clear_hud(&mut self) { self.inner.lock().unwrap().clear_hud = true; }

    pub fn get_viewport_width(&mut self) -> i64 { self.inner.lock().unwrap().viewport_size.0 as i64 }
    pub fn get_viewport_height(&mut self) -> i64 { self.inner.lock().unwrap().viewport_size.1 as i64 }

    // ── V0.4.5 Logic & AI Update ──────────────────────────────────────────────

    /// Finite raycast from (x1, y1) to (x2, y2).
    /// Returns: [entity_id, hit_x, hit_y] for the first solid hit, or empty array if no hit.
    /// mask: Optional array of layer names to hit. If empty, hits everything.
    pub fn raycast(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, mask: Array) -> Array {
        let s = self.inner.lock().unwrap();
        let ox = x1 as f32;
        let oy = y1 as f32;
        let dx = (x2 - x1) as f32;
        let dy = (y2 - y1) as f32;
        
        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();

        let mut closest_id = -1i64;
        let mut closest_t = 1.0f32; // normalized distance [0, 1] along the segment

        for (&id, &(w, h, solid, ref layer, ref _entity_mask)) in &s.colliders {
            if id == self.entity_id { continue; } // Don't hit self
            if !solid { continue; }
            if !mask_vec.is_empty() && !mask_vec.contains(layer) { continue; }

            if let Some(&(px, py)) = s.positions.get(&id) {
                let rect = crate::math::Rect::new(px, py, w, h);
                if let Some(t) = rect.ray_intersects(ox, oy, dx, dy) {
                    if t >= 0.0 && t < closest_t {
                        closest_t = t;
                        closest_id = id;
                    }
                }
            }
        }

        if closest_id != -1 {
            let hit_x = ox + dx * closest_t;
            let hit_y = oy + dy * closest_t;
            vec![Dynamic::from(closest_id), Dynamic::from(hit_x as f64), Dynamic::from(hit_y as f64)].into()
        } else {
            Array::new()
        }
    }

    /// A* pathfinding on the integer grid.
    /// Returns: [[x, y], [x, y], ...] path from (x1, y1) to (x2, y2).
    /// mask: Optional array of layer names that act as obstacles. If empty, all solid entities block.
    pub fn get_path(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, mask: Array) -> Array {
        let s = self.inner.lock().unwrap();
        let start_x = x1.round() as i32;
        let start_y = y1.round() as i32;
        let target_x = x2.round() as i32;
        let target_y = y2.round() as i32;
        
        if start_x == target_x && start_y == target_y { return Array::new(); }

        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();

        use std::collections::{BinaryHeap, HashMap};
        use std::cmp::Ordering;

        #[derive(Copy, Clone, Eq, PartialEq)]
        struct Node {
            x: i32, y: i32, g: i32, f: i32,
        }

        impl Ord for Node {
            fn cmp(&self, other: &Self) -> Ordering { other.f.cmp(&self.f) } // Min-heap
        }
        impl PartialOrd for Node {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
        }

        let mut open_set = BinaryHeap::new();
        let mut came_from = HashMap::new();
        let mut g_score = HashMap::new();

        open_set.push(Node { x: start_x, y: start_y, g: 0, f: (start_x - target_x).abs() + (start_y - target_y).abs() });
        g_score.insert((start_x, start_y), 0);

        let mut found = false;
        let mut iterations = 0;

        while let Some(current) = open_set.pop() {
            iterations += 1;
            if iterations > 2000 { break; } // Safety limit

            if current.x == target_x && current.y == target_y {
                found = true;
                break;
            }

            for (dx, dy) in &[(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let nx = current.x + dx;
                let ny = current.y + dy;

                // Check collision at (nx, ny)
                let mut blocked = false;
                for (&id, &(w, h, solid, ref layer, ref _entity_mask)) in &s.colliders {
                    if !solid { continue; }
                    // If mask is empty, ALL solids block.
                    // If mask is NOT empty, only solids with layers in the mask block.
                    if !mask_vec.is_empty() && !mask_vec.contains(layer) { continue; }
                    
                    if let Some(&(px, py)) = s.positions.get(&id) {
                        if nx >= px.round() as i32 && nx < (px + w).round() as i32 &&
                           ny >= py.round() as i32 && ny < (py + h).round() as i32 {
                            blocked = true;
                            break;
                        }
                    }
                }

                if blocked { continue; }

                let tentative_g = current.g + 1;
                if tentative_g < *g_score.get(&(nx, ny)).unwrap_or(&i32::MAX) {
                    came_from.insert((nx, ny), (current.x, current.y));
                    g_score.insert((nx, ny), tentative_g);
                    let f = tentative_g + (nx - target_x).abs() + (ny - target_y).abs();
                    open_set.push(Node { x: nx, y: ny, g: tentative_g, f });
                }
            }
        }

        if found {
            let mut path = Vec::new();
            let mut curr = (target_x, target_y);
            while curr != (start_x, start_y) {
                path.push(Dynamic::from(vec![Dynamic::from(curr.0 as f64), Dynamic::from(curr.1 as f64)]));
                if let Some(&prev) = came_from.get(&curr) { curr = prev; } else { break; }
            }
            path.reverse();
            path.into()
        } else {
            Array::new()
        }
    }

    // 9. Timers
    pub fn start_timer(&mut self, name: String, duration: f64) {
        if self.entity_id != -1 {
            self.inner.lock().unwrap().pending_timers.push((self.entity_id as crate::world::EntityId, name, duration));
        }
    }
    pub fn timer_done(&mut self, name: String) -> bool {
        let mut s = self.inner.lock().unwrap();
        if let Some(entity_timers) = s.timers.get(&(self.entity_id as crate::world::EntityId)) {
            if let Some(&val) = entity_timers.get(&name) {
                if val <= 0.0 && val > -500.0 {
                    // Mark for removal/consumed by setting to a special value
                    s.pending_timers.push((self.entity_id as crate::world::EntityId, name, -999.0)); 
                    return true;
                }
            }
        }
        false
    }
    pub fn cancel_timer(&mut self, name: String) {
        if self.entity_id != -1 {
            self.inner.lock().unwrap().pending_timers.push((self.entity_id as crate::world::EntityId, name, -1.0f64));
        }
    }

    // 10. Collision Layers & Masks
    pub fn get_collider_layer(&mut self, id: i64) -> String { self.inner.lock().unwrap().colliders.get(&id).map(|c| c.3.clone()).unwrap_or_default() }
    pub fn set_collider_layer(&mut self, id: i64, layer: String) { self.inner.lock().unwrap().pending_collider_layer.push((id, layer)); }

    pub fn get_collider_mask(&mut self, id: i64) -> Array {
        self.inner.lock().unwrap().colliders.get(&id).map(|c| c.4.iter().map(|s| Dynamic::from(s.clone())).collect()).unwrap_or_default()
    }
    pub fn set_collider_mask(&mut self, id: i64, mask: Array) {
        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();
        self.inner.lock().unwrap().pending_collider_mask.push((id, mask_vec));
    }
}
