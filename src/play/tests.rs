// play/tests.rs — PlayState unit tests.
//
// Split out of play.rs (via the sibling-directory `mod tests;` convention
// already used for `mod spawn;`) once play.rs approached the project's
// 600-line hard limit — see CLAUDE.md "Development Rules". This file's
// content is `play::tests`, with full access to play.rs's private items
// via `use super::*`.

use super::*;
use crate::components::{Sprite, Transform};

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
use crate::level::TileRecord;
use crate::mouse::MouseState;

/// Build a level with one exit tile (tagged "exit") pointing at
/// `demo/level1.level` — a real file, so a successful (unlocked) exit
/// produces a genuine `Transition::ToPlay`, not a load failure that would
/// be ambiguous with "the lock blocked it".
fn level_with_exit() -> LevelData {
    let mut data = LevelData::empty(20, 10);
    let mut exit_tile = TileRecord::new(3, 3, 1, '>', Color::White, Color::Reset, false, true, "exit");
    exit_tile.next_level = Some("demo/level1.level".to_string());
    data.tiles.push(exit_tile);
    data
}

fn collide_player_with_exit(play: &mut PlayState, world: &mut World, exit_id: EntityId) -> Option<Transition> {
    let mut events = EventBus::new();
    events.emit(GameEvent::Collision { entity_a: play_player_id(play), entity_b: exit_id });

    let mut input = InputManager::new();
    let mouse = MouseState::new();
    let gamepad = GamepadState::new();
    let prev_positions: HashMap<EntityId, Vec2> = HashMap::new();
    let mut quit = false;
    let mut turn_triggered = false;
    let mut persistent: HashMap<String, rhai::Dynamic> = HashMap::new();

    play.late_update(UpdateContext {
        world,
        input: &mut input,
        mouse: &mouse,
        gamepad: &gamepad,
        events: &mut events,
        prev_positions: &prev_positions,
        delta_time: 1.0 / 60.0,
        elapsed: 0.0,
        quit: &mut quit,
        turn_triggered: &mut turn_triggered,
        viewport_width: 20,
        viewport_height: 10,
        persistent: &mut persistent,
    });

    play.take_transition()
}

// `player_id` is private to PlayState; this test module is a child of
// `play`, so it can read the field directly.
fn play_player_id(play: &PlayState) -> EntityId { play.player_id }

#[test]
fn a_locked_exit_does_not_trigger_a_level_transition() {
    let data = level_with_exit();
    let mut play = PlayState::from_level(data, HashMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: HashMap<String, rhai::Dynamic> = HashMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let exit_id = world.find_by_tag("exit").expect("exit entity should have spawned");
    world.colliders.get_mut(&exit_id).unwrap().locked = true;

    let transition = collide_player_with_exit(&mut play, &mut world, exit_id);
    assert!(transition.is_none(), "a locked exit must not trigger a level transition");
}

#[test]
fn an_unlocked_exit_triggers_a_level_transition() {
    let data = level_with_exit();
    let mut play = PlayState::from_level(data, HashMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: HashMap<String, rhai::Dynamic> = HashMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let exit_id = world.find_by_tag("exit").expect("exit entity should have spawned");
    assert!(!world.colliders.get(&exit_id).unwrap().locked, "an exit should be unlocked by default");

    let transition = collide_player_with_exit(&mut play, &mut world, exit_id);
    assert!(matches!(transition, Some(Transition::ToPlay(_))), "an unlocked exit must trigger a level transition");
}

#[test]
fn setting_the_collider_layer_to_the_string_locked_no_longer_blocks_the_exit() {
    // The specific regression this defect was about: "locked" used to be a
    // magic *layer name*, not a real flag. Reusing that string as an actual
    // layer (e.g. for collision filtering) must not resurrect the old bug.
    let data = level_with_exit();
    let mut play = PlayState::from_level(data, HashMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: HashMap<String, rhai::Dynamic> = HashMap::new();
    play.on_start(&mut world, &mut events, 20, 10, &mut persistent);

    let exit_id = world.find_by_tag("exit").expect("exit entity should have spawned");
    world.colliders.get_mut(&exit_id).unwrap().layer = "locked".to_string();

    let transition = collide_player_with_exit(&mut play, &mut world, exit_id);
    assert!(matches!(transition, Some(Transition::ToPlay(_))), "the layer name \"locked\" must be meaningless now — only Collider::locked gates the exit");
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

    let mut play_a = PlayState::from_level(data_a, HashMap::new());
    let mut play_b = PlayState::from_level(data_b, HashMap::new());

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

    let mut play_a = PlayState::from_level(data_a, HashMap::new());
    let mut play_b = PlayState::from_level(data_b, HashMap::new());

    let seq_a: Vec<i32> = (0..10).map(|_| play_a.rng.gen_range(-100..100)).collect();
    let seq_b: Vec<i32> = (0..10).map(|_| play_b.rng.gen_range(-100..100)).collect();
    assert_ne!(seq_a, seq_b, "different level seeds should not produce the same particle/shake sequence");
}

// ── Test: Step 2e's flagged risk — script-facing camera math is unchanged ──────
// (docs/ember2d-refactor-plan.md Phase 2 — PlayState migrated from a
// hand-rolled cam_x/cam_y offset to a real Camera. get_mouse_world_x/y and
// get_camera_x/y read whatever PlayState passes as the script camera origin;
// this pins that value to the exact pre-refactor formula so a script like
// player.rhai's click-to-teleport keeps landing where it always did.)

#[test]
fn script_camera_origin_matches_the_pre_refactor_formula() {
    let data = LevelData::empty(40, 20); // player.camera_follow defaults true
    let mut play = PlayState::from_level(data, HashMap::new());
    let mut world = World::new();
    let mut events = EventBus::new();
    let mut persistent: HashMap<String, rhai::Dynamic> = HashMap::new();
    play.on_start(&mut world, &mut events, 40, 20, &mut persistent);

    // Move the player off-center (but still inside the clamp range) so the
    // camera has a non-trivial offset to compute.
    world.transforms.get_mut(&play_player_id(&play)).unwrap().position = Vec2::new(20.0, 10.0);

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
        elapsed: 0.0,
        quit: &mut quit,
        turn_triggered: &mut turn_triggered,
        viewport_width: 40,
        viewport_height: 20,
        persistent: &mut persistent,
    });

    // The old formula, by hand: game_h = viewport_height - 2 = 18, so
    // half_h = 9; the camera snaps straight to the (clamped) target on its
    // first-ever update (no prior position to lerp from — see the
    // `== Vec2::ZERO` check).
    let half_w = 20.0f32;
    let half_h = 9.0f32;
    let target = Vec2::new(
        20.0f32.clamp(half_w, (40.0f32 - half_w).max(half_w)),
        10.0f32.clamp(half_h, (20.0f32 - half_h).max(half_h)),
    );
    let expected = Vec2::new((target.x - half_w).round(), (target.y - half_h).round());

    assert_eq!(play.script_camera_origin(), expected);
}
