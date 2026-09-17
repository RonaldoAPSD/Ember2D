// tests/level_source.rs — regression coverage for Step 7.5-9 (docs/ember2d-
// master-plan.md §5.6, R17 fix): Simulation routes a level transition
// through its own LevelSource instead of calling LevelData::load directly,
// defaulting to a no-op NullLevelSource that fails closed rather than
// touching a filesystem `ember2d-sim` isn't supposed to know exists.

use ember2d::prelude::*;
use ember2d_sim::level_source::LevelSource;
use ember2d_sim::simulation::{Simulation, StepOutcome};
use std::collections::BTreeMap;

/// A trivial in-memory `LevelSource` — knows about exactly one path.
struct FakeLevelSource {
    known_path: String,
    next_level: LevelData,
}

impl LevelSource for FakeLevelSource {
    fn exists(&self, path: &str) -> bool {
        path == self.known_path
    }
    fn read_to_string(&self, _path: &str) -> Result<String, String> {
        Err("FakeLevelSource has no file contents".to_string())
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        if path == self.known_path {
            Ok(self.next_level.clone())
        } else {
            Err(format!("FakeLevelSource: no such level '{path}'"))
        }
    }
}

/// A level whose spawn point sits directly on a non-solid exit tile — the
/// player overlaps it from the very first step, no movement needed.
fn exit_tile_level() -> LevelData {
    let mut data = LevelData::empty(10, 10);
    let mut exit = TileRecord::new(5, 5, 1, '>', Color::Cyan, Color::Reset, false, true, "stairs");
    exit.next_level = Some("next.level".to_string());
    data.tiles.push(exit);
    data.spawn_point = (5.0, 5.0);
    data
}

fn step_onto_the_exit(sim: &mut Simulation, world: &mut World) -> StepOutcome {
    let mut persistent = BTreeMap::new();
    sim.on_start(world, 10, 10, &mut persistent);
    let mut events = EventBus::new();
    world.detect_collisions(&mut events);
    let prev_positions = world.snapshot_positions();
    sim.late_step(world, &events, &prev_positions, Vec2::ZERO, 1.0 / 60.0, 0.0, 10, 10, &mut persistent)
}

#[test]
fn a_level_transition_succeeds_once_a_working_level_source_is_configured() {
    let mut sim = Simulation::new(exit_tile_level());
    sim.set_level_source(Box::new(FakeLevelSource {
        known_path: "next.level".to_string(),
        next_level: LevelData::empty(10, 10),
    }));
    let mut world = World::new();

    let outcome = step_onto_the_exit(&mut sim, &mut world);

    assert!(
        outcome.pending_level.is_some(),
        "a level transition must succeed once a working LevelSource resolves the target path"
    );
}

#[test]
fn a_level_transition_fails_closed_with_the_default_null_level_source() {
    let mut sim = Simulation::new(exit_tile_level());
    // Deliberately no `set_level_source` call — `Simulation`'s own default.
    let mut world = World::new();

    let outcome = step_onto_the_exit(&mut sim, &mut world);

    assert!(
        outcome.pending_level.is_none(),
        "with no LevelSource configured, a transition must fail closed (not panic, not \
         silently succeed), same as it would for a genuinely missing file"
    );
    assert!(
        !outcome.logs.is_empty(),
        "a failed transition must still be logged, not silently swallowed"
    );
}
