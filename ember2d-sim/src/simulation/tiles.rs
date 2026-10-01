// simulation/tiles.rs — applying a script's tile requests (`tile_def`,
// `tile_set`, `tile_fill`, `tile_clear`, `tilemap_resize`) to the level's
// tilemap.
//
// Step 9.5-1 (docs/ember2d-master-plan.md §5.8.5). The script engine queues
// them (`scripting/tiles.rs`, whose header says what a script sees and
// when) and hands them back in `ScriptUpdateResult::tile_ops`. They're
// applied here, in call order, because two things they need live on
// `Simulation`: the `LayerRegistry` a solid cell's collision bits come
// from, and the tileset loader a `sprite:` tile is resolved through (the
// same cache 9-7's `set_sprite` fills, so a region is read once per level).
//
// The map is behind an `Rc` that this step's snapshot may still share, so
// the first edit in a batch takes it with `Rc::make_mut` (a copy only if
// it IS shared, once per batch), every edit writes cells directly, and the
// batch ends with ONE `refresh` — rebuilding the solid/sprite caches once,
// not per cell. Cost is bounded by the grid: a rect is clipped to the map
// before any cell is visited, so `tile_fill(-1e9, -1e9, 2e9, 2e9, ...)` is
// one pass over the map, never two billion iterations.

use std::collections::BTreeSet;
use std::rc::Rc;

use crate::components::tilemap::MAX_TILEMAP_CELLS;
use crate::components::{Tilemap, Transform};
use crate::scripting::{LogEntry, TileOp};
use crate::world::{EntityId, World};

use super::tilesets::TilesetResolver;
use super::Simulation;

