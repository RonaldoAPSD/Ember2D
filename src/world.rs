// world.rs — The game world: entity management and component storage.

use std::collections::HashMap;

use crate::components::{Animator, Collider, Script, Sprite, Tag, Transform};
use crate::event::{EventBus, GameEvent};
use crate::math::{Rect, Vec2};

use serde::{Serialize, Deserialize};

/// An entity is just a unique integer ID.
pub type EntityId = u64;

/// The game world: holds all entities and their component data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    /// Counter used to generate unique entity IDs.
    pub next_id: EntityId,

    // ── Component stores ─────────────────────────────────────────────────
    pub transforms: HashMap<EntityId, Transform>,
    pub sprites:    HashMap<EntityId, Sprite>,
    pub colliders:  HashMap<EntityId, Collider>,
    pub tags:       HashMap<EntityId, Tag>,
    pub scripts:    HashMap<EntityId, Script>,
    /// Animation playback state (Phase 3, Step 3c). `#[serde(default)]` so a
    /// save file from before this field existed still deserializes — it
    /// just loads with no entities animating, same as any other new
    /// component store would.
    #[serde(default)]
    pub animators:  HashMap<EntityId, Animator>,
}

impl World {
    /// Create an empty world with no entities or components.
    pub fn new() -> Self {
        World {
            next_id: 1,
            transforms: HashMap::new(),
            sprites:    HashMap::new(),
            colliders:  HashMap::new(),
            tags:       HashMap::new(),
            scripts:    HashMap::new(),
            animators:  HashMap::new(),
        }
    }

    // ── Entity lifecycle ─────────────────────────────────────────────────

