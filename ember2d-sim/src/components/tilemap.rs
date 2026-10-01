// components/tilemap.rs — Tilemap: every static tile of a level, as one
// compact grid on one entity instead of one entity per tile.
//
// ── WHY THIS EXISTS (Step 8-1, docs/ember2d-master-plan.md §5.7) ──────────────
//
// Before this step every tile a level placed was its own entity: a
// `Transform`, a `Sprite`, usually a `Tag`, and — for a wall — a `Collider`.
// floor2 was 2,570 entities, all but ~10 of them walls and floors that never
// move, never run a script, and never change. They still paid full price
// everywhere an entity does:
//
//   - `WorldSnapshot::build` copied every one of them into a dozen maps,
//     every step (3.18 ms at 5,000 tiles, release — the §7.2 gate number).
//   - `is_solid_at`/`raycast`/`get_path`/`reachable_within` scanned every
//     collider linearly — A* did it once per neighbour check.
//   - `World::detect_collisions`' sweep-and-prune sorted every wall.
//   - Play mode built and sorted a draw command for every tile before
//     culling any of them.
//
// A `Tilemap` holds those static tiles as a grid instead: per layer, one
// `u16` per cell indexing a small palette of distinct `TileDef`s. A query
// becomes a direct cell lookup (O(1)); the broad phase never sees a cell;
// rendering walks only the visible window. Anything interactive — a script,
// a trigger, an actor, an exit, a collider mask, camera follow — still
// spawns as its own entity, exactly as before. `TileRecord::is_static`
// (level.rs) is the one rule deciding which is which.
//
// ── ONE TYPE ON DISK AND AT RUNTIME ───────────────────────────────────────────
//
// The same struct is a level file's v4 `tilemap` section (`LevelData.
// tilemap`), a `World` component (`World.tilemaps`), and part of a save
// file (because it's part of `World`). The fields that only exist to make
// runtime lookups fast — per-cell solidity and layer bits, the per-palette
// `SpriteSource` the renderer borrows — are `#[serde(skip)]` and rebuilt by
// `refresh`, the same "strings survive serialization, resolved bits don't"
// split `Collider` already has (see its own header comment). Forgetting to
// call `refresh` after deserializing is the one mistake here that doesn't
// fail loudly: every cell would read as non-solid. `World::
// refresh_collider_bits` is the one place that does it for a loaded save.

use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::components::SpriteSource;
use crate::layers::LayerRegistry;
use crate::math::Rect;

/// A cell value of `0` means "no tile on this layer here"; `n` means
/// `palette[n - 1]`. So a palette can hold at most `u16::MAX` distinct
/// tile definitions — a level that somehow needs more just keeps the
/// overflow as ordinary entities (`TilemapBuilder::add` returns `false`),
/// losing nothing.
pub const MAX_TILE_DEFS: usize = u16::MAX as usize;

/// The largest grid (width × height, per layer) a builder will allocate.
/// A level's static tiles are normally one dense rectangle, but nothing
/// stops a hand-edited file from placing two tiles a billion cells apart —
/// the bounding box of that would be an allocation the size of the
/// address space. Past this (2048 × 2048) the builder refuses, and every
/// tile stays an entity: slow, but correct, never a crash.
pub const MAX_TILEMAP_CELLS: u64 = 2048 * 2048;

/// Everything about one static tile except where it is — the palette entry
/// a cell points at. Two tiles that look and behave identically share one.
/// Field meanings are exactly `TileRecord`'s own (level.rs); `trigger`/
/// `script`/`actor`/`next_level`/`collider_mask`/`camera_follow`/`graph`
/// have no counterpart because a tile using any of them isn't static and
/// never reaches a tilemap at all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileDef {
    pub glyph: char,
    pub fg: Color,
    pub bg: Color,
    #[serde(default)]
    pub solid: bool,
    /// Kept per cell (8-1's own scoping decision: a tagged static tile
    /// still collapses) so the editor round-trips it losslessly and
    /// `ctx.get_tile_tag(x, y)` can read it back. Not visible to
    /// `find_by_tag`/`has_tag` — a cell has no entity id of its own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    /// Same meaning as `TileRecord::collider_layer` — empty on a solid tile
    /// means `"solid"`, the same default `Simulation::do_on_start` has
    /// always given an unlabeled wall's `Collider`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub collider_layer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<String>,
}

