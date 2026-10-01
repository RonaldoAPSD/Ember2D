// tileset.rs — a sprite sheet sliced into a grid of named regions, and the
// reference a placed tile uses to point at one of those regions.
//
// ── WHY (Step 8-2, docs/ember2d-master-plan.md §5.7) ─────────────────────────
//
// Until 8-2 a tile could only be a glyph, or a whole image file
// (`TileRecord::texture`). Real tile art lives in sprite SHEETS — one PNG
// holding dozens of 16×16 tiles — and the renderer has always been able to
// draw a sub-rect of an image (`SpriteSource::Texture { src }`); nothing in a
// level could ever ask it to.
//
// A `TilesetData` is the description of one sheet: which image, how big a
// cell is, the margin around the grid and the spacing between cells, and a
// list of NAMED regions (each one or more cells). It is written by the
// editor's importer as `<project>/assets/tilesets/<name>.ron`, next to a copy
// of the image.
//
// A placed tile doesn't copy a pixel rect out of it. It stores a `SpriteRef`
// — tileset name + region name (the user's own 8-2 scoping decision) — and
// the rect is looked up when the level loads. Re-slicing a tileset (a
// different cell size, a region moved) therefore updates every tile already
// painted with it, and a script will be able to name a region the same way.
//
// This module is pure data + arithmetic: it never touches the filesystem
// (CLAUDE.md's determinism rules for this crate). Finding and reading the
// `.ron` goes through the caller's `LevelSource` (simulation/tilesets.rs);
// reading the image's pixel size is the editor's job (it owns image loading).

use serde::{Deserialize, Serialize};

use crate::math::Rect;

/// Where a project keeps its tilesets, relative to the project root.
pub const TILESET_DIR: &str = "assets/tilesets";

/// A tile's sprite: region `region` of tileset `tileset`. Resolved to an
/// image path + pixel rect at level load (`TilesetData::region_rect`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteRef {
    pub tileset: String,
    pub region: String,
}

impl SpriteRef {
    pub fn new(tileset: impl Into<String>, region: impl Into<String>) -> Self {
        SpriteRef { tileset: tileset.into(), region: region.into() }
    }
}

/// One named region: the cell at (`col`, `row`), spanning `w` × `h` cells
/// (1 × 1 unless a region covers a larger sprite, e.g. a 2×2 tree).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TilesetRegion {
    pub name: String,
    pub col: u32,
    pub row: u32,
    #[serde(default = "one")]
    pub w: u32,
    #[serde(default = "one")]
    pub h: u32,
}

