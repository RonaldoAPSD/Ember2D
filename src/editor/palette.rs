// editor/palette.rs — The tile selection palette shown on the right side of the editor.
//
// ── WHAT IS A PALETTE? ────────────────────────────────────────────────────────
//
// A palette is just a list of "stamp definitions" — pre-configured tile types
// that the user can select and then click to paint onto the grid.
//
// Each entry in the palette has:
//   - A display name ("Wall", "Floor", "Item", …)
//   - A keyboard shortcut (1 = Wall, 2 = Floor, …)
//   - The visual properties: glyph, fg color, bg color
//   - The gameplay properties: solid (blocks movement), trigger (fires events)
//   - An optional tag string for game logic
//
// The palette is FIXED at startup (hard-coded here). In a future version you
// could make it configurable via a RON file.
//
// ── PANEL LAYOUT (rendered by editor/ui.rs) ───────────────────────────────────
//
//   ┌─ PALETTE ──────┐
//   │ 1: # Wall      │  ← selected entry is highlighted with a box
//   │ 2: . Floor     │
//   │ 3: * Item      │
//   │ 4: @ Spawn     │
//   │ 5: ~ Water     │
//   └────────────────┘

use serde::{Serialize, Deserialize};
use crate::renderer::color::Color;
use crate::level::TileRecord;

// ── TileDefinition ────────────────────────────────────────────────────────────

/// One entry in the editor palette: a named, pre-configured tile type.
///
/// When the user selects this entry and left-clicks on the canvas, a TileRecord
/// is created from these fields and placed in the LevelGrid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileDefinition {
    /// Short human-readable name shown in the palette panel.
    pub name: String,

    /// The ASCII character drawn at each cell of this type.
    pub glyph: char,

    /// Foreground (glyph) color.
    pub fg: Color,

    /// Background color. Use `Color::Reset` for the engine default dark background.
    pub bg: Color,

    /// If true, the tile blocks movement (treated as a solid wall in play mode).
    pub solid: bool,

    /// If true, the tile fires Collision events when overlapped but does NOT
    /// block movement (useful for items, zone triggers, checkpoints).
    pub trigger: bool,

    /// Tag string stored in the TileRecord. Game logic can query `world.find_by_tag`.
    /// Empty string = no tag.
    pub tag: String,
}

impl TileDefinition {
    /// Build a TileRecord at position (x, y) using this definition's properties.
    ///
    /// Called when the user left-clicks on the canvas.
    pub fn to_tile_record(&self, x: i32, y: i32) -> TileRecord {
        TileRecord::new(
            x, y,
            0, // Default layer, will be overwritten by grid.place()
            self.glyph,
            self.fg, self.bg,
            self.solid,
            self.trigger,
            &self.tag,
        )
    }
}

// ── TilePalette ───────────────────────────────────────────────────────────────

/// The full palette: a list of tile definitions and a "currently selected" index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TilePalette {
    /// All available tile types, in the order they appear in the panel.
    /// Index 0 = key "1", index 1 = key "2", etc.
    pub tiles: Vec<TileDefinition>,

    /// Index into `tiles` for the currently selected entry.
    pub selected: usize,
}

impl TilePalette {
    /// Build the default palette with the standard tile types.
    pub fn default_palette() -> Self {
        TilePalette {
            tiles: vec![
                TileDefinition {
                    name:    "Wall".into(),
                    glyph:   '#',
                    fg:      Color::Grey,
                    bg:      Color::Reset,
                    solid:   true,
                    trigger: false,
                    tag:     "wall".into(),
                },
                TileDefinition {
                    name:    "Floor".into(),
                    glyph:   '.',
                    fg:      Color::DarkGrey,
                    bg:      Color::Reset,
                    solid:   false,
                    trigger: false,
                    tag:     "floor".into(),
                },
                TileDefinition {
                    name:    "Item".into(),
                    glyph:   '*',
                    fg:      Color::Yellow,
                    bg:      Color::Reset,
                    solid:   false,
                    trigger: true,
                    tag:     "item".into(),
                },
                TileDefinition {
                    name:    "Spawn".into(),
                    glyph:   '@',
                    fg:      Color::Green,
                    bg:      Color::Reset,
                    solid:   false,
                    trigger: false,
                    tag:     "spawn".into(),
                },
                TileDefinition {
                    name:    "Water".into(),
                    glyph:   '~',
                    fg:      Color::Cyan,
                    bg:      Color::DarkBlue,
                    solid:   false,
                    trigger: true,
                    tag:     "water".into(),
                },
                TileDefinition {
                    name:    "Door".into(),
                    glyph:   '+',
                    fg:      Color::DarkYellow,
                    bg:      Color::Reset,
                    solid:   true,
                    trigger: false,
                    tag:     "door".into(),
                },
                TileDefinition {
                    name:    "Chest".into(),
                    glyph:   '$',
                    fg:      Color::Yellow,
                    bg:      Color::Reset,
                    solid:   false,
                    trigger: true,
                    tag:     "chest".into(),
                },
                TileDefinition {
                    name:    "Pillar".into(),
                    glyph:   'O',
                    fg:      Color::White,
                    bg:      Color::Reset,
                    solid:   true,
                    trigger: false,
                    tag:     "pillar".into(),
                },
                TileDefinition {
                    name:    "Danger".into(),
                    glyph:   '^',
                    fg:      Color::Red,
                    bg:      Color::Reset,
                    solid:   false,
                    trigger: true,
                    tag:     "danger".into(),
                },
            ],
            selected: 0,
        }
    }

    /// Save the palette to a RON file.
    pub fn save(&self, path: &str) -> Result<(), String> {
        let pretty = ron::ser::PrettyConfig::default()
            .depth_limit(2)
            .separate_tuple_members(true)
            .enumerate_arrays(true);
        let s = ron::ser::to_string_pretty(self, pretty).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Load a palette from a RON file.
    pub fn load(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let palette: TilePalette = ron::de::from_str(&content).map_err(|e| e.to_string())?;
        Ok(palette)
    }

    /// Select the tile at `index` (0-based). Clamps to valid range.
    pub fn select(&mut self, index: usize) {
        if index < self.tiles.len() {
            self.selected = index;
        }
    }

    /// Select by keyboard number key (1–9 → indices 0–8).
    ///
    /// Returns true if the index was in range (the selection changed).
    pub fn select_by_key(&mut self, key_number: usize) -> bool {
        if key_number >= 1 && key_number <= self.tiles.len() {
            self.selected = key_number - 1;
            true
        } else {
            false
        }
    }

    /// Return a reference to the currently selected tile definition.
    pub fn current(&self) -> &TileDefinition {
        self.tiles.get(self.selected).unwrap_or(&self.tiles[0])
    }

    /// Number of tile entries in the palette.
    pub fn len(&self) -> usize {
        self.tiles.len()
    }
}
