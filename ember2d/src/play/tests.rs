// play/tests.rs — PlayState unit tests.
//
// Split out of play.rs (via the sibling-directory `mod tests;` convention
// already used for `mod spawn;`) once play.rs approached the project's
// 600-line hard limit — see CLAUDE.md "Development Rules". This file's
// content is `play::tests`, with full access to play.rs's private items
// via `use super::*`.

use super::*;
use ember2d_sim::components::{Sprite, Transform};

// ── Tests: D5 draw order, D13 viewport culling (ember2d-refactor-plan.md §3) ───

fn spawn_at(world: &mut World, x: f32, y: f32, z: i32, glyph: char) -> EntityId {
    let id = world.spawn();
    world.add_transform(id, Transform::new(x, y));
    world.add_sprite(id, Sprite::new(glyph, Color::White, Color::Reset, z));
    id
}

fn spawn_textured_at(world: &mut World, z: i32, texture: &str) -> EntityId {
    let id = world.spawn();
    world.add_transform(id, Transform::new(0.0, 0.0));
    world.add_sprite(id, Sprite::new('T', Color::White, Color::Reset, z).with_texture(texture));
    id
}

#[test]
fn equal_z_entities_sort_by_entity_id() {
    let mut world = World::new();
    let a = spawn_at(&mut world, 5.0, 5.0, 3, 'a');
    let b = spawn_at(&mut world, 5.0, 5.0, 3, 'b');
    let c = spawn_at(&mut world, 5.0, 5.0, 3, 'c');

    let list = DrawList::from_world(&world);
    let ids: Vec<EntityId> = list.commands.iter().map(|c| c.id).collect();
    assert_eq!(ids, vec![a, b, c], "entities sharing a z_order must draw in a stable, id-ordered sequence");
}

#[test]
fn draw_order_is_stable_across_repeated_calls() {
    let mut world = World::new();
    for i in 0..20 { spawn_at(&mut world, i as f32, 0.0, i % 3, 'x'); }

    let first:  Vec<EntityId> = DrawList::from_world(&world).commands.iter().map(|c| c.id).collect();
    let second: Vec<EntityId> = DrawList::from_world(&world).commands.iter().map(|c| c.id).collect();
    assert_eq!(first, second, "repeated calls over the same world must produce the same draw order");
}

#[test]
fn lower_z_still_sorts_first_regardless_of_id() {
    let mut world = World::new();
    let high_id_low_z = spawn_at(&mut world, 0.0, 0.0, 0, 'a');
    let _ = spawn_at(&mut world, 0.0, 0.0, 5, 'b'); // higher z, spawned after
    let list = DrawList::from_world(&world);
    assert_eq!(list.commands.first().map(|c| c.id), Some(high_id_low_z), "z_order must still take priority over the id tiebreak");
}

#[test]
fn invisible_sprites_are_excluded() {
    let mut world = World::new();
    let id = spawn_at(&mut world, 1.0, 1.0, 0, 'x');
    world.sprites.get_mut(&id).unwrap().visible = false;
    assert!(DrawList::from_world(&world).commands.is_empty());
}

#[test]
fn same_texture_entries_land_adjacent_even_when_spawned_interleaved() {
    // Step 2d's whole point: WgpuBackend's ensure_batch only merges
    // *consecutive* same-texture instances, so an interleaved list
    // degenerates to one draw call per sprite. Sorting by texture (with z
    // and id as further tiebreaks, all equal here) must group every command
    // sharing a texture into one contiguous run.
    let mut world = World::new();
    let g1 = spawn_at(&mut world, 0.0, 0.0, 0, 'a');          // glyph (texture: None)
    let t1 = spawn_textured_at(&mut world, 0, "a.png");
    let g2 = spawn_at(&mut world, 0.0, 0.0, 0, 'b');          // glyph (texture: None)
    let t2 = spawn_textured_at(&mut world, 0, "a.png");
    let t3 = spawn_textured_at(&mut world, 0, "b.png");

    let list = DrawList::from_world(&world);
    let ids: Vec<EntityId> = list.commands.iter().map(|c| c.id).collect();
    // None (glyphs) sorts before Some(_), then "a.png" before "b.png";
    // id is the tiebreak within each texture group.
    assert_eq!(ids, vec![g1, g2, t1, t2, t3]);
}

