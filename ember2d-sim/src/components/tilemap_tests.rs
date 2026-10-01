// components/tilemap_tests.rs — Step 8-1 (docs/ember2d-master-plan.md
// §5.7): unit tests for `Tilemap`/`TilemapBuilder` and the level-side
// baking in level/bake.rs, kept in their own file so tilemap.rs itself
// stays readable. The integration-level "a wall answers the same whether
// it's an entity or a cell" equivalence lives in
// `ember2d/tests/tilemap_equivalence.rs`, against the real floor2.

use super::*;
use crate::level::{LevelData, TileRecord};

fn wall() -> TileDef {
    TileDef {
        glyph: '#',
        fg: Color::Grey,
        bg: Color::Reset,
        solid: true,
        tag: "wall".to_string(),
        collider_layer: String::new(),
        texture: None,
        sprite: None,
        src: None,
        name: String::new(),
    }
}

fn floor() -> TileDef {
    TileDef { glyph: '.', solid: false, tag: "floor".to_string(), ..wall() }
}

fn registry() -> LayerRegistry {
    LayerRegistry::new(&["solid".to_string(), "water".to_string()])
}

/// A 4×3 map at origin (10, 20): row 0 all wall, row 1 floor except a
/// wall at x = 13, row 2 empty. Refreshed, ready to query.
fn small_map() -> Tilemap {
    let mut b = TilemapBuilder::new((10, 20), 4, 3).unwrap();
    for x in 10..14 {
        assert!(b.add(1, x, 20, wall()));
    }
    for x in 10..13 {
        assert!(b.add(0, x, 21, floor()));
    }
    assert!(b.add(1, 13, 21, wall()));
    let mut m = b.finish().unwrap();
    m.refresh(&registry());
    m
}

#[test]
fn identical_tiles_share_one_palette_entry() {
    let m = small_map();
    assert_eq!(m.palette.len(), 2, "5 walls + 3 floors are 2 distinct defs");
    assert_eq!(m.tile_count(), 8);
    assert_eq!(
        m.layers.iter().map(|l| l.layer).collect::<Vec<_>>(),
        vec![0, 1],
        "layers ascending"
    );
}

#[test]
fn a_second_tile_in_an_occupied_cell_is_refused_not_overwritten() {
    let mut b = TilemapBuilder::new((0, 0), 2, 2).unwrap();
    assert!(b.add(1, 0, 0, wall()));
    assert!(!b.add(1, 0, 0, floor()), "the second tile must stay an entity, not replace the first");
    assert!(b.add(0, 0, 0, floor()), "a different layer in the same cell is a different slot");
    assert!(!b.add(1, 5, 5, wall()), "outside the box is refused, not wrapped");
}

#[test]
fn an_oversized_or_empty_box_builds_nothing() {
    assert!(TilemapBuilder::new((0, 0), 0, 10).is_none());
    assert!(
        TilemapBuilder::new((0, 0), 4096, 4096).is_none(),
        "past MAX_TILEMAP_CELLS must refuse rather than allocate"
    );
}

#[test]
fn solid_at_reads_cells_and_filters_by_mask() {
    let m = small_map();
    let reg = registry();
    assert!(m.solid_at(10, 20, 0));
    assert!(m.solid_at(13, 21, 0));
    assert!(!m.solid_at(11, 21, 0), "a floor is not solid");
    assert!(!m.solid_at(9, 20, 0), "outside the grid is not solid");
    assert!(m.solid_at(10, 20, reg.mask_bits(&["solid".to_string()])));
    assert!(
        !m.solid_at(10, 20, reg.mask_bits(&["water".to_string()])),
        "a mask that excludes the wall's layer must skip it"
    );
}

#[test]
fn a_solid_tile_on_an_unregistered_layer_is_still_solid_but_unmaskable() {
    let mut b = TilemapBuilder::new((0, 0), 1, 1).unwrap();
    b.add(1, 0, 0, TileDef { collider_layer: "lava".to_string(), ..wall() });
    let mut m = b.finish().unwrap();
    m.refresh(&registry());
    assert!(
        m.solid_at(0, 0, 0),
        "an unfiltered query must still see it — same as a Collider on an unknown layer"
    );
    assert_eq!(m.solid_bits_at(0, 0), Some(0));
    assert!(!m.solid_at(0, 0, registry().mask_bits(&["solid".to_string()])));
}

#[test]
fn overlapping_cells_come_back_row_major_and_clamped() {
    let m = small_map();
    let cells = m.overlapping_solid_cells(Rect::new(12.5, 20.5, 1.0, 1.0), 0);
    assert_eq!(cells, vec![(12, 20), (13, 20), (13, 21)]);
    // A billion-cell rect only walks the grid it overlaps.
    let all = m.overlapping_solid_cells(Rect::new(-1e9, -1e9, 2e9, 2e9), 0);
    assert_eq!(all.len(), 5);
    assert!(m.any_solid_in(Rect::new(13.2, 21.2, 0.5, 0.5), 0));
    assert!(!m.any_solid_in(Rect::new(11.2, 21.2, 0.5, 0.5), 0), "floor only");
}

