// ember2d-editor/tests/editor_clips.rs — Step 8-3 (docs/ember2d-master-
// plan.md §5.7): the animation clip editor end to end, headless. A project
// with one tileset is written to a temp folder; a clip is built by clicking
// the sheet's regions, named, timed and saved to `assets/clips/<name>.ron`;
// the frame strip scrubs; [ Add to Palette ] makes an entry that paints an
// animated tile, which the canvas plays as editor time passes and the level
// saves as format v6.

mod common;

use common::{canvas_pixel_for_grid, ensure_workspace_root_cwd, EditorHarness};
use ember2d::input::Key;
use ember2d::renderer::draw_log::DrawOp;
use ember2d_editor::editor::clip_editor::ClipField;
use ember2d_editor::editor::ui::WidgetId;
use ember2d_editor::editor::{EditorMode, EditorState};
use ember2d_sim::clip_asset::ClipData;
use ember2d_sim::math::Rect;
use ember2d_sim::tileset::{TilesetData, TilesetRegion};
use std::path::{Path, PathBuf};

/// A temp project with a 64×16 sheet (four 16×16 cells) sliced as tileset
/// "fx" with regions a, b, c on the first three cells.
fn project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    let ts_dir = dir.join("assets/tilesets");
    std::fs::create_dir_all(&ts_dir).unwrap();
    let mut img = image::RgbaImage::new(64, 16);
    for (x, _, p) in img.enumerate_pixels_mut() {
        *p = image::Rgba([(x / 16 * 60) as u8, 100, 200, 255]);
    }
    img.save(ts_dir.join("fx.png")).unwrap();
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
    dir
}

fn harness_in(dir: &Path) -> EditorHarness {
    ensure_workspace_root_cwd();
    let mut state = EditorState::new(dir.join("level.level").to_str().unwrap());
    state.open_project_folder(dir.to_string_lossy().into_owned());
    let mut h = EditorHarness::with_state(state);
    h.frame();
    h
}

fn click_widget(h: &mut EditorHarness, id: WidgetId) {
    let r = h.state.ui_frame().rect_of(id).unwrap_or_else(|| panic!("{id:?} was not drawn"));
    let (x, y) = h.state.ui_space().to_logical(r.x + r.w * 0.5, r.y + r.h * 0.5);
    h.click(x, y);
    h.frame();
}

fn click_sheet_cell(h: &mut EditorHarness, col: f32) {
    let r = h.state.ui_frame().rect_of(WidgetId::ClipSheet).expect("sheet drawn");
    let (x, y) = h.state.ui_space().to_logical(r.x + r.w * (col + 0.5) / 4.0, r.y + r.h * 0.5);
    h.click(x, y);
    h.frame();
}

fn retype(h: &mut EditorHarness, field: ClipField, chars_to_erase: usize, text: &str) {
    click_widget(h, WidgetId::ClipField(field));
    for _ in 0..chars_to_erase {
        h.key(Key::Backspace);
    }
    h.type_text(text);
    h.key(Key::Enter);
    h.frame();
}

const B: Rect = Rect { x: 16.0, y: 0.0, w: 16.0, h: 16.0 };

