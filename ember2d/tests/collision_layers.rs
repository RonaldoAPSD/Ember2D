// tests/collision_layers.rs — Phase 6 Step 7 (docs/ember2d-phase6-plan.md):
// the collision-layer bitmask's actual filtering behavior, exercised through
// a real Simulation/World, plus the one thing that fails completely
// silently if forgotten: a save -> load -> still-filters round trip.
//
// `Collider::layer_bits`/`mask_bits` are `#[serde(skip)]` — a `Collider`
// that just came out of RON deserialization has its `layer`/`mask` STRINGS
// intact but its bits zeroed. `Simulation::on_start`'s `is_loading_save`
// branch calls `World::refresh_collider_bits` specifically to fix that; if
// a future edit ever drops that call, every loaded save's collision masks
// would silently start "matching everything" (`mask_bits == 0` is that
// encoding's OWN "matches everything" value — see
// `ember2d_sim::layers::LayerRegistry::mask_bits`'s doc comment) instead of
// erroring or panicking. `bits_survive_a_save_load_round_trip` below is the
// test that would have failed before that call existed.
//
// EVERY TILE BELOW SITS AT THE SAME CELL. Every pair physically overlaps,
// so any pair that does NOT emit a Collision event was excluded by layer/
// mask filtering specifically, not by geometry — that isolates exactly the
// thing this file exists to check.

use std::collections::BTreeMap;
use ember2d::prelude::*;
use ember2d_sim::event::GameEvent;
use ember2d_sim::save::SaveState;

/// A non-solid trigger so nothing here is ever physically blocked or moved
/// by `late_step`'s solid-collision resolver — this file is purely about
/// which pairs raise a `Collision` event.
fn overlapping_tile(tag: &str, layer: &str, mask: Vec<String>) -> TileRecord {
    let mut t = TileRecord::new(0, 0, 1, '*', Color::White, Color::Reset, false, true, tag);
    t.collider_layer = layer.to_string();
    t.collider_mask = mask;
    t
}

/// Three real layers plus one name deliberately never registered
/// ("ghost_layer" isn't in `collision_layers`), so `ghost`'s mask below
/// exercises the "unregistered mask entry" case directly.
fn build_level() -> LevelData {
    let mut data = LevelData::empty(10, 10);
    data.collision_layers = vec!["solid".to_string(), "enemy".to_string(), "pickup".to_string()];
    data.tiles = vec![
        overlapping_tile("wall",  "solid",  vec![]),                              // empty mask: matches everything
        overlapping_tile("rat",   "enemy",  vec!["solid".to_string()]),           // wants only solids
        overlapping_tile("gold",  "pickup", vec!["enemy".to_string()]),           // wants only enemies, not solids
        overlapping_tile("ghost", "pickup", vec!["ghost_layer".to_string()]),     // wants an UNREGISTERED layer
    ];
    data
}

fn spawn_and_detect(data: LevelData) -> (World, EventBus) {
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 10, 10, &mut persistent);

    let mut collisions = EventBus::new();
    world.detect_collisions(&mut collisions);
    (world, collisions)
}

fn collided(events: &EventBus, world: &World, tag_a: &str, tag_b: &str) -> bool {
    let a = world.find_by_tag(tag_a).unwrap_or_else(|| panic!("{} should have spawned", tag_a));
    let b = world.find_by_tag(tag_b).unwrap_or_else(|| panic!("{} should have spawned", tag_b));
    events.events().iter().any(|e| matches!(e,
        GameEvent::Collision { entity_a, entity_b }
        if (*entity_a == a && *entity_b == b) || (*entity_a == b && *entity_b == a)
    ))
}

#[test]
fn a_mask_naming_a_registered_layer_matches_it() {
    let (world, events) = spawn_and_detect(build_level());
    assert!(collided(&events, &world, "wall", "rat"), "rat's mask [\"solid\"] should match wall's \"solid\" layer");
}

#[test]
fn a_mask_naming_only_other_layers_excludes_a_non_matching_pair() {
    let (world, events) = spawn_and_detect(build_level());
    assert!(!collided(&events, &world, "wall", "gold"), "gold's mask [\"enemy\"] must not match wall's \"solid\" layer");
}