impl TileDef {
    /// The layer name this def's cells collide as — `collider_layer`, or
    /// `"solid"` when that's empty (see the field's own doc comment).
    pub fn effective_layer(&self) -> &str {
        if self.collider_layer.is_empty() {
            "solid"
        } else {
            &self.collider_layer
        }
    }
}

/// One editor layer's worth of cells (`TileRecord::layer`: 0 background,
/// 1 main, 2 foreground). Row-major, `width * height` long, same grid as
/// every other layer in the same `Tilemap`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TileLayer {
    pub layer: u8,
    pub cells: Vec<u16>,
}

impl TileLayer {
    /// The draw order a tile on this layer gets — `layer * 10`, the exact
    /// value `do_on_start` has always given a tile entity's `Sprite`, so a
    /// collapsed tile sorts against the player (15) and every remaining
    /// entity precisely where its entity used to.
    pub fn z(&self) -> i32 {
        self.layer as i32 * 10
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tilemap {
    /// World-space cell of the grid's top-left corner.
    pub origin: (i32, i32),
    pub width: u32,
    pub height: u32,
    pub palette: Vec<TileDef>,
    /// Sorted ascending by `layer`, at most one entry per layer value —
    /// `TilemapBuilder` guarantees both. Ascending matters: it's the order
    /// solid cells get resolved in (`overlapping_solid_cells`), matching
    /// the (layer, y, x) id order tile entities used to have.
    pub layers: Vec<TileLayer>,

    // ── Runtime caches (`refresh` rebuilds all three) ─────────────────────
    /// Per cell: is ANY layer's tile here solid? A separate flag, not
    /// `solid_bits != 0`, because a solid tile on a layer name the level
    /// never registered resolves to bit `0` — still solid, just unmaskable,
    /// exactly like a `Collider` in the same situation.
    #[serde(skip)]
    solid: Vec<bool>,
    /// Per cell: the OR of every solid layer's resolved bit. Two solid
    /// tiles stacked in one cell used to be two colliders; a mask test
    /// against the OR is the same as "either one passes".
    #[serde(skip)]
    solid_bits: Vec<u32>,
    /// Per palette entry: the `SpriteSource` the play-mode renderer
    /// borrows for this def (it draws through `&SpriteSource`, the same
    /// type a `Sprite` carries).
    #[serde(skip)]
    sources: Vec<SpriteSource>,
}

impl Tilemap {
    pub fn new(origin: (i32, i32), width: u32, height: u32) -> Self {
        Tilemap {
            origin,
            width,
            height,
            palette: Vec::new(),
            layers: Vec::new(),
            solid: Vec::new(),
            solid_bits: Vec::new(),
            sources: Vec::new(),
        }
    }

    /// Row-major index of world cell (x, y), or `None` outside the grid.
    /// `i64` arithmetic so an extreme `i32` coordinate can't overflow.
    pub fn index(&self, x: i32, y: i32) -> Option<usize> {
        let lx = x as i64 - self.origin.0 as i64;
        let ly = y as i64 - self.origin.1 as i64;
        if lx < 0 || ly < 0 || lx >= self.width as i64 || ly >= self.height as i64 {
            return None;
        }
        Some((ly * self.width as i64 + lx) as usize)
    }

    /// The tile on `layer` at world cell (x, y), if any.
    pub fn get(&self, layer: u8, x: i32, y: i32) -> Option<&TileDef> {
        let i = self.index(x, y)?;
        let l = self.layers.iter().find(|l| l.layer == layer)?;
        self.def(l.cells[i])
    }

    fn def(&self, cell: u16) -> Option<&TileDef> {
        if cell == 0 {
            None
        } else {
            self.palette.get(cell as usize - 1)
        }
    }

    /// Recompute every `#[serde(skip)]` cache — see this module's header
    /// comment for when that's needed. `resolve_texture` maps a def's
    /// stored texture path to the path the renderer should load (a level
    /// file stores it relative to itself; a save already stores it
    /// resolved, so a loaded save passes the identity).
    pub fn refresh(&mut self, registry: &LayerRegistry) {
        let n = self.width as usize * self.height as usize;
        self.solid = vec![false; n];
        self.solid_bits = vec![0; n];
        for layer in &self.layers {
            for (i, &cell) in layer.cells.iter().enumerate().take(n) {
                if let Some(def) = self.palette.get((cell as usize).wrapping_sub(1)) {
                    if def.solid {
                        self.solid[i] = true;
                        self.solid_bits[i] |= registry.bit_for(def.effective_layer());
                    }
                }
            }
        }
        self.sources = self
            .palette
            .iter()
            .map(|d| match &d.texture {
                Some(path) => SpriteSource::Texture { path: path.clone(), src: None },
                None => SpriteSource::Glyph { ch: d.glyph, bg: d.bg },
            })
            .collect();
    }

    /// Is world cell (x, y) solid, filtered by `mask_bits` the same way
    /// `raycast`/`get_path` filter colliders (`0` = every solid counts)?
    pub fn solid_at(&self, x: i32, y: i32, mask_bits: u32) -> bool {
        match self.index(x, y) {
            Some(i) => {
                self.solid.get(i).copied().unwrap_or(false)
                    && (mask_bits == 0 || mask_bits & self.solid_bits[i] != 0)
            }
            None => false,
        }
    }

    /// The resolved layer bits of a solid cell, or `None` if (x, y) isn't
    /// solid — what `World::detect_collisions` tests a mover's mask against.
    pub fn solid_bits_at(&self, x: i32, y: i32) -> Option<u32> {
        let i = self.index(x, y)?;
        if self.solid.get(i).copied().unwrap_or(false) {
            Some(self.solid_bits[i])
        } else {
            None
        }
    }

    /// The tag of the topmost layer's tile at (x, y) that has one, or `""`.
    /// Topmost because that's the one a player sees — a foreground decal
    /// tagged over a floor tag is the more specific answer.
    pub fn tag_at(&self, x: i32, y: i32) -> &str {
        let Some(i) = self.index(x, y) else { return "" };
        for layer in self.layers.iter().rev() {
            if let Some(def) = self.def(layer.cells[i]) {
                if !def.tag.is_empty() {
                    return &def.tag;
                }
            }
        }
        ""
    }

    /// The world-cell range of this grid that could touch `rect`, clamped
    /// to the grid itself — so a script-sized billion-cell rect costs no
    /// more than the grid does. Inclusive-exclusive `(x0, y0, x1, y1)`.
    /// One cell of slack each side, then every caller re-tests with the
    /// real `Rect` math: the exact edge rules live in exactly one place.
    fn cell_range(&self, rect: Rect) -> (i32, i32, i32, i32) {
        let clamp_x = |v: f32| {
            (v.floor() as i64).clamp(self.origin.0 as i64, self.origin.0 as i64 + self.width as i64)
                as i32
        };
        let clamp_y = |v: f32| {
            (v.floor() as i64)
                .clamp(self.origin.1 as i64, self.origin.1 as i64 + self.height as i64)
                as i32
        };
        (
            clamp_x(rect.x - 1.0),
            clamp_y(rect.y - 1.0),
            clamp_x(rect.right() + 1.0),
            clamp_y(rect.bottom() + 1.0),
        )
    }

    /// Every solid cell (passing `mask_bits`, `0` = all) whose unit rect
    /// intersects `rect`, row-major. Row-major is load-bearing: it's the
    /// order `World::resolve_tilemap_collision` pushes a mover out of each
    /// cell in, which reproduces the (layer, y, x) id order the same walls
    /// had as entities (and so the same order their collision events used
    /// to resolve in).
    pub fn overlapping_solid_cells(&self, rect: Rect, mask_bits: u32) -> Vec<(i32, i32)> {
        let (x0, y0, x1, y1) = self.cell_range(rect);
        let mut out = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                if self.solid_at(x, y, mask_bits)
                    && rect.intersects(Rect::new(x as f32, y as f32, 1.0, 1.0))
                {
                    out.push((x, y));
                }
            }
        }
        out
    }

