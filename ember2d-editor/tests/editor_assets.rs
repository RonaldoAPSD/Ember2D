// ember2d-editor/tests/editor_assets.rs — Step 8-4 (docs/ember2d-master-
// plan.md §5.7): asset preview and drag-and-drop, headless. A temp project
// holds a three-region tileset "fx", a clip "pulse" playing it, and two
// loose images under `art/`; the File Browser lists and thumbnails them,
// previews the selected one, and each kind is dragged onto the palette and
// onto a canvas tile. Plus R101: painting one sprite entry over another
// that shares its fallback glyph.

mod common;

use common::{canvas_pixel_for_grid, ensure_workspace_root_cwd, select_dock_tab, EditorHarness};
use ember2d::input::Key;
use ember2d::renderer::draw_log::DrawOp;
use ember2d_editor::editor::assets::AssetRef;
use ember2d_editor::editor::panel::PanelId;
use ember2d_editor::editor::ui::WidgetId;
use ember2d_editor::editor::{EditorMode, EditorState};
use ember2d_sim::clip_asset::ClipData;
use ember2d_sim::math::Rect;
use ember2d_sim::tileset::{SpriteRef, TilesetData, TilesetRegion};
use std::path::{Path, PathBuf};

fn png(path: &Path, w: u32, h: u32, shade: u8) {
    let mut img = image::RgbaImage::new(w, h);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([shade, (x * 7) as u8, 200, 255]);
    }
    img.save(path).unwrap();
}

/// `<tmp>/<tag>/`: tileset "fx" (64×16, regions a/b/c), clip "pulse"
/// (a, b, c at 8 fps), and loose images `art/hero.png` (16×16), `art/bad
/// name.png`, `art/brick.png` and `art/barrel.png`.
fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    let ts_dir = dir.join("assets/tilesets");
    std::fs::create_dir_all(&ts_dir).unwrap();
    std::fs::create_dir_all(dir.join("assets/clips")).unwrap();
    std::fs::create_dir_all(dir.join("art")).unwrap();
    png(&ts_dir.join("fx.png"), 64, 16, 40);
    png(&dir.join("art/hero.png"), 16, 16, 220);
    png(&dir.join("art/bad name.png"), 8, 8, 120);
    png(&dir.join("art/brick.png"), 8, 8, 60);
    png(&dir.join("art/barrel.png"), 8, 8, 160);
    let regions = ["a", "b", "c"]
        .iter()
        .enumerate()
        .map(|(i, n)| TilesetRegion { name: n.to_string(), col: i as u32, row: 0, w: 1, h: 1 })
        .collect();
    let ts = TilesetData {
        name: "fx".into(),
        image: "fx.png".into(),
        cell_w: 16,
        cell_h: 16,
        margin: 0,
        spacing: 0,
        columns: 4,
        rows: 1,
        regions,
    };
    std::fs::write(ts_dir.join("fx.ron"), ron::ser::to_string(&ts).unwrap()).unwrap();
    let clip = ClipData {
        name: "pulse".into(),
        tileset: "fx".into(),
        frames: vec!["a".into(), "b".into(), "c".into()],
        fps: 8.0,
        looping: true,
    };
    std::fs::write(dir.join("assets/clips/pulse.ron"), ron::ser::to_string(&clip).unwrap())
        .unwrap();
    dir
}

fn harness_in(dir: &Path) -> EditorHarness {
    ensure_workspace_root_cwd();
    let mut state = EditorState::new(dir.join("level.level").to_str().unwrap());
    state.open_project_folder(dir.to_string_lossy().into_owned());
    let mut h = EditorHarness::with_state(state);
    h.frame();
    select_dock_tab(&mut h, PanelId::FileBrowser);
    h
}

/// Logical-pixel center of File Browser row `i`, scrolling the (short,
/// bottom-docked) list down with the wheel until that row is drawn.
fn row_center(h: &mut EditorHarness, i: usize) -> (f32, f32) {
    for _ in 0..20 {
        if let Some(r) = h.state.ui_frame().rect_of(WidgetId::FileBrowserRow(i)) {
            return h.state.ui_space().to_logical(r.x + 12.0, r.y + r.h * 0.5);
        }
        let top = (0..h.state.file_browser_files().len())
            .find_map(|j| h.state.ui_frame().rect_of(WidgetId::FileBrowserRow(j)))
            .expect("some file row is drawn");
        let (x, y) = h.state.ui_space().to_logical(top.x + 12.0, top.y + top.h * 0.5);
        h.wheel(x, y, -1.0);
        h.frame();
    }
    panic!("file row {i} never came into view")
}