impl Simulation {
    pub(super) fn apply_tile_ops(
        &mut self,
        world: &mut World,
        ops: Vec<TileOp>,
        logs: &mut Vec<LogEntry>,
    ) {
        if ops.is_empty() {
            return;
        }
        let mut touched: Option<EntityId> = None;
        // Reported once per batch, not once per cell: a script stamping a
        // whole floor with an undeclared name is one mistake.
        let mut unknown: BTreeSet<String> = BTreeSet::new();
        let mut outside = 0usize;
        for op in ops {
            match op {
                TileOp::Bad(why) => logs.push(LogEntry::warn(why)),
                TileOp::Def(name, mut stamp) => {
                    if let Some(sprite) = stamp.def.sprite.clone() {
                        let key = (sprite.tileset.clone(), sprite.region.clone());
                        if !self.sprite_regions.contains_key(&key) {
                            let level_path = self.level.path.clone();
                            let source = Rc::clone(&self.level_source);
                            let found =
                                TilesetResolver::new(&level_path, &*source).resolve(&sprite);
                            if let Err(why) = &found {
                                logs.push(LogEntry::warn(format!("tile_def(\"{name}\"): {why}")));
                            }
                            self.sprite_regions.insert(key.clone(), found);
                        }
                        // A missing region leaves the def a glyph tile, the
                        // same fallback a level's broken sprite tile gets.
                        if let Some(Ok((image, rect))) = self.sprite_regions.get(&key) {
                            stamp.def.texture = Some(image.clone());
                            stamp.def.src = Some(*rect);
                        }
                    }
                    world.tile_defs.insert(name, stamp);
                }
                TileOp::Resize(w, h) => {
                    let fits =
                        w > 0 && h > 0 && (w as u64).saturating_mul(h as u64) <= MAX_TILEMAP_CELLS;
                    if !fits {
                        logs.push(LogEntry::warn(format!(
                            "tilemap_resize({w}, {h}): a tilemap is 1 to {MAX_TILEMAP_CELLS} cells"
                        )));
                        continue;
                    }
                    let id = self.level_tilemap(world);
                    world.tilemaps.insert(id, Rc::new(Tilemap::new((0, 0), w as u32, h as u32)));
                    touched = Some(id);
                }
                TileOp::Rect { x, y, w, h, layer, name } => {
                    if w <= 0 || h <= 0 {
                        continue;
                    }
                    let stamp = match &name {
                        Some(n) => match world.tile_defs.get(n) {
                            Some(s) => Some(s.clone()),
                            None => {
                                unknown.insert(n.clone());
                                continue;
                            }
                        },
                        None => None,
                    };
                    let id = self.level_tilemap(world);
                    let Some(rc) = world.tilemaps.get_mut(&id) else { continue };
                    let map = Rc::make_mut(rc);
                    // The rect, clipped to the grid (i64: no overflow on
                    // any script-supplied number).
                    let (ox, oy) = (map.origin.0 as i64, map.origin.1 as i64);
                    let x0 = x.max(ox);
                    let y0 = y.max(oy);
                    let x1 = x.saturating_add(w).min(ox + map.width as i64);
                    let y1 = y.saturating_add(h).min(oy + map.height as i64);
                    if x0 >= x1 || y0 >= y1 {
                        outside += 1;
                        continue;
                    }
                    match stamp {
                        Some(stamp) => {
                            let Some(cell) = map.intern(&stamp.def) else {
                                logs.push(LogEntry::warn(
                                    "tile_set: the tilemap's palette is full (65,535 distinct tiles)",
                                ));
                                continue;
                            };
                            let lyr = layer.unwrap_or(stamp.layer);
                            for cy in y0..y1 {
                                for cx in x0..x1 {
                                    map.set_cell(lyr, cx as i32, cy as i32, cell);
                                }
                            }
                        }
                        None => {
                            for cy in y0..y1 {
                                for cx in x0..x1 {
                                    match layer {
                                        Some(l) => map.set_cell(l, cx as i32, cy as i32, 0),
                                        None => map.clear_cell(cx as i32, cy as i32),
                                    };
                                }
                            }
                        }
                    }
                    touched = Some(id);
                }
            }
        }
        if let Some(id) = touched {
            if let Some(rc) = world.tilemaps.get_mut(&id) {
                Rc::make_mut(rc).refresh(&self.layers);
            }
        }
        if !unknown.is_empty() {
            let names: Vec<_> = unknown.into_iter().collect();
            logs.push(LogEntry::warn(format!(
                "tile_set/tile_fill: no tile named {} — declare it with tile_def first",
                names.join(", ")
            )));
        }
        if outside > 0 {
            logs.push(LogEntry::warn(format!(
                "{outside} tile request(s) fell outside the tilemap (tilemap_resize sets its size)"
            )));
        }
    }

    /// The level's tilemap — the first one `World` holds — covering at
    /// least the whole level: a level's baked map spans only the box its
    /// painted tiles fill, so it's grown (once; `grow_to_cover` is a
    /// bounds check after that) to take the rest. A level with no static
    /// tiles at all gets an empty one the level's size. Its `Transform`
    /// sits at the grid's origin, as a level-loaded tilemap's does
    /// (simulation/spawn.rs).
    fn level_tilemap(&mut self, world: &mut World) -> EntityId {
        let w = self.level.width.clamp(1, 2048) as u32;
        let h = self.level.height.clamp(1, 2048) as u32;
        if let Some(&id) = world.tilemaps.keys().next() {
            let map = &world.tilemaps[&id];
            let covers = map.origin.0 <= 0
                && map.origin.1 <= 0
                && map.origin.0 as i64 + map.width as i64 >= w as i64
                && map.origin.1 as i64 + map.height as i64 >= h as i64;
            if !covers {
                if let Some(rc) = world.tilemaps.get_mut(&id) {
                    let map = Rc::make_mut(rc);
                    map.grow_to_cover(0, 0, w as i32, h as i32);
                    let origin = map.origin;
                    if let Some(tf) = world.transforms.get_mut(&id) {
                        tf.position = crate::math::Vec2::new(origin.0 as f32, origin.1 as f32);
                    }
                }
            }
            return id;
        }
        let id = world.spawn();
        world.add_transform(id, Transform::new(0.0, 0.0));
        world.add_tilemap(id, Tilemap::new((0, 0), w, h));
        id
    }
}