    /// Does any solid cell passing `mask_bits` intersect `rect`?
    pub fn any_solid_in(&self, rect: Rect, mask_bits: u32) -> bool {
        let (x0, y0, x1, y1) = self.cell_range(rect);
        (y0..y1).any(|y| {
            (x0..x1).any(|x| {
                self.solid_at(x, y, mask_bits)
                    && rect.intersects(Rect::new(x as f32, y as f32, 1.0, 1.0))
            })
        })
    }

    /// Nearest `t` in `[0, 1)` at which the segment from (ox, oy) along
    /// (dx, dy) enters a solid cell passing `mask_bits`, or `None`. Tests
    /// every solid cell in the segment's bounding box with the exact same
    /// `Rect::ray_intersects` a collider gets, rather than a DDA grid walk
    /// — DDA is faster on a long ray but has its own corner-grazing rules,
    /// and a wall must answer a raycast identically whether it's an entity
    /// or a cell. Cost is the bounding box's area, clamped to the grid.
    pub fn raycast(&self, ox: f32, oy: f32, dx: f32, dy: f32, mask_bits: u32) -> Option<f32> {
        let bbox = Rect::new(ox.min(ox + dx), oy.min(oy + dy), dx.abs(), dy.abs());
        let (x0, y0, x1, y1) = self.cell_range(bbox);
        let mut best: Option<f32> = None;
        for y in y0..y1 {
            for x in x0..x1 {
                if !self.solid_at(x, y, mask_bits) {
                    continue;
                }
                let cell = Rect::new(x as f32, y as f32, 1.0, 1.0);
                if let Some(t) = cell.ray_intersects(ox, oy, dx, dy) {
                    if t >= 0.0 && t < best.unwrap_or(1.0) {
                        best = Some(t);
                    }
                }
            }
        }
        best
    }