#[test]
fn in_viewport_accepts_cells_within_bounds() {
    assert!(in_viewport(0, 0, 80, 24));
    assert!(in_viewport(79, 22, 80, 24));
}

#[test]
fn in_viewport_rejects_negative_coordinates() {
    assert!(!in_viewport(-1, 5, 80, 24));
    assert!(!in_viewport(5, -1, 80, 24));
}

#[test]
fn in_viewport_rejects_past_the_right_and_bottom_edge() {
    assert!(!in_viewport(80, 5, 80, 24));
    assert!(!in_viewport(5, 23, 80, 24), "the last row is reserved for the HUD bar and must be culled");
}

#[test]
fn in_viewport_never_panics_on_a_degenerate_viewport() {
    assert!(!in_viewport(0, 0, 0, 0));
}

// ── Tests: D12 locked exits use a real flag, not a magic layer string ──────────
// (docs/ember2d-refactor-plan.md §3 — late_update used to compare a
// collider's `layer` string against "locked", corrupting the layer field's
// real purpose for any locked exit tile.)

use crate::gamepad::GamepadState;
use crate::input::InputManager;
use ember2d_sim::level::TileRecord;
use crate::mouse::MouseState;

/// Build a level with one exit tile (tagged "exit") pointing at a small
/// level this function writes to the temp directory itself — a real file,
/// so a successful (unlocked) exit produces a genuine `Transition::ToPlay`,
/// not a load failure that would be ambiguous with "the lock blocked it".
///
/// Used to point at `demo/level1.level`, coupling these tests (which are
/// about the `locked` flag, not about any particular level's content) to
/// demo content that no longer exists once `demo/` is archived. `name` must
/// be unique per call site — three tests share this helper and each needs
/// its own target file, both so cleanup in one test can't race a load in
/// another and so a stale file from a previous run can't mask a real bug.
fn level_with_exit(name: &str) -> (LevelData, std::path::PathBuf) {
    let mut target_path = std::env::temp_dir();
    target_path.push(format!("ember2d_test_exit_target_{}.level", name));
    LevelData::empty(4, 4).save(target_path.to_str().unwrap()).expect("write hermetic exit target");

    let mut data = LevelData::empty(20, 10);
    let mut exit_tile = TileRecord::new(3, 3, 1, '>', Color::White, Color::Reset, false, true, "exit");
    exit_tile.next_level = Some(target_path.to_string_lossy().to_string());
    data.tiles.push(exit_tile);
    (data, target_path)
}

fn collide_player_with_exit(play: &mut PlayState, world: &mut World, exit_id: EntityId) -> Option<Transition> {
    let mut events = EventBus::new();
    let player_id = world.find_by_tag("player").expect("player should have spawned");
    events.emit(GameEvent::Collision { entity_a: player_id, entity_b: exit_id });

    let mut input = InputManager::new();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let prev_positions: HashMap<EntityId, Vec2> = HashMap::new();
    let mut quit = false;
    let mut turn_triggered = false;
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();

    play.late_update(UpdateContext {
        world,
        input: &mut input,
        mouse: &mouse,
        gamepad: &gamepad,
        events: &mut events,
        prev_positions: &prev_positions,
        delta_time: 1.0 / 60.0,
        frame_delta_time: 1.0 / 60.0,
        elapsed: 0.0,
        quit: &mut quit,
        turn_triggered: &mut turn_triggered,
        viewport_width: 20,
        viewport_height: 10,
        persistent: &mut persistent,
    });

    play.take_transition()
}

