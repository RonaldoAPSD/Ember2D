// editor/importer.rs — the tileset importer's in-progress state: the picked
// sheet image, the slicing settings being typed, which cell is selected,
// and the names given to cells so far.
//
// Step 8-2 (docs/ember2d-master-plan.md §5.7; the user's own "full modal"
// scoping choice). Lives on `EditorState::tileset_import` while
// `EditorMode::TilesetImport` is active — a payload that's edited every
// frame, so it's a field rather than inside the mode variant (the
// convention `EditorMode`'s own doc comment sets). Drawing is
// `ui/panels/importer_panel.rs`; input is `input/importer.rs`; turning it
// into files and palette entries is `impl_state/tileset_import.rs`. This
// file is the pure part: no drawing, no files, no `EditorState` — so its
// arithmetic (what grid do these numbers make, which cell is under this
// pixel, what tileset does this produce) is unit-testable on its own.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ember2d::renderer::Texture;
use ember2d_sim::tileset::{valid_name, TilesetData, TilesetRegion};

/// Which text field has keyboard focus. `Hash` so it can be a `WidgetId`
/// payload, the same reason `PaletteField` derives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportField {
    Name,
    CellW,
    CellH,
    Margin,
    Spacing,
    /// The selected cell's region name.
    Region,
}

pub struct TilesetImport {
    /// The image the user picked (copied into the project on import).
    pub source: PathBuf,
    pub texture: Texture,
    pub name: String,
    /// Numeric fields are kept as typed text so a half-typed value (empty,
    /// mid-edit) is representable; `settings()` parses them.
    pub cell_w: String,
    pub cell_h: String,
    pub margin: String,
    pub spacing: String,
    pub focus: Option<ImportField>,
    pub selected: Option<(u32, u32)>,
    /// Region name per (col, row). An empty name means "unnamed".
    pub names: BTreeMap<(u32, u32), String>,
    /// The last failed import's reason, shown in the dialog.
    pub error: Option<String>,
}

impl TilesetImport {
    /// A fresh import of `source`. If the project already has a tileset of
    /// the same name (re-importing a sheet to re-slice it), its settings and
    /// region names are carried over, so re-slicing doesn't mean renaming
    /// every region by hand.
    pub fn new(source: PathBuf, texture: Texture, existing: Option<&TilesetData>) -> Self {
        let stem = source
            .file_stem()
            .and_then(|s| s.to_str())
            .map(sanitize)
            .unwrap_or_else(|| "tileset".to_string());
        let mut imp = TilesetImport {
            source,
            texture,
            name: stem,
            cell_w: "16".to_string(),
            cell_h: "16".to_string(),
            margin: "0".to_string(),
            spacing: "0".to_string(),
            focus: None,
            selected: None,
            names: BTreeMap::new(),
            error: None,
        };
        if let Some(t) = existing {
            imp.name = t.name.clone();
            imp.cell_w = t.cell_w.to_string();
            imp.cell_h = t.cell_h.to_string();
            imp.margin = t.margin.to_string();
            imp.spacing = t.spacing.to_string();
            for r in &t.regions {
                imp.names.insert((r.col, r.row), r.name.clone());
            }
        }
        imp
    }

    /// `(cell_w, cell_h, margin, spacing)`, or `None` while any field isn't
    /// a whole number (or a cell size is 0).
    pub fn settings(&self) -> Option<(u32, u32, u32, u32)> {
        let n = |s: &str| s.trim().parse::<u32>().ok();
        let (w, h, m, s) =
            (n(&self.cell_w)?, n(&self.cell_h)?, n(&self.margin)?, n(&self.spacing)?);
        if w == 0 || h == 0 {
            return None;
        }
        Some((w, h, m, s))
    }

    /// The grid these settings slice the image into, `(columns, rows)`.
    pub fn grid(&self) -> (u32, u32) {
        match self.settings() {
            Some((w, h, m, s)) => {
                TilesetData::grid_size(self.texture.width, self.texture.height, w, h, m, s)
            }
            None => (0, 0),
        }
    }

