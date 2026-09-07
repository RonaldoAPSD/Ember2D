// world.rs — The game world: entity management and component storage.

use std::collections::{BTreeMap, HashMap};

use crate::components::{Actor, Animator, Collider, Script, Sprite, Tag, Transform};
use crate::event::{EventBus, GameEvent};
use crate::math::{Rect, Vec2};

use serde::{Serialize, Deserialize};

/// An entity is just a unique integer ID.
pub type EntityId = u64;

/// The game world: holds all entities and their component data.
///
/// Component stores are `BTreeMap`, not `HashMap` (Step 5b,
/// docs/ember2d-phase5-plan.md, §5.2 H1 in the refactor plan) — `HashMap`'s
/// iteration order varies between processes, and this world gets iterated
/// in order-sensitive ways all over the sim: `find_by_tag` returns the
/// first match, `detect_collisions` builds its pairwise list by iterating
/// `colliders`, and every script-execution pass in `ScriptEngine` iterates
/// `scripts` to decide call order — which, since scripts mutate shared
/// state via last-write-wins, is exactly what determines the outcome when
/// two scripts touch the same key in one frame. `BTreeMap` makes every one
/// of those deterministic (sorted by `EntityId`) for free, without touching
/// the call sites — see `docs/ember2d-refactor-plan.md` §5.2 H1.
/// Phase 6 Step 11 (docs/ember2d-phase6-plan.md): a new component store
/// needs all SIX of these touched to stay correct — the readable, low-tech
/// version of the "component registration macro" the refactor plan
/// originally asked for (rejected: it would contradict CLAUDE.md's
/// "deliberate learning artifact" rule, and the one place this list *was*
/// out of sync — `entity_ids()`, until this step — had zero production
/// callers to ever surface the bug, meaning a macro would have hidden it
/// just as effectively as hand-writing it wrong did).
///
/// 1. The field itself, in the `World` struct below.
/// 2. `World::new()`'s initializer.
/// 3. `despawn()`'s removal line.
/// 4. `entity_ids()`'s union — missed for `scripts`/`animators`/`actors`
///    until this step; see that method's own doc comment for the fix.
/// 5. An `add_<component>` method, in "Component accessors" below.
/// 6. A `remove_<component>` method, same section — only if anything ever
///    needs to remove that component independently of despawning the whole
///    entity (`animators`/`actors` didn't have one until this step either;
///    see `add_animator`/`remove_actor`/`remove_animator`'s own comments).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct World {
    /// Counter used to generate unique entity IDs.
    pub next_id: EntityId,

    // ── Component stores ─────────────────────────────────────────────────
    pub transforms: BTreeMap<EntityId, Transform>,
    pub sprites:    BTreeMap<EntityId, Sprite>,
    pub colliders:  BTreeMap<EntityId, Collider>,
    pub tags:       BTreeMap<EntityId, Tag>,
    pub scripts:    BTreeMap<EntityId, Script>,
    /// Animation playback state (Phase 3, Step 3c). `#[serde(default)]` so a
    /// save file from before this field existed still deserializes — it
    /// just loads with no entities animating, same as any other new
    /// component store would.
    #[serde(default)]
    pub animators:  BTreeMap<EntityId, Animator>,
    /// Turn-scheduling eligibility (Step 5f, docs/ember2d-phase5-plan.md).
    /// `#[serde(default)]` for the same reason `animators` has it — an old
    /// save predating this field just loads with no entities schedulable,
    /// same as any other new component store would.
    #[serde(default)]
    pub actors:     BTreeMap<EntityId, Actor>,
}

