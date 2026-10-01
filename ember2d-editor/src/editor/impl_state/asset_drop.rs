// editor/impl_state/asset_drop.rs — dragging an asset out of the File
// Browser and dropping it on the palette or on a canvas tile.
//
// Step 8-4 (docs/ember2d-master-plan.md §5.7). Which row is which asset,
// and the drag's own state, are pure and live in `editor/assets.rs`; this
// file is what a drop DOES, which touches the palette, the grid, undo and
// (for an image that was never imported) the project folder.
//
// What a drop does, by target:
//
//   on the palette — adds entries: a tileset adds one per region not
//     already there; a clip adds its entry; an image that's already some
//     tileset's sheet counts as that tileset; any other image opens the
//     File > Import Tileset... dialog on it, since only the user knows how
//     a sheet should be sliced.
//   on a canvas tile — paints that tile, when the asset names exactly one
//     thing to paint: a clip, a one-region tileset, or an image never
//     imported (imported on the spot as a one-region tileset named after
//     the file — "drag a texture onto a tile"). A tileset with several
//     regions is ambiguous: its regions go into the palette instead, and
//     the status line says to pick one.
//
// Adding palette entries and painting are separate undo steps (an
// `UpdatePalette`, then a `PlaceTile`), the same two steps the user would
// have taken by hand.

use std::path::Path;

use ember2d::renderer::Texture;
use ember2d_sim::scripting::LogEntry;
use ember2d_sim::tileset::{valid_name, SpriteRef, TilesetData, TilesetRegion};

use super::super::assets::{self, AssetDrag, AssetRef, DRAG_THRESHOLD_PT};
use super::super::commands::Command;
use super::super::panel::PanelId;
use super::super::EditorState;
use super::tileset_import::write_tileset;

impl EditorState {
    /// A press on a File Browser asset row: remember it. It becomes a drag
    /// only once the mouse moves (`update_asset_drag`).
    pub(in crate::editor) fn begin_asset_drag(&mut self, asset: AssetRef, px: f32, py: f32) {
        self.asset_drag = Some(AssetDrag { asset, start_pt: (px, py), active: false });
    }

    /// Runs first in `handle_panel_input` every frame. While a press that
    /// started on an asset row is held, it owns the mouse (`true` — nothing
    /// else in the panels runs); on release, an active drag drops; Escape
    /// cancels. `false` when there's no such press.
    pub(in crate::editor) fn update_asset_drag(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) -> bool {
        let Some(drag) = self.asset_drag.as_mut() else { return false };
        if input.just_pressed(ember2d::input::Key::Escape) {
            self.asset_drag = None;
            self.flash("Drag cancelled.");
            return true;
        }
        if mouse.left_held() {
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let (dx, dy) = (px - drag.start_pt.0, py - drag.start_pt.1);
            if !drag.active && (dx * dx + dy * dy).sqrt() >= DRAG_THRESHOLD_PT {
                drag.active = true;
            }
            return true;
        }
        let drag = self.asset_drag.take().expect("checked above");
        if drag.active {
            self.drop_asset(drag.asset, mouse);
        }
        true
    }

    /// Topmost panel actually DRAWN at `(px, py)` (points) — unlike
    /// `PanelManager::panel_at`, a docked panel hidden behind another tab
    /// doesn't count.
    fn drawn_panel_at(&self, px: f32, py: f32) -> Option<PanelId> {
        self.panels
            .in_draw_order()
            .into_iter()
            .rev()
            .find(|&id| self.panels.get(id).contains(px, py))
    }

    /// Where a drag released at the mouse would land — `Some(None)` for the
    /// palette, `Some(Some(cell))` for a canvas cell, `None` for nowhere.
    /// Also what the drag ghost uses to show the target while dragging.
    pub(in crate::editor) fn asset_drop_target(
        &self,
        mouse: &ember2d::mouse::MouseState,
    ) -> Option<Option<(i32, i32)>> {
        if !mouse.in_bounds {
            return None;
        }
        let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
        match self.drawn_panel_at(px, py) {
            Some(PanelId::Palette) => Some(None),
            Some(PanelId::Viewport) => self
                .mouse_to_grid(mouse.pixel_x, mouse.pixel_y)
                .filter(|&(gx, gy)| self.grid.in_bounds(gx, gy))
                .map(Some),
            _ => None,
        }
    }