fn one() -> u32 {
    1
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TilesetData {
    /// Also the file stem: `assets/tilesets/<name>.ron`.
    pub name: String,
    /// The sheet image, relative to the tileset `.ron` file's own directory
    /// (the importer copies the PNG next to it, so this is a bare filename).
    pub image: String,
    pub cell_w: u32,
    pub cell_h: u32,
    /// Pixels between the image edge and the first cell.
    #[serde(default)]
    pub margin: u32,
    /// Pixels between neighbouring cells.
    #[serde(default)]
    pub spacing: u32,
    /// Grid size in cells — derived from the image size by `grid_size` when
    /// the importer writes the file, stored so the sim (which never opens
    /// images) can bounds-check regions.
    pub columns: u32,
    pub rows: u32,
    #[serde(default)]
    pub regions: Vec<TilesetRegion>,
}

impl TilesetData {
    /// How many whole cells fit in an image `image_w` × `image_h` pixels at
    /// this cell size, margin and spacing. A partial cell at the right or
    /// bottom edge is not a cell.
    pub fn grid_size(
        image_w: u32,
        image_h: u32,
        cell_w: u32,
        cell_h: u32,
        margin: u32,
        spacing: u32,
    ) -> (u32, u32) {
        let fit = |len: u32, cell: u32| -> u32 {
            if cell == 0 || len < 2 * margin + cell {
                return 0;
            }
            // n cells need n*cell + (n-1)*spacing pixels inside the margins.
            (len - 2 * margin + spacing) / (cell + spacing)
        };
        (fit(image_w, cell_w), fit(image_h, cell_h))
    }

    /// Pixel rect of a `w` × `h`-cell block whose top-left cell is (`col`,
    /// `row`): the spacing between the cells it covers is part of the
    /// sprite, the spacing around it isn't.
    pub fn cell_rect(&self, col: u32, row: u32, w: u32, h: u32) -> Rect {
        let x = self.margin + col * (self.cell_w + self.spacing);
        let y = self.margin + row * (self.cell_h + self.spacing);
        let pw = w * self.cell_w + w.saturating_sub(1) * self.spacing;
        let ph = h * self.cell_h + h.saturating_sub(1) * self.spacing;
        Rect::new(x as f32, y as f32, pw as f32, ph as f32)
    }

    pub fn region(&self, name: &str) -> Option<&TilesetRegion> {
        self.regions.iter().find(|r| r.name == name)
    }

    /// The pixel rect a region names, or `None` if no region has that name.
    pub fn region_rect(&self, name: &str) -> Option<Rect> {
        self.region(name).map(|r| self.cell_rect(r.col, r.row, r.w, r.h))
    }

    /// Everything that would make this tileset unusable or ambiguous: a zero
    /// cell size, a name that isn't a safe file stem, an unnamed or
    /// duplicate region, or a region running off the grid.
    pub fn validate(&self) -> Result<(), String> {
        if !valid_name(&self.name) {
            return Err(format!(
                "tileset name '{}' must be letters, digits, '_' or '-'",
                self.name
            ));
        }
        if self.cell_w == 0 || self.cell_h == 0 {
            return Err("cell width and height must be at least 1 pixel".to_string());
        }
        for (i, r) in self.regions.iter().enumerate() {
            if !valid_name(&r.name) {
                return Err(format!(
                    "region name '{}' must be letters, digits, '_' or '-'",
                    r.name
                ));
            }
            if self.regions[..i].iter().any(|o| o.name == r.name) {
                return Err(format!("two regions are both named '{}'", r.name));
            }
            if r.w == 0 || r.h == 0 || r.col + r.w > self.columns || r.row + r.h > self.rows {
                return Err(format!(
                    "region '{}' runs off the {}×{} grid",
                    r.name, self.columns, self.rows
                ));
            }
        }
        Ok(())
    }
}

/// A tileset or region name: non-empty ASCII letters, digits, `_` and `-` —
/// safe as a file stem on every OS and unambiguous inside a RON string.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> TilesetData {
        TilesetData {
            name: "dungeon".to_string(),
            image: "dungeon.png".to_string(),
            cell_w: 16,
            cell_h: 16,
            margin: 1,
            spacing: 2,
            columns: 4,
            rows: 3,
            regions: vec![
                TilesetRegion { name: "wall".to_string(), col: 0, row: 0, w: 1, h: 1 },
                TilesetRegion { name: "tree".to_string(), col: 2, row: 1, w: 2, h: 2 },
            ],
        }
    }

    #[test]
    fn grid_size_counts_whole_cells_inside_margin_and_spacing() {
        // 1 + 4*16 + 3*2 + 1 = 72 px wide holds exactly 4 columns...
        assert_eq!(TilesetData::grid_size(72, 54, 16, 16, 1, 2), (4, 3));
        // ...and one pixel less doesn't.
        assert_eq!(TilesetData::grid_size(71, 54, 16, 16, 1, 2), (3, 3));
        assert_eq!(TilesetData::grid_size(64, 64, 16, 16, 0, 0), (4, 4));
        assert_eq!(TilesetData::grid_size(10, 10, 16, 16, 0, 0), (0, 0));
        assert_eq!(
            TilesetData::grid_size(10, 10, 0, 16, 0, 0),
            (0, 0),
            "a zero cell size never divides"
        );
    }

    #[test]
    fn region_rects_include_inner_spacing_but_not_outer() {
        let t = sheet();
        assert_eq!(t.region_rect("wall"), Some(Rect::new(1.0, 1.0, 16.0, 16.0)));
        // col 2 starts at 1 + 2*(16+2) = 37; a 2-wide region spans 16+2+16.
        assert_eq!(t.region_rect("tree"), Some(Rect::new(37.0, 19.0, 34.0, 34.0)));
        assert_eq!(t.region_rect("missing"), None);
    }

    #[test]
    fn validate_rejects_bad_names_duplicates_and_off_grid_regions() {
        assert!(sheet().validate().is_ok());
        let mut t = sheet();
        t.regions[1].col = 3; // a 2-wide region at the last column runs off
        assert!(t.validate().is_err());
        let mut t = sheet();
        t.regions[1].name = "wall".to_string();
        assert!(t.validate().unwrap_err().contains("both named"));
        let mut t = sheet();
        t.name = "../evil".to_string();
        assert!(t.validate().is_err(), "a name must never escape assets/tilesets/");
        let mut t = sheet();
        t.cell_w = 0;
        assert!(t.validate().is_err());
    }

    #[test]
    fn a_tileset_round_trips_through_ron_with_defaults() {
        let text = ron::ser::to_string(&sheet()).unwrap();
        let back: TilesetData = ron::de::from_str(&text).unwrap();
        assert_eq!(back, sheet());
        // A hand-written file may omit margin/spacing/regions and w/h.
        let minimal = r#"(name: "t", image: "t.png", cell_w: 8, cell_h: 8, columns: 2, rows: 2, regions: [(name: "a", col: 1, row: 1)])"#;
        let t: TilesetData = ron::de::from_str(minimal).unwrap();
        assert_eq!((t.margin, t.spacing, t.regions[0].w, t.regions[0].h), (0, 0, 1, 1));
    }
}
