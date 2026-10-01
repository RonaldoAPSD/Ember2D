// editor/grid.rs — In-memory tile grid that the editor works on directly.
//
// ── WHAT IS A LevelGrid? ──────────────────────────────────────────────────────
//
// LevelGrid is the editor's live, mutable representation of a level.
// It's distinct from LevelData (in src/level.rs):
//
//   LevelData  — the serialized format: a Vec<TileRecord> list, written to disk.
//   LevelGrid  — the editor's working copy: a BTreeMap for O(log n) lookup by position.
//
// The editor always works with a LevelGrid. When the user saves or presses F5,
// `to_level_data()` converts it back into the flat Vec format for disk/playmode.
//
// ── WHY BTreeMap<(i32, i32, u8), TileRecord>? ────────────────────────────────
//
// A Vec of tiles would require searching the entire list to find (or replace)
// the tile at a given (x, y, layer) position — O(n) per click. With a map
// keyed by (x, y, layer), every get/place/erase is fast: one lookup, no scan.
//
// D18 (7C-6, docs/ember2d-master-plan.md §5.3): this was a `HashMap` until
// here — iteration order (`iter()`, `to_level_data()`'s old unsorted
// `.values().collect()`) depended on insertion order and the process's own
// hash seed, so saving the identical level twice, or on two machines, could
// write the tiles in a different order each time (a meaningless diff, but a
// diff — `roguelike_level_integrity.rs`'s own "editor-saved level" check
// exists because of this). `BTreeMap` iterates in a fixed key order
// regardless of insertion history or process; `to_level_data()` below still
// does its own explicit `(layer, y, x)` sort on top (matching
// `gen_roguelike.rs`'s convention) since the map's own key order —
// `(x, y, layer)`, chosen for lookup ergonomics, not file layout — isn't
// the order the file wants.
//
// Using i32 (not usize) allows negative coordinates without panics.
// The editor clamps to [0..width, 0..height] in in_bounds(), but storing i32
// avoids any underflow issues when computing neighbor positions.
//
// SPARSITY: only cells that have a tile placed on them exist in the map.
// An empty 80×24 grid has zero entries — the map is completely empty.
// This is efficient: most levels are sparse (lots of empty floor space).

use std::collections::BTreeMap;

use ember2d_sim::level::{LevelData, PlayerRecord, TileRecord};

/// The editor's working representation of a level.
///
/// Contrast with `LevelData` (the serialized disk format):
///   - LevelGrid is mutable and optimized for editor operations (fast lookup by position).
///   - LevelData is a plain data struct with a Vec of tiles, suitable for saving/loading.
///
/// Call `to_level_data()` to convert this to the save format.
/// Call `from_level_data()` to load a save file back into the editor.
pub struct LevelGrid {
    /// Width of the level canvas in character columns.
    pub width: usize,

    /// Height of the level canvas in character rows.
    pub height: usize,

    /// All tiles that have been placed, keyed by (column, row, layer).
    ///
    /// Only cells with a tile exist as entries — empty cells are simply absent.
    /// This means a brand-new empty level has `tiles.len() == 0`. `BTreeMap`,
    /// not `HashMap` — see this module's own header comment (D18).
    pub tiles: BTreeMap<(i32, i32, u8), TileRecord>,

    /// The position (column, row) where the player entity spawns when playing.
    /// Displayed as the green '@' marker on the canvas.
    pub spawn_point: (f32, f32),

    /// Additional named spawn points placed with Shift+P.
    ///
    /// Each entry is (name, column, row). Game logic can look these up by name
    /// to spawn enemies, NPCs, or scripted entities at the right location,
    /// and a level transition can enter at one (Step 9-4). Since format v7
    /// the file keeps these and `spawn_point` in ONE name → position map
    /// (`LevelData::spawns`, `spawn_point` under `"player"`); the editor
    /// keeps this list shape — the hierarchy, undo, and the inspector all
    /// index it — and converts in `to_level_data`/`from_level_data`.
    pub extra_spawns: Vec<(String, f32, f32)>,

    /// Human-readable level name shown in the editor title bar and saved in the file.
    pub name: String,

    /// Properties of the player entity (glyph, color, script, camera, etc.).
    /// Editable through the inspector when the Player entry in the hierarchy is selected.
    pub player: PlayerRecord,