impl World {
    /// Create an empty world with no entities or components.
    pub fn new() -> Self {
        World {
            next_id: 1,
            transforms: BTreeMap::new(),
            sprites:    BTreeMap::new(),
            colliders:  BTreeMap::new(),
            tags:       BTreeMap::new(),
            scripts:    BTreeMap::new(),
            animators:  BTreeMap::new(),
            actors:     BTreeMap::new(),
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
        self.actors.remove(&id);
    }

    // ── Component accessors ───────────────────────────────────────────────

    pub fn add_transform(&mut self, id: EntityId, t: Transform) { self.transforms.insert(id, t); }
    pub fn add_sprite(&mut self, id: EntityId, s: Sprite)       { self.sprites.insert(id, s); }
    pub fn add_collider(&mut self, id: EntityId, c: Collider)   { self.colliders.insert(id, c); }
    pub fn add_tag(&mut self, id: EntityId, t: Tag)             { self.tags.insert(id, t); }
    pub fn add_script(&mut self, id: EntityId, s: Script)       { self.scripts.insert(id, s); }
    pub fn remove_script(&mut self, id: EntityId)               { self.scripts.remove(&id); }
    pub fn add_actor(&mut self, id: EntityId, a: Actor)         { self.actors.insert(id, a); }
    /// Phase 6 Step 11 (docs/ember2d-phase6-plan.md): added for symmetry
    /// with every other component (see the checklist on `World`'s own doc
    /// comment) — existing call sites still insert into `self.animators`
    /// directly where a plain `insert` doesn't fit (e.g. `apply_ctx`'s
    /// `play_clip` handling, which needs `.entry(id).or_insert_with(...)`
    /// to preserve an already-playing `Animator`'s state), so this isn't a
    /// call site migration, just closing the API gap for whoever writes the
    /// next one.
    pub fn add_animator(&mut self, id: EntityId, a: Animator)   { self.animators.insert(id, a); }
    pub fn remove_actor(&mut self, id: EntityId)                { self.actors.remove(&id); }
    pub fn remove_animator(&mut self, id: EntityId)             { self.animators.remove(&id); }

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

    /// Returns the *lowest* `EntityId` tagged `name`, deterministically —
    /// `self.tags` is a `BTreeMap`, so `.iter().find(...)` walks in
    /// ascending id order. When more than one entity shares a tag (common —
    /// every "enemy" on a floor shares one), which one this returns is a
    /// real, load-bearing choice, not an arbitrary HashMap artifact.
    pub fn find_by_tag(&self, name: &str) -> Option<EntityId> {
        self.tags.iter().find(|(_, tag)| tag.name == name).map(|(id, _)| *id)
    }

    pub fn entities_with_transform(&self) -> Vec<EntityId> {
        self.transforms.keys().copied().collect()
    }

    /// Sorted ascending — `entity_ids` used to build an intermediate
    /// `HashSet` purely to de-duplicate across the component stores, which
    /// threw away the `BTreeMap` ordering the stores themselves now
    /// guarantee. `BTreeSet` keeps the de-dup and restores the order.
    ///
    /// Phase 6 Step 11 (docs/ember2d-phase6-plan.md): this used to union
    /// only four of the seven stores (`transforms`/`sprites`/`colliders`/
    /// `tags`), silently omitting an entity whose only components are
    /// `scripts`/`animators`/`actors` — latent today (grep confirms nothing
    /// in this workspace calls `entity_ids` outside its own test), but a
    /// real bug for the first real caller. Fixed to union all seven — see
    /// the checklist on `World`'s own doc comment above for what a future
    /// new component store must touch to avoid the same drift.
    pub fn entity_ids(&self) -> Vec<EntityId> {
        let mut ids: std::collections::BTreeSet<EntityId> = self.transforms.keys().copied().collect();
        ids.extend(self.sprites.keys().copied());
        ids.extend(self.colliders.keys().copied());
        ids.extend(self.tags.keys().copied());
        ids.extend(self.scripts.keys().copied());
        ids.extend(self.animators.keys().copied());
        ids.extend(self.actors.keys().copied());
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

    /// Phase 6 Step 7 (docs/ember2d-phase6-plan.md): `collidables` is fully
    /// `Copy` now — `(EntityId, Rect, layer_bits: u32, mask_bits: u32)` reads
    /// straight off each `Collider`'s own pre-resolved bits instead of
    /// cloning a `String` layer and a `Vec<String>` mask per entity (1,704
    /// of each at floor2 scale) just to build this list, and the pairwise
    /// test below is a bitwise AND instead of a string compare / `Vec`
    /// `.contains()` scan. `mask_bits == 0` still means "matches everything"
    /// — see `crate::layers::LayerRegistry::mask_bits`'s doc comment for why
    /// that's the same encoding the old `Vec::is_empty()` check used.
    ///
    /// Phase 6 Step 8 (docs/ember2d-phase6-plan.md): sweep-and-prune broad
    /// phase, replacing the old pure O(colliders²) pairwise scan (~1.45M pair
    /// tests at floor2 scale). Sorting `collidables` by `rect.x` first means
    /// the inner loop can `break` the moment `rect_b`'s left edge has moved
    /// past `rect_a`'s right edge — every entry beyond that point is sorted
    /// further right still, so none of them can overlap `a` on the x-axis
    /// either (~1.45M → ~68k pair tests at floor2). Zero extra allocation:
    /// `collidables` sorts in place, and `Rect`/the bit fields are all `Copy`.
    ///
    /// **Determinism trap, not a redundant step:** sorting by `rect.x`
    /// destroys the old double loop's emission order (walking `collidables`
    /// in ascending `EntityId`, since it was built from `self.colliders`'s
    /// `BTreeMap` iteration), and `on_collide` call order
    /// (`Simulation::late_step` → `ScriptEngine::run_collisions`) is exactly
    /// that emission order — two scripts writing the same global in one pass
    /// last-write-wins on whichever ran later (see `player.rhai`'s own header
    /// comment), so a changed emission order is a real behavior change, not
    /// a cosmetic one. Fixed by collecting every hit as a normalized
    /// `(min(id_a, id_b), max(id_a, id_b))` pair into `hits` *without*
    /// emitting, then sorting `hits` (plain tuple `Ord`, ascending) and only
    /// *then* emitting — this reproduces the old loop's exact emission order
    /// (which always had `entity_a` be the lower id, walked in ascending-pair
    /// order) byte-for-byte, so `on_collide` sees an identical call sequence
    /// to before this step. This collect-then-sort-then-emit shape looks like
    /// wasted work next to just emitting inline during the sweep — it isn't:
    /// the sweep's own discovery order is sorted by position, not by id, and
    /// only the final sort restores the id-ordered sequence scripts depend on.
    pub fn detect_collisions(&self, events: &mut EventBus) {
        // Build world-space rects for all collidable entities.
        let mut collidables: Vec<(EntityId, Rect, u32, u32)> = self
            .colliders
            .keys()
            .filter_map(|&id| {
                if !self.transforms.contains_key(&id) { return None; }
                let pos = self.get_global_position(id);
                let col = self.colliders.get(&id).unwrap();
                Some((id, col.world_rect(pos.x, pos.y), col.layer_bits(), col.mask_bits()))
            })
            .collect();

        // R6 (7A-1, docs/ember2d-master-plan.md) / CLAUDE.md's determinism
        // rule: `partial_cmp(..).unwrap_or(Equal)` treats every NaN
        // comparison as "equal" to everything, including values that are
        // NOT equal to each other — an inconsistent ordering that broke
        // `sort_unstable_by`'s own invariants (a NaN position reaches here
        // if a script's own bad math, e.g. `0.0 / 0.0`, ever got past
        // `set_position`'s guard — see that method's own R6 comment for the
        // fix at the source). `total_cmp` is a genuine total order over
        // every `f32` bit pattern, NaN included, so the sort never sees an
        // inconsistent comparison in the first place.
        collidables.sort_unstable_by(|a, b| a.1.x.total_cmp(&b.1.x));

        let mut hits: Vec<(EntityId, EntityId)> = Vec::new();
        for i in 0..collidables.len() {
            let (id_a, rect_a, layer_a, mask_a) = collidables[i];
            for j in (i + 1)..collidables.len() {
                let (id_b, rect_b, layer_b, mask_b) = collidables[j];
                // Sorted by x ascending: once b's left edge is past a's right
                // edge, every later entry (sorted further right still) is
                // too — nothing beyond this point can overlap `a`.
                if rect_b.x >= rect_a.right() { break; }

                let a_allows_b = mask_a == 0 || (mask_a & layer_b) != 0;
                let b_allows_a = mask_b == 0 || (mask_b & layer_a) != 0;

                if a_allows_b && b_allows_a && rect_a.intersects(rect_b) {
                    hits.push(if id_a < id_b { (id_a, id_b) } else { (id_b, id_a) });
                }
            }
        }

        hits.sort_unstable();
        for (entity_a, entity_b) in hits {
            events.emit(GameEvent::Collision { entity_a, entity_b });
        }
    }

    /// Recompute every `Collider`'s `layer_bits`/`mask_bits` against
    /// `registry` — needed after loading a saved `World` (`Collider`'s bits
    /// are `#[serde(skip)]`, so they deserialize as `0`) since the strings
    /// that DID survive serialization are otherwise never re-resolved. See
    /// `Collider`'s own header comment (components/collider.rs) for why
    /// forgetting to call this is the one mistake here with no visible
    /// symptom — `Simulation::on_start`'s `is_loading_save` branch is the
    /// one caller.
    pub fn refresh_collider_bits(&mut self, registry: &crate::layers::LayerRegistry) {
        for col in self.colliders.values_mut() { col.refresh_bits(registry); }
    }

    pub fn snapshot_positions(&self) -> HashMap<EntityId, Vec2> {
        self.transforms.iter().map(|(id, tf)| (*id, tf.position)).collect()
    }

    /// Same content as `snapshot_positions`, written into a caller-owned
    /// buffer instead of allocating a fresh `HashMap` every call. Phase 6
    /// Step 10 (docs/ember2d-phase6-plan.md): `ember2d::sim::step` calls
    /// this once per real frame (`Engine` owns the buffer across frames),
    /// where a fresh `HashMap::collect()` at floor2 scale (2,570 entities)
    /// was a real per-step allocation with nothing to show for it — the
    /// content is identical every time this runs, only the entity
    /// positions change. `HashMap::clear()` drops every entry but keeps the
    /// table's allocated capacity, so after the first frame that grows `out`
    /// to fit the level's entity count, every later call reuses that
    /// capacity: zero allocation from frame two onward. `snapshot_positions`
    /// itself stays as-is — this is additive, not a replacement, since its
    /// other callers (`TurnHarness`, `tests/shooter_arena.rs`,
    /// `bench_sim.rs`) are one-off per-test/per-benchmark uses with nothing
    /// to gain from a caller-owned buffer they'd only ever call once anyway.
    pub fn snapshot_positions_into(&self, out: &mut HashMap<EntityId, Vec2>) {
        out.clear();
        out.extend(self.transforms.iter().map(|(id, tf)| (*id, tf.position)));
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

    #[test]
    fn despawn_removes_the_entitys_actor() {
        let mut world = World::new();
        let id = world.spawn();
        world.add_actor(id, crate::components::Actor::ai(100));
        world.despawn(id);
        assert!(!world.actors.contains_key(&id), "despawn must clean up the actors store like every other component store");
    }

    // ── Tests: Step 5b deterministic iteration (docs/ember2d-phase5-plan.md
    // §5.2 H1) — component stores are BTreeMap, not HashMap, specifically so
    // iteration order is a property of the type and can't regress back to
    // HashMap's per-process randomness by accident. ─────────────────────────

    #[test]
    fn component_store_iteration_is_sorted_by_entity_id_regardless_of_insertion_order() {
        let mut world = World::new();
        // Insert out of order — a HashMap would happily accept this and
        // still iterate in its own (unspecified, per-process-random) order;
        // a BTreeMap must always yield ascending key order regardless.
        let ids: Vec<EntityId> = [30u64, 10, 20].iter().map(|&x| x).collect();
        for &id in &ids {
            world.transforms.insert(id, crate::components::Transform::new(0.0, 0.0));
            world.colliders.insert(id, crate::components::Collider::unit());
            world.tags.insert(id, crate::components::Tag::new("thing"));
        }

        let observed: Vec<EntityId> = world.transforms.keys().copied().collect();
        assert_eq!(observed, vec![10, 20, 30], "transforms must iterate in ascending EntityId order");
        let observed: Vec<EntityId> = world.colliders.keys().copied().collect();
        assert_eq!(observed, vec![10, 20, 30], "colliders must iterate in ascending EntityId order");
        let observed: Vec<EntityId> = world.tags.keys().copied().collect();
        assert_eq!(observed, vec![10, 20, 30], "tags must iterate in ascending EntityId order");
    }

    #[test]
    fn find_by_tag_deterministically_returns_the_lowest_id_when_multiple_entities_share_a_tag() {
        let mut world = World::new();
        // Insert the higher id first — if find_by_tag were still driven by
        // HashMap iteration order, insertion order (or process hash state)
        // could change which one comes back.
        world.tags.insert(50, crate::components::Tag::new("enemy"));
        world.tags.insert(5, crate::components::Tag::new("enemy"));
        world.tags.insert(25, crate::components::Tag::new("enemy"));
        assert_eq!(world.find_by_tag("enemy"), Some(5), "find_by_tag must deterministically return the lowest EntityId sharing the tag");
    }

    #[test]
    fn entity_ids_is_sorted_and_deduplicated_across_stores() {
        let mut world = World::new();
        world.transforms.insert(3, crate::components::Transform::new(0.0, 0.0));
        world.sprites.insert(1, crate::components::Sprite::simple('@', crate::color::Color::White));
        world.colliders.insert(2, crate::components::Collider::unit());
        // 3 appears in both transforms and tags — entity_ids must not list it twice.
        world.tags.insert(3, crate::components::Tag::new("dup"));

        assert_eq!(world.entity_ids(), vec![1, 2, 3]);
    }

    #[test]
    fn entity_ids_includes_entities_whose_only_component_is_a_script_animator_or_actor() {
        // Phase 6 Step 11 (docs/ember2d-phase6-plan.md): entity_ids() used
        // to union only transforms/sprites/colliders/tags — an entity with
        // nothing but a Script, Animator, or Actor component was silently
        // invisible to it. Pins the fix directly rather than trusting the
        // union list by inspection alone.
        let mut world = World::new();
        let script_only = world.spawn();
        world.add_script(script_only, crate::components::Script::new("x.rhai"));
        let animator_only = world.spawn();
        world.add_animator(animator_only, crate::components::Animator::new("clip"));
        let actor_only = world.spawn();
        world.add_actor(actor_only, crate::components::Actor::ai(100));

        let ids = world.entity_ids();
        assert!(ids.contains(&script_only), "an entity with only a Script component must be listed");
        assert!(ids.contains(&animator_only), "an entity with only an Animator component must be listed");
        assert!(ids.contains(&actor_only), "an entity with only an Actor component must be listed");
    }
}