fn row_index(h: &EditorHarness, needle: &str) -> usize {
    h.state
        .file_browser_files()
        .iter()
        .position(|f| f.contains(needle))
        .unwrap_or_else(|| panic!("'{needle}' not listed in {:?}", h.state.file_browser_files()))
}

/// Click File Browser folder rows to walk into `path` from the root.
fn open_folder(h: &mut EditorHarness, path: &[&str]) {
    for dir in path {
        let i = row_index(h, &format!("/ {dir} "));
        let (x, y) = row_center(h, i);
        h.click(x, y);
        h.frame();
    }
}

/// Show the palette panel (B toggles it) so it can be a drop target.
fn show_palette(h: &mut EditorHarness) {
    if h.state.ui_frame().rect_of(WidgetId::PaletteNewBtn).is_none() {
        h.key(Key::B);
        h.frame();
    }
    assert!(h.state.ui_frame().rect_of(WidgetId::PaletteNewBtn).is_some(), "palette drawn");
}

fn palette_center(h: &EditorHarness) -> (f32, f32) {
    let r = h.state.ui_frame().rect_of(WidgetId::PaletteNewBtn).unwrap();
    h.state.ui_space().to_logical(r.x + r.w * 0.5, r.y + r.h * 0.5)
}

fn drag_row_to(h: &mut EditorHarness, needle: &str, to: (f32, f32)) {
    let from = {
        let i = row_index(h, needle);
        row_center(h, i)
    };
    h.drag(from, to);
    h.frame();
}

fn palette_has(
    h: &EditorHarness,
    f: impl Fn(&ember2d_editor::editor::palette::TileDefinition) -> bool,
) -> usize {
    (0..h.state.palette_tile_count()).filter(|&i| f(h.state.palette_tile(i))).count()
}

fn textures_inside(h: &EditorHarness, area: Rect) -> usize {
    h.draw_ops()
        .iter()
        .filter(|op| match op {
            DrawOp::Texture { dest, .. } => {
                dest.x >= area.x - 0.5
                    && dest.y >= area.y - 0.5
                    && dest.x + dest.w <= area.x + area.w + 0.5
                    && dest.y + dest.h <= area.y + area.h + 0.5
            }
            _ => false,
        })
        .count()
}

#[test]
fn the_file_browser_lists_thumbnails_and_previews_project_assets() {
    let dir = project("assets_browse");
    let mut h = harness_in(&dir);
    open_folder(&mut h, &["assets", "tilesets"]);
    let files = h.state.file_browser_files().to_vec();
    assert!(files.iter().any(|f| f.starts_with("<> fx.png")), "{files:?}");
    assert!(files.iter().any(|f| f.starts_with("## fx.ron")), "{files:?}");

    // Each asset row draws a thumbnail inside its own row.
    h.start_recording();
    h.frame();
    for needle in ["fx.png", "fx.ron"] {
        let r =
            h.state.ui_frame().rect_of(WidgetId::FileBrowserRow(row_index(&h, needle))).unwrap();
        let k = h.state.ui_space().pt_to_logical();
        let logical = Rect::new(r.x * k, r.y * k, r.w * k, r.h * k);
        assert_eq!(textures_inside(&h, logical), 1, "{needle}'s row draws one thumbnail");
    }

    // A click (no drag) on an asset row selects it and the panel previews
    // it: one more image than the row thumbnails alone.
    let (x, y) = {
        let i = row_index(&h, "fx.ron");
        row_center(&mut h, i)
    };
    h.click(x, y);
    h.frame();
    assert_eq!(h.state.file_browser_cursor(), row_index(&h, "fx.ron"));
    assert!(h.state.asset_drag().is_none(), "a click is not a drag");
    h.start_recording();
    h.frame();
    let images = h.draw_ops().iter().filter(|op| matches!(op, DrawOp::Texture { .. })).count();
    assert_eq!(images, 3, "two row thumbnails + the preview");

    // The clips folder lists the clip, its thumbnail playing a frame.
    let up = row_index(&h, "[UP]");
    let (x, y) = row_center(&mut h, up);
    h.click(x, y);
    h.frame();
    open_folder(&mut h, &["clips"]);
    assert!(h.state.file_browser_files().iter().any(|f| f.starts_with("~~ pulse.ron")));
}