    /// Seeds every RNG stream play mode uses for this level (see
    /// `LevelData::seed` / defect D3). Carried through untouched by
    /// `from_level_data`/`to_level_data` so saving an existing level doesn't
    /// silently re-randomize it; a brand-new grid picks a fresh one, same as
    /// `LevelData::empty`.
    pub seed: u64,

    /// The name<->bit table collision filtering resolves layer names
    /// against (Phase 6 Step 7, docs/ember2d-phase6-plan.md;
    /// `LevelData::collision_layers`). Carried through untouched by
    /// `from_level_data`/`to_level_data`, same as `seed` above — this editor
    /// has no UI to edit the list yet, so its only job here is to not lose
    /// it on a save. A brand-new grid gets `LevelData::default_collision_layers()`,
    /// same one-entry default `LevelData::empty` uses.
    pub collision_layers: Vec<String>,
}

impl LevelGrid {
    /// Create a new, empty level canvas with the given dimensions.
    ///
    /// No tiles are placed — the grid is completely empty.
    /// The player spawns at (1, 1) by default (one cell in from the top-left corner).
    pub fn new(width: usize, height: usize) -> Self {
        LevelGrid {
            width,
            height,
            tiles: BTreeMap::new(),
            spawn_point: (1.0, 1.0),
            extra_spawns: Vec::new(),
            name: "Untitled".to_string(),
            player: PlayerRecord::default(),
            seed: rand::random(),
            collision_layers: ember2d_sim::level::default_collision_layers(),
        }
    }

    // ── Tile operations ───────────────────────────────────────────────────────
    //
    // These are the core editor actions: click to place, right-click to erase,
    // hover to inspect. Each is O(1) — no scanning of the tile list.

    /// Place a tile at (x, y, layer), replacing any tile already there.
    ///
    /// Returns the old tile if one existed (used by the undo system to record
    /// what was there before the edit so it can be restored on undo).
    pub fn place(&mut self, x: i32, y: i32, layer: u8, mut tile: TileRecord) -> Option<TileRecord> {
        tile.layer = layer;
        self.tiles.insert((x, y, layer), tile)
    }

    /// Remove the tile at (x, y, layer) if one exists.
    ///
    /// Returns the removed tile (so the undo system can restore it).
    /// Does nothing if the cell was already empty.
    pub fn erase(&mut self, x: i32, y: i32, layer: u8) -> Option<TileRecord> {
        self.tiles.remove(&(x, y, layer))
    }

    /// Return a reference to the tile at (x, y, layer), or None if the cell is empty.
    ///
    /// The `Option<&TileRecord>` return type forces the caller to handle the
    /// "no tile here" case — there's no null pointer to forget to check.
    pub fn get(&self, x: i32, y: i32, layer: u8) -> Option<&TileRecord> {
        self.tiles.get(&(x, y, layer))
    }

