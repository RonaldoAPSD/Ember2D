// simulation/tileset_spawn_tests.rs — Step 8-2 (docs/ember2d-master-plan.md
// §5.7): a tile's `SpriteRef` (tileset + region name) resolves to the sheet
// image + region rect at level load, for tiles that stay entities and for
// cells baked into the tilemap alike; a broken reference warns once and
// falls back to the glyph. Driven through the real `Simulation::on_start`
// with an in-memory `LevelSource`.

use super::*;
use crate::color::Color;
use crate::components::SpriteSource;
use crate::level::{LevelData, TileRecord};
use crate::level_source::LevelSource;
use crate::math::Rect;
use crate::tileset::{SpriteRef, TilesetData, TilesetRegion};
use std::collections::BTreeMap as Map;

/// In-memory files, keyed by '/'-separated path (`std::path::Path::join`
/// produces '\\' on Windows; normalized here so tests read the same on
/// every OS).
struct Files(Map<String, String>);

fn norm(p: &str) -> String {
    p.replace('\\', "/")
}

impl LevelSource for Files {
    fn exists(&self, path: &str) -> bool {
        self.0.contains_key(&norm(path))
    }
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        self.0.get(&norm(path)).cloned().ok_or_else(|| format!("not found: {path}"))
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        Err(format!("unused: {path}"))
    }
}

fn dungeon() -> TilesetData {
    TilesetData {
        name: "dungeon".to_string(),
        image: "dungeon.png".to_string(),
        cell_w: 16,
        cell_h: 16,
        margin: 0,
        spacing: 0,
        columns: 4,
        rows: 4,
        regions: vec![
            TilesetRegion { name: "wall".to_string(), col: 1, row: 0, w: 1, h: 1 },
            TilesetRegion { name: "door".to_string(), col: 2, row: 3, w: 1, h: 1 },
        ],
    }
}

/// The level sits one folder BELOW the project root, so the tileset is only
/// found by climbing (`proj/levels/a.level` -> `proj/assets/tilesets/`).
fn project_files() -> Files {
    let mut m = Map::new();
    m.insert(
        "proj/assets/tilesets/dungeon.ron".to_string(),
        ron::ser::to_string(&dungeon()).unwrap(),
    );
    Files(m)
}

fn level_with(tiles: Vec<TileRecord>) -> LevelData {
    let mut level = LevelData::empty(10, 10);
    level.path = "proj/levels/a.level".to_string();
    level.tiles = tiles;
    level
}

fn sprite_tile(x: i32, region: &str, trigger: bool) -> TileRecord {
    let mut t = TileRecord::new(x, 0, 1, '#', Color::Grey, Color::Reset, !trigger, trigger, "");
    t.sprite = Some(SpriteRef::new("dungeon", region));
    t
}

fn start(level: LevelData, files: Files) -> (World, Vec<String>) {
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(files));
    let mut world = World::new();
    let mut persistent = Map::new();
    let logs = sim.on_start(&mut world, 10, 10, &mut persistent);
    (world, logs.into_iter().map(|l| l.text).collect())
}

#[test]
fn an_entity_tiles_sprite_resolves_to_the_sheet_image_and_region_rect_at_one_cell() {
    // trigger: true keeps it an entity (TileRecord::is_static).
    let (world, logs) = start(level_with(vec![sprite_tile(3, "door", true)]), project_files());
    assert!(logs.iter().all(|l| !l.contains("Tile sprite")), "no warning expected: {logs:?}");
    let sprite = world.sprites.values().find(|s| s.layer == 10).expect("the tile's sprite");
    match &sprite.source {
        SpriteSource::Texture { path, src } => {
            assert_eq!(norm(path), "proj/assets/tilesets/dungeon.png");
            assert_eq!(*src, Some(Rect::new(32.0, 48.0, 16.0, 16.0)), "door is cell (2,3)");
        }
        other => panic!("expected a texture sprite, got {other:?}"),
    }
    assert_eq!(sprite.size, Some(Vec2::new(1.0, 1.0)), "drawn at one cell, like its glyph");
}

#[test]
fn baked_tilemap_cells_resolve_their_sprite_too() {
    // solid, no trigger: static, so both collapse into the tilemap.
    let tiles = vec![sprite_tile(0, "wall", false), sprite_tile(1, "wall", false)];
    let (world, logs) = start(level_with(tiles), project_files());
    assert!(logs.iter().all(|l| !l.contains("Tile sprite")), "{logs:?}");
    let map = world.tilemaps.values().next().expect("static tiles collapse");
    assert_eq!(map.palette.len(), 1, "two identical sprite walls share one def");
    let def = &map.palette[0];
    assert_eq!(def.src, Some(Rect::new(16.0, 0.0, 16.0, 16.0)));
    assert_eq!(norm(def.texture.as_deref().unwrap()), "proj/assets/tilesets/dungeon.png");
    let cells = map.visible_cells(None);
    assert!(cells.iter().all(|c| c.5), "every cell is flagged as a one-cell sprite");
    assert!(matches!(cells[0].3, SpriteSource::Texture { src: Some(_), .. }));
    assert!(map.solid_at(0, 0, 0), "a sprite wall is still a wall");
}