    fn drop_asset(&mut self, asset: AssetRef, mouse: &ember2d::mouse::MouseState) {
        let asset = self.imported_form(asset);
        match self.asset_drop_target(mouse) {
            Some(None) => self.drop_on_palette(asset),
            Some(Some((gx, gy))) => self.drop_on_tile(asset, gx, gy),
            None => self.flash("Drop an asset on the palette or on a canvas tile."),
        }
    }

    /// An image that's already some tileset's sheet drags as that tileset:
    /// the sheet file itself (`assets/tilesets/<image>`), or — so dropping
    /// the same loose picture twice paints the tileset the first drop made
    /// instead of failing on the name — a file with the same name and the
    /// same bytes as tileset `<stem>`'s sheet.
    fn imported_form(&self, asset: AssetRef) -> AssetRef {
        let AssetRef::Image(rel) = &asset else { return asset };
        if let Some(name) = assets::tileset_for_image(&self.sprites, rel) {
            return AssetRef::Tileset(name);
        }
        let Some(folder) = self.project_folder.as_deref() else { return asset };
        let source = Path::new(folder).join(rel);
        let stem = source.file_stem().and_then(|f| f.to_str()).unwrap_or_default();
        let Some(t) = self.sprites.tilesets.get(stem) else { return asset };
        let sheet = super::super::sprites::SpriteAssets::dir(folder).join(&t.data.image);
        let same_name = source.file_name() == sheet.file_name();
        match (same_name, std::fs::read(&source), std::fs::read(&sheet)) {
            (true, Ok(a), Ok(b)) if a == b => AssetRef::Tileset(stem.to_string()),
            _ => asset,
        }
    }

    /// Drop on the palette: add entries (see the header comment).
    pub(in crate::editor) fn drop_on_palette(&mut self, asset: AssetRef) {
        match asset {
            AssetRef::Tileset(name) => {
                let Some(regions) = self.tileset_regions(&name) else { return };
                let added = self.add_tileset_regions_to_palette(&name, &regions);
                if let Some(first) = regions.first() {
                    self.select_sprite_entry(&SpriteRef::new(name.clone(), first.clone()));
                }
                self.flash(&format!(
                    "Tileset '{name}': {added} new palette entr{}.",
                    if added == 1 { "y" } else { "ies" }
                ));
            }
            AssetRef::Clip(name) => match self.ensure_clip_in_palette(&name) {
                Some((i, added)) => {
                    self.palette.select(i);
                    self.flash(&if added {
                        format!("Added clip '{name}' to the palette.")
                    } else {
                        format!("Clip '{name}' is already in the palette.")
                    });
                }
                None => self.drop_error(format!("clip '{name}' isn't loaded (see the console)")),
            },
            AssetRef::Image(rel) => {
                let Some(folder) = self.project_folder.clone() else { return };
                if let Err(e) = self.begin_tileset_import(&Path::new(&folder).join(&rel)) {
                    self.drop_error(e);
                }
            }
        }
    }

    /// Drop on canvas tile `(gx, gy)`: paint it (see the header comment).
    pub(in crate::editor) fn drop_on_tile(&mut self, asset: AssetRef, gx: i32, gy: i32) {
        let entry = match asset {
            AssetRef::Clip(name) => match self.ensure_clip_in_palette(&name) {
                Some((i, _)) => i,
                None => {
                    return self.drop_error(format!("clip '{name}' isn't loaded (see the console)"))
                }
            },
            AssetRef::Image(rel) => match self.import_single_sprite(&rel) {
                Ok(name) => match self.single_region_entry(&name) {
                    Some(i) => i,
                    None => return,
                },
                Err(e) => return self.drop_error(e),
            },
            AssetRef::Tileset(name) => {
                let Some(regions) = self.tileset_regions(&name) else { return };
                if regions.len() != 1 {
                    let added = self.add_tileset_regions_to_palette(&name, &regions);
                    return self.flash(&format!(
                        "Tileset '{name}' has {} regions ({added} added to the palette) - pick one there to paint.",
                        regions.len()
                    ));
                }
                match self.single_region_entry(&name) {
                    Some(i) => i,
                    None => return,
                }
            }
        };
        self.palette.select(entry);
        let layer = self.active_layer;
        let mut tile = self.palette.tiles[entry].to_tile_record(gx, gy);
        tile.layer = layer;
        let before = self.grid.get(gx, gy, layer).cloned();
        self.undo.push(Command::PlaceTile { before, after: tile.clone() });
        self.grid.place(gx, gy, layer, tile);
        self.unsaved = true;
        let name = self.palette.tiles[entry].name.clone();
        self.flash(&format!("Painted '{name}' at ({gx},{gy})."));
    }

