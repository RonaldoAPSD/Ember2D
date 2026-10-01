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

// ── Step 8-1: tilemap cells (docs/ember2d-master-plan.md §5.7) ──────────────
//
// Every query below checks the snapshot's `tilemaps` alongside its
// `colliders`. The rule, from 8-1's own scoping decision: a SOLID cell
// answers exactly like the wall entity it replaced would have, except that
// the id it reports is the tilemap's own entity id (a cell has none of its
// own) — so `is_solid_at`/`get_path` are unchanged for a script, and
// `get_entity_at`/`find_entities_in_rect`/`raycast` return the tilemap's id
// where they used to return a wall's. A non-solid cell (a floor) is
// invisible to all of them, same as a floor tile entity was: it never had a
// collider. Where several hits compete for "first", ties still resolve to
// the lowest id, the tilemap's included.

/// The grid cell a script-supplied coordinate falls in — `None` for a NaN,
/// infinite, or out-of-`i32`-range value, which a collider's own
/// comparison-based containment test could never match either (every
/// comparison against NaN is false), so neither may a cell. Without this, a
/// NaN would `floor() as i32` to `0` and "hit" cell (0, 0).
fn cell_of(v: f64) -> Option<i32> {
    let f = v.floor();
    if f.is_finite() && f >= i32::MIN as f64 && f <= i32::MAX as f64 {
        Some(f as i32)
    } else {
        None
    }
}

