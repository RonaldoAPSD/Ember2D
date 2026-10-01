// world/tilemap_collision.rs — colliders against `Tilemap` cells: the
// detection half (`tilemap_hits`, called from `World::detect_collisions`)
// and the resolution half (`resolve_tilemap_collision`, called from
// `Simulation::late_step`).
//
// Step 8-1 (docs/ember2d-master-plan.md §5.7). A genuine child module of
// world.rs (Rust 2018's non-`mod.rs` layout, same as `simulation/`), so
// it's a second `impl World` block with full access to `World`'s fields —
// split out only to keep world.rs well under CLAUDE.md's 750-line limit.
//
// ── THE CONTRACT: A WALL IS A WALL, HOWEVER IT'S STORED ─────────────────────
//
// Before 8-1 a wall was an entity with a unit `Collider`; the sweep-and-
// prune in `detect_collisions` found every (wall, mover) overlap, and
// `late_step` pushed the mover out of each wall in ascending pair order.
// Now the same wall is a cell, and scripts must not be able to tell:
//
//   - One `Collision` event per (collider, tilemap) pair, however many
//     cells overlap — carrying the tilemap entity's id, the same id every
//     spatial query returns for a cell hit (8-1's scoping decision). A
//     script's `on_collide(me, other)` sees `other` = the tilemap, which
//     `is_tilemap(other)` identifies.
//   - The mask test is the same one `detect_collisions` applies to two
//     colliders, with the cell's side of it always passing (a cell has a
//     layer, never a mask — a masked tile stays an entity,
//     `TileRecord::is_static`).
//   - Resolution pushes out of each overlapped cell in row-major order —
//     the (layer, y, x) order the same walls' entity ids had, so the
//     sequence of pushes, and therefore where the mover ends up, is the
//     same as before.

use super::{EntityId, World};
use crate::math::Rect;

impl World {
    /// Appends one normalized `(min_id, max_id)` pair to `hits` for every
    /// collider in `collidables` (the list `detect_collisions` already
    /// built: id, world rect, layer bits, mask bits) that overlaps at least
    /// one solid cell of any tilemap. Per collider that's a handful of
    /// direct cell lookups (`Tilemap::any_solid_in` clamps to the rect's
    /// own footprint), not a place in the sweep — which is the whole
    /// point: 40,000 walls used to be 40,000 sweep entries.
    pub(super) fn tilemap_hits(
        &self,
        collidables: &[(EntityId, Rect, u32, u32)],
        hits: &mut Vec<(EntityId, EntityId)>,
    ) {
        for (&map_id, map) in &self.tilemaps {
            for &(id, rect, _layer, mask) in collidables {
                if map.any_solid_in(rect, mask) {
                    hits.push(if id < map_id { (id, map_id) } else { (map_id, id) });
                }
            }
        }
    }

    /// Push `mover_id` out of every solid cell of tilemap `map_id` its
    /// collider currently overlaps (filtered by its own mask, the same test
    /// `tilemap_hits` used to report the pair), one cell at a time in
    /// row-major order through the same `push_out_of` a wall entity gets —
    /// see this file's header for why that order matters. A no-op if either
    /// id is gone or the mover has no collider.
    pub fn resolve_tilemap_collision(&mut self, mover_id: EntityId, map_id: EntityId) {
        let Some(map) = self.tilemaps.get(&map_id).cloned() else { return };
        let Some(col) = self.colliders.get(&mover_id) else { return };
        let pos = self.get_global_position(mover_id);
        let rect = col.world_rect(pos.x, pos.y);
        let mask = col.mask_bits();
        // The cell list is taken once, up front, from the pre-push rect —
        // exactly like the pre-8-1 events, which were all detected before
        // any of them resolved. `push_out_of` re-checks the overlap against
        // the mover's CURRENT rect before each push, so a cell the previous
        // push already cleared is skipped, again matching the old per-wall
        // resolution.
        for (x, y) in map.overlapping_solid_cells(rect, mask) {
            self.push_out_of(mover_id, Rect::new(x as f32, y as f32, 1.0, 1.0));
        }
    }
}
