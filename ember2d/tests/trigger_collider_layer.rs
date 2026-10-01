// Regression test for defect D4 (docs/ember2d-refactor-plan.md §3):
// a trigger tile with no explicit collider_layer used to default to layer
// "solid", corrupting mask-based collision filtering against real obstacles.

use ember2d::prelude::*;
use std::collections::BTreeMap;

#[test]
fn trigger_tile_without_collider_layer_stays_unlabeled() {
    let mut data = LevelData::empty(10, 10);
    data.tiles.push(TileRecord::new(
        2,
        2,
        1,
        '*',
        Color::Yellow,
        Color::Reset,
        false,
        true,
        "item",
    ));

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();

    play.on_start(&mut world, &mut events, 10, 10, &mut persistent);

    let item_id = world.find_by_tag("item").expect("item entity should have spawned");
    let collider = world.colliders.get(&item_id).expect("item should have a trigger collider");
    assert!(!collider.solid, "item tile should be a non-solid trigger");
    assert_eq!(
        collider.layer(),
        "",
        "an unlabeled trigger must not default to the \"solid\" layer"
    );
}

#[test]
fn solid_tile_without_collider_layer_still_defaults_to_solid() {
    let mut data = LevelData::empty(10, 10);
    data.tiles.push(TileRecord::new(3, 3, 1, '#', Color::Grey, Color::Reset, true, false, "wall"));
    // Step 8-1 (docs/ember2d-master-plan.md §5.7): a plain wall is static,
    // so it's a `Tilemap` cell now, not an entity with a `Collider` — its
    // default layer is checked there. An actor wall (a solid tile that
    // stays an entity, like an enemy) still takes the entity path, and is
    // checked the way this test always checked a wall.
    let mut actor_wall =
        TileRecord::new(5, 5, 1, 'B', Color::Grey, Color::Reset, true, false, "blocker");
    actor_wall.actor = Some(Default::default());
    data.tiles.push(actor_wall);

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();

    play.on_start(&mut world, &mut events, 10, 10, &mut persistent);

    let map = world.tilemaps.values().next().expect("the static wall should be a tilemap cell");
    let solid_bit =
        ember2d_sim::layers::LayerRegistry::new(&["solid".to_string()]).bit_for("solid");
    assert_eq!(
        map.solid_bits_at(3, 3),
        Some(solid_bit),
        "an unlabeled solid cell should still default to the \"solid\" layer"
    );

    let wall_id = world.find_by_tag("blocker").expect("actor wall entity should have spawned");
    let collider = world.colliders.get(&wall_id).expect("wall should have a solid collider");
    assert!(collider.solid);
    assert_eq!(
        collider.layer(),
        "solid",
        "an unlabeled solid tile should still default to the \"solid\" layer"
    );
}