#[test]
fn a_locked_exit_does_not_trigger_a_level_transition() {
    let (data, target_path) = level_with_exit("locked");
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let exit_id = world.find_by_tag("exit").expect("exit entity should have spawned");
    world.colliders.get_mut(&exit_id).unwrap().locked = true;

    let transition = collide_player_with_exit(&mut play, &mut world, exit_id);
    assert!(transition.is_none(), "a locked exit must not trigger a level transition");
    let _ = std::fs::remove_file(&target_path);
}

#[test]
fn an_unlocked_exit_triggers_a_level_transition() {
    let (data, target_path) = level_with_exit("unlocked");
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let exit_id = world.find_by_tag("exit").expect("exit entity should have spawned");
    assert!(!world.colliders.get(&exit_id).unwrap().locked, "an exit should be unlocked by default");

    let transition = collide_player_with_exit(&mut play, &mut world, exit_id);
    assert!(matches!(transition, Some(Transition::ToPlay(_))), "an unlocked exit must trigger a level transition");
    let _ = std::fs::remove_file(&target_path);
}

#[test]
fn setting_the_collider_layer_to_the_string_locked_no_longer_blocks_the_exit() {
    // The specific regression this defect was about: "locked" used to be a
    // magic *layer name*, not a real flag. Reusing that string as an actual
    // layer (e.g. for collision filtering) must not resurrect the old bug.
    let (data, target_path) = level_with_exit("layer_locked");
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let exit_id = world.find_by_tag("exit").expect("exit entity should have spawned");
    world.colliders.get_mut(&exit_id).unwrap().layer = "locked".to_string();

    let transition = collide_player_with_exit(&mut play, &mut world, exit_id);
    assert!(matches!(transition, Some(Transition::ToPlay(_))), "the layer name \"locked\" must be meaningless now — only Collider::locked gates the exit");
    let _ = std::fs::remove_file(&target_path);
}

// ── Tests: Phase 4 de-hardcoding (ember2d-refactor-plan.md §7 Phase 4) ─────────
// z_for_tag removed — a tile's z is now just its authored `layer * 10`, with
// no per-tag sub-ordering. Player collider size is now a PlayerRecord field
// instead of a hardcoded Collider::new(0.75, 0.75).

#[test]
fn tile_z_uses_only_the_authored_layer_not_a_tag_based_offset() {
    use ember2d_sim::level::TileRecord;

    let mut data = LevelData::empty(10, 10);
    // Three different tags that z_for_tag used to bucket differently
    // (floor=0, item=1, wall=2), all on the same editor layer — must all
    // land at the exact same z now.
    data.tiles.push(TileRecord::new(1, 1, 1, '.', Color::White, Color::Reset, false, false, "floor"));
    data.tiles.push(TileRecord::new(2, 1, 1, '*', Color::White, Color::Reset, false, true,  "item"));
    data.tiles.push(TileRecord::new(3, 1, 1, '#', Color::White, Color::Reset, true,  false, "wall"));
    // One tile per remaining layer, to confirm the Background < Main <
    // Player < Foreground tiering still holds using the layer alone.
    data.tiles.push(TileRecord::new(4, 1, 0, '~', Color::White, Color::Reset, false, false, "water"));
    data.tiles.push(TileRecord::new(5, 1, 2, '^', Color::White, Color::Reset, false, true,  "danger"));

    // Step 4g: the player's z is PlayerRecord.layer now, not a hardcoded
    // Z_PLAYER constant — capture it before `data` moves into from_level.
    let player_layer = data.player.layer;

    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let z_of = |tag: &str| world.sprites.get(&world.find_by_tag(tag).unwrap()).unwrap().layer;
    assert_eq!(z_of("floor"), 10, "layer 1 must land at z=10 regardless of tag");
    assert_eq!(z_of("item"),  10, "an item shares its layer's z with a floor tile — no more per-tag bucket");
    assert_eq!(z_of("wall"),  10);
    assert_eq!(z_of("water"),  0, "layer 0 (Background) must land at z=0");
    assert_eq!(z_of("danger"), 20, "layer 2 (Foreground) must land at z=20");

    assert!(z_of("water") < player_layer, "Background must still draw under the player");
    assert!(z_of("floor") < player_layer, "Main must still draw under the player");
    assert!(player_layer < z_of("danger"), "Foreground must still draw over the player");
}

