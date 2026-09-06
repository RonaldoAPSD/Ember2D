// components/collider.rs — Collision detection component.
//
// A Collider defines the physical "hitbox" of an entity — the region of space
// it occupies for the purpose of collision detection.
//
// WE USE AABB (Axis-Aligned Bounding Box) COLLISION:
//   An AABB is a rectangle whose sides are always parallel to the axes.
//   It cannot rotate. In an ASCII grid this is a perfect fit because
//   characters are always grid-aligned — there's no concept of a
//   "rotated character."
//
//   AABB overlap test: two rects overlap if and only if they overlap
//   on BOTH the X axis AND the Y axis simultaneously. See Rect::intersects.
//
// SOLID vs TRIGGER colliders:
//   SOLID   — physical object; the collision system should prevent overlap.
//             Examples: walls, floor edges, doors.
//   TRIGGER — ghost zone; detects overlap but doesn't block movement.
//             Examples: pickup items, damage zones, room transitions.
//
//   The Collider itself doesn't enforce this distinction — the game's
//   late_update() responds to collision events and decides what to do
//   based on whether the other entity is solid.
//
// POSITION:
//   The Collider doesn't store a position. It's always the size of the box,
//   and the position comes from the entity's Transform component.
//   Use `world_rect()` to compute the actual world-space bounding box.

// LAYER/MASK AS A BITMASK (Phase 6 Step 7, docs/ember2d-phase6-plan.md):
//   `layer`/`mask` below are still the serialized truth — `.level` files,
//   the editor, node-graph codegen, and every script-facing function
//   (`get_collider_layer`/`set_collider_layer`/etc.) all still speak plain
//   strings, unchanged. `layer_bits`/`mask_bits` are a `#[serde(skip)]`
//   derived cache resolved against a `crate::layers::LayerRegistry` — see
//   that module's own doc comment for why a registry (not a hash) and why
//   built once, never grown at runtime. `World::detect_collisions`'s
//   O(colliders²) pairwise test reads only the bits, never the strings, so
//   it needs no per-pair string comparison or per-entity `String`/
//   `Vec<String>` clone.
//
//   Both fields are PRIVATE, with `set_layer`/`set_mask` as the only way to
//   change them — compiler-enforced sync between a `layer`/`mask` string
//   and its own `layer_bits`/`mask_bits`, so a future call site can't set
//   one without the other (which is exactly what happened before this
//   struct existed: nothing stopped `col.layer = x` from leaving stale bits
//   behind, because there were no bits to leave stale).
//
//   Because the bits are `#[serde(skip)]`, deserializing a `Collider` (a
//   saved game, or `World`'s own `Debug`/test round-trips) leaves them
//   zeroed — `refresh_bits` recomputes them from the strings that DID
//   survive serialization. `Simulation::on_start`'s `is_loading_save` path
//   calls `World::refresh_collider_bits` for exactly this reason; forgetting
//   it is the one mistake here that fails completely silently (every
//   collider would just stop filtering, matching-everything-with-mask-zero
//   being indistinguishable from "the mask matched"), which is why a
//   save→load→still-filters round trip is a mandatory test
//   (`ember2d/tests/collision_layers.rs`), not an optional nice-to-have.

use crate::layers::LayerRegistry;
use crate::math::Rect;
use serde::{Serialize, Deserialize};

