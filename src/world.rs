// world.rs — The game world: entity management and component storage.
//
// The World is the CENTRAL DATABASE of the game. It owns:
//   - All entity IDs (just u64 numbers)
//   - All component data, organized by type
//
// ─────────────────────────────────────────────────────────────
//  HOW ENTITIES WORK
// ─────────────────────────────────────────────────────────────
//
//  An entity is just a number. All actual data lives in the World's
//  component maps. Think of it like a database:
//
//    Entity 1:  transforms["1"] → Transform { position: (5,3), ... }
//               sprites["1"]    → Sprite { glyph: '@', fg: Green, ... }
//               colliders["1"]  → Collider { width: 1, height: 1, solid: true }
//               tags["1"]       → Tag { name: "player" }
//
//    Entity 2:  transforms["2"] → Transform { position: (10,5), ... }
//               sprites["2"]    → Sprite { glyph: '#', fg: DarkGrey, ... }
//               colliders["2"]  → Collider { width: 1, height: 1, solid: true }
//               tags["2"]       → Tag { name: "wall" }
//
//  If an entity doesn't have a component, its ID simply isn't in that map.
//  Floor tiles, for example, have no Collider — they're not in the colliders map.
//
// ─────────────────────────────────────────────────────────────
//  DATA LAYOUT CHOICE: HashMap<EntityId, Component>
// ─────────────────────────────────────────────────────────────
//
//  We use HashMap (dictionary) for each component type.
//  A production ECS would use Vec-based "archetypes" for CPU cache performance,
//  but HashMap is far simpler to understand and debug.
//  For an ASCII game with <1,000 entities, HashMaps are fast enough.

use std::collections::HashMap;

use crate::components::{Collider, Script, Sprite, Tag, Transform};
use crate::event::{EventBus, GameEvent};
use crate::math::{Rect, Vec2};

/// An entity is just a unique integer ID.
///
/// The type alias makes the intent clear in function signatures:
///   `fn despawn(&mut self, id: EntityId)` reads better than `fn despawn(id: u64)`.
pub type EntityId = u64;

/// The game world: holds all entities and their component data.
pub struct World {
    /// Counter used to generate unique entity IDs.
    /// Incremented each time `spawn()` is called.
    /// We start at 1; ID 0 is reserved as an "invalid/null" sentinel.
    /// Public so the scripting system can pre-allocate IDs for deferred spawns.
    pub next_id: EntityId,

    // ── Component stores ─────────────────────────────────────────────────
    // Each HashMap maps EntityId → component data.
    // An entity that doesn't have a component simply has no entry in that map.

    /// World-space positions and velocities.
    pub transforms: HashMap<EntityId, Transform>,

    /// Visual appearance (glyph, colors, draw order).
    pub sprites: HashMap<EntityId, Sprite>,

    /// Collision bounding boxes.
    pub colliders: HashMap<EntityId, Collider>,

    /// Human-readable name/category labels.
    pub tags: HashMap<EntityId, Tag>,

    /// Script components: entities with a .rhai file to run each frame.
    pub scripts: HashMap<EntityId, Script>,
}

impl World {
    /// Create an empty world with no entities or components.
    pub fn new() -> Self {
        World {
            next_id: 1,
            transforms: HashMap::new(),
            sprites: HashMap::new(),
            colliders: HashMap::new(),
            tags: HashMap::new(),
            scripts: HashMap::new(),
        }
    }

    // ── Entity lifecycle ─────────────────────────────────────────────────