#[test]
fn player_collider_size_defaults_to_the_legacy_hardcoded_value() {
    let data = LevelData::empty(10, 10);
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let player_id = world.find_by_tag("player").expect("player should have spawned");
    let col = world.colliders.get(&player_id).unwrap();
    assert_eq!((col.width, col.height), (0.75, 0.75), "must match the value that used to be hardcoded in play/spawn.rs");
}

#[test]
fn player_collider_size_is_configurable_per_level() {
    let mut data = LevelData::empty(10, 10);
    data.player.collider_w = 2.0;
    data.player.collider_h = 1.5;
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let player_id = world.find_by_tag("player").expect("player should have spawned");
    let col = world.colliders.get(&player_id).unwrap();
    assert_eq!((col.width, col.height), (2.0, 1.5));
}

// ── Tests: Step 4g player draw order (ember2d-refactor-plan.md §7 Phase 4) ─────
// PlayerRecord.layer replaces the hardcoded Z_PLAYER constant, mirroring
// Step 4a's collider_w/collider_h treatment above.

#[test]
fn player_layer_defaults_to_the_legacy_hardcoded_z_player_value() {
    let data = LevelData::empty(10, 10);
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let player_id = world.find_by_tag("player").expect("player should have spawned");
    let z = world.sprites.get(&player_id).unwrap().layer;
    assert_eq!(z, 15, "must match the value that used to be the hardcoded Z_PLAYER constant in play.rs");
}

#[test]
fn player_layer_is_configurable_per_level() {
    let mut data = LevelData::empty(10, 10);
    data.player.layer = 42;
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let player_id = world.find_by_tag("player").expect("player should have spawned");
    let z = world.sprites.get(&player_id).unwrap().layer;
    assert_eq!(z, 42);
}

// ── Tests: D3 deterministic particle/shake RNG (ember2d-refactor-plan.md §3) ───
// (PlayState::rng used to be a fresh `SmallRng::from_entropy()` allocated on
// every particle emission and every shaking frame — nondeterministic and
// wasteful. It's now seeded once from the level's own stored seed.)

#[test]
fn playstate_rng_is_deterministic_for_the_same_level_seed() {
    let mut data_a = LevelData::empty(10, 10);
    data_a.seed = 777;
    let mut data_b = LevelData::empty(10, 10);
    data_b.seed = 777;

    let mut play_a = PlayState::from_level(data_a, BTreeMap::new());
    let mut play_b = PlayState::from_level(data_b, BTreeMap::new());

    let seq_a: Vec<i32> = (0..10).map(|_| play_a.rng.gen_range(-100..100)).collect();
    let seq_b: Vec<i32> = (0..10).map(|_| play_b.rng.gen_range(-100..100)).collect();
    assert_eq!(seq_a, seq_b, "PlayState's particle/shake RNG must be deterministic for the same level seed");
}

#[test]
fn playstate_rng_differs_across_level_seeds() {
    let mut data_a = LevelData::empty(10, 10);
    data_a.seed = 1;
    let mut data_b = LevelData::empty(10, 10);
    data_b.seed = 2;

    let mut play_a = PlayState::from_level(data_a, BTreeMap::new());
    let mut play_b = PlayState::from_level(data_b, BTreeMap::new());

    let seq_a: Vec<i32> = (0..10).map(|_| play_a.rng.gen_range(-100..100)).collect();
    let seq_b: Vec<i32> = (0..10).map(|_| play_b.rng.gen_range(-100..100)).collect();
    assert_ne!(seq_a, seq_b, "different level seeds should not produce the same particle/shake sequence");
}