/// Defines the bounding box used for collision detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Collider {
    /// Width of the hitbox in world units (characters wide).
    pub width: f32,

    /// Height of the hitbox in world units (character rows tall).
    pub height: f32,

    /// If true: this is a physical obstacle. The game should prevent other solid
    /// entities from overlapping it (via position rollback in late_update).
    ///
    /// If false: this is a "trigger zone." It detects overlap and fires a
    /// Collision event, but movement is not blocked.
    pub solid: bool,

    /// Optional layer name for fine-grained collision filtering in scripts.
    /// Private — see this file's header comment. Read via `layer()`, write
    /// via `set_layer()`.
    layer: String,

    /// Optional collision mask: a list of layer names this collider should
    /// interact with. If empty, it interacts with ALL layers (default).
    /// Private — see this file's header comment. Read via `mask()`, write
    /// via `set_mask()`.
    mask: Vec<String>,

    /// `layer`'s resolved bit, or `0` if `layer` is empty or unregistered —
    /// see `LayerRegistry::bit_for`. `#[serde(skip)]`: never written to
    /// disk, always recomputed — see this file's header comment.
    #[serde(skip)]
    layer_bits: u32,

    /// `mask`'s resolved bits, or `0` if `mask` is empty ("matches
    /// everything") — see `LayerRegistry::mask_bits`. `#[serde(skip)]`, same
    /// reasoning as `layer_bits`.
    #[serde(skip)]
    mask_bits: u32,

    /// If true, an exit trigger referencing this collider will not fire.
    ///
    /// Purely a gameplay gate — the collider still physically detects overlap
    /// as normal. Scripts toggle this with `set_collider_locked` (e.g. a door
    /// that won't open until all items are collected). Defect D12: this used
    /// to be smuggled through `layer == "locked"`, which corrupted the layer
    /// field's real purpose (collision filtering) for any locked exit tile.
    #[serde(default)]
    pub locked: bool,
}

impl Collider {
    /// Create a solid collider with the given dimensions.
    pub fn new(width: f32, height: f32) -> Self {
        Collider { width, height, solid: true, layer: String::new(), mask: Vec::new(), layer_bits: 0, mask_bits: 0, locked: false }
    }

    /// A 1×1 solid collider — the standard size for a single-character entity.
    /// Most game objects (player, enemies, items) are 1×1.
    pub fn unit() -> Self {
        Collider::new(1.0, 1.0)
    }

    /// A non-solid trigger zone of the given size.
    ///
    /// Trigger colliders fire Collision events but don't block movement.
    /// Use them for: pickups, damage areas, door triggers, room boundaries.
    pub fn trigger(width: f32, height: f32) -> Self {
        Collider { width, height, solid: false, layer: String::new(), mask: Vec::new(), layer_bits: 0, mask_bits: 0, locked: false }
    }

    /// Compute the world-space bounding Rect for this collider given the
    /// entity's current position (from its Transform).
    ///
    /// The position is the TOP-LEFT corner of the bounding box.
    /// For a 1×1 collider at position (5, 3), the rect is (5, 3, 1, 1).
    pub fn world_rect(&self, pos_x: f32, pos_y: f32) -> Rect {
        Rect::new(pos_x, pos_y, self.width, self.height)
    }

    pub fn layer(&self) -> &str { &self.layer }
    pub fn mask(&self) -> &[String] { &self.mask }
    pub fn layer_bits(&self) -> u32 { self.layer_bits }
    pub fn mask_bits(&self) -> u32 { self.mask_bits }

    /// Set this collider's layer, resolving its bit against `registry` in
    /// the same call — the only way to change `layer` from outside this
    /// module, specifically so it's impossible to update the string without
    /// also updating its bit.
    pub fn set_layer(&mut self, registry: &LayerRegistry, layer: impl Into<String>) {
        let layer = layer.into();
        self.layer_bits = registry.bit_for(&layer);
        self.layer = layer;
    }

    /// Set this collider's mask, resolving its bits against `registry` in
    /// the same call — see `set_layer`'s doc comment for why.
    pub fn set_mask(&mut self, registry: &LayerRegistry, mask: Vec<String>) {
        self.mask_bits = registry.mask_bits(&mask);
        self.mask = mask;
    }

    /// Recompute `layer_bits`/`mask_bits` from the current `layer`/`mask`
    /// strings against `registry`, without changing either string. For the
    /// one case `set_layer`/`set_mask` can't cover: a `Collider` that just
    /// came out of deserialization, whose bits are zeroed (`#[serde(skip)]`)
    /// but whose strings are exactly as they were saved. See this file's
    /// header comment for why forgetting to call this after a load is the
    /// one mistake here with no visible symptom.
    pub fn refresh_bits(&mut self, registry: &LayerRegistry) {
        self.layer_bits = registry.bit_for(&self.layer);
        self.mask_bits = registry.mask_bits(&self.mask);
    }
}