#[test]
fn an_empty_mask_matches_everything_regardless_of_the_other_layer() {
    // The pairwise test is an AND across both sides (see
    // World::detect_collisions), so proving "empty mask matches everything"
    // as a standalone property needs BOTH sides empty — a non-empty mask on
    // the OTHER side can still veto the pair (see the exclusion test above,
    // where wall's empty mask doesn't save the wall/gold pair from gold's
    // own non-matching one). This tile's layer name isn't even registered,
    // proving the empty-mask side of the check ignores layer_bits entirely
    // rather than merely tolerating an unlucky match.
    let mut data = build_level();
    data.tiles.push(overlapping_tile("puddle", "unregistered_layer_name", vec![]));
    let (world, events) = spawn_and_detect(data);
    assert!(
        collided(&events, &world, "wall", "puddle"),
        "two empty-mask colliders must always match, even when one's own layer name isn't registered at all"
    );
}

#[test]
fn a_mask_naming_an_unregistered_layer_matches_nothing_not_everything() {
    let (world, events) = spawn_and_detect(build_level());
    // ghost's mask names "ghost_layer", which is not in collision_layers —
    // LayerRegistry::mask_bits resolves that to LAYER_UNKNOWN (bit 31), a
    // bit no real collider's layer_bits ever has, rather than to 0 (which
    // would mean "matches everything" — the opposite of what a filtering
    // mask naming a specific layer should ever do on a typo or an
    // authored-before-registered name). ghost must therefore collide with
    // NOTHING here, despite physically overlapping all three other tiles.
    assert!(!collided(&events, &world, "ghost", "wall"), "an unregistered-layer mask must not accidentally match \"solid\"");
    assert!(!collided(&events, &world, "ghost", "rat"), "an unregistered-layer mask must not accidentally match \"enemy\"");
    assert!(!collided(&events, &world, "ghost", "gold"), "an unregistered-layer mask must not accidentally match \"pickup\"");
}

/// The mandatory round trip (see this file's header comment). Builds the
/// same level, saves the resulting `World` through a REAL RON string (not
/// just an in-memory clone — `Collider`'s `#[serde(skip)]` bits only
/// actually zero out through real (de)serialization), reloads it, and
/// confirms every pairwise filtering result from before the round trip
/// still holds after it.
#[test]
fn collider_bits_survive_a_save_load_round_trip() {
    let data = build_level();
    let mut play = PlayState::from_level(data.clone(), BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 10, 10, &mut persistent);

    // Sanity: the pre-save behavior matches every other test in this file
    // (same level, same assertions) before we trust the post-load side of
    // the comparison.
    let mut before = EventBus::new();
    world.detect_collisions(&mut before);
    assert!(collided(&before, &world, "wall", "rat"));
    assert!(!collided(&before, &world, "wall", "gold"));
    assert!(!collided(&before, &world, "ghost", "wall"));

    // A REAL RON round trip — SaveState::to_ron/from_ron, not
    // std::mem::clone — is what actually exercises `#[serde(skip)]`.
    let save = SaveState::new(world.clone(), persistent.clone(), play.globals().clone(), play.clips().clone(), "unused.level".to_string());
    let ron = save.to_ron().expect("SaveState must serialize");
    let restored = SaveState::from_ron(&ron).expect("SaveState must deserialize");
    let mut loaded_world = restored.world;

    // Same LevelData (same collision_layers), is_loading_save = true this
    // time — this is PlayState::from_save's whole contract, matching
    // ember2d-app/src/app.rs's real load-game flow exactly: the level is
    // reloaded for its structural data (here, just cloned instead of
    // re-read from disk, since this test never wrote a .level file) while
    // the entities themselves come from the restored World, not a fresh
    // do_on_start spawn.
    let mut loaded_play = PlayState::from_save(data, restored.persistent, restored.globals, restored.clips);
    let mut loaded_persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    let mut loaded_events = EventBus::new();
    loaded_play.on_start(&mut loaded_world, &mut loaded_events, 10, 10, &mut loaded_persistent);

    let mut after = EventBus::new();
    loaded_world.detect_collisions(&mut after);

    // The exact regression this test exists to catch: if
    // World::refresh_collider_bits were never called (or silently removed
    // by a future edit), every collider's mask_bits would read 0 —
    // indistinguishable from "matches everything" — and gold/wall,
    // normally excluded by gold's own real mask, would suddenly collide.
    assert!(collided(&after, &loaded_world, "wall", "rat"), "rat must still match wall's \"solid\" layer after a save/load round trip");
    assert!(!collided(&after, &loaded_world, "wall", "gold"), "gold's mask must still exclude wall after a save/load round trip -- a bare 0 here means the bits were never refreshed");
    assert!(!collided(&after, &loaded_world, "ghost", "wall"), "an unregistered-layer mask must still match nothing after a save/load round trip");
}