impl ScriptCtx {
    // 3. Spatial Queries
    pub fn get_entity_at(&mut self, x: f64, y: f64) -> i64 {
        let s = self.inner.borrow_mut();
        let mut hit = -1i64;
        for (&id, &(w, h, _, _, _, _, _)) in &s.colliders {
            if let Some(&(px, py)) = s.positions.get(&id) {
                if x >= px as f64 && x < (px + w) as f64 && y >= py as f64 && y < (py + h) as f64 {
                    hit = id;
                    break;
                }
            }
        }
        // Step 8-1: a solid cell counts as its tilemap's id; lowest id wins.
        if let (Some(cx), Some(cy)) = (cell_of(x), cell_of(y)) {
            for (&map_id, map) in &s.tilemaps {
                if map.solid_at(cx, cy, 0) && (hit == -1 || map_id < hit) {
                    hit = map_id;
                }
            }
        }
        hit
    }
    /// `i64` overload (7.5-1, docs/ember2d-master-plan.md §5.6, R31) — see
    /// `api.rs`'s `draw_hud_f` for the full reasoning every coordinate/
    /// size/layer-order function in this crate gets one of these.
    pub fn get_entity_at_i(&mut self, x: i64, y: i64) -> i64 {
        self.get_entity_at(x as f64, y as f64)
    }
    pub fn is_solid_at(&mut self, x: f64, y: f64) -> bool {
        let s = self.inner.borrow_mut();
        // Step 8-1: one cell lookup per tilemap, before the collider scan.
        if let (Some(cx), Some(cy)) = (cell_of(x), cell_of(y)) {
            if s.tilemaps.values().any(|m| m.solid_at(cx, cy, 0)) {
                return true;
            }
        }
        for (&id, &(w, h, solid, _, _, _, _)) in &s.colliders {
            if !solid {
                continue;
            }
            if let Some(&(px, py)) = s.positions.get(&id) {
                if x >= px as f64 && x < (px + w) as f64 && y >= py as f64 && y < (py + h) as f64 {
                    return true;
                }
            }
        }
        false
    }
    /// `i64` overload — same reasoning as `get_entity_at_i` above.
    pub fn is_solid_at_i(&mut self, x: i64, y: i64) -> bool {
        self.is_solid_at(x as f64, y as f64)
    }
    pub fn find_entities_in_rect(&mut self, x: f64, y: f64, w: f64, h: f64) -> Array {
        let s = self.inner.borrow_mut();
        let mut found: Vec<i64> = Vec::new();
        let r1 = crate::math::Rect::new(x as f32, y as f32, w as f32, h as f32);
        for (&id, &(cw, ch, _, _, _, _, _)) in &s.colliders {
            if let Some(&(px, py)) = s.positions.get(&id) {
                let r2 = crate::math::Rect::new(px, py, cw, ch);
                if r1.intersects(r2) {
                    found.push(id);
                }
            }
        }
        // Step 8-1: a tilemap is listed once if any of its solid cells
        // intersects — then everything re-sorted into ascending id order,
        // the order this always returned (colliders iterate a `BTreeMap`).
        for (&map_id, map) in &s.tilemaps {
            if map.any_solid_in(r1, 0) {
                found.push(map_id);
            }
        }
        found.sort_unstable();
        found.into_iter().map(Dynamic::from).collect()
    }
    /// `i64` overload — same reasoning as `get_entity_at_i` above.
    pub fn find_entities_in_rect_i(&mut self, x: i64, y: i64, w: i64, h: i64) -> Array {
        self.find_entities_in_rect(x as f64, y as f64, w as f64, h as f64)
    }
    // Phase 6 Step 12 (docs/ember2d-phase6-plan.md, §5.2 H2): `get_distance`
    // is rewritten from `.powi(2)` to explicit `dx*dx` — not because `powi`
    // was itself a determinism hazard (small-integer `powi` is repeated
    // multiplication, not a libm call, so it was already IEEE-754-exact),
    // but so the exact operation sequence is spelled out here rather than
    // resting on an intrinsic's implementation detail. `get_angle_to` is the
    // real fix: `crate::math::atan2_approx` replaces the platform-libm
    // `f64::atan2` call this function used to make — see that function's
    // own doc comment for the full "why" and "how."
    pub fn get_distance(&mut self, id_a: i64, id_b: i64) -> f64 {
        let s = self.inner.borrow_mut();
        match (s.positions.get(&id_a), s.positions.get(&id_b)) {
            (Some(&(x1, y1)), Some(&(x2, y2))) => {
                let (dx, dy) = ((x2 - x1) as f64, (y2 - y1) as f64);
                (dx * dx + dy * dy).sqrt()
            }
            _ => 0.0,
        }
    }
    pub fn get_angle_to(&mut self, from_id: i64, to_id: i64) -> f64 {
        let s = self.inner.borrow_mut();
        match (s.positions.get(&from_id), s.positions.get(&to_id)) {
            (Some(&(x1, y1)), Some(&(x2, y2))) => {
                crate::math::atan2_approx((y2 - y1) as f64, (x2 - x1) as f64)
            }
            _ => 0.0,
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

        // Phase 6 Step 7 (docs/ember2d-phase6-plan.md): the incoming mask
        // array is folded to bits ONCE, here, rather than compared as
        // strings against every candidate inside the loop below —
        // `mask_bits` encodes "hits everything" as `0`, same as an empty
        // array did before (`LayerRegistry::mask_bits`'s own doc comment).
        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();
        let mask_bits = s.layers.mask_bits(&mask_vec);

        let mut closest_id = -1i64;
        let mut closest_t = 1.0f32; // normalized distance [0, 1] along the segment

        for (&id, &(w, h, solid, _, _, _, layer_bits)) in &s.colliders {
            if id == self.entity_id {
                continue;
            } // Don't hit self
            if !solid {
                continue;
            }
            if mask_bits != 0 && (mask_bits & layer_bits) == 0 {
                continue;
            }

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

        // Step 8-1: the nearest solid cell of each tilemap competes on the
        // same terms; an exact tie in `t` goes to the lower id, which is
        // what the ascending collider loop above already did implicitly.
        for (&map_id, map) in &s.tilemaps {
            if map_id == self.entity_id {
                continue;
            }
            if let Some(t) = map.raycast(ox, oy, dx, dy, mask_bits) {
                if t < closest_t || (t == closest_t && closest_id != -1 && map_id < closest_id) {
                    closest_t = t;
                    closest_id = map_id;
                }
            }
        }

        if closest_id != -1 {
            let hit_x = ox + dx * closest_t;
            let hit_y = oy + dy * closest_t;
            vec![
                Dynamic::from(closest_id),
                Dynamic::from(hit_x as f64),
                Dynamic::from(hit_y as f64),
            ]
        } else {
            Array::new()
        }
    }
    /// `i64` overload — same reasoning as `get_entity_at_i` above.
    pub fn raycast_i(&mut self, x1: i64, y1: i64, x2: i64, y2: i64, mask: Array) -> Array {
        self.raycast(x1 as f64, y1 as f64, x2 as f64, y2 as f64, mask)
    }

    /// A* pathfinding on the integer grid, 4-directional (`diagonal:
    /// false` — see `get_path_diag` below for the Step 7.5-6 addition).
    /// Returns: [[x, y], [x, y], ...] path from (x1, y1) to (x2, y2).
    /// mask: Optional array of layer names that act as obstacles. If empty, all solid entities block.
    pub fn get_path(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, mask: Array) -> Array {
        self.get_path_diag(x1, y1, x2, y2, mask, false)
    }
    /// `i64` overload — same reasoning as `get_entity_at_i` above.
    pub fn get_path_i(&mut self, x1: i64, y1: i64, x2: i64, y2: i64, mask: Array) -> Array {
        self.get_path(x1 as f64, y1 as f64, x2 as f64, y2 as f64, mask)
    }

    /// Step 7.5-6 (docs/ember2d-master-plan.md §5.6, tactical-RPG need from
    /// the old plan's open question 4): the same A* as `get_path` above,
    /// with an 8-directional option. Registered under the same Rhai name
    /// ("get_path") as a 6-argument overload — `spawn_entity`'s own 4-arg/
    /// 11-arg split is the precedent for more than one arity sharing a
    /// name; this isn't a breaking change to the 5-arg form, which just
    /// forwards here with `diagonal: false`.
    ///
    /// Costs are scaled ×10 (10 for a straight step, 14 ≈ 10×√2 for a
    /// diagonal one) so both the heuristic and `g`/`f` stay plain `i32` —
    /// no runtime `sqrt` in the loop (`atan2_approx` is this crate's one
    /// approved transcendental-math replacement; a fixed, hand-rounded
    /// integer constant sidesteps needing one here at all). When
    /// `diagonal` is set the heuristic switches to the scaled Chebyshev
    /// distance (the straight-line cost of the longer axis, plus the
    /// diagonal-vs-straight cost difference times the shorter axis) —
    /// Manhattan distance would overestimate remaining cost once diagonal
    /// moves are legal, which breaks A*'s admissibility guarantee (the
    /// path found is no longer guaranteed shortest).
    ///
    /// A diagonal move is only legal when BOTH orthogonal neighbors next to
    /// it are unblocked — the standard "no cutting a corner" rule, since
    /// without it a mover could squeeze diagonally between two solid cells
    /// that share only a corner, visually clipping through both.
    pub fn get_path_diag(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        mask: Array,
        diagonal: bool,
    ) -> Array {
        let s = self.inner.borrow_mut();
        let start_x = x1.round() as i32;
        let start_y = y1.round() as i32;
        let target_x = x2.round() as i32;
        let target_y = y2.round() as i32;

        if start_x == target_x && start_y == target_y {
            return Array::new();
        }

        // Phase 6 Step 7 (docs/ember2d-phase6-plan.md): folded to bits ONCE
        // here, before A* even starts, rather than re-compared as strings
        // against every collider on every one of the (up to 2000 * 4 or 8)
        // neighbor checks below — see `raycast`'s matching comment above.
        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();
        let mask_bits = s.layers.mask_bits(&mask_vec);

        use std::cmp::Ordering;
        use std::collections::{BinaryHeap, HashMap};

        #[derive(Copy, Clone, Eq, PartialEq)]
        struct Node {
            x: i32,
            y: i32,
            g: i32,
            f: i32,
        }

        impl Ord for Node {
            fn cmp(&self, other: &Self) -> Ordering {
                other.f.cmp(&self.f)
            } // Min-heap
        }
        impl PartialOrd for Node {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        const STRAIGHT: i32 = 10;
        const DIAGONAL: i32 = 14;

        let heuristic = |x: i32, y: i32| {
            let dx = (x - target_x).abs();
            let dy = (y - target_y).abs();
            if diagonal {
                let (lo, hi) = (dx.min(dy), dx.max(dy));
                hi * STRAIGHT + lo * (DIAGONAL - STRAIGHT)
            } else {
                (dx + dy) * STRAIGHT
            }
        };

        let blocked_at = |nx: i32, ny: i32| -> bool {
            // Step 8-1: the O(1) tilemap check first — on a tile-heavy map
            // this answers almost every neighbour check without the
            // collider scan below ever running (which, before 8-1, was a
            // scan over every wall, per neighbour, per A* node).
            if s.tilemaps.values().any(|m| m.solid_at(nx, ny, mask_bits)) {
                return true;
            }
            for (&id, &(w, h, solid, _, _, _, layer_bits)) in &s.colliders {
                if !solid {
                    continue;
                }
                // If mask is empty (mask_bits == 0), ALL solids block.
                // Otherwise only solids whose own layer bit intersects the
                // requested mask block.
                if mask_bits != 0 && (mask_bits & layer_bits) == 0 {
                    continue;
                }
                if let Some(&(px, py)) = s.positions.get(&id) {
                    if nx >= px.round() as i32
                        && nx < (px + w).round() as i32
                        && ny >= py.round() as i32
                        && ny < (py + h).round() as i32
                    {
                        return true;
                    }
                }
            }
            false
        };

        let neighbors: &[(i32, i32, i32)] = if diagonal {
            &[
                (0, 1, STRAIGHT),
                (0, -1, STRAIGHT),
                (1, 0, STRAIGHT),
                (-1, 0, STRAIGHT),
                (1, 1, DIAGONAL),
                (1, -1, DIAGONAL),
                (-1, 1, DIAGONAL),
                (-1, -1, DIAGONAL),
            ]
        } else {
            &[(0, 1, STRAIGHT), (0, -1, STRAIGHT), (1, 0, STRAIGHT), (-1, 0, STRAIGHT)]
        };

        let mut open_set = BinaryHeap::new();
        let mut came_from = HashMap::new();
        let mut g_score = HashMap::new();

        open_set.push(Node {
            x: start_x,
            y: start_y,
            g: 0,
            f: heuristic(start_x, start_y),
        });
        g_score.insert((start_x, start_y), 0);

        let mut found = false;
        let mut iterations = 0;

        while let Some(current) = open_set.pop() {
            iterations += 1;
            if iterations > 2000 {
                break;
            } // Safety limit

            if current.x == target_x && current.y == target_y {
                found = true;
                break;
            }

            for &(dx, dy, cost) in neighbors {
                let nx = current.x + dx;
                let ny = current.y + dy;

                if blocked_at(nx, ny) {
                    continue;
                }
                // No-corner-cutting: a diagonal step is only legal if both
                // of the orthogonal cells it would otherwise clip past are
                // also open.
                if dx != 0 && dy != 0 && (blocked_at(current.x + dx, current.y) || blocked_at(current.x, current.y + dy)) {
                    continue;
                }

                let tentative_g = current.g + cost;
                if tentative_g < *g_score.get(&(nx, ny)).unwrap_or(&i32::MAX) {
                    came_from.insert((nx, ny), (current.x, current.y));
                    g_score.insert((nx, ny), tentative_g);
                    let f = tentative_g + heuristic(nx, ny);
                    open_set.push(Node { x: nx, y: ny, g: tentative_g, f });
                }
            }
        }

        if found {
            let mut path = Vec::new();
            let mut curr = (target_x, target_y);
            while curr != (start_x, start_y) {
                path.push(Dynamic::from(vec![
                    Dynamic::from(curr.0 as f64),
                    Dynamic::from(curr.1 as f64),
                ]));
                if let Some(&prev) = came_from.get(&curr) {
                    curr = prev;
                } else {
                    break;
                }
            }
            path.reverse();
            path
        } else {
            Array::new()
        }
    }
    /// `i64` overload — same reasoning as `get_entity_at_i` above.
    pub fn get_path_diag_i(
        &mut self,
        x1: i64,
        y1: i64,
        x2: i64,
        y2: i64,
        mask: Array,
        diagonal: bool,
    ) -> Array {
        self.get_path_diag(x1 as f64, y1 as f64, x2 as f64, y2 as f64, mask, diagonal)
    }

    /// Every cell reachable from `id`'s own current position within
    /// `budget` orthogonal steps (Step 7.5-6, docs/ember2d-master-plan.md
    /// §5.6 — the tactical-RPG movement-range preview the old plan's open
    /// question 4 asked for). A plain BFS, budget in whole cells — no
    /// `diagonal`/`mask` params unlike `get_path`/`raycast` above: every
    /// solid blocks unconditionally, matching `is_solid_at`'s own
    /// behavior, since a range preview should show exactly what a script's
    /// own `is_solid_at`-gated move would allow, not a filtered subset.
    /// Returns `[[x, y], [x, y], ...]`, NOT including `id`'s own starting
    /// cell (nothing "moves zero steps"). An unknown `id` or a
    /// non-positive `budget` both just return an empty array — no reason
    /// to panic on either.
    pub fn reachable_within(&mut self, id: i64, budget: i64) -> Array {
        let s = self.inner.borrow_mut();
        let Some(&(start_x, start_y)) = s.positions.get(&id) else { return Array::new() };
        let start_x = start_x.round() as i32;
        let start_y = start_y.round() as i32;
        if budget <= 0 {
            return Array::new();
        }

        let blocked_at = |nx: i32, ny: i32| -> bool {
            // Step 8-1: same tilemap-first check as `get_path`'s, unmasked
            // like the rest of this function.
            if s.tilemaps.iter().any(|(&mid, m)| mid != id && m.solid_at(nx, ny, 0)) {
                return true;
            }
            for (&cid, &(w, h, solid, _, _, _, _)) in &s.colliders {
                if cid == id || !solid {
                    continue;
                }
                if let Some(&(px, py)) = s.positions.get(&cid) {
                    if nx >= px.round() as i32
                        && nx < (px + w).round() as i32
                        && ny >= py.round() as i32
                        && ny < (py + h).round() as i32
                    {
                        return true;
                    }
                }
            }
            false
        };

        use std::collections::{HashSet, VecDeque};
        // Only ever `.contains`/`.insert` below, never iterated — the "no
        // HashMap/HashSet iteration in sim code" rule (CLAUDE.md's
        // Determinism section) is about iteration order leaking into
        // output, not lookup-only membership tests. Output order instead
        // comes entirely from the `VecDeque` frontier's own deterministic
        // visit order (a fixed neighbor-offset list, breadth-first).
        let mut visited: HashSet<(i32, i32)> = HashSet::new();
        visited.insert((start_x, start_y));
        let mut frontier = VecDeque::new();
        frontier.push_back((start_x, start_y, 0i64));
        let mut result = Array::new();

        while let Some((x, y, steps)) = frontier.pop_front() {
            if steps >= budget {
                continue;
            }
            for (dx, dy) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
                let (nx, ny) = (x + dx, y + dy);
                if visited.contains(&(nx, ny)) || blocked_at(nx, ny) {
                    continue;
                }
                visited.insert((nx, ny));
                result.push(Dynamic::from(vec![Dynamic::from(nx as f64), Dynamic::from(ny as f64)]));
                frontier.push_back((nx, ny, steps + 1));
            }
        }
        result
    }

    /// Step 8-1 (additive, no `API_VERSION` bump): is `id` a tilemap
    /// entity — the id `get_entity_at`/`find_entities_in_rect`/`raycast`/
    /// `on_collide` report for a static tile? A tilemap has a position
    /// (its grid's top-left cell) but no glyph, collider, or tag of its
    /// own; per-cell detail is `get_tile_tag` below and `is_solid_at`.
    pub fn is_tilemap(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().tilemaps.contains_key(&id)
    }

    /// Step 8-1 (additive): the tag of the static tile at (x, y) — the
    /// topmost layer's that has one — or `""`. The per-cell replacement for
    /// `has_tag(get_entity_at(x, y), ...)` now that a wall or floor has no
    /// entity of its own. Floors answer too (unlike every other query here,
    /// this reads the cell whether or not it's solid). Checks tilemaps in
    /// ascending id order; the first non-empty tag wins.
    pub fn get_tile_tag(&mut self, x: f64, y: f64) -> String {
        let s = self.inner.borrow_mut();
        let (Some(cx), Some(cy)) = (cell_of(x), cell_of(y)) else { return String::new() };
        for map in s.tilemaps.values() {
            let tag = map.tag_at(cx, cy);
            if !tag.is_empty() {
                return tag.to_string();
            }
        }
        String::new()
    }
    /// `i64` overload — same reasoning as `get_entity_at_i` above.
    pub fn get_tile_tag_i(&mut self, x: i64, y: i64) -> String {
        self.get_tile_tag(x as f64, y as f64)
    }
}
