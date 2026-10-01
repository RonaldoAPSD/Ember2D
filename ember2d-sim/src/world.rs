// world.rs — The game world: entity management and component storage.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::components::{Actor, Animator, Collider, Script, Sprite, Tag, Tilemap, Transform, Vars};
use crate::event::{EventBus, GameEvent};
use crate::math::{Rect, Vec2};

use serde::{Deserialize, Serialize};

/// An entity is just a unique integer ID.
pub type EntityId = u64;

/// A structured warning `World` records about its own state, instead of
/// `eprintln!`-ing directly — Step 7.5-9 (docs/ember2d-master-plan.md §5.6,
/// R41 fix: `get_global_position`'s hierarchy-cycle bail-out used to print
/// straight to stderr, forbidden inside `ember2d-sim` by CLAUDE.md's
/// Determinism section). Drained into `StepOutcome::diagnostics`
/// (simulation.rs) once per `step`/`late_step`/`on_start` call, the same
/// "accumulate now, drain later" shape `ScriptEngine.pending_hud_draws`
/// already uses. A separate type from `scripting::types::LogEntry` rather
/// than reusing it: `world.rs` sits BELOW `scripting` in this crate's own
/// layering (scripting depends on world, not the reverse), so importing a
/// scripting-side type here would invert that.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub message: String,
}

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
    pub sprites: BTreeMap<EntityId, Sprite>,
    pub colliders: BTreeMap<EntityId, Collider>,
    pub tags: BTreeMap<EntityId, Tag>,
    pub scripts: BTreeMap<EntityId, Script>,
    /// Animation playback state (Phase 3, Step 3c). `#[serde(default)]` so a
    /// save file from before this field existed still deserializes — it
    /// just loads with no entities animating, same as any other new
    /// component store would.
    #[serde(default)]
    pub animators: BTreeMap<EntityId, Animator>,
    /// Turn-scheduling eligibility (Step 5f, docs/ember2d-phase5-plan.md).
    /// `#[serde(default)]` for the same reason `animators` has it — an old
    /// save predating this field just loads with no entities schedulable,
    /// same as any other new component store would.
    #[serde(default)]
    pub actors: BTreeMap<EntityId, Actor>,
    /// Arbitrary per-entity script state (Step 7.5-3, docs/ember2d-master-
    /// plan.md §5.6) — see `components/vars.rs`'s own doc comment for what
    /// this replaces. `#[serde(default)]` for the same reason `animators`/
    /// `actors` have it — an old save predating this field still loads, as
    /// if every entity's `Vars` were empty.
    #[serde(default)]
    pub vars: BTreeMap<EntityId, Vars>,
    /// Every static tile of the level, as a grid (Step 8-1, docs/ember2d-
    /// master-plan.md §5.7 — see `components/tilemap.rs`'s own header
    /// comment). Normally exactly one entry, spawned first by
    /// `Simulation::do_on_start`. Behind an `Rc` so `WorldSnapshot::build`
    /// shares it per step with a refcount bump rather than copying every
    /// cell — nothing mutates a tilemap after load today; `Rc::make_mut`
    /// (see `refresh_collider_bits`) is the seam if something ever does.
    /// `#[serde(default)]` so a pre-8-1 save — whose walls are all still
    /// entities — loads with none, and keeps working exactly as it did.
    #[serde(default)]
    pub tilemaps: BTreeMap<EntityId, Rc<Tilemap>>,
    /// Exit tiles' destination level paths, keyed by the exit entity's
    /// REAL id (Step 8-1 — R93, §3.2). `Simulation` used to keep this map
    /// itself and rebuild it on load by assuming "entity id = tile index +
    /// 1", true only while every tile spawned as an entity in order; the
    /// moment static tiles stopped spawning, that assumption pointed every
    /// stairs at the wrong entity. Living here, it's recorded at spawn from
    /// the id `spawn()` actually returned and travels with the `World`
    /// through a save — no reconstruction to get wrong. `#[serde(default)]`:
    /// a pre-8-1 save loads with none, and `Simulation::on_start`'s loading
    /// branch rebuilds them the old way (still correct for such a save —
    /// see `restore_legacy_exits`).
    #[serde(default)]
    pub exits: BTreeMap<EntityId, String>,
    /// Step 9-7 (docs/ember2d-master-plan.md §5.8): among sprites with the
    /// same layer order, draw whichever sits lower on screen in front
    /// (`ctx.set_y_sort`). Saved with the world so a loaded game keeps it.
    #[serde(default)]
    pub y_sort: bool,
    /// Step 7.5-9 (R41 fix, see `Diagnostic`'s own doc comment above) — a
    /// `RefCell`, not a plain `Vec`, specifically so `get_global_position`
    /// (a pure `&self` query every existing caller relies on staying
    /// read-only) can still record one without becoming `&mut self` and
    /// breaking every call site that borrows `World` immutably alongside
    /// it (`WorldSnapshot::build`, several `ScriptCtx` methods). Never part
    /// of a save — `#[serde(skip)]` — these are ephemeral, this-step-only
    /// warnings, not world state.
    #[serde(skip)]
    pub diagnostics: RefCell<Vec<Diagnostic>>,
}