#[test]
fn nan_rects_and_rays_hit_nothing() {
    let m = small_map();
    assert!(!m.any_solid_in(Rect::new(f32::NAN, 20.0, 1.0, 1.0), 0));
    assert!(m.overlapping_solid_cells(Rect::new(f32::NAN, f32::NAN, 1.0, 1.0), 0).is_empty());
    assert_eq!(m.raycast(f32::NAN, 20.5, 5.0, 0.0, 0), None);
}

#[test]
fn raycast_returns_the_nearest_cell_by_the_same_math_as_a_collider() {
    let m = small_map();
    // From (11.5, 22.5) straight up: row 2 is empty, (11, 21) is floor,
    // first solid is (11, 20) whose bottom edge is at y = 21.
    let t = m.raycast(11.5, 22.5, 0.0, -2.0, 0).expect("must hit the top wall row");
    let expected = Rect::new(11.0, 20.0, 1.0, 1.0).ray_intersects(11.5, 22.5, 0.0, -2.0).unwrap();
    assert_eq!(t, expected);
    assert_eq!(m.raycast(11.5, 22.5, 0.0, 0.4, 0), None, "a ray moving away hits nothing");
}

#[test]
fn tag_at_prefers_the_topmost_tagged_layer() {
    let m = small_map();
    assert_eq!(m.tag_at(10, 20), "wall");
    assert_eq!(m.tag_at(11, 21), "floor");
    assert_eq!(m.tag_at(11, 22), "", "an empty cell has no tag");
}

#[test]
fn a_deserialized_map_is_non_solid_until_refreshed() {
    let text = ron::ser::to_string(&small_map()).unwrap();
    let mut back: Tilemap = ron::de::from_str(&text).unwrap();
    assert!(
        !back.solid_at(10, 20, 0),
        "the caches are #[serde(skip)] — this is why refresh exists"
    );
    back.refresh(&registry());
    assert!(back.solid_at(10, 20, 0));
    assert_eq!(back.tile_count(), 8);
}

// ── level/bake.rs ────────────────────────────────────────────────────────────

fn mixed_level() -> LevelData {
    let mut data = LevelData::empty(8, 8);
    for x in 0..4 {
        data.tiles.push(TileRecord::new(
            x,
            0,
            1,
            '#',
            Color::Grey,
            Color::Reset,
            true,
            false,
            "wall",
        ));
        data.tiles.push(TileRecord::new(
            x,
            1,
            0,
            '.',
            Color::DarkGrey,
            Color::Reset,
            false,
            false,
            "floor",
        ));
    }
    let mut stairs =
        TileRecord::new(2, 1, 1, '>', Color::Yellow, Color::Reset, false, true, "stairs");
    stairs.next_level = Some("floor2.level".to_string());
    data.tiles.push(stairs);
    let mut masked = TileRecord::new(3, 3, 1, '~', Color::Blue, Color::Reset, true, false, "");
    masked.collider_mask = vec!["solid".to_string()];
    data.tiles.push(masked);
    data.tiles.sort_by_key(|t| (t.layer, t.y, t.x));
    data
}

#[test]
fn is_static_keeps_every_interactive_kind_of_tile_an_entity() {
    let plain = TileRecord::new(0, 0, 1, '#', Color::Grey, Color::Reset, true, false, "wall");
    assert!(
        plain.is_static(),
        "a tag alone doesn't make a tile interactive (8-1's scoping decision)"
    );
    let mut t = plain.clone();
    t.trigger = true;
    assert!(!t.is_static());
    let mut t = plain.clone();
    t.script = Some("x.rhai".to_string());
    assert!(!t.is_static());
    let mut t = plain.clone();
    t.next_level = Some("x.level".to_string());
    assert!(!t.is_static());
    let mut t = plain.clone();
    t.actor = Some(Default::default());
    assert!(!t.is_static());
    let mut t = plain.clone();
    t.collider_mask = vec!["solid".to_string()];
    assert!(!t.is_static());
    let mut t = plain;
    t.camera_follow = true;
    assert!(!t.is_static());
}

#[test]
fn baking_is_lossless_and_idempotent() {
    let original = mixed_level();
    let before = format!("{:?}", original.all_tiles());

    let mut baked = original.clone();
    baked.bake_tilemap();
    assert_eq!(baked.tiles.len(), 2, "only the stairs and the masked tile stay entities");
    assert_eq!(baked.tilemap.as_ref().map(|m| m.tile_count()), Some(8));
    assert_eq!(format!("{:?}", baked.all_tiles()), before, "all_tiles must be unchanged by baking");

    let once = format!("{:?}", baked);
    baked.bake_tilemap();
    assert_eq!(format!("{:?}", baked), once, "baking an already-baked level must change nothing");
}

