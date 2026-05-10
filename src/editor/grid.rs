// editor/grid.rs — In-memory tile grid that the editor works on directly.
//
// ── WHAT IS A LevelGrid? ──────────────────────────────────────────────────────
//
// LevelGrid is the editor's live, mutable representation of a level.
// It's distinct from LevelData (in src/level.rs):
//
//   LevelData  — the serialized format: a Vec<TileRecord> list, written to disk.
//   LevelGrid  — the editor's working copy: a HashMap for O(1) lookup by position.
//
// The editor always works with a LevelGrid. When the user saves or presses F5,
// `to_level_data()` converts it back into the flat Vec format for disk/playmode.
//
// ── WHY HashMap<(i32, i32), TileRecord>? ─────────────────────────────────────
//
// A Vec of tiles would require searching the entire list to find (or replace)
// the tile at a given (x, y) position — O(n) per click. With a HashMap keyed
// by (x, y), every get/place/erase is O(1): one hash lookup, always instant.
//
// Using i32 (not usize) allows negative coordinates without panics.
// The editor clamps to [0..width, 0..height] in in_bounds(), but storing i32
// avoids any underflow issues when computing neighbor positions.
//
// SPARSITY: only cells that have a tile placed on them exist in the map.
// An empty 80×24 grid has zero entries — the HashMap is completely empty.
// This is efficient: most levels are sparse (lots of empty floor space).

use std::collections::HashMap;

use crate::level::{LevelData, PlayerRecord, TileRecord};

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
    pub width:  usize,

    /// Height of the level canvas in character rows.
    pub height: usize,

    /// All tiles that have been placed, keyed by (column, row).
    ///
    /// Only cells with a tile exist as entries — empty cells are simply absent.
    /// This means a brand-new empty level has `tiles.len() == 0`.
    pub tiles:  HashMap<(i32, i32, u8), TileRecord>,

    /// The position (column, row) where the player entity spawns when playing.
    /// Displayed as the green '@' marker on the canvas.
    pub spawn_point:   (f32, f32),

    /// Additional named spawn points placed with Shift+P.
    ///
    /// Each entry is (name, column, row). Game logic can look these up by name
    /// to spawn enemies, NPCs, or scripted entities at the right location.
    pub extra_spawns:  Vec<(String, f32, f32)>,

    /// Human-readable level name shown in the editor title bar and saved in the file.
    pub name:   String,

    /// Properties of the player entity (glyph, color, script, camera, etc.).
    /// Editable through the inspector when the Player entry in the hierarchy is selected.
    pub player: PlayerRecord,
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
            tiles:        HashMap::new(),
            spawn_point:  (1.0, 1.0),
            extra_spawns: Vec::new(),
            name:         "Untitled".to_string(),
            player:       PlayerRecord::default(),
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
    /// The iteration order is undefined (HashMap doesn't guarantee order).
    /// The editor's render function sorts by z_order after collecting.
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
        self.width  = new_w;
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
    /// The HashMap's values are collected into a Vec. The order is unspecified,
    /// but the renderer in play mode sorts by z_order before drawing anyway.
    pub fn to_level_data(&self) -> LevelData {
        LevelData {
            name:         self.name.clone(),
            width:        self.width,
            height:       self.height,
            spawn_point:  self.spawn_point,
            extra_spawns: self.extra_spawns.clone(),
            tiles:        self.tiles.values().cloned().collect(),
            player:       self.player.clone(),
            path:         String::new(),
        }
    }

    /// Build a LevelGrid from a saved LevelData.
    ///
    /// Called when the editor opens a .level file from disk.
    /// The flat Vec<TileRecord> in LevelData is inserted into the HashMap
    /// one tile at a time, keyed by each tile's (x, y) position.
    pub fn from_level_data(data: &LevelData) -> Self {
        let mut grid = LevelGrid::new(data.width, data.height);
        grid.name         = data.name.clone();
        grid.spawn_point  = data.spawn_point;
        grid.extra_spawns = data.extra_spawns.clone();
        grid.player       = data.player.clone();

        for tile in &data.tiles {
            grid.tiles.insert((tile.x, tile.y, tile.layer), tile.clone());
        }

        grid
    }
}