#[test]
fn dropping_a_clip_on_a_tile_paints_it_and_on_the_palette_adds_it_once() {
    let dir = project("assets_clip");
    let mut h = harness_in(&dir);
    show_palette(&mut h);
    open_folder(&mut h, &["assets", "clips"]);

    let cell = canvas_pixel_for_grid(&h, 5, 3);
    drag_row_to(&mut h, "pulse.ron", cell);
    let layer = 1; // the editor's default active layer (Main)
    let tile = h.state.grid().get(5, 3, layer).expect("the drop painted a tile").clone();
    assert_eq!(tile.clip.as_deref(), Some("pulse"));
    assert_eq!(palette_has(&h, |t| t.clip.as_deref() == Some("pulse")), 1);

    // Dropping it on the palette again adds nothing new.
    let to = palette_center(&h);
    drag_row_to(&mut h, "pulse.ron", to);
    assert_eq!(palette_has(&h, |t| t.clip.as_deref() == Some("pulse")), 1);

    // The paint is its own undo step.
    h.key(Key::U);
    h.frame();
    assert!(h.state.grid().get(5, 3, layer).is_none(), "undo removes the dropped tile");
}

#[test]
fn a_multi_region_tileset_fills_the_palette_but_paints_nothing() {
    let dir = project("assets_tileset");
    let mut h = harness_in(&dir);
    show_palette(&mut h);
    open_folder(&mut h, &["assets", "tilesets"]);

    let cell = canvas_pixel_for_grid(&h, 2, 2);
    drag_row_to(&mut h, "fx.ron", cell);
    assert!(h.state.grid().get(2, 2, 1).is_none(), "ambiguous: which region?");
    assert_eq!(palette_has(&h, |t| t.sprite.as_ref().is_some_and(|s| s.tileset == "fx")), 3);

    // The sheet image drags as its tileset — no second import.
    let to = palette_center(&h);
    drag_row_to(&mut h, "fx.png", to);
    assert!(!matches!(h.state.mode(), EditorMode::TilesetImport));
    assert_eq!(palette_has(&h, |t| t.sprite.as_ref().is_some_and(|s| s.tileset == "fx")), 3);
}

#[test]
fn a_loose_image_dropped_on_a_tile_becomes_a_one_sprite_tileset_and_paints() {
    let dir = project("assets_image");
    let mut h = harness_in(&dir);
    show_palette(&mut h);
    open_folder(&mut h, &["art"]);

    let cell = canvas_pixel_for_grid(&h, 4, 4);
    drag_row_to(&mut h, "hero.png", cell);
    let tile = h.state.grid().get(4, 4, 1).expect("painted").clone();
    assert_eq!(tile.sprite, Some(SpriteRef::new("hero", "hero")));
    let written: TilesetData =
        ron::de::from_str(&std::fs::read_to_string(dir.join("assets/tilesets/hero.ron")).unwrap())
            .unwrap();
    assert_eq!((written.cell_w, written.cell_h, written.regions.len()), (16, 16, 1));
    assert!(dir.join("assets/tilesets/hero.png").exists());

    // The same picture dropped again paints from that tileset.
    let cell = canvas_pixel_for_grid(&h, 6, 4);
    drag_row_to(&mut h, "hero.png", cell);
    assert_eq!(
        h.state.grid().get(6, 4, 1).and_then(|t| t.sprite.clone()),
        Some(SpriteRef::new("hero", "hero"))
    );
    assert_eq!(palette_has(&h, |t| t.sprite == Some(SpriteRef::new("hero", "hero"))), 1);

    // A file name that can't be a tileset name is refused, nothing written.
    let cell = canvas_pixel_for_grid(&h, 8, 4);
    drag_row_to(&mut h, "bad name.png", cell);
    assert!(h.state.grid().get(8, 4, 1).is_none());
    assert!(!dir.join("assets/tilesets/bad name.ron").exists());
    assert!(h.state.console_log().iter().any(|e| e.text.contains("can't name a tileset")));

    // On the palette, an un-imported image opens the importer on it.
    let to = palette_center(&h);
    drag_row_to(&mut h, "bad name.png", to);
    assert!(matches!(h.state.mode(), EditorMode::TilesetImport));
}

