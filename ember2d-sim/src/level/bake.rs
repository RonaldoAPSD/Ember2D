// level/bake.rs — which tiles are "static", and moving them between
// `LevelData.tiles` (one `TileRecord` each) and `LevelData.tilemap` (one
// compact grid) in both directions, losslessly.
//
// Step 8-1 (docs/ember2d-master-plan.md §5.7). A child module of level.rs
// (Rust 2018's non-`mod.rs` layout — `level.rs` and `level/` coexist, the
// same arrangement `simulation.rs`/`simulation/` already uses) rather than
// more code in level.rs itself, which was at 638 of CLAUDE.md's 750 lines.
//
// Three callers, one rule:
//   - `Simulation::do_on_start` calls `split_static` — at load, every static
//     tile goes into one runtime `Tilemap`, whether it came from a v4 file's
//     baked `tilemap` section or a v3 file's plain `tiles` list. A v3 level
//     needs no re-save to get 8-1's speedup.
//   - The editor's `LevelGrid::from_level_data` calls `all_tiles` — a baked
//     level unpacks back into ordinary `TileRecord`s, so painting, undo,
//     flood fill, and the inspector never learn tilemaps exist.
//   - The editor's `LevelGrid::to_level_data` (and both demo generators)
//     call `bake_tilemap` — "auto-bake on save", 8-1's own scoping decision.

use super::{LevelData, TileRecord};
use crate::components::{TileDef, Tilemap, TilemapBuilder};

impl TileRecord {
    /// A tile that can live in a `Tilemap` cell instead of being its own
    /// entity: nothing about it ever runs, moves, reacts, or needs an
    /// entity id. The tag is deliberately NOT part of this rule (8-1's
    /// scoping decision — every shipped wall and floor is tagged, so a
    /// tag-less-only rule would have collapsed nothing); a cell keeps its
    /// tag as data (`TileDef::tag`, `ctx.get_tile_tag`).
    ///
    /// Stays an entity if it has:
    ///   - a script or node graph (it runs code),
    ///   - `trigger` (its collisions are an event something listens for),
    ///   - an `actor` (it takes turns and moves),
    ///   - `next_level` (an exit — `World.exits` keys it by entity id),
    ///   - a non-empty `collider_mask` (a cell has no mask — only a layer),
    ///   - `camera_follow` (the camera follows an entity id),
    ///   - a `clip` (Step 8-3: it animates — a tilemap cell can't).
    pub fn is_static(&self) -> bool {
        self.script.is_none()
            && self.graph.is_none()
            && !self.trigger
            && self.actor.is_none()
            && self.next_level.is_none()
            && self.collider_mask.is_empty()
            && !self.camera_follow
            && self.clip.is_none()
    }

    fn to_tile_def(&self) -> TileDef {
        TileDef {
            glyph: self.glyph,
            fg: self.fg,
            bg: self.bg,
            solid: self.solid,
            tag: self.tag.clone(),
            collider_layer: self.collider_layer.clone(),
            texture: self.texture.clone(),
            sprite: self.sprite.clone(),
            src: None,
        }
    }

    fn from_tile_def(layer: u8, x: i32, y: i32, def: &TileDef) -> Self {
        let mut t = TileRecord::new(
            x,
            y,
            layer,
            def.glyph,
            def.fg,
            def.bg,
            def.solid,
            false,
            def.tag.clone(),
        );
        t.collider_layer = def.collider_layer.clone();
        t.texture = def.texture.clone();
        t.sprite = def.sprite.clone();
        t
    }
}

/// Builds one `Tilemap` from `tiles`, returning it plus the indices (into
/// `tiles`) of every tile that stayed an entity — non-static, or refused
/// by the builder (see `TilemapBuilder::add`) — in ascending order.
fn build(tiles: &[&TileRecord]) -> (Option<Tilemap>, Vec<usize>) {
    let bounds = tiles.iter().filter(|t| t.is_static()).fold(
        None,
        |acc: Option<(i32, i32, i32, i32)>, t| {
            Some(match acc {
                None => (t.x, t.y, t.x, t.y),
                Some((x0, y0, x1, y1)) => (x0.min(t.x), y0.min(t.y), x1.max(t.x), y1.max(t.y)),
            })
        },
    );
    let mut builder = bounds.and_then(|(x0, y0, x1, y1)| {
        let w = (x1 as i64 - x0 as i64 + 1).clamp(0, u32::MAX as i64) as u32;
        let h = (y1 as i64 - y0 as i64 + 1).clamp(0, u32::MAX as i64) as u32;
        TilemapBuilder::new((x0, y0), w, h)
    });

    let mut entities = Vec::new();
    for (i, t) in tiles.iter().enumerate() {
        let placed = t.is_static()
            && builder.as_mut().map(|b| b.add(t.layer, t.x, t.y, t.to_tile_def())).unwrap_or(false);
        if !placed {
            entities.push(i);
        }
    }
    (builder.and_then(TilemapBuilder::finish), entities)
}

/// A baked tilemap's cells, unpacked back into `TileRecord`s.
fn unpack(map: Option<&Tilemap>) -> Vec<TileRecord> {
    map.iter()
        .flat_map(|m| m.iter_tiles().map(|(l, x, y, def)| TileRecord::from_tile_def(l, x, y, def)))
        .collect()
}

impl LevelData {
    /// Every tile in the level as a plain `TileRecord` — the baked
    /// `tilemap`'s cells unpacked, then `tiles`, sorted by (layer, y, x)
    /// (the order every generator and the editor already write). The
    /// inverse of `bake_tilemap`: `all_tiles` on a baked level equals
    /// `all_tiles` on the same level before baking.
    pub fn all_tiles(&self) -> Vec<TileRecord> {
        let mut out = unpack(self.tilemap.as_ref());
        out.extend(self.tiles.iter().cloned());
        out.sort_by_key(|t| (t.layer, t.y, t.x));
        out
    }

    /// Move every static tile into `self.tilemap` (replacing any existing
    /// one, which is unpacked first — baking twice is a no-op). Tiles that
    /// stay entities remain in `self.tiles`, still sorted (layer, y, x).
    pub fn bake_tilemap(&mut self) {
        let all = self.all_tiles();
        let refs: Vec<&TileRecord> = all.iter().collect();
        let (map, entities) = build(&refs);
        self.tiles = entities.into_iter().map(|i| all[i].clone()).collect();
        self.tilemap = map;
    }

    /// What `Simulation::do_on_start` spawns from: one `Tilemap` holding
    /// every static tile (from the baked section AND from `tiles` — a v3
    /// level collapses too), and the tiles that stay entities. Baked cells
    /// go first, so they win a cell that a stray static `tiles` entry also
    /// claims — that entry then stays an entity, exactly as it would have
    /// before baking. Entity tiles come back owned, in the order they were
    /// found (baked leftovers — none in practice, a baked cell always
    /// re-places — then `self.tiles` order). The map's caches are not
    /// built yet (no `LayerRegistry` here); the caller owes it a
    /// `Tilemap::refresh`.
    pub fn split_static(&self) -> (Option<Tilemap>, Vec<TileRecord>) {
        let baked = unpack(self.tilemap.as_ref());
        let refs: Vec<&TileRecord> = baked.iter().chain(self.tiles.iter()).collect();
        let (map, entities) = build(&refs);
        (map, entities.into_iter().map(|i| refs[i].clone()).collect())
    }
}