// ── Test: script-facing camera math (docs/ember2d-refactor-plan.md Phase 2,
// updated for Step 4g) ──────────────────────────────────────────────────────
// get_mouse_world_x/y and get_camera_x/y read whatever PlayState passes as
// the script camera origin; this pins that formula so a script like
// player.rhai's click-to-teleport lands where it should. Step 4g changed
// the formula itself: `game_h` used to reserve two rows for the hardcoded
// HUD bars (`viewport_height - 2`); now that those bars are gone, it's the
// full `viewport_height`.

#[test]
fn script_camera_origin_uses_the_full_viewport_now_that_the_hud_bars_are_gone() {
    let data = LevelData::empty(40, 20); // player.camera_follow defaults true
    let mut play = PlayState::from_level(data, BTreeMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: BTreeMap<String, rhai::Dynamic> = BTreeMap::new();
    play.on_start(&mut world, &mut events, 40, 20, &mut persistent);

    // Move the player off-center (but still inside the clamp range) so the
    // camera has a non-trivial offset to compute.
    let player_id = world.find_by_tag("player").expect("player should have spawned");
    world.transforms.get_mut(&player_id).unwrap().position = Vec2::new(20.0, 10.0);

    let mut input = InputManager::new();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let prev_positions: HashMap<EntityId, Vec2> = HashMap::new();
    let mut quit = false;
    let mut turn_triggered = false;

    play.update(UpdateContext {
        world: &mut world,
        input: &mut input,
        mouse: &mouse,
        gamepad: &gamepad,
        events: &mut events,
        prev_positions: &prev_positions,
        delta_time: 1.0 / 60.0,
        frame_delta_time: 1.0 / 60.0,
        elapsed: 0.0,
        quit: &mut quit,
        turn_triggered: &mut turn_triggered,
        viewport_width: 40,
        viewport_height: 20,
        persistent: &mut persistent,
    });

    // The formula, by hand: game_h = viewport_height = 20 (Step 4g — no
    // more HUD bars reserving rows), so half_h = 10; the camera snaps
    // straight to the (clamped) target on its first-ever update (no prior
    // position to lerp from — see the `== Vec2::ZERO` check).
    let half_w = 20.0f32;
    let half_h = 10.0f32;
    let target = Vec2::new(
        20.0f32.clamp(half_w, (40.0f32 - half_w).max(half_w)),
        10.0f32.clamp(half_h, (20.0f32 - half_h).max(half_h)),
    );
    let expected = Vec2::new((target.x - half_w).round(), (target.y - half_h).round());

    assert_eq!(play.script_camera_origin(), expected);
}

// ── Tests: Step 3b sprite/asset model (ember2d-refactor-plan.md Phase 3) ───────

#[test]
fn sprite_size_uses_the_explicit_size_when_given() {
    let explicit = Vec2::new(2.5, 1.5);
    assert_eq!(sprite_size(Some(explicit), 64, 32, 8.0), explicit);
}

#[test]
fn sprite_size_falls_back_to_pixels_over_pixels_per_unit() {
    // A 64x32 texture at 8 pixels/unit is an 8x4 world-unit sprite —
    // nothing like the old hardcoded `* 4.0` magic scale this replaces.
    assert_eq!(sprite_size(None, 64, 32, 8.0), Vec2::new(8.0, 4.0));
}

#[test]
fn sprite_size_natural_size_scales_with_pixels_per_unit() {
    // Halving pixels_per_unit doubles the natural size — an 8x8 sprite
    // authored for a chunkier grid should look twice as big on one that
    // packs half as many pixels into a world unit.
    assert_eq!(sprite_size(None, 8, 8, 4.0), Vec2::new(2.0, 2.0));
}