    /// Every non-empty cell inside `view` (or all of them, for `None`), as
    /// `(z, world_x, world_y, source, tint)` — what play mode turns into
    /// draw commands. Walks only the clamped window, not the whole grid.
    pub fn visible_cells(&self, view: Option<Rect>) -> Vec<(i32, i32, i32, &SpriteSource, Color)> {
        let (x0, y0, x1, y1) = match view {
            Some(r) => self.cell_range(r),
            None => (
                self.origin.0,
                self.origin.1,
                self.origin.0 + self.width as i32,
                self.origin.1 + self.height as i32,
            ),
        };
        let mut out = Vec::new();
        for layer in &self.layers {
            for y in y0..y1 {
                for x in x0..x1 {
                    let Some(i) = self.index(x, y) else { continue };
                    let cell = layer.cells[i];
                    if cell == 0 {
                        continue;
                    }
                    let idx = cell as usize - 1;
                    if let (Some(def), Some(src)) = (self.palette.get(idx), self.sources.get(idx)) {
                        out.push((layer.z(), x, y, src, def.fg));
                    }
                }
            }
        }
        out
    }

    /// Every tile, as `(layer, world_x, world_y, def)`, layer-major then
    /// row-major — how `LevelData::all_tiles` turns a baked tilemap back
    /// into `TileRecord`s for the editor.
    pub fn iter_tiles(&self) -> impl Iterator<Item = (u8, i32, i32, &TileDef)> + '_ {
        let w = self.width.max(1) as usize;
        self.layers.iter().flat_map(move |layer| {
            layer.cells.iter().enumerate().filter_map(move |(i, &cell)| {
                let def = self.def(cell)?;
                let x = self.origin.0 + (i % w) as i32;
                let y = self.origin.1 + (i / w) as i32;
                Some((layer.layer, x, y, def))
            })
        })
    }

    /// Number of non-empty cells across every layer.
    pub fn tile_count(&self) -> usize {
        self.layers.iter().map(|l| l.cells.iter().filter(|&&c| c != 0).count()).sum()
    }
}