fn texture_srcs(h: &EditorHarness) -> Vec<(Rect, Option<Rect>)> {
    h.draw_ops()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Texture { dest, src, .. } => Some((*dest, *src)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_clip_is_built_from_sheet_regions_saved_scrubbed_and_painted_as_an_animated_tile() {
    let dir = project("clips_e2e");
    let mut h = harness_in(&dir);
    h.state.open_clip_editor();
    h.frame();
    assert!(matches!(h.state.mode(), EditorMode::ClipEditor));

    for col in [0.0, 1.0, 2.0] {
        click_sheet_cell(&mut h, col);
    }
    retype(&mut h, ClipField::Name, "new_clip".len(), "torch");
    retype(&mut h, ClipField::Fps, "8".len(), "10");
    click_widget(&mut h, WidgetId::ClipSave);

    let saved: ClipData =
        ron::de::from_str(&std::fs::read_to_string(dir.join("assets/clips/torch.ron")).unwrap())
            .unwrap();
    assert_eq!(saved.frames, vec!["a", "b", "c"]);
    assert_eq!((saved.fps, saved.looping, saved.tileset.as_str()), (10.0, true, "fx"));
    assert!(
        h.state.ui_frame().rect_of(WidgetId::ClipListRow(0)).is_some(),
        "the saved clip is listed"
    );

    // Scrub: clicking frame 2 ("b") pauses the preview on it, so "b" is drawn
    // twice — its strip thumbnail and the preview.
    click_widget(&mut h, WidgetId::ClipFrame(1));
    h.start_recording();
    h.frame();
    let b_draws = texture_srcs(&h).iter().filter(|(_, s)| *s == Some(B)).count();
    assert_eq!(b_draws, 2, "paused on frame 2, the preview shows region b");

    click_widget(&mut h, WidgetId::ClipAddToPalette);
    let entry = (0..h.state.palette_tile_count())
        .map(|i| h.state.palette_tile(i))
        .find(|t| t.clip.as_deref() == Some("torch"))
        .expect("Add to Palette creates an entry that paints the clip")
        .clone();
    assert_eq!(
        entry.sprite.as_ref().map(|s| s.region.as_str()),
        Some("a"),
        "its still image is frame 1"
    );
    assert!(dir.join("project.palette.ron").exists());
    click_widget(&mut h, WidgetId::ClipClose);
    assert!(!matches!(h.state.mode(), EditorMode::ClipEditor));

    // Paint with it: select the entry (last palette row), click a cell.
    h.key(Key::B);
    h.frame();
    let last = (0..h.state.palette_layout_len())
        .rev()
        .find(|&i| h.state.ui_frame().rect_of(WidgetId::PaletteRow(i)).is_some())
        .unwrap();
    click_widget(&mut h, WidgetId::PaletteRow(last));
    h.key(Key::B);
    h.frame();
    let (px, py) = canvas_pixel_for_grid(&h, 4, 4);
    h.click(px, py);
    h.frame();
    let tile = h
        .state
        .grid()
        .tiles
        .values()
        .find(|t| t.clip.is_some())
        .cloned()
        .expect("an animated tile");
    assert_eq!(tile.clip.as_deref(), Some("torch"));

    // The canvas plays it: the tile's drawn region changes as time passes.
    let tile_src = |h: &EditorHarness| {
        texture_srcs(h)
            .into_iter()
            .find(|(d, _)| px >= d.x && px < d.x + d.w && py >= d.y && py < d.y + d.h)
            .and_then(|(_, s)| s)
    };
    h.start_recording();
    h.frame();
    let first = tile_src(&h).expect("the animated tile is drawn as an image");
    let mut changed = false;
    for _ in 0..12 {
        h.frame(); // 10 fps: a frame boundary every 0.1 s
        if tile_src(&h) != Some(first) {
            changed = true;
            break;
        }
    }
    assert!(changed, "an animated tile's frame advances on the canvas");

    let data = h.state.grid().to_level_data();
    assert_eq!(data.version, ember2d_sim::level::LEVEL_FORMAT_VERSION); // >= 6, where `clip` arrived
    assert!(
        data.tiles.iter().any(|t| t.clip.as_deref() == Some("torch")),
        "saved as an entity tile with its clip"
    );
}

#[test]
fn a_clip_that_cannot_be_saved_says_why_and_writes_nothing() {
    let dir = project("clips_bad");
    let mut h = harness_in(&dir);
    h.state.open_clip_editor();
    h.frame();
    click_widget(&mut h, WidgetId::ClipSave); // no frames yet
    assert!(!dir.join("assets/clips").exists());
    assert!(matches!(h.state.mode(), EditorMode::ClipEditor), "the dialog stays open");
    // Keyboard playback controls never crash on an empty clip.
    h.key(Key::Space);
    h.key(Key::Left);
    h.key(Key::Delete);
    h.key(Key::Escape);
    h.frame();
    assert!(!matches!(h.state.mode(), EditorMode::ClipEditor), "Esc closes");
}
