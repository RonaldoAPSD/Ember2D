// scripting/api_spatial.rs — ScriptCtx's spatial/query methods:
// get_entity_at/is_solid_at/find_entities_in_rect/get_distance/get_angle_to/
// raycast/get_path.
//
// Split into its own file rather than left in api.rs: api.rs was at 629/600
// lines (CLAUDE.md's hard limit) before this move, and Phase 6
// (docs/ember2d-phase6-plan.md) edits exactly this set of functions in
// several later steps (the collision-layer bitmask touches `raycast`/
// `get_path`'s mask handling directly; `get_distance`/`get_angle_to` are
// the §5.2 H2 transcendental-math item). Splitting the set Phase 6 actually
// modifies into its own file, same second-`impl ScriptCtx`-in-a-sibling-file
// pattern `api_animation.rs` already established, is functionally motivated
// rather than arbitrary. Nothing here changed behavior, only location.

use rhai::{Array, Dynamic};

use super::api::ScriptCtx;

impl ScriptCtx {
    // 3. Spatial Queries
    pub fn get_entity_at(&mut self, x: f64, y: f64) -> i64 {
        let s = self.inner.borrow_mut();
        for (&id, &(w, h, _, _, _, _)) in &s.colliders {
            if let Some(&(px, py)) = s.positions.get(&id) {
                if x >= px as f64 && x < (px + w) as f64 && y >= py as f64 && y < (py + h) as f64 { return id; }
            }
        }
        -1
    }
    pub fn is_solid_at(&mut self, x: f64, y: f64) -> bool {
        let s = self.inner.borrow_mut();
        for (&id, &(w, h, solid, _, _, _)) in &s.colliders {
            if !solid { continue; }
            if let Some(&(px, py)) = s.positions.get(&id) {
                if x >= px as f64 && x < (px + w) as f64 && y >= py as f64 && y < (py + h) as f64 { return true; }
            }
        }
        false
    }
    pub fn find_entities_in_rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> Array {
        let s = self.inner.borrow_mut();
        let mut found = Vec::new();
        let r1 = crate::math::Rect::new(x as f32, y as f32, w as f32, h as f32);
        for (&id, &(cw, ch, _, _, _, _)) in &s.colliders {
            if let Some(&(px, py)) = s.positions.get(&id) {
                let r2 = crate::math::Rect::new(px, py, cw, ch);
                if r1.intersects(r2) { found.push(Dynamic::from(id)); }
            }
        }
        found.into()
    }
    pub fn get_distance(&mut self, id_a: i64, id_b: i64) -> f64 {
        let s = self.inner.borrow_mut();
        match (s.positions.get(&id_a), s.positions.get(&id_b)) {
            (Some(&(x1, y1)), Some(&(x2, y2))) => (((x2-x1).powi(2) + (y2-y1).powi(2)) as f64).sqrt(),
            _ => 0.0
        }
    }
    pub fn get_angle_to(&mut self, from_id: i64, to_id: i64) -> f64 {
        let s = self.inner.borrow_mut();
        match (s.positions.get(&from_id), s.positions.get(&to_id)) {
            (Some(&(x1, y1)), Some(&(x2, y2))) => ((y2-y1) as f64).atan2((x2-x1) as f64),
            _ => 0.0
        }
    }

    // ── V0.4.5 Logic & AI Update ──────────────────────────────────────────────

    /// Finite raycast from (x1, y1) to (x2, y2).
    /// Returns: [entity_id, hit_x, hit_y] for the first solid hit, or empty array if no hit.
    /// mask: Optional array of layer names to hit. If empty, hits everything.
    pub fn raycast(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, mask: Array) -> Array {
        let s = self.inner.borrow_mut();
        let ox = x1 as f32;
        let oy = y1 as f32;
        let dx = (x2 - x1) as f32;
        let dy = (y2 - y1) as f32;

        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();

        let mut closest_id = -1i64;
        let mut closest_t = 1.0f32; // normalized distance [0, 1] along the segment

        for (&id, &(w, h, solid, ref layer, ref _entity_mask, _)) in &s.colliders {
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
        let s = self.inner.borrow_mut();
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
                for (&id, &(w, h, solid, ref layer, ref _entity_mask, _)) in &s.colliders {
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
}
