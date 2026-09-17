// tests/turn_model.rs — regression coverage for Step 7.5-7 (docs/ember2d-
// master-plan.md §5.6): TurnModel::Energy makes Actor::speed live (a faster
// actor acts more often), and TurnModel::ActionCost honors Command.cost.
// Driven directly against Simulation with no script on either AI actor —
// an unscripted Ai actor's turn always counts regardless (see
// `Simulation::run_actor_turn`'s own "AI turn always counts" comment,
// simulation/step.rs), so the model's own cost fallback is all that's
// under test here, not anything a script does.
//
// THE PLAYER IS GIVEN AN ENORMOUS ONE-TIME COST, NOT REMOVED: `do_on_start`
// always spawns a Local(0) player and `rebuild_scheduler` always inserts
// it. Two things have to both be true for it to stop competing with the AI
// actors after that: (1) a Local actor's turn is consumed ONLY if a script
// calls `ctx.act` (see run_actor_turn's own "a Local actor's turn counts
// only if it called ctx.act" comment) — `player.rhai` below does exactly
// that, with a huge cost; (2) `run_actor_turn` itself only runs for a Local
// actor when it already has a command queued (`!is_local || has_command`,
// `Simulation::step`) — an external "wait" command is fed in for the
// player on EVERY step below (harmless once its due is astronomically
// high; it just never becomes front again) so its on_turn actually gets
// the chance to call `ctx.act` at all. Skipping either half leaves the
// player permanently parked at the scheduler's front, starving both AI
// actors completely — confirmed by hand while writing this file (both
// counts read exactly 0 until this was fixed).

use ember2d::prelude::*;
use ember2d_sim::command::{Command, GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::BTreeMap;

mod common;

fn two_ai_actors_level(speed_a: u32, speed_b: u32, player_script: &str) -> LevelData {
    let mut data = LevelData::empty(20, 20);
    data.player.script = Some(player_script.to_string());

    let mut a = TileRecord::new(2, 2, 1, 'a', Color::White, Color::Reset, false, true, "a");
    a.actor = Some(ActorRecord { speed: speed_a, ..Default::default() });
    data.tiles.push(a);

    let mut b = TileRecord::new(4, 4, 1, 'b', Color::White, Color::Reset, false, true, "b");
    b.actor = Some(ActorRecord { speed: speed_b, ..Default::default() });
    data.tiles.push(b);

    data
}

fn write_player_out_script(name: &str) -> std::path::PathBuf {
    let script = common::test_temp_dir().join(format!("ember2d_test_turn_model_{}.rhai", name));
    std::fs::write(&script, "fn on_turn(id, ctx) { ctx.act(1000000.0); }\n").unwrap();
    script
}

/// One step's worth of empty input, plus a "wait" command for `player` (see
/// this file's header comment for why every step, not just the first) and
/// whichever `extra` commands the caller wants the two AI actors to see
/// this step (empty for the Energy/Alternating tests, which read no
/// command cost at all).
fn step(
    sim: &mut Simulation,
    world: &mut World,
    persistent: &mut BTreeMap<String, rhai::Dynamic>,
    player: EntityId,
    extra: &[Command],
) {
    let input = InputSnapshot::default();
    let gamepad = GamepadSnapshot::default();
    let mut external = vec![Command {
        actor: player,
        action: "wait".to_string(),
        params: vec![],
        cost: None,
    }];
    external.extend_from_slice(extra);
    sim.step(
        world,
        StepInput {
            input: &input,
            mouse: MouseSnapshot::default(),
            gamepad: &gamepad,
            external_commands: &external,
            animating: &[],
            camera_origin: Vec2::ZERO,
            sim_dt: 1.0 / 60.0,
            elapsed: 0.0,
            viewport_w: 20,
            viewport_h: 20,
        },
        persistent,
    );
}

#[test]
fn energy_model_makes_a_faster_actor_act_about_twice_as_often() {
    let script = write_player_out_script("energy");
    let data = two_ai_actors_level(200, 100, &script.to_string_lossy());
    let mut sim = Simulation::new(data);
    sim.set_turn_model(TurnModel::Energy);

    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 20, 20, &mut persistent);

    let player = world.find_by_tag("player").expect("player should have spawned");
    let fast = world.find_by_tag("a").expect("a should have spawned");
    let slow = world.find_by_tag("b").expect("b should have spawned");

    let mut fast_turns = 0u32;
    let mut slow_turns = 0u32;
    for _ in 0..400 {
        let front = sim.current_actor();
        step(&mut sim, &mut world, &mut persistent, player, &[]);
        if front == Some(fast) {
            fast_turns += 1;
        } else if front == Some(slow) {
            slow_turns += 1;
        }
    }
    let _ = std::fs::remove_file(&script);

    assert!(slow_turns > 10, "the slow actor should still get plenty of turns, got {slow_turns}");
    let ratio = fast_turns as f64 / slow_turns as f64;
    assert!(
        (ratio - 2.0).abs() < 0.25,
        "a speed-200 actor should act ~2x as often as a speed-100 one under TurnModel::Energy \
         (fast={fast_turns}, slow={slow_turns}, ratio={ratio:.2})"
    );
}