    /// The cell under image pixel (`px`, `py`), or `None` for a pixel in the
    /// margin, the spacing between cells, or past the last whole cell.
    pub fn cell_at_pixel(&self, px: f32, py: f32) -> Option<(u32, u32)> {
        let (w, h, m, s) = self.settings()?;
        let (cols, rows) = self.grid();
        let axis = |p: f32, cell: u32, count: u32| -> Option<u32> {
            let rel = p - m as f32;
            if rel < 0.0 {
                return None;
            }
            let stride = (cell + s) as f32;
            let i = (rel / stride).floor() as u32;
            let within = rel - i as f32 * stride;
            (i < count && within < cell as f32).then_some(i)
        };
        Some((axis(px, w, cols)?, axis(py, h, rows)?))
    }

    /// The selected cell's region name (empty if unnamed or nothing is
    /// selected).
    pub fn selected_name(&self) -> &str {
        self.selected.and_then(|c| self.names.get(&c)).map(String::as_str).unwrap_or("")
    }

    /// Select `cell` and move keyboard focus to its name field.
    pub fn select(&mut self, cell: (u32, u32)) {
        self.selected = Some(cell);
        self.focus = Some(ImportField::Region);
    }

    /// Typed text into the focused field. Numeric fields take digits only;
    /// names take the characters a tileset/region name allows (anything
    /// else would just be rejected at import time).
    pub fn type_char(&mut self, ch: char) {
        let Some(focus) = self.focus else { return };
        let name_ok = ch.is_ascii_alphanumeric() || ch == '_' || ch == '-';
        match focus {
            ImportField::Name if name_ok && self.name.len() < 40 => self.name.push(ch),
            ImportField::Region if name_ok => {
                if let Some(c) = self.selected {
                    let n = self.names.entry(c).or_default();
                    if n.len() < 40 {
                        n.push(ch);
                    }
                }
            }
            ImportField::CellW
            | ImportField::CellH
            | ImportField::Margin
            | ImportField::Spacing
                if ch.is_ascii_digit() =>
            {
                let f = self.numeric_mut(focus);
                if f.len() < 4 {
                    f.push(ch);
                }
            }
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        match self.focus {
            Some(ImportField::Name) => {
                self.name.pop();
            }
            Some(ImportField::Region) => {
                if let Some(c) = self.selected {
                    if let Some(n) = self.names.get_mut(&c) {
                        n.pop();
                    }
                }
            }
            Some(f) => {
                self.numeric_mut(f).pop();
            }
            None => {}
        }
    }

    fn numeric_mut(&mut self, f: ImportField) -> &mut String {
        match f {
            ImportField::CellW => &mut self.cell_w,
            ImportField::CellH => &mut self.cell_h,
            ImportField::Margin => &mut self.margin,
            _ => &mut self.spacing,
        }
    }

    /// The tileset this import would write: every named cell inside the
    /// current grid becomes a 1×1 region, in row-major order (names on
    /// cells the current settings no longer reach are dropped, not kept
    /// pointing off the sheet). `image` is the file name the sheet will be
    /// copied to, next to the `.ron`.
    pub fn to_tileset(&self) -> Result<TilesetData, String> {
        let (cell_w, cell_h, margin, spacing) = self
            .settings()
            .ok_or("cell size, margin and spacing must be whole numbers, cell size at least 1")?;
        let (columns, rows) = self.grid();
        if columns == 0 || rows == 0 {
            return Err(format!(
                "a {}×{} image holds no whole {cell_w}×{cell_h} cell at these settings",
                self.texture.width, self.texture.height
            ));
        }
        if !valid_name(&self.name) {
            return Err("give the tileset a name (letters, digits, '_' or '-')".to_string());
        }
        let mut cells: Vec<(&(u32, u32), &String)> = self
            .names
            .iter()
            .filter(|((c, r), n)| !n.is_empty() && *c < columns && *r < rows)
            .collect();
        cells.sort_by_key(|((c, r), _)| (*r, *c));
        let regions = cells
            .into_iter()
            .map(|(&(col, row), name)| TilesetRegion { name: name.clone(), col, row, w: 1, h: 1 })
            .collect();
        let data = TilesetData {
            name: self.name.clone(),
            image: format!("{}.png", self.name),
            cell_w,
            cell_h,
            margin,
            spacing,
            columns,
            rows,
            regions,
        };
        data.validate()?;
        Ok(data)
    }
}

/// A file stem made safe as a tileset name: anything outside `[A-Za-z0-9_-]`
/// becomes `_`.
fn sanitize(stem: &str) -> String {
    let s: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if s.is_empty() {
        "tileset".to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4×3-cell, 16 px sheet with 1 px margin and 2 px spacing:
    /// 1 + 4*16 + 3*2 + 1 = 72 wide, 1 + 3*16 + 2*2 + 1 = 54 tall.
    fn imp() -> TilesetImport {
        let tex = Texture { id: 0, width: 72, height: 54, pixels: vec![0; 72 * 54] };
        let mut i = TilesetImport::new(PathBuf::from("my sheet.png"), tex, None);
        i.margin = "1".to_string();
        i.spacing = "2".to_string();
        i
    }

    #[test]
    fn a_new_import_sanitizes_the_file_stem_into_a_name() {
        assert_eq!(imp().name, "my_sheet");
    }

    #[test]
    fn the_grid_follows_the_typed_settings() {
        let mut i = imp();
        assert_eq!(i.grid(), (4, 3));
        i.cell_w = "".to_string();
        assert_eq!(i.grid(), (0, 0), "a half-typed field slices nothing");
        i.cell_w = "0".to_string();
        assert_eq!(i.settings(), None);
    }

    #[test]
    fn pixels_map_to_cells_and_gaps_map_to_none() {
        let i = imp();
        assert_eq!(i.cell_at_pixel(0.5, 5.0), None, "the margin");
        assert_eq!(i.cell_at_pixel(1.0, 1.0), Some((0, 0)));
        assert_eq!(i.cell_at_pixel(17.5, 1.0), None, "the spacing after cell 0");
        assert_eq!(i.cell_at_pixel(19.0, 19.0), Some((1, 1)));
        assert_eq!(i.cell_at_pixel(71.5, 1.0), None, "past the last cell");
    }

    #[test]
    fn typing_respects_each_fields_character_rules() {
        let mut i = imp();
        i.focus = Some(ImportField::CellW);
        i.cell_w.clear();
        for c in "3a2".chars() {
            i.type_char(c);
        }
        assert_eq!(i.cell_w, "32", "letters never enter a number field");
        i.select((2, 1));
        for c in "wall top!".chars() {
            i.type_char(c);
        }
        assert_eq!(i.selected_name(), "walltop", "spaces and punctuation are not name characters");
        i.backspace();
        assert_eq!(i.selected_name(), "wallto");
    }

    #[test]
    fn to_tileset_writes_named_cells_row_major_and_drops_off_grid_names() {
        let mut i = imp();
        i.names.insert((3, 0), "b".to_string());
        i.names.insert((0, 1), "c".to_string());
        i.names.insert((1, 0), "a".to_string());
        i.names.insert((2, 2), String::new()); // unnamed: skipped
        i.names.insert((9, 9), "gone".to_string()); // off the grid: dropped
        let t = i.to_tileset().unwrap();
        let order: Vec<&str> = t.regions.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(order, vec!["a", "b", "c"]);
        assert_eq!((t.columns, t.rows, t.image.as_str()), (4, 3, "my_sheet.png"));
        assert_eq!(t.region_rect("c"), Some(ember2d_sim::math::Rect::new(1.0, 19.0, 16.0, 16.0)));
    }

    #[test]
    fn to_tileset_explains_what_is_wrong() {
        let mut i = imp();
        i.names.insert((0, 0), "x".to_string());
        i.names.insert((1, 0), "x".to_string());
        assert!(i.to_tileset().unwrap_err().contains("both named"));
        let mut i = imp();
        i.cell_w = "100".to_string();
        assert!(i.to_tileset().unwrap_err().contains("no whole"));
        let mut i = imp();
        i.name.clear();
        assert!(i.to_tileset().unwrap_err().contains("name"));
    }

    #[test]
    fn reimporting_carries_over_the_existing_settings_and_names() {
        let existing = TilesetData {
            name: "dungeon".to_string(),
            image: "dungeon.png".to_string(),
            cell_w: 8,
            cell_h: 8,
            margin: 0,
            spacing: 0,
            columns: 9,
            rows: 6,
            regions: vec![TilesetRegion { name: "wall".to_string(), col: 2, row: 1, w: 1, h: 1 }],
        };
        let tex = Texture { id: 0, width: 72, height: 54, pixels: vec![0; 72 * 54] };
        let i = TilesetImport::new(PathBuf::from("dungeon.png"), tex, Some(&existing));
        assert_eq!((i.cell_w.as_str(), i.grid()), ("8", (9, 6)));
        assert_eq!(i.names.get(&(2, 1)).map(String::as_str), Some("wall"));
    }
}