/// Builds a `Tilemap` one tile at a time over a fixed bounding box —
/// palette interning, one grid per layer, and the refusals that keep a
/// tile an entity instead (`add` returning `false`).
pub struct TilemapBuilder {
    map: Tilemap,
    /// Palette lookup. A `Vec` scan, not a map keyed by `TileDef` — `Color`
    /// has no `Ord`/`Hash`, and adding either to a public type just for
    /// this would be API churn; a real level has a handful of distinct defs,
    /// so the scan is a few comparisons. `last` short-circuits the common
    /// run of identical walls to a single comparison.
    last: Option<u16>,
}

impl TilemapBuilder {
    /// `None` if the box is empty or larger than `MAX_TILEMAP_CELLS`.
    pub fn new(origin: (i32, i32), width: u32, height: u32) -> Option<Self> {
        let cells = width as u64 * height as u64;
        if cells == 0 || cells > MAX_TILEMAP_CELLS {
            return None;
        }
        Some(TilemapBuilder { map: Tilemap::new(origin, width, height), last: None })
    }

    /// Place `def` on `layer` at world cell (x, y). `false` — and nothing
    /// placed — if the cell is outside the box, already occupied on that
    /// layer (two tiles in one cell: the second keeps its own entity, as
    /// it always had), or the palette is full.
    pub fn add(&mut self, layer: u8, x: i32, y: i32, def: TileDef) -> bool {
        let Some(i) = self.map.index(x, y) else { return false };
        let n = self.map.width as usize * self.map.height as usize;
        let li = match self.map.layers.binary_search_by_key(&layer, |l| l.layer) {
            Ok(li) => li,
            Err(li) => {
                self.map.layers.insert(li, TileLayer { layer, cells: vec![0; n] });
                li
            }
        };
        if self.map.layers[li].cells[i] != 0 {
            return false;
        }
        let cell = match self.intern(def) {
            Some(c) => c,
            None => return false,
        };
        self.map.layers[li].cells[i] = cell;
        true
    }

    fn intern(&mut self, def: TileDef) -> Option<u16> {
        if let Some(c) = self.last {
            if self.map.palette[c as usize - 1] == def {
                return Some(c);
            }
        }
        let c = match self.map.palette.iter().position(|d| *d == def) {
            Some(p) => (p + 1) as u16,
            None => {
                if self.map.palette.len() >= MAX_TILE_DEFS {
                    return None;
                }
                self.map.palette.push(def);
                self.map.palette.len() as u16
            }
        };
        self.last = Some(c);
        Some(c)
    }

    /// The finished map, or `None` if nothing was ever placed. Drops any
    /// layer that ended up empty. Caches are NOT built yet — the caller
    /// still owes a `refresh` (it's the one that has the `LayerRegistry`).
    pub fn finish(mut self) -> Option<Tilemap> {
        self.map.layers.retain(|l| l.cells.iter().any(|&c| c != 0));
        if self.map.layers.is_empty() {
            None
        } else {
            Some(self.map)
        }
    }
}

#[cfg(test)]
#[path = "tilemap_tests.rs"]
mod tests;