#[test]
fn action_cost_model_honors_an_externally_supplied_commands_cost() {
    // Both actors have the SAME speed — under Alternating or Energy they'd
    // act at identical rates. ActionCost ignores speed entirely and reads
    // the cost off the currently-resolving actor's own Command instead.
    let script = write_player_out_script("action_cost");
    let data = two_ai_actors_level(100, 100, &script.to_string_lossy());
    let mut sim = Simulation::new(data);
    sim.set_turn_model(TurnModel::ActionCost);

    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 20, 20, &mut persistent);

    let player = world.find_by_tag("player").expect("player should have spawned");
    let cheap = world.find_by_tag("a").expect("a should have spawned");
    let expensive = world.find_by_tag("b").expect("b should have spawned");

    let mut cheap_turns = 0u32;
    let mut expensive_turns = 0u32;
    for _ in 0..400 {
        let front = sim.current_actor();
        // `a` is cheap (cost 20), `b` is expensive (cost 200) — a tenth the
        // cost should mean roughly ten times the turn rate.
        let extra = [
            Command { actor: cheap, action: "act".to_string(), params: vec![], cost: Some(20.0) },
            Command {
                actor: expensive,
                action: "act".to_string(),
                params: vec![],
                cost: Some(200.0),
            },
        ];
        step(&mut sim, &mut world, &mut persistent, player, &extra);
        if front == Some(cheap) {
            cheap_turns += 1;
        } else if front == Some(expensive) {
            expensive_turns += 1;
        }
    }
    let _ = std::fs::remove_file(&script);

    assert!(
        expensive_turns > 5,
        "the expensive actor should still get some turns, got {expensive_turns}"
    );
    let ratio = cheap_turns as f64 / expensive_turns as f64;
    assert!(
        (ratio - 10.0).abs() < 2.0,
        "a cost-20 command should let its actor act ~10x as often as a cost-200 one under \
         TurnModel::ActionCost (cheap={cheap_turns}, expensive={expensive_turns}, ratio={ratio:.2})"
    );
}

#[test]
fn alternating_is_still_the_default_turn_model_for_a_fresh_simulation() {
    // No `set_turn_model` call at all — every pre-7.5-7 project (and every
    // demo shipped today) must keep costing every turn identically
    // regardless of Actor::speed, unchanged.
    let script = write_player_out_script("alternating");
    let data = two_ai_actors_level(200, 100, &script.to_string_lossy());
    let mut sim = Simulation::new(data);
    // Deliberately no sim.set_turn_model(...) call.

    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    sim.on_start(&mut world, 20, 20, &mut persistent);

    let player = world.find_by_tag("player").expect("player should have spawned");
    let fast = world.find_by_tag("a").expect("a should have spawned");
    let slow = world.find_by_tag("b").expect("b should have spawned");

    let mut fast_turns = 0u32;
    let mut slow_turns = 0u32;
    for _ in 0..200 {
        let front = sim.current_actor();
        step(&mut sim, &mut world, &mut persistent, player, &[]);
        if front == Some(fast) {
            fast_turns += 1;
        } else if front == Some(slow) {
            slow_turns += 1;
        }
    }
    let _ = std::fs::remove_file(&script);

    assert!(fast_turns > 10 && slow_turns > 10, "both actors should get plenty of turns");
    // Off by at most 1 rather than an exact match: the player's own single
    // early turn (see this file's header comment) lands in whichever of
    // the 200 iterations it happens to fall in, which can shift one
    // actor's count by one step relative to the other's — real turn-cost
    // parity, not a rounding artifact this test should paper over with a
    // wider tolerance.
    assert!(
        fast_turns.abs_diff(slow_turns) <= 1,
        "Actor::speed must have no more than rounding-level effect under the default \
         Alternating model (fast={fast_turns}, slow={slow_turns})"
    );
}