#[test]
fn a_missing_tileset_warns_once_and_keeps_the_glyph() {
    let tiles = vec![sprite_tile(0, "wall", true), sprite_tile(1, "wall", true)];
    let (world, logs) = start(level_with(tiles), Files(Map::new()));
    let warnings: Vec<_> = logs.iter().filter(|l| l.contains("Tile sprite dungeon/wall")).collect();
    assert_eq!(warnings.len(), 1, "one broken reference, one warning: {logs:?}");
    assert!(warnings[0].contains("not found"));
    for s in world.sprites.values().filter(|s| s.layer == 10) {
        assert!(matches!(s.source, SpriteSource::Glyph { ch: '#', .. }), "falls back to the glyph");
    }
}

#[test]
fn a_missing_region_names_the_region_in_its_warning() {
    let (_, logs) = start(level_with(vec![sprite_tile(0, "lava", true)]), project_files());
    assert!(
        logs.iter().any(|l| l.contains("no region named 'lava'")),
        "the warning must say which region is missing: {logs:?}"
    );
}

#[test]
fn an_invalid_tileset_file_is_reported_not_trusted() {
    let mut bad = dungeon();
    bad.regions[0].col = 9; // off the 4x4 grid
    let mut m = Map::new();
    m.insert("proj/assets/tilesets/dungeon.ron".to_string(), ron::ser::to_string(&bad).unwrap());
    let (_, logs) = start(level_with(vec![sprite_tile(0, "wall", true)]), Files(m));
    assert!(logs.iter().any(|l| l.contains("runs off")), "{logs:?}");
}

// ── Step 8-3: animated tiles ────────────────────────────────────────────────

fn files_with_torch_clip() -> Files {
    let mut f = project_files();
    let clip = crate::clip_asset::ClipData {
        name: "torch".to_string(),
        tileset: "dungeon".to_string(),
        frames: vec!["wall".to_string(), "door".to_string()],
        fps: 4.0,
        looping: true,
    };
    f.0.insert("proj/assets/clips/torch.ron".to_string(), ron::ser::to_string(&clip).unwrap());
    f
}

fn clip_tile(x: i32, clip: &str) -> TileRecord {
    let mut t = TileRecord::new(x, 0, 1, '*', Color::Yellow, Color::Reset, false, false, "");
    t.clip = Some(clip.to_string());
    t
}

#[test]
fn an_animated_tile_plays_its_project_clip_as_a_rects_animation() {
    let level = level_with(vec![clip_tile(2, "torch")]);
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(files_with_torch_clip()));
    let mut world = World::new();
    let mut persistent = Map::new();
    let logs = sim.on_start(&mut world, 10, 10, &mut persistent);
    assert!(logs.iter().all(|l| !l.text.contains("Tile animation")), "{logs:?}");

    assert!(world.tilemaps.is_empty(), "an animated tile is never collapsed into the tilemap");
    let (&id, sprite) = world.sprites.iter().find(|(_, s)| s.layer == 10).expect("the tile");
    assert!(matches!(&sprite.source, SpriteSource::Clip { name } if name == "torch"));
    assert_eq!(sprite.size, Some(Vec2::new(1.0, 1.0)));
    assert!(world.animators.get(&id).is_some_and(|a| a.clip == "torch" && a.playing));

    let clip = sim.clips().get("torch").expect("the clip is in the simulation's clip table");
    assert_eq!((clip.fps, clip.looping), (4.0, true));
    match &clip.frames {
        crate::components::ClipFrames::Rects { texture, frames } => {
            assert_eq!(norm(texture), "proj/assets/tilesets/dungeon.png");
            assert_eq!(
                frames,
                &vec![Rect::new(16.0, 0.0, 16.0, 16.0), Rect::new(32.0, 48.0, 16.0, 16.0)]
            );
        }
        other => panic!("expected a Rects clip, got {other:?}"),
    }
}

#[test]
fn an_animated_tiles_frame_advances_with_simulation_steps() {
    use crate::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
    use crate::simulation::StepInput;
    let level = level_with(vec![clip_tile(2, "torch")]);
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(files_with_torch_clip()));
    let mut world = World::new();
    let mut persistent = Map::new();
    sim.on_start(&mut world, 10, 10, &mut persistent);
    let id = *world.animators.keys().next().unwrap();
    let input = InputSnapshot::default();
    let gamepad = GamepadSnapshot::default();
    // 4 fps: 0.3 s of 1/60 steps crosses one frame boundary.
    for i in 0..18 {
        sim.step(
            &mut world,
            StepInput {
                input: &input,
                mouse: MouseSnapshot::default(),
                gamepad: &gamepad,
                external_commands: &[],
                animating: &[],
                camera_origin: Vec2::ZERO,
                sim_dt: 1.0 / 60.0,
                elapsed: i as f32 / 60.0,
                viewport_w: 10,
                viewport_h: 10,
            },
            &mut persistent,
        );
    }
    assert_eq!(world.animators[&id].frame, 1, "0.3 s at 4 fps is frame 1");
}

#[test]
fn a_missing_clip_warns_once_and_the_tile_stays_unanimated() {
    let level = level_with(vec![clip_tile(1, "flag"), clip_tile(2, "flag")]);
    let (world, logs) = start(level, files_with_torch_clip());
    let warnings: Vec<_> = logs.iter().filter(|l| l.contains("Tile animation 'flag'")).collect();
    assert_eq!(warnings.len(), 1, "{logs:?}");
    assert!(world.animators.is_empty());
    assert!(world
        .sprites
        .values()
        .filter(|s| s.layer == 10)
        .all(|s| matches!(s.source, SpriteSource::Glyph { ch: '*', .. })));
}