#[test]
fn a_baked_level_round_trips_through_ron() {
    let mut data = mixed_level();
    data.bake_tilemap();
    let text =
        ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::new().depth_limit(4)).unwrap();
    let back: LevelData = ron::de::from_str(&text).unwrap();
    assert_eq!(format!("{:?}", back.all_tiles()), format!("{:?}", data.all_tiles()));
}

#[test]
fn split_static_collapses_a_v3_level_and_keeps_entity_tiles_in_order() {
    // Unbaked (a v3 file): static tiles are in `tiles`, and still collapse.
    let data = mixed_level();
    let (map, entities) = data.split_static();
    assert_eq!(map.map(|m| m.tile_count()), Some(8));
    let glyphs: Vec<char> = entities.iter().map(|t| t.glyph).collect();
    assert_eq!(glyphs, vec!['>', '~'], "entity tiles in level order");

    // Baked (v4): the same split, from the tilemap section instead.
    let mut baked = mixed_level();
    baked.bake_tilemap();
    let (map2, entities2) = baked.split_static();
    assert_eq!(map2.map(|m| m.tile_count()), Some(8));
    assert_eq!(entities2.iter().map(|t| t.glyph).collect::<Vec<_>>(), glyphs);
}

#[test]
fn static_tiles_too_far_apart_to_grid_all_stay_entities() {
    let mut data = LevelData::empty(8, 8);
    data.tiles.push(TileRecord::new(0, 0, 1, '#', Color::Grey, Color::Reset, true, false, ""));
    data.tiles.push(TileRecord::new(
        100_000,
        100_000,
        1,
        '#',
        Color::Grey,
        Color::Reset,
        true,
        false,
        "",
    ));
    let (map, entities) = data.split_static();
    assert!(map.is_none(), "a 10-billion-cell bounding box must not be allocated");
    assert_eq!(entities.len(), 2, "…and nothing is lost: both stay entities");
}

#[test]
fn baking_keeps_a_tiles_sprite_ref_and_treats_it_as_part_of_the_defs_identity() {
    // Step 8-2: two walls identical except for which tileset region draws
    // them are two different defs, and the reference survives the bake.
    use crate::tileset::SpriteRef;
    let mut data = LevelData::empty(4, 4);
    let mut a = TileRecord::new(0, 0, 1, '#', Color::Grey, Color::Reset, true, false, "wall");
    a.sprite = Some(SpriteRef::new("dungeon", "wall"));
    let mut b = a.clone();
    b.x = 1;
    b.sprite = Some(SpriteRef::new("dungeon", "wall_cracked"));
    data.tiles = vec![a, b];
    let before = format!("{:?}", data.all_tiles());
    data.bake_tilemap();
    let map = data.tilemap.as_ref().unwrap();
    assert_eq!(map.palette.len(), 2);
    assert_eq!(format!("{:?}", data.all_tiles()), before, "sprite refs must survive baking");
    let text = ron::ser::to_string(&data).unwrap();
    let back: LevelData = ron::de::from_str(&text).unwrap();
    assert_eq!(format!("{:?}", back.all_tiles()), before);
}

#[test]
fn editing_cells_interns_defs_and_growing_keeps_every_tile_in_place() {
    // Step 9.5-1: what a script's tile_set/tile_clear/growth do to a map.
    let mut map = Tilemap::new((2, 3), 4, 2);
    let w = map.intern(&wall()).unwrap();
    assert_eq!(map.intern(&wall()), Some(w), "an existing def is reused");
    let f = map.intern(&floor()).unwrap();
    assert!(map.set_cell(1, 2, 3, w));
    assert!(map.set_cell(0, 5, 4, f));
    assert!(!map.set_cell(1, 0, 0, w), "outside the grid: nothing placed");
    assert!(map.set_cell(2, 3, 3, 0), "clearing an empty layer is fine and adds no layer");
    assert_eq!(map.layers.iter().map(|l| l.layer).collect::<Vec<_>>(), vec![0, 1], "layers stay sorted");

    assert!(map.grow_to_cover(0, 0, 10, 8));
    assert_eq!((map.origin, map.width, map.height), ((0, 0), 10, 8));
    map.refresh(&registry());
    assert!(map.solid_at(2, 3, 0), "the wall is where it was");
    assert_eq!(map.tag_at(5, 4), "floor", "and so is the floor");
    assert_eq!(map.tile_count(), 2);
    assert!(map.grow_to_cover(1, 1, 2, 2), "already covered: nothing changes");
    assert_eq!((map.origin, map.width, map.height), ((0, 0), 10, 8));
    assert!(!map.grow_to_cover(0, 0, 5000, 5000), "past MAX_TILEMAP_CELLS: refused");

    assert!(map.clear_cell(2, 3));
    map.refresh(&registry());
    assert!(!map.solid_at(2, 3, 0));
}