impl World {
    /// Create an empty world with no entities or components.
    pub fn new() -> Self {
        World {
            next_id: 1,
            transforms: BTreeMap::new(),
            sprites: BTreeMap::new(),
            colliders: BTreeMap::new(),
            tags: BTreeMap::new(),
            scripts: BTreeMap::new(),
            animators: BTreeMap::new(),
            actors: BTreeMap::new(),
            vars: BTreeMap::new(),
            tilemaps: BTreeMap::new(),
            exits: BTreeMap::new(),
            y_sort: false,
            diagnostics: RefCell::new(Vec::new()),
        }
    }

    // ── Entity lifecycle ─────────────────────────────────────────────────

    pub fn spawn(&mut self) -> EntityId {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn despawn(&mut self, id: EntityId) {
        // Step 7.5-9 (docs/ember2d-master-plan.md §5.6): a child of `id`
        // used to keep `tf.parent == Some(id)` forever after this call —
        // a dangling reference to a dead entity, not just visually wrong
        // (its position math would silently stop counting the dead
        // parent's own contribution the instant `id`'s own `Transform` is
        // removed below) but a real hazard if entity ids are ever reused.
        // Computed BEFORE `id`'s own `Transform` is removed, so each
        // child's current world position — needed to keep it from visually
        // jumping the instant its parent's own contribution disappears —
        // still resolves correctly through the chain that includes `id`.
        let children: Vec<EntityId> = self
            .transforms
            .iter()
            .filter(|(_, tf)| tf.parent == Some(id))
            .map(|(&child, _)| child)
            .collect();
        for child in children {
            let world_pos = self.get_global_position(child);
            if let Some(tf) = self.transforms.get_mut(&child) {
                tf.parent = None;
                tf.position = world_pos;
            }
        }

        self.transforms.remove(&id);
        self.sprites.remove(&id);
        self.colliders.remove(&id);
        self.tags.remove(&id);
        self.scripts.remove(&id);
        self.animators.remove(&id);
        self.actors.remove(&id);
        self.vars.remove(&id);
        self.tilemaps.remove(&id);
        self.exits.remove(&id);
    }

    // ── Component accessors ───────────────────────────────────────────────

    pub fn add_transform(&mut self, id: EntityId, t: Transform) {
        self.transforms.insert(id, t);
    }
    pub fn add_sprite(&mut self, id: EntityId, s: Sprite) {
        self.sprites.insert(id, s);
    }
    pub fn add_collider(&mut self, id: EntityId, c: Collider) {
        self.colliders.insert(id, c);
    }
    pub fn add_tag(&mut self, id: EntityId, t: Tag) {
        self.tags.insert(id, t);
    }
    pub fn add_script(&mut self, id: EntityId, s: Script) {
        self.scripts.insert(id, s);
    }
    pub fn remove_script(&mut self, id: EntityId) {
        self.scripts.remove(&id);
    }
    pub fn add_actor(&mut self, id: EntityId, a: Actor) {
        self.actors.insert(id, a);
    }
    /// Phase 6 Step 11 (docs/ember2d-phase6-plan.md): added for symmetry
    /// with every other component (see the checklist on `World`'s own doc
    /// comment) — existing call sites still insert into `self.animators`
    /// directly where a plain `insert` doesn't fit (e.g. `apply_ctx`'s
    /// `play_clip` handling, which needs `.entry(id).or_insert_with(...)`
    /// to preserve an already-playing `Animator`'s state), so this isn't a
    /// call site migration, just closing the API gap for whoever writes the
    /// next one.
    pub fn add_animator(&mut self, id: EntityId, a: Animator) {
        self.animators.insert(id, a);
    }
    pub fn remove_actor(&mut self, id: EntityId) {
        self.actors.remove(&id);
    }
    pub fn remove_animator(&mut self, id: EntityId) {
        self.animators.remove(&id);
    }
    pub fn add_vars(&mut self, id: EntityId, v: Vars) {
        self.vars.insert(id, v);
    }
    pub fn remove_vars(&mut self, id: EntityId) {
        self.vars.remove(&id);
    }
    /// Step 8-1. The caller owes the map a `Tilemap::refresh` first —
    /// `Simulation::do_on_start` does it before calling this.
    pub fn add_tilemap(&mut self, id: EntityId, t: Tilemap) {
        self.tilemaps.insert(id, Rc::new(t));
    }
    pub fn remove_tilemap(&mut self, id: EntityId) {
        self.tilemaps.remove(&id);
    }
    pub fn add_exit(&mut self, id: EntityId, path: impl Into<String>) {
        self.exits.insert(id, path.into());
    }

    // ── Hierarchy ─────────────────────────────────────────────────────────

    /// Get the world-space position of an entity by traversing up its parent chain.
    ///
    /// The depth-100 bail-out below is a safety net, not the real fix —
    /// `set_parent` (below) rejects a cycle-creating reparent up front as
    /// of Step 7.5-9, so nothing reachable through the sanctioned API can
    /// build one anymore. This only still fires for a cycle baked directly
    /// into raw save/level data by hand, bypassing `set_parent` entirely.
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
                // Step 7.5-9 (R41 fix): a `Diagnostic`, not `eprintln!` —
                // see that type's own doc comment above for why.
                self.diagnostics.borrow_mut().push(Diagnostic {
                    message: format!("entity hierarchy cycle detected for entity {id}"),
                });
                break;
            }
        }
        pos
    }

    /// Set the parent of an entity.
    /// If `keep_world_position` is true, the local position is adjusted so the
    /// entity doesn't jump in world space.
    /// Step 7.5-9 (docs/ember2d-master-plan.md §5.6): rejects a reparent
    /// that would create a cycle, as a no-op — same convention every other
    /// setter here uses for an invalid request, rather than letting
    /// `get_global_position`'s own depth-100 safety net catch it later.
    /// Checked by walking UP from the proposed `parent`; if `id` itself
    /// ever appears, this assignment would close a loop. The `depth > 100`
    /// cap mirrors `get_global_position`'s own — this walk is over the
    /// SAME chain that function bails out of, so it needs the identical
    /// bound to stay safe against a cycle that already exists in the data
    /// (bypassing this very check, e.g. hand-edited save/level content).
    pub fn set_parent(
        &mut self,
        id: EntityId,
        parent: Option<EntityId>,
        keep_world_position: bool,
    ) {
        if let Some(p) = parent {
            if p == id {
                return;
            }
            let mut current = Some(p);
            let mut depth = 0;
            while let Some(cid) = current {
                if cid == id {
                    return;
                }
                current = self.transforms.get(&cid).and_then(|tf| tf.parent);
                depth += 1;
                if depth > 100 {
                    break;
                }
            }
        }

        let (parent_id, new_pos) = if keep_world_position {
            let current_global = self.get_global_position(id);
            let new_parent_global =
                parent.map(|p| self.get_global_position(p)).unwrap_or(Vec2::ZERO);
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
        let mut ids: std::collections::BTreeSet<EntityId> =
            self.transforms.keys().copied().collect();
        ids.extend(self.sprites.keys().copied());
        ids.extend(self.colliders.keys().copied());
        ids.extend(self.tags.keys().copied());
        ids.extend(self.scripts.keys().copied());
        ids.extend(self.animators.keys().copied());
        ids.extend(self.actors.keys().copied());
        ids.extend(self.vars.keys().copied());
        ids.extend(self.tilemaps.keys().copied());
        ids.extend(self.exits.keys().copied());
        ids.into_iter().collect()
    }

    pub fn remove_transform(&mut self, id: EntityId) {
        self.transforms.remove(&id);
    }
    pub fn remove_sprite(&mut self, id: EntityId) {
        self.sprites.remove(&id);
    }
    pub fn remove_collider(&mut self, id: EntityId) {
        self.colliders.remove(&id);
    }
    pub fn remove_tag(&mut self, id: EntityId) {
        self.tags.remove(&id);
    }

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
                if !self.transforms.contains_key(&id) {
                    return None;
                }
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
                if rect_b.x >= rect_a.right() {
                    break;
                }

                let a_allows_b = mask_a == 0 || (mask_a & layer_b) != 0;
                let b_allows_a = mask_b == 0 || (mask_b & layer_a) != 0;

                if a_allows_b && b_allows_a && rect_a.intersects(rect_b) {
                    hits.push(if id_a < id_b { (id_a, id_b) } else { (id_b, id_a) });
                }
            }
        }

        // Step 8-1: a collider against a tilemap is one cell lookup per
        // overlapped cell, not a place in the sweep above — see
        // `tilemap_hits`' own doc comment (world/tilemap_collision.rs).
        // Pushed into the same `hits` before the sort, so the emission
        // order stays the one ascending-pair sequence scripts rely on.
        self.tilemap_hits(&collidables, &mut hits);

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
    ///
    /// Step 8-1: also rebuilds every `Tilemap`'s runtime caches (its cells'
    /// solidity and layer bits are `#[serde(skip)]` for the same reason a
    /// `Collider`'s bits are — see `components/tilemap.rs`'s header). A
    /// loaded save is the only case this runs on: `Rc::make_mut` finds the
    /// refcount at 1 (no snapshot shares it yet), so it's an in-place
    /// update, not a copy.
    pub fn refresh_collider_bits(&mut self, registry: &crate::layers::LayerRegistry) {
        for col in self.colliders.values_mut() {
            col.refresh_bits(registry);
        }
        for map in self.tilemaps.values_mut() {
            Rc::make_mut(map).refresh(registry);
        }
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
        mover_id: EntityId,
        obstacle_id: EntityId,
        _prev: &HashMap<EntityId, Vec2>,
    ) {
        let obstacle_rect = {
            if !self.colliders.contains_key(&obstacle_id) {
                return;
            };
            let col = &self.colliders[&obstacle_id];
            let pos = self.get_global_position(obstacle_id);
            col.world_rect(pos.x, pos.y)
        };
        self.push_out_of(mover_id, obstacle_rect);
    }

    /// Minimum-overlap push-out of `mover_id`'s collider from one solid
    /// rect: shove it along whichever axis overlaps less, away from the
    /// obstacle's centre, and zero that axis' velocity. Pulled out of
    /// `resolve_solid_collision` at Step 8-1 so a tilemap cell
    /// (`resolve_tilemap_collision`, world/tilemap_collision.rs) and a
    /// wall entity get pushed out by literally the same arithmetic — a
    /// wall must behave identically whichever way it's stored.
    pub(crate) fn push_out_of(&mut self, mover_id: EntityId, obstacle_rect: Rect) {
        let (global_x, global_y, mover_w, mover_h) = {
            if !self.colliders.contains_key(&mover_id) {
                return;
            };
            let col = &self.colliders[&mover_id];
            let pos = self.get_global_position(mover_id);
            (pos.x, pos.y, col.width, col.height)
        };

        let mover_rect = Rect::new(global_x, global_y, mover_w, mover_h);

        let overlap_x =
            mover_rect.right().min(obstacle_rect.right()) - mover_rect.x.max(obstacle_rect.x);
        let overlap_y =
            mover_rect.bottom().min(obstacle_rect.bottom()) - mover_rect.y.max(obstacle_rect.y);

        if overlap_x <= 0.0 || overlap_y <= 0.0 {
            return;
        }

        if let Some(tf) = self.transforms.get_mut(&mover_id) {
            if overlap_x <= overlap_y {
                let mover_cx = global_x + mover_w * 0.5;
                let obstacle_cx = obstacle_rect.x + obstacle_rect.w * 0.5;
                if mover_cx < obstacle_cx {
                    tf.position.x -= overlap_x;
                } else {
                    tf.position.x += overlap_x;
                }
                tf.velocity.x = 0.0;
            } else {
                let mover_cy = global_y + mover_h * 0.5;
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

// Step 8-1: collider-vs-tilemap detection and resolution, in their own
// child module (world.rs was at 562 of CLAUDE.md's 750 lines, and the
// tilemap half is a self-contained concern) — see that file's header.
mod tilemap_collision;

// Tests split into world_tests.rs (Step 7.5-9, docs/ember2d-master-plan.md
// §5.6) — see that file's own header comment for why, once this file
// crossed CLAUDE.md's 750-line limit.
#[cfg(test)]
#[path = "world_tests.rs"]
mod tests;