#[test]
fn a_drag_dropped_nowhere_or_cancelled_changes_nothing() {
    let dir = project("assets_nowhere");
    let mut h = harness_in(&dir);
    open_folder(&mut h, &["assets", "clips"]);
    let palette_before = h.state.palette_tile_count();
    let undo_before = h.state.undo_len();
    let from = {
        let i = row_index(&h, "pulse.ron");
        row_center(&mut h, i)
    };
    let cell = canvas_pixel_for_grid(&h, 3, 3);

    // Released over the File Browser itself — not a drop target.
    drag_row_to(&mut h, "pulse.ron", (from.0 + 40.0, from.1));

    // Escape mid-drag, then released over a canvas tile: cancelled.
    h.press_left_at(from.0, from.1);
    h.move_held(cell.0, cell.1);
    assert!(h.state.asset_drag().is_some_and(|d| d.active), "moving past the threshold drags");
    h.key(Key::Escape);
    assert!(h.state.asset_drag().is_none(), "Escape cancels");
    h.release_left();
    h.frame();

    assert_eq!(h.state.palette_tile_count(), palette_before);
    assert_eq!(h.state.undo_len(), undo_before);
    assert!(h.state.grid().get(3, 3, 1).is_none(), "nothing painted");
}

#[test]
fn a_drag_shows_a_ghost_and_outlines_the_palette_it_would_drop_on() {
    let dir = project("assets_ghost");
    let mut h = harness_in(&dir);
    show_palette(&mut h);
    open_folder(&mut h, &["assets", "clips"]);
    let from = {
        let i = row_index(&h, "pulse.ron");
        row_center(&mut h, i)
    };
    let over = palette_center(&h);
    h.press_left_at(from.0, from.1);
    h.move_held(over.0, over.1);
    h.start_recording();
    h.move_held(over.0 + 2.0, over.1);
    let drag = h.state.asset_drag().expect("dragging");
    assert_eq!(drag.asset, AssetRef::Clip("pulse".into()));
    // The ghost: the clip's current frame drawn just below-right of the
    // mouse (it's offset so it never hides the drop point).
    let ghost = h.draw_ops().iter().any(|op| match op {
        DrawOp::Texture { dest, .. } => {
            dest.x > over.0 && dest.y > over.1 && dest.x < over.0 + 60.0
        }
        _ => false,
    });
    assert!(ghost, "the dragged clip's frame follows the mouse");
    h.release_left();
    h.frame();
    assert!(h.state.asset_drag().is_none());
    assert_eq!(palette_has(&h, |t| t.clip.as_deref() == Some("pulse")), 1);
}

/// R101: two sprite entries sharing a fallback glyph (`brick`/`barrel`,
/// both 'b') used to count as "the same tile" to the paint tool, so
/// painting one over the other silently did nothing.
#[test]
fn r101_painting_a_sprite_over_another_with_the_same_glyph_replaces_it() {
    let dir = project("assets_r101");
    let mut h = harness_in(&dir);
    show_palette(&mut h);
    open_folder(&mut h, &["art"]);
    // Two drops import brick and barrel; each drop selects its entry.
    let a = canvas_pixel_for_grid(&h, 1, 1);
    drag_row_to(&mut h, "brick.png", a);
    let b = canvas_pixel_for_grid(&h, 9, 9);
    drag_row_to(&mut h, "barrel.png", b);
    assert_eq!(h.state.grid().get(1, 1, 1).map(|t| t.glyph), Some('b'));
    // The paint tool, barrel selected, over the brick tile.
    h.click(a.0, a.1);
    h.frame();
    let region = h.state.grid().get(1, 1, 1).and_then(|t| t.sprite.clone()).map(|s| s.region);
    assert_eq!(region.as_deref(), Some("barrel"), "the paint replaced brick with barrel");
}
