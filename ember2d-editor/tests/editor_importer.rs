// ember2d-editor/tests/editor_importer.rs — Step 8-2 (docs/ember2d-master-
// plan.md §5.7): the tileset importer end to end, headless. A real PNG sheet
// is written into a temp project; the dialog is opened on it (bypassing only
// the OS file picker), cells are named by clicking the sheet preview and
// typing, and [ Import ] must write `assets/tilesets/<name>.ron` + a copy of
// the image, add a palette entry per region, draw those entries as sprite
// thumbnails, and paint sprite tiles that the canvas draws as images.

mod common;

use common::{canvas_pixel_for_grid, ensure_workspace_root_cwd, EditorHarness};
use ember2d::input::Key;
use ember2d::renderer::draw_log::DrawOp;
use ember2d_editor::editor::importer::ImportField;
use ember2d_editor::editor::ui::WidgetId;
use ember2d_editor::editor::{EditorMode, EditorState};
use ember2d_sim::math::Rect;
use ember2d_sim::tileset::{SpriteRef, TilesetData};
use std::path::{Path, PathBuf};

/// A fresh, empty project folder unique to this test process + `tag`, with
/// a 32×16 sheet (two 16×16 cells: red, then green) named `sheet.png`.
fn project(tag: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut img = image::RgbaImage::new(32, 16);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = if x < 16 { image::Rgba([200, 30, 30, 255]) } else { image::Rgba([30, 200, 30, 255]) };
    }
    let png = dir.join("sheet.png");
    img.save(&png).unwrap();
    (dir, png)
}

fn harness_in(dir: &Path) -> EditorHarness {
    ensure_workspace_root_cwd();
    let mut state = EditorState::new(dir.join("level.level").to_str().unwrap());
    state.open_project_folder(dir.to_string_lossy().into_owned());
    let mut h = EditorHarness::with_state(state);
    h.frame();
    h
}

/// Click the center of a widget the last frame registered (points ->
/// logical, the harness's input space).
fn click_widget(h: &mut EditorHarness, id: WidgetId) {
    let r = h.state.ui_frame().rect_of(id).unwrap_or_else(|| panic!("{id:?} was not drawn"));
    let (x, y) = h.state.ui_space().to_logical(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.click(x, y);
    h.frame();
}

/// Click the sheet preview at fraction (`fx`, `fy`) of its drawn size.
fn click_sheet(h: &mut EditorHarness, fx: f32, fy: f32) {
    let r = h.state.ui_frame().rect_of(WidgetId::ImporterSheet).expect("sheet preview drawn");
    let (x, y) = h.state.ui_space().to_logical(r.x + r.w * fx, r.y + r.h * fy);
    h.click(x, y);
    h.frame();
}

fn name_both_cells(h: &mut EditorHarness) {
    click_sheet(h, 0.25, 0.5); // cell (0,0)
    h.type_text("wall");
    click_sheet(h, 0.75, 0.5); // cell (1,0)
    h.type_text("grass");
}

#[test]
fn importing_writes_the_tileset_and_adds_a_palette_entry_per_region() {
    let (dir, png) = project("import_ok");
    let mut h = harness_in(&dir);
    let palette_before = h.state.palette_tile_count();

    h.state.begin_tileset_import(&png).unwrap();
    h.frame();
    assert!(matches!(h.state.mode(), EditorMode::TilesetImport));
    name_both_cells(&mut h);
    click_widget(&mut h, WidgetId::ImporterImport);

    assert!(
        !matches!(h.state.mode(), EditorMode::TilesetImport),
        "a successful import closes the dialog"
    );
    let ron_path = dir.join("assets/tilesets/sheet.ron");
    let data: TilesetData =
        ron::de::from_str(&std::fs::read_to_string(&ron_path).unwrap()).unwrap();
    assert_eq!((data.cell_w, data.cell_h, data.columns, data.rows), (16, 16, 2, 1));
    assert_eq!(data.region_rect("wall"), Some(Rect::new(0.0, 0.0, 16.0, 16.0)));
    assert_eq!(data.region_rect("grass"), Some(Rect::new(16.0, 0.0, 16.0, 16.0)));
    assert!(dir.join("assets/tilesets/sheet.png").exists(), "the sheet is copied into the project");

    assert_eq!(h.state.palette_tile_count(), palette_before + 2);
    let sprites: Vec<_> = (0..h.state.palette_tile_count())
        .filter_map(|i| h.state.palette_tile(i).sprite.clone())
        .collect();
    assert!(sprites.contains(&SpriteRef::new("sheet", "wall")));
    assert!(sprites.contains(&SpriteRef::new("sheet", "grass")));
    assert!(dir.join("project.palette.ron").exists(), "the new entries are saved with the palette");

    // Re-importing the same sheet doesn't duplicate entries that already exist.
    h.state.begin_tileset_import(&dir.join("assets/tilesets/sheet.png")).unwrap();
    h.frame();
    click_widget(&mut h, WidgetId::ImporterImport);
    assert_eq!(h.state.palette_tile_count(), palette_before + 2);
}

#[test]
fn imported_entries_draw_as_thumbnails_and_paint_sprite_tiles_onto_the_canvas() {
    let (dir, png) = project("import_draw");
    let mut h = harness_in(&dir);
    h.state.begin_tileset_import(&png).unwrap();
    h.frame();
    name_both_cells(&mut h);
    click_widget(&mut h, WidgetId::ImporterImport);

    // Palette open: every sprite entry's row shows an image, inside its row.
    h.key(Key::B);
    h.frame();
    h.start_recording();
    h.frame();
    let space = h.state.ui_space();
    let thumbs: Vec<Rect> = h
        .draw_ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Texture { dest, src: Some(_), .. } => Some(*dest),
            _ => None,
        })
        .collect();
    let mut rows_with_thumb = 0;
    for i in 0..h.state.palette_layout_len() {
        let Some(r) = h.state.ui_frame().rect_of(WidgetId::PaletteRow(i)) else { continue };
        let row = space.rect_to_logical(Rect::new(r.x, r.y, r.w, r.h));
        if thumbs.iter().any(|t| {
            t.x >= row.x
                && t.x + t.w <= row.x + row.w + 0.5
                && t.y >= row.y - 0.5
                && t.y + t.h <= row.y + row.h + 0.5
        }) {
            rows_with_thumb += 1;
        }
    }
    assert_eq!(
        rows_with_thumb, 2,
        "both imported entries show a sprite thumbnail inside their row"
    );

    // Pick the last entry ("grass") from the palette and paint a cell.
    let last = (0..h.state.palette_layout_len())
        .rev()
        .find(|&i| h.state.ui_frame().rect_of(WidgetId::PaletteRow(i)).is_some())
        .unwrap();
    click_widget(&mut h, WidgetId::PaletteRow(last));
    h.key(Key::B); // close the palette so it isn't over the canvas
    h.frame();
    let (px, py) = canvas_pixel_for_grid(&h, 3, 3);
    h.click(px, py);
    h.frame();
    let placed = h.state.grid().tiles.values().find(|t| t.sprite.is_some()).cloned();
    let placed = placed.expect("painting with a sprite entry places a sprite tile");
    assert_eq!(placed.sprite, Some(SpriteRef::new("sheet", "grass")));

    h.start_recording();
    h.frame();
    let grass_src = Some(Rect::new(16.0, 0.0, 16.0, 16.0));
    assert!(
        h.draw_ops()
            .iter()
            .any(|op| matches!(op, DrawOp::Texture { src, .. } if *src == grass_src)),
        "the canvas draws the placed tile as its tileset region, not its glyph"
    );

    // ...and it's saved as a v5 sprite reference.
    let data = h.state.grid().to_level_data();
    assert_eq!(data.version, 5);
    assert!(data.all_tiles().iter().any(|t| t.sprite == Some(SpriteRef::new("sheet", "grass"))));
}