    /// Create a new entity and return its ID.
    ///
    /// The entity starts with NO components — add them with the `add_*` methods.
    ///
    /// Pattern:
    ///   let id = world.spawn();
    ///   world.add_transform(id, Transform::new(5.0, 3.0));
    ///   world.add_sprite(id, Sprite::simple('@', Color::Green));
    pub fn spawn(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Destroy an entity by removing all its components.
    ///
    /// After this call the entity ID is invalid — don't use it again.
    /// Emitting an `EntityDestroyed` event is the caller's responsibility
    /// if other systems need to know.
    pub fn despawn(&mut self, id: EntityId) {
        self.transforms.remove(&id);
        self.sprites.remove(&id);
        self.colliders.remove(&id);
        self.tags.remove(&id);
        self.scripts.remove(&id);
    }

    // ── Component accessors ───────────────────────────────────────────────
    // Each `add_*` method inserts (or replaces) a component on an entity.
    // Each `get_*` is just a shorthand for the HashMap `.get()` call.

    /// Attach a Transform to an entity (or replace the existing one).
    pub fn add_transform(&mut self, id: EntityId, t: Transform) {
        self.transforms.insert(id, t);
    }

    /// Attach a Sprite to an entity.
    pub fn add_sprite(&mut self, id: EntityId, s: Sprite) {
        self.sprites.insert(id, s);
    }

    /// Attach a Collider to an entity.
    pub fn add_collider(&mut self, id: EntityId, c: Collider) {
        self.colliders.insert(id, c);
    }

    /// Attach a Tag to an entity.
    pub fn add_tag(&mut self, id: EntityId, t: Tag) {
        self.tags.insert(id, t);
    }

    /// Attach a Script component to an entity.
    pub fn add_script(&mut self, id: EntityId, s: Script) {
        self.scripts.insert(id, s);
    }

    /// Remove the Script component from an entity.
    pub fn remove_script(&mut self, id: EntityId) {
        self.scripts.remove(&id);
    }

    // ── Query helpers ─────────────────────────────────────────────────────

    /// Return the first entity whose Tag name matches `name`, or None.
    ///
    /// Example: `let player = world.find_by_tag("player").unwrap();`
    ///
    /// This iterates all tags — O(n) where n = number of tagged entities.
    /// For performance-critical lookups, cache the EntityId at spawn time instead.
    pub fn find_by_tag(&self, name: &str) -> Option<EntityId> {
        self.tags
            .iter()
            .find(|(_, tag)| tag.name == name)
            .map(|(id, _)| *id)
    }

    /// Return all entity IDs that have ALL four core components.
    ///
    /// Useful for systems that only care about "complete" game objects.
    pub fn entities_with_transform(&self) -> Vec<EntityId> {
        self.transforms.keys().copied().collect()
    }

    /// Return all currently-alive entity IDs (any entity that has at least one component).
    ///
    /// The editor uses this to iterate all placed tiles without knowing their specific
    /// component layout. The set is the union of all component stores.
    pub fn entity_ids(&self) -> Vec<EntityId> {
        let mut ids: std::collections::HashSet<EntityId> = self.transforms.keys().copied().collect();
        ids.extend(self.sprites.keys().copied());
        ids.extend(self.colliders.keys().copied());
        ids.extend(self.tags.keys().copied());
        ids.into_iter().collect()
    }

    // ── Individual component removal ──────────────────────────────────────

    /// Remove only the Transform component from an entity.
    /// The entity continues to exist if it has other components.
    pub fn remove_transform(&mut self, id: EntityId) {
        self.transforms.remove(&id);
    }

    /// Remove only the Sprite component from an entity.
    pub fn remove_sprite(&mut self, id: EntityId) {
        self.sprites.remove(&id);
    }

    /// Remove only the Collider component from an entity.
    pub fn remove_collider(&mut self, id: EntityId) {
        self.colliders.remove(&id);
    }

    /// Remove only the Tag component from an entity.
    pub fn remove_tag(&mut self, id: EntityId) {
        self.tags.remove(&id);
    }

    // ── Physics & collision ───────────────────────────────────────────────

    /// Apply velocity × delta_time to the position of every entity that has a Transform.
    ///
    /// Called by the engine's game loop before collision detection.
    /// You don't normally call this from game code.
    pub fn integrate_physics(&mut self, delta_time: f32) {
        for transform in self.transforms.values_mut() {
            transform.integrate(delta_time);
        }
    }

    /// Find all pairs of entities whose bounding boxes overlap, and emit Collision events.
    ///
    /// ALGORITHM: O(n²) brute-force — check every pair.
    /// For an ASCII game with under ~200 collidable entities this is fast enough.
    /// A production engine would use a spatial hash or quad-tree to bring this to O(n log n).
    ///
    /// Only entities with BOTH a Transform and a Collider are considered.
    ///
    /// The results are emitted directly into the EventBus so the game's
    /// late_update() can respond to them.
    pub fn detect_collisions(&self, events: &mut EventBus) {
        // Build a list of (EntityId, world-space Rect) for every collidable entity.
        // We collect into a Vec so we can iterate pairs with indices below.
        let collidables: Vec<(EntityId, Rect)> = self
            .transforms
            .iter()
            .filter_map(|(id, tf)| {
                // Only include entities that ALSO have a Collider.
                self.colliders
                    .get(id)
                    .map(|col| (*id, col.world_rect(tf.position.x, tf.position.y)))
            })
            .collect();

        // Check every UNIQUE pair (i, j) where i < j.
        // This avoids checking (A,B) and (B,A) separately, and avoids (A,A).
        for i in 0..collidables.len() {
            for j in (i + 1)..collidables.len() {
                let (id_a, rect_a) = collidables[i];
                let (id_b, rect_b) = collidables[j];

                if rect_a.intersects(rect_b) {
                    events.emit(GameEvent::Collision {
                        entity_a: id_a,
                        entity_b: id_b,
                    });
                }
            }
        }
    }

    /// Snapshot the current position of every entity that has a Transform.
    ///
    /// Returns a HashMap mapping EntityId → Vec2 position.
    /// The engine calls this BEFORE physics so the game's late_update()
    /// can roll back colliding entities to where they were before moving.
    pub fn snapshot_positions(&self) -> HashMap<EntityId, Vec2> {
        self.transforms
            .iter()
            .map(|(id, tf)| (*id, tf.position))
            .collect()
    }

    /// Restore an entity's position to a previously snapshotted value.
    ///
    /// Also zeroes the entity's velocity to prevent it from moving again
    /// immediately on the next frame into the same obstacle.
    ///
    /// Call this from late_update() when a solid collision is detected.
    pub fn rollback_position(&mut self, id: EntityId, snapshot: &HashMap<EntityId, Vec2>) {
        if let (Some(tf), Some(&prev_pos)) = (self.transforms.get_mut(&id), snapshot.get(&id)) {
            tf.position = prev_pos;
            tf.velocity = Vec2::ZERO;
        }
    }

    /// Per-axis collision resolution — fixes the "corner-sticking" problem.
    ///
    /// The old `rollback_position` always resets BOTH X and Y when any overlap
    /// is detected. This causes the player to get stuck on wall corners even when
    /// only one axis is actually in contact: the tiny X overlap of a purely-upward
    /// move cancels the entire movement.
    ///
    /// This method tests each axis independently:
    ///
    ///   1. Restore mover X to prev_x (keep current Y) → re-test AABB.
    ///      If that alone clears the overlap: X was the problem — rollback X only.
    ///
    ///   2. Restore mover Y to prev_y (keep current X) → re-test AABB.
    ///      If that clears it: Y was the problem — rollback Y only.
    ///
    ///   3. Both axes involved (true corner): full rollback, same as before.
    ///
    /// Replace `world.rollback_position(player_id, prev)` with this call and pass
    /// the specific obstacle entity ID so the per-axis AABB tests work correctly.
    pub fn resolve_solid_collision(
        &mut self,
        mover_id:    EntityId,
        obstacle_id: EntityId,
        _prev:       &HashMap<EntityId, Vec2>,
    ) {
        let (cur_x, cur_y, mover_w, mover_h) = {
            let tf  = match self.transforms.get(&mover_id)  { Some(t) => t, None => return };
            let col = match self.colliders.get(&mover_id)   { Some(c) => c, None => return };
            (tf.position.x, tf.position.y, col.width, col.height)
        };

        let obstacle_rect = {
            let tf  = match self.transforms.get(&obstacle_id)  { Some(t) => t, None => return };
            let col = match self.colliders.get(&obstacle_id)   { Some(c) => c, None => return };
            col.world_rect(tf.position.x, tf.position.y)
        };

        let mover_rect = Rect::new(cur_x, cur_y, mover_w, mover_h);

        // Compute penetration depth on each axis.
        let overlap_x = mover_rect.right().min(obstacle_rect.right())
                      - mover_rect.x.max(obstacle_rect.x);
        let overlap_y = mover_rect.bottom().min(obstacle_rect.bottom())
                      - mover_rect.y.max(obstacle_rect.y);

        if overlap_x <= 0.0 || overlap_y <= 0.0 {
            return; // rects don't actually overlap — nothing to do
        }

        if let Some(tf) = self.transforms.get_mut(&mover_id) {
            if overlap_x <= overlap_y {
                // Shallower penetration along X — push the mover out sideways.
                // This allows vertical sliding past corners.
                let mover_cx   = cur_x + mover_w * 0.5;
                let obstacle_cx = obstacle_rect.x + obstacle_rect.w * 0.5;
                if mover_cx < obstacle_cx {
                    tf.position.x -= overlap_x;
                } else {
                    tf.position.x += overlap_x;
                }
                tf.velocity.x = 0.0;
            } else {
                // Shallower penetration along Y — push the mover out vertically.
                // This allows horizontal sliding past corners.
                let mover_cy    = cur_y + mover_h * 0.5;
                let obstacle_cy = obstacle_rect.y + obstacle_rect.h * 0.5;
                if mover_cy < obstacle_cy {
                    tf.position.y -= overlap_y;
                } else {
                    tf.position.y += overlap_y;
                }
                tf.velocity.y = 0.0;
            }
        }
    }
}