    pub fn spawn(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn despawn(&mut self, id: EntityId) {
        self.transforms.remove(&id);
        self.sprites.remove(&id);
        self.colliders.remove(&id);
        self.tags.remove(&id);
        self.scripts.remove(&id);
        self.animators.remove(&id);
    }

    // ── Component accessors ───────────────────────────────────────────────

    pub fn add_transform(&mut self, id: EntityId, t: Transform) { self.transforms.insert(id, t); }
    pub fn add_sprite(&mut self, id: EntityId, s: Sprite)       { self.sprites.insert(id, s); }
    pub fn add_collider(&mut self, id: EntityId, c: Collider)   { self.colliders.insert(id, c); }
    pub fn add_tag(&mut self, id: EntityId, t: Tag)             { self.tags.insert(id, t); }
    pub fn add_script(&mut self, id: EntityId, s: Script)       { self.scripts.insert(id, s); }
    pub fn remove_script(&mut self, id: EntityId)               { self.scripts.remove(&id); }

    // ── Hierarchy ─────────────────────────────────────────────────────────

    /// Get the world-space position of an entity by traversing up its parent chain.
    pub fn get_global_position(&self, id: EntityId) -> Vec2 {
        let mut pos = Vec2::ZERO;
        let mut current_id = Some(id);
        let mut depth = 0;
        
        while let Some(cid) = current_id {
            if let Some(tf) = self.transforms.get(&cid) {
                pos += tf.position;
                current_id = tf.parent;
            } else {
                break;
            }
            depth += 1;
            if depth > 100 { 
                eprintln!("WARN: entity hierarchy cycle detected for entity {}", id);
                break; 
            }
        }
        pos
    }

    /// Set the parent of an entity. 
    /// If `keep_world_position` is true, the local position is adjusted so the 
    /// entity doesn't jump in world space.
    pub fn set_parent(&mut self, id: EntityId, parent: Option<EntityId>, keep_world_position: bool) {
        let (parent_id, new_pos) = if keep_world_position {
            let current_global = self.get_global_position(id);
            let new_parent_global = parent.map(|p| self.get_global_position(p)).unwrap_or(Vec2::ZERO);
            (parent, current_global - new_parent_global)
        } else {
            (parent, Vec2::ZERO)
        };

        if let Some(tf) = self.transforms.get_mut(&id) {
            tf.parent = parent_id;
            if keep_world_position {
                tf.position = new_pos;
            }
        }
    }

    // ── Query helpers ─────────────────────────────────────────────────────

    pub fn find_by_tag(&self, name: &str) -> Option<EntityId> {
        self.tags.iter().find(|(_, tag)| tag.name == name).map(|(id, _)| *id)
    }

    pub fn entities_with_transform(&self) -> Vec<EntityId> {
        self.transforms.keys().copied().collect()
    }

    pub fn entity_ids(&self) -> Vec<EntityId> {
        let mut ids: std::collections::HashSet<EntityId> = self.transforms.keys().copied().collect();
        ids.extend(self.sprites.keys().copied());
        ids.extend(self.colliders.keys().copied());
        ids.extend(self.tags.keys().copied());
        ids.into_iter().collect()
    }

    pub fn remove_transform(&mut self, id: EntityId) { self.transforms.remove(&id); }
    pub fn remove_sprite(&mut self, id: EntityId)    { self.sprites.remove(&id); }
    pub fn remove_collider(&mut self, id: EntityId) { self.colliders.remove(&id); }
    pub fn remove_tag(&mut self, id: EntityId)      { self.tags.remove(&id); }

    // ── Physics & collision ───────────────────────────────────────────────

    pub fn integrate_physics(&mut self, delta_time: f32) {
        for transform in self.transforms.values_mut() {
            transform.integrate(delta_time);
        }
    }

    pub fn detect_collisions(&self, events: &mut EventBus) {
        // Build world-space rects for all collidable entities.
        let collidables: Vec<(EntityId, Rect, String, Vec<String>)> = self
            .colliders
            .keys()
            .filter_map(|&id| {
                if !self.transforms.contains_key(&id) { return None; }
                let pos = self.get_global_position(id);
                let col = self.colliders.get(&id).unwrap();
                Some((id, col.world_rect(pos.x, pos.y), col.layer.clone(), col.mask.clone()))
            })
            .collect();

        for i in 0..collidables.len() {
            for j in (i + 1)..collidables.len() {
                let (id_a, rect_a, layer_a, mask_a) = &collidables[i];
                let (id_b, rect_b, layer_b, mask_b) = &collidables[j];

                let a_allows_b = mask_a.is_empty() || mask_a.contains(layer_b);
                let b_allows_a = mask_b.is_empty() || mask_b.contains(layer_a);

                if a_allows_b && b_allows_a && rect_a.intersects(*rect_b) {
                    events.emit(GameEvent::Collision { entity_a: *id_a, entity_b: *id_b });
                }
            }
        }
    }

    pub fn snapshot_positions(&self) -> HashMap<EntityId, Vec2> {
        self.transforms.iter().map(|(id, tf)| (*id, tf.position)).collect()
    }

    pub fn rollback_position(&mut self, id: EntityId, snapshot: &HashMap<EntityId, Vec2>) {
        if let (Some(tf), Some(&prev_pos)) = (self.transforms.get_mut(&id), snapshot.get(&id)) {
            tf.position = prev_pos;
            tf.velocity = Vec2::ZERO;
        }
    }

    pub fn resolve_solid_collision(
        &mut self,
        mover_id:    EntityId,
        obstacle_id: EntityId,
        _prev:       &HashMap<EntityId, Vec2>,
    ) {
        let (global_x, global_y, mover_w, mover_h) = {
            if !self.colliders.contains_key(&mover_id) { return };
            let col = &self.colliders[&mover_id];
            let pos = self.get_global_position(mover_id);
            (pos.x, pos.y, col.width, col.height)
        };

        let obstacle_rect = {
            if !self.colliders.contains_key(&obstacle_id) { return };
            let col = &self.colliders[&obstacle_id];
            let pos = self.get_global_position(obstacle_id);
            col.world_rect(pos.x, pos.y)
        };

        let mover_rect = Rect::new(global_x, global_y, mover_w, mover_h);

        let overlap_x = mover_rect.right().min(obstacle_rect.right()) - mover_rect.x.max(obstacle_rect.x);
        let overlap_y = mover_rect.bottom().min(obstacle_rect.bottom()) - mover_rect.y.max(obstacle_rect.y);

        if overlap_x <= 0.0 || overlap_y <= 0.0 { return; }

        if let Some(tf) = self.transforms.get_mut(&mover_id) {
            if overlap_x <= overlap_y {
                let mover_cx   = global_x + mover_w * 0.5;
                let obstacle_cx = obstacle_rect.x + obstacle_rect.w * 0.5;
                if mover_cx < obstacle_cx { tf.position.x -= overlap_x; } 
                else { tf.position.x += overlap_x; }
                tf.velocity.x = 0.0;
            } else {
                let mover_cy    = global_y + mover_h * 0.5;
                let obstacle_cy = obstacle_rect.y + obstacle_rect.h * 0.5;
                if mover_cy < obstacle_cy { tf.position.y -= overlap_y; }
                else { tf.position.y += overlap_y; }
                tf.velocity.y = 0.0;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::Animator;

    #[test]
    fn a_world_with_animators_round_trips_through_ron() {
        let mut world = World::new();
        let id = world.spawn();
        world.animators.insert(id, Animator::new("flicker"));

        let ron = ron::to_string(&world).expect("World must serialize");
        let restored: World = ron::from_str(&ron).expect("World must deserialize");
        assert_eq!(restored.animators.get(&id).map(|a| a.clip.as_str()), Some("flicker"));
    }

    #[test]
    fn a_saved_world_from_before_the_animators_store_existed_still_loads() {
        // Step 3c added `animators` to an already-shipped serialized type;
        // #[serde(default)] is what keeps an old save (missing the field
        // entirely) loading instead of erroring out.
        let pre_step_3c_ron = "(next_id:1,transforms:{},sprites:{},colliders:{},tags:{},scripts:{})";
        let restored: World = ron::from_str(pre_step_3c_ron).expect("a World RON with no `animators` key must still deserialize");
        assert!(restored.animators.is_empty());
    }

    #[test]
    fn despawn_removes_the_entitys_animator() {
        let mut world = World::new();
        let id = world.spawn();
        world.animators.insert(id, Animator::new("flicker"));
        world.despawn(id);
        assert!(!world.animators.contains_key(&id), "despawn must clean up the animators store like every other component store");
    }
}