#[test]
fn a_bad_import_keeps_the_dialog_open_with_the_reason_and_esc_cancels() {
    let (dir, png) = project("import_bad");
    let mut h = harness_in(&dir);
    h.state.begin_tileset_import(&png).unwrap();
    h.frame();
    // Both cells the same name: rejected.
    click_sheet(&mut h, 0.25, 0.5);
    h.type_text("dup");
    click_sheet(&mut h, 0.75, 0.5);
    h.type_text("dup");
    click_widget(&mut h, WidgetId::ImporterImport);
    assert!(matches!(h.state.mode(), EditorMode::TilesetImport), "a failed import stays open");
    assert!(!dir.join("assets/tilesets/sheet.ron").exists(), "and writes nothing");

    // A cell size that fits no whole cell: the field takes digits only.
    click_widget(&mut h, WidgetId::ImporterField(ImportField::CellW));
    h.key(Key::Backspace);
    h.key(Key::Backspace);
    h.type_text("x99");
    click_widget(&mut h, WidgetId::ImporterImport);
    assert!(matches!(h.state.mode(), EditorMode::TilesetImport));

    h.key(Key::Escape);
    h.frame();
    assert!(!matches!(h.state.mode(), EditorMode::TilesetImport), "Esc cancels");
    assert!(!dir.join("assets").exists(), "cancelling leaves the project untouched");
}

#[test]
fn an_old_palette_file_without_sprites_still_loads() {
    let (dir, _) = project("old_palette");
    std::fs::write(
        dir.join("project.palette.ron"),
        "(tiles: [(name: \"Wall\", glyph: '#', fg: Grey, bg: Reset, solid: true, trigger: false, tag: \"\")], selected: 0, collapsed: [])",
    )
    .unwrap();
    let h = harness_in(&dir);
    assert_eq!(h.state.palette_tile_count(), 1);
    assert_eq!(h.state.palette_tile(0).sprite, None);
}
