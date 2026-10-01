// tests/actor_physics.rs — regression coverage for Step 7.5-6 (docs/ember2d-
// master-plan.md §5.6): Simulation::late_step's engine-side solid-collision
// resolution now covers any Actor with its own `physics` flag set (default
// true, an opt-out), not just the local player. Driven directly against
// Simulation/World with no script involved — the behavior under test is
// purely late_step's own dispatch logic, not anything a script triggers.
//
// `resolve_solid_collision` itself doesn't care about velocity or movement
// history (its own `prev` parameter is unused) — only the CURRENT overlap —
// so every test here places two colliders already overlapping rather than
// simulating movement into a wall; that exercises the exact same resolver
// call with far less setup.

use ember2d::prelude::*;
use ember2d_sim::simulation::{Simulation, StepOutcome};
use std::collections::BTreeMap;

fn spawn(data: LevelData) -> (Simulation, World) {
    let mut sim = Simulation::new(data);
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 10, 10, &mut persistent);
    (sim, world)
}

/// A solid wall tile and a solid AI-actor tile, both at the same cell (a
/// guaranteed full overlap) — `physics` on the actor's own `ActorRecord`
/// set as requested.
fn wall_and_mover_level(physics: bool) -> LevelData {
    let mut data = LevelData::empty(10, 10);
    data.tiles.push(TileRecord::new(5, 5, 1, '#', Color::White, Color::Reset, true, false, "wall"));

    let mut mover =
        TileRecord::new(5, 5, 1, 'm', Color::White, Color::Reset, true, false, "mover");
    let mut actor = ActorRecord::default();
    actor.physics = physics;
    mover.actor = Some(actor);
    data.tiles.push(mover);
    data
}

fn resolve(sim: &mut Simulation, world: &mut World) -> StepOutcome {
    let mut events = EventBus::new();
    world.detect_collisions(&mut events);
    let prev_positions = world.snapshot_positions();
    sim.late_step(world, &events, &prev_positions, Vec2::ZERO, 1.0 / 60.0, 0.0, 10, 10, &mut BTreeMap::new())
}

#[test]
fn an_ai_actor_with_physics_is_pushed_out_of_a_wall_it_overlaps() {
    let (mut sim, mut world) = spawn(wall_and_mover_level(true));
    let mover = world.find_by_tag("mover").expect("mover should have spawned");
    let before = world.transforms[&mover].position;

    resolve(&mut sim, &mut world);

    let after = world.transforms[&mover].position;
    assert_ne!(
        after, before,
        "an AI actor with physics: true overlapping a solid wall must be pushed out by \
         late_step's own solid-collision resolution, not just the local player"
    );
}

#[test]
fn an_ai_actor_with_physics_disabled_is_left_overlapping_the_wall() {
    let (mut sim, mut world) = spawn(wall_and_mover_level(false));
    let mover = world.find_by_tag("mover").expect("mover should have spawned");
    let before = world.transforms[&mover].position;

    resolve(&mut sim, &mut world);

    let after = world.transforms[&mover].position;
    assert_eq!(
        after, before,
        "ActorRecord::physics: false must opt an actor out of engine-side solid resolution"
    );
}

/// The local player still gets solid resolution too, unaffected by this
/// step's restructuring of late_step's collision loop (it used to be the
/// ONLY case resolved; now it's resolved through the same shared path an
/// AI actor's own case above uses).
#[test]
fn the_local_player_is_still_pushed_out_of_a_wall_it_overlaps() {
    let mut data = LevelData::empty(10, 10);
    data.tiles.push(TileRecord::new(5, 5, 1, '#', Color::White, Color::Reset, true, false, "wall"));
    data.set_player_spawn((5.0, 5.0));
    let (mut sim, mut world) = spawn(data);

    let player = world.find_by_tag("player").expect("player should have spawned");
    let before = world.transforms[&player].position;

    resolve(&mut sim, &mut world);

    let after = world.transforms[&player].position;
    assert_ne!(after, before, "the local player must still be pushed out of an overlapped wall");
}

/// Regression for this step's own late_step restructuring: exit-tile
/// handling must stay local-player-only. Before this step there was no
/// other way to reach it at all (only the local player's own branch ever
/// checked `exit_targets`); after splitting solid resolution into its own
/// shared path, an AI actor must still never trigger one.
#[test]
fn an_ai_actor_walking_onto_an_exit_tile_never_triggers_a_level_transition() {
    let mut data = LevelData::empty(10, 10);
    let mut exit =
        TileRecord::new(5, 5, 1, '>', Color::Cyan, Color::Reset, false, true, "stairs");
    exit.next_level = Some("unused.level".to_string());
    data.tiles.push(exit);
    let mut ai_mover =
        TileRecord::new(5, 5, 1, 'a', Color::White, Color::Reset, true, false, "ai_mover");
    ai_mover.actor = Some(ActorRecord::default());
    data.tiles.push(ai_mover);

    let (mut sim, mut world) = spawn(data);
    let outcome = resolve(&mut sim, &mut world);

    assert!(
        outcome.pending_level.is_none(),
        "an AI actor (or any non-player entity) stepping onto an exit tile must never trigger \
         a level transition — only the local player should"
    );
}