    /// Return true if (x, y) is within the level canvas boundaries.
    ///
    /// Used to prevent placing tiles outside the visible area or computing
    /// neighbor positions that would wrap around the grid edges.
    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && y >= 0 && (x as usize) < self.width && (y as usize) < self.height
    }

    /// Iterate over all placed tiles as ((column, row, layer), TileRecord) pairs.
    ///
    /// Iterates in `(x, y, layer)` key order (deterministic — `BTreeMap`,
    /// D18) — not draw order. The editor's render function sorts by
    /// z_order after collecting, same as before this iterated a `HashMap`.
    pub fn iter(&self) -> impl Iterator<Item = (&(i32, i32, u8), &TileRecord)> {
        self.tiles.iter()
    }

    /// Remove all tiles, leaving an empty canvas.
    ///
    /// Does NOT change width, height, spawn_point, or name — only the tile data.
    pub fn clear_all(&mut self) {
        self.tiles.clear();
    }

    /// Resize the level canvas to new_w × new_h.
    ///
    /// Tiles that fall outside the new bounds are removed.
    /// Newly exposed area (if the canvas grows) is left empty — no tiles are added.
    /// Spawn points are clamped to the new bounds so they stay on the canvas.
    pub fn resize(&mut self, new_w: usize, new_h: usize) {
        self.width = new_w;
        self.height = new_h;

        // Remove tiles outside the new canvas. `retain` keeps entries where
        // the closure returns true, removes all others.
        self.tiles.retain(|&(x, y, _), _| {
            x >= 0 && y >= 0 && (x as usize) < new_w && (y as usize) < new_h
        });

        // Clamp the main player spawn point to stay within the resized canvas.
        self.spawn_point.0 = self.spawn_point.0.min((new_w as f32) - 1.0).max(0.0);
        self.spawn_point.1 = self.spawn_point.1.min((new_h as f32) - 1.0).max(0.0);

        // Remove any named spawns that fell outside the new bounds.
        self.extra_spawns.retain(|&(_, x, y)| {
            x >= 0.0 && y >= 0.0 && (x as usize) < new_w && (y as usize) < new_h
        });
    }

    // ── Format conversion ─────────────────────────────────────────────────────

    /// Convert this working grid into the serializable LevelData format.
    ///
    /// Called by:
    ///   - The save function (S key) to write a .level file to disk.
    ///   - F5 / play button to hand level data to PlayState.
    ///
    /// D18 (7C-6, docs/ember2d-master-plan.md §5.3): explicitly sorted by
    /// `(layer, y, x)`, matching `gen_roguelike.rs`'s own convention — the
    /// map's own key order is `(x, y, layer)` (chosen for O(log n) lookup by
    /// position, not file layout), so switching `tiles` to a `BTreeMap`
    /// alone would still write a different tile order than the shipped
    /// demos use. This sort is what actually makes saving the same level
    /// twice produce a byte-identical file — the `BTreeMap` (vs. the old
    /// `HashMap`) is what makes every OTHER iteration of `tiles` (this
    /// method's own `.values()` below, `iter()`, `resize()`) deterministic
    /// too, not just this one call site.
    ///
    /// Step 8-1 (docs/ember2d-master-plan.md §5.7): the result is baked —
    /// every static tile (`TileRecord::is_static`) moved into the v4
    /// `tilemap` section, only interactive tiles left in `tiles` ("auto-
    /// bake on save", the step's own scoping decision; the grid itself
    /// never changes shape, only what gets written). Deterministic like
    /// the sort above: `bake_tilemap` re-sorts and interns its palette in
    /// that same (layer, y, x) order.
    pub fn to_level_data(&self) -> LevelData {
        let mut tiles: Vec<TileRecord> = self.tiles.values().cloned().collect();
        tiles.sort_by_key(|t| (t.layer, t.y, t.x));
        // `LevelData::empty` rather than a struct literal: since Step 9-4 it
        // has private fields (the pre-v7 spawn fields `load` migrates).
        let mut data = LevelData::empty(self.width, self.height);
        data.name = self.name.clone();
        data.tiles = tiles;
        data.player = self.player.clone();
        data.seed = self.seed;
        data.collision_layers = self.collision_layers.clone();
        // Step 9-4: the player spawn under "player", then each named one in
        // list order — `add_spawn` renames a duplicate ("door" twice, or a
        // spawn literally named "player") to "door_2" rather than dropping it.
        data.set_player_spawn(self.spawn_point);
        for (name, x, y) in &self.extra_spawns {
            data.add_spawn(name, (*x, *y));
        }
        data.bake_tilemap();
        data
    }

    /// Build a LevelGrid from a saved LevelData.
    ///
    /// Called when the editor opens a .level file from disk.
    /// Every tile — `all_tiles()`, so a v4 level's baked `tilemap` cells
    /// are unpacked back into ordinary `TileRecord`s first (Step 8-1): the
    /// editor paints, undoes, and inspects plain tiles and never sees a
    /// tilemap — is inserted into the map one at a time, keyed by each
    /// tile's (x, y, layer) position.
    pub fn from_level_data(data: &LevelData) -> Self {
        let mut grid = LevelGrid::new(data.width, data.height);
        grid.name = data.name.clone();
        // Step 9-4: `spawns` back into the editor's two fields — every
        // entry except "player" becomes a named spawn, in name order.
        grid.spawn_point = data.player_spawn();
        grid.extra_spawns = data
            .spawns
            .iter()
            .filter(|(name, _)| name.as_str() != ember2d_sim::level::PLAYER_SPAWN)
            .map(|(name, &(x, y))| (name.clone(), x, y))
            .collect();
        grid.player = data.player.clone();
        grid.seed = data.seed;
        grid.collision_layers = data.collision_layers.clone();

        for tile in data.all_tiles() {
            grid.tiles.insert((tile.x, tile.y, tile.layer), tile);
        }

        grid
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember2d::renderer::color::Color;

    /// D18 (7C-6, docs/ember2d-master-plan.md §5.3): `to_level_data` used
    /// to collect a `HashMap`'s `.values()` directly with no sort at all —
    /// saving the same grid twice (in the same process, in two different
    /// processes, or after any edit that happened to rebuild the map in a
    /// different insertion order) could write tiles in a different order
    /// each time. Placed here in an order that's already sorted by
    /// insertion (so a bug that only sorted correctly by accident would
    /// still be caught by inserting out of order too, below).
    #[test]
    fn to_level_data_sorts_tiles_by_layer_then_y_then_x_regardless_of_insertion_order() {
        let mut grid = LevelGrid::new(10, 10);
        // Insert deliberately out of (layer, y, x) order.
        grid.place(
            5,
            0,
            1,
            TileRecord::new(5, 0, 1, 'e', Color::White, Color::Reset, false, false, ""),
        );
        grid.place(
            0,
            0,
            0,
            TileRecord::new(0, 0, 0, 'a', Color::White, Color::Reset, false, false, ""),
        );
        grid.place(
            1,
            0,
            1,
            TileRecord::new(1, 0, 1, 'c', Color::White, Color::Reset, false, false, ""),
        );
        grid.place(
            0,
            5,
            1,
            TileRecord::new(0, 5, 1, 'd', Color::White, Color::Reset, false, false, ""),
        );
        grid.place(
            9,
            0,
            0,
            TileRecord::new(9, 0, 0, 'b', Color::White, Color::Reset, false, false, ""),
        );

        // Step 8-1: every tile here is static, so after `to_level_data`'s
        // bake they all live in `data.tilemap`, not `data.tiles` —
        // `all_tiles()` is the whole level, in the same (layer, y, x) order.
        let data = grid.to_level_data();
        let glyphs: Vec<char> = data.all_tiles().iter().map(|t| t.glyph).collect();
        assert_eq!(
            glyphs,
            vec!['a', 'b', 'c', 'e', 'd'],
            "expected (layer, y, x) order: (0,0,0)='a' (0,0,9)='b' (1,0,1)='c' (1,0,5)='e' (1,5,0)='d'"
        );
    }

    /// The same grid saved twice (no edits in between) must produce
    /// byte-identical `LevelData` — the actual "Test" this step's own plan
    /// entry names ("open/save floor1.level with no edits -> git diff
    /// empty"), pinned at the `LevelGrid` level rather than needing a real
    /// file on disk.
    #[test]
    fn saving_the_same_grid_twice_produces_identical_tile_order() {
        let mut grid = LevelGrid::new(10, 10);
        grid.place(
            3,
            2,
            1,
            TileRecord::new(3, 2, 1, '#', Color::White, Color::Reset, true, false, ""),
        );
        grid.place(
            1,
            1,
            0,
            TileRecord::new(1, 1, 0, '.', Color::White, Color::Reset, false, false, ""),
        );

        let first = grid.to_level_data();
        let second = grid.to_level_data();
        let positions = |data: &LevelData| {
            data.all_tiles().iter().map(|t| (t.layer, t.y, t.x)).collect::<Vec<_>>()
        };
        assert_eq!(positions(&first), positions(&second));
        // Step 8-1: the baked tilemap itself (palette order, cell values)
        // must be just as deterministic as the tile order.
        assert_eq!(format!("{:?}", first.tilemap), format!("{:?}", second.tilemap));
    }

    /// Step 9-4: the grid's player spawn + named list go into the file's one
    /// `spawns` map and come back out — a duplicate name, or a named spawn
    /// called "player", is renamed rather than lost.
    #[test]
    fn spawns_convert_to_the_v7_map_and_back_without_losing_a_duplicate() {
        let mut grid = LevelGrid::new(10, 10);
        grid.spawn_point = (2.0, 3.0);
        grid.extra_spawns = vec![
            ("door".into(), 1.0, 1.0),
            ("door".into(), 4.0, 4.0),
            ("player".into(), 9.0, 9.0),
        ];
        let data = grid.to_level_data();
        assert_eq!(data.player_spawn(), (2.0, 3.0));
        assert_eq!(data.spawns.len(), 4);
        let back = LevelGrid::from_level_data(&data);
        assert_eq!(back.spawn_point, (2.0, 3.0));
        let names: Vec<&str> = back.extra_spawns.iter().map(|(n, _, _)| n.as_str()).collect();
        assert_eq!(names, ["door", "door_2", "player_2"]);
        assert_eq!(back.extra_spawns[1], ("door_2".into(), 4.0, 4.0));
    }
}