    /// Tileset `name`'s region names, or `None` (with a console error) if
    /// the project has no such tileset loaded.
    fn tileset_regions(&mut self, name: &str) -> Option<Vec<String>> {
        match self.sprites.tilesets.get(name) {
            Some(t) => Some(t.data.regions.iter().map(|r| r.name.clone()).collect()),
            None => {
                self.drop_error(format!("tileset '{name}' isn't loaded (see the console)"));
                None
            }
        }
    }

    /// The palette entry for one-region tileset `name`'s region, added if
    /// missing.
    fn single_region_entry(&mut self, name: &str) -> Option<usize> {
        let regions = self.tileset_regions(name)?;
        self.add_tileset_regions_to_palette(name, &regions);
        let sprite = SpriteRef::new(name.to_string(), regions.first()?.clone());
        self.palette
            .tiles
            .iter()
            .position(|t| t.sprite.as_ref() == Some(&sprite) && t.clip.is_none())
    }

    fn select_sprite_entry(&mut self, sprite: &SpriteRef) {
        if let Some(i) = self
            .palette
            .tiles
            .iter()
            .position(|t| t.sprite.as_ref() == Some(sprite) && t.clip.is_none())
        {
            self.palette.select(i);
        }
    }

    /// "Drag a texture onto a tile": import project image `rel` as a
    /// tileset of ONE region covering the whole picture, both named after
    /// the file (`hero.png` -> tileset `hero`, region `hero`). Refuses a
    /// file name that isn't a valid tileset name, a name an existing
    /// tileset already uses, and a sheet file that would overwrite a
    /// different file already in `assets/tilesets/`. Returns the tileset's
    /// name.
    pub(in crate::editor) fn import_single_sprite(&mut self, rel: &str) -> Result<String, String> {
        let folder = self.project_folder.clone().ok_or("no project is open")?;
        let source = Path::new(&folder).join(rel);
        let file = source.file_name().and_then(|f| f.to_str()).unwrap_or_default().to_string();
        let stem = source.file_stem().and_then(|f| f.to_str()).unwrap_or_default().to_string();
        if !valid_name(&stem) {
            return Err(format!(
                "'{file}' can't name a tileset (letters, digits, '_' or '-' only) - rename it, or drop it on the palette to import it under another name"
            ));
        }
        if self.sprites.tilesets.contains_key(&stem) {
            return Err(format!(
                "a tileset named '{stem}' already exists - drop the image on the palette to re-slice it instead"
            ));
        }
        let dest = super::super::sprites::SpriteAssets::dir(&folder).join(&file);
        if dest.exists() && std::fs::canonicalize(&dest).ok() != std::fs::canonicalize(&source).ok()
        {
            return Err(format!("assets/tilesets/{file} already exists and is a different file"));
        }
        let (w, h) = match self.file_thumbs.get(rel) {
            Some(t) => (t.width, t.height),
            None => {
                let t = Texture::load(&source).map_err(|e| format!("{rel}: {e}"))?;
                (t.width, t.height)
            }
        };
        let data = TilesetData {
            name: stem.clone(),
            image: file,
            cell_w: w,
            cell_h: h,
            margin: 0,
            spacing: 0,
            columns: 1,
            rows: 1,
            regions: vec![TilesetRegion { name: stem.clone(), col: 0, row: 0, w: 1, h: 1 }],
        };
        data.validate()?;
        write_tileset(&folder, &source, &data)?;
        for problem in self.sprites.reload(Some(&folder)) {
            self.console_log.push(LogEntry::warn(problem));
        }
        self.refresh_project_files();
        self.console_log.push(LogEntry::info(format!(
            "Imported '{rel}' as one-sprite tileset '{stem}' (assets/tilesets/{stem}.ron)"
        )));
        Ok(stem)
    }

    /// A short message in the title bar's flash slot and the console.
    fn flash(&mut self, msg: &str) {
        self.save_message = Some(msg.to_string());
        self.save_message_timer = 0;
        self.console_log.push(LogEntry::info(msg.to_string()));
    }

    fn drop_error(&mut self, msg: String) {
        self.save_message = Some(format!("Drop failed: {msg}"));
        self.save_message_timer = 0;
        self.console_log.push(LogEntry::error(format!("Drop failed: {msg}")));
    }
}
