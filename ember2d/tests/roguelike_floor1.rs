// tests/roguelike_floor1.rs — behavior tests for the roguelike's floor 1,
// driven headlessly through TurnHarness (tests/common/mod.rs).
//
// These pin the load-bearing assumptions the Phase 4 plan is built on —
// see the plan file at C:\Users\ronal\.claude\plans\memoized-discovering-glacier.md
// for the full reasoning. Most important: test #4 below (gold pickup) is
// the regression test for "a script's set_position lands before
// integrate_physics/detect_collisions in the same frame", which is the
// single most load-bearing assumption the whole turn-based design rests on.
//
// Coordinates below are pinned to examples/gen_roguelike.rs's floor1()
// layout: spawn (4,4), gold at (10,6) and (20,10), potion at (30,5),
// stairs at (36,16), open floor everywhere in x:[2,38) / y:[2,18).
// Level-integrity tests (do these coordinates still make sense, is the
// level well-formed) are Step 4j's job, not this file's — this file only
// tests behavior against whatever the committed floor1.level currently is.

mod common;
use common::{find_tagged_entity_at, TurnHarness};
use ember2d::prelude::*;

// `CARGO_MANIFEST_DIR`-relative, not CWD-relative — see tests/replay.rs's
// own comment on this (Step 5i's workspace split, docs/ember2d-phase5-plan.md).
const FLOOR1: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../roguelike/floor1.level");

#[test]
fn pressing_w_moves_the_player_one_cell_up_and_triggers_a_turn() {
    let mut h = TurnHarness::load(FLOOR1);
    let before = h.player_pos();

    let triggered = h.frame(Some(Key::W));

    assert!(triggered, "a move onto open floor must trigger a turn");
    assert_eq!(h.player_pos(), Vec2::new(before.x, before.y - 1.0), "\"w\" must move the player exactly one cell up (-y)");
}

#[test]
fn walking_into_a_wall_does_not_move_the_player_or_consume_a_turn() {
    let mut h = TurnHarness::load(FLOOR1);
    // x=2 is the westmost floor column of the room (map.room(2,2,36,16));
    // x=1 is wall. Placed here directly rather than walked there over
    // several turns, to keep this test about the wall bump specifically.
    h.world.transforms.get_mut(&h.player_id()).unwrap().position = Vec2::new(2.0, 4.0);

    let triggered = h.frame(Some(Key::A));

    assert!(!triggered, "bumping a wall must not consume a turn — the standard roguelike rule");
    assert_eq!(h.player_pos(), Vec2::new(2.0, 4.0), "a blocked move must not change position at all");
}

#[test]
fn no_input_triggers_no_turn_and_nothing_moves() {
    let mut h = TurnHarness::load(FLOOR1);
    let before = h.player_pos();

    // on_update still runs every frame in TurnBased mode (only
    // integrate_physics/detect_collisions/late_update are gated on
    // turn_triggered) — this proves that alone doesn't advance the world.
    let triggered = h.frame(None);

    assert!(!triggered, "no keypress must mean no turn");
    assert_eq!(h.player_pos(), before);
}

#[test]
fn stepping_onto_gold_despawns_it_and_credits_the_player_on_the_same_turn() {
    let mut h = TurnHarness::load(FLOOR1);
    let gold_id = find_tagged_entity_at(&h.world, "gold", 10.0, 6.0).expect("a gold tile should exist at (10,6)");
    h.world.transforms.get_mut(&h.player_id()).unwrap().position = Vec2::new(9.0, 6.0);

    let triggered = h.frame(Some(Key::D));

    assert!(triggered);
    assert_eq!(h.player_pos(), Vec2::new(10.0, 6.0), "the player should have moved onto the gold's cell");
    assert!(!h.world.sprites.contains_key(&gold_id), "the gold entity must be despawned the same turn it's touched — not one turn later");
    let gold_count = h.persistent.get("gold").and_then(|d| d.as_int().ok()).unwrap_or(-1);
    assert_eq!(gold_count, 1, "picking up gold must credit persistent \"gold\" on the same turn, matching pickup.rhai's on_collide");
}

#[test]
fn stairs_stay_unlocked_when_no_boss_is_present() {
    let mut h = TurnHarness::load(FLOOR1);
    h.frame(None); // let stairs.rhai's on_update actually run at least once
    let stairs_id = h.world.find_by_tag("stairs").expect("a stairs tile should exist");
    let locked = h.world.colliders.get(&stairs_id).unwrap().locked;
    assert!(!locked, "stairs must stay unlocked on a floor with no boss tiles");
}
