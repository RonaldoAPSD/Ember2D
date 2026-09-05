// tests/roguelike_combat.rs — combat and turn-cadence behavior tests
// (Step 4j, docs/HANDOFF.md / the Phase 4 plan file's "Tests to write
// now" list). Driven headlessly through TurnHarness (tests/common/mod.rs),
// same discipline as tests/roguelike_floor1.rs — these pin the load-bearing
// assumptions the roguelike's whole combat design rests on, several of
// which were only found to be broken by writing tests exactly like these
// (see docs/HANDOFF.md's 4h/4i/quaff-amendment write-ups).

mod common;
use common::TurnHarness;
use ember2d::prelude::*;

const FLOOR2: &str = "roguelike/floor2.level";
const FLOOR3: &str = "roguelike/floor3.level";

fn first_id_tagged(h: &TurnHarness, tag: &str) -> EntityId {
    h.world.tags.iter().find(|(_, t)| t.name == tag).map(|(&id, _)| id)
        .unwrap_or_else(|| panic!("no entity tagged \"{}\" found", tag))
}

fn all_ids_tagged(h: &TurnHarness, tag: &str) -> Vec<EntityId> {
    h.world.tags.iter().filter(|(_, t)| t.name == tag).map(|(&id, _)| id).collect()
}

#[test]
fn bump_attack_kills_a_rat_in_two_hits_and_each_hit_costs_a_turn() {
    let mut h = TurnHarness::load(FLOOR2);
    let rat_id = first_id_tagged(&h, "enemy");
    let rat_pos = h.world.transforms.get(&rat_id).unwrap().position;
    h.world.transforms.get_mut(&h.player_id()).unwrap().position = Vec2::new(rat_pos.x - 1.0, rat_pos.y);

    let triggered1 = h.turn(Key::D);
    assert!(triggered1, "a bump-attack must consume a turn");
    assert_eq!(h.player_pos(), Vec2::new(rat_pos.x - 1.0, rat_pos.y), "attacking must not move the player onto the target's cell");
    assert!(h.world.sprites.contains_key(&rat_id), "one hit (3 dmg) must not kill a 6-hp rat");

    let triggered2 = h.turn(Key::D);
    assert!(triggered2, "a second bump-attack must also consume a turn");
    assert!(!h.world.sprites.contains_key(&rat_id), "a second hit must kill the rat (6 hp, 3 dmg/hit)");
}

#[test]
fn a_rat_acts_at_most_once_per_player_turn_even_across_many_idle_frames() {
    let mut h = TurnHarness::load(FLOOR2);
    let rat_id = first_id_tagged(&h, "enemy");
    let rat_start = h.world.transforms.get(&rat_id).unwrap().position;
    // Same open room, several cells away, clear line of sight.
    h.world.transforms.get_mut(&h.player_id()).unwrap().position = Vec2::new(rat_start.x - 3.0, rat_start.y);

    h.turn(Key::Space); // one real player turn: the rat wakes and takes exactly one step
    let after_one_turn = h.world.transforms.get(&rat_id).map(|t| t.position);
    assert_ne!(after_one_turn, Some(rat_start), "the rat should have moved on its one turn");

    // 10 input-less frames, no new player turn: the rat's own
    // acted_<id>==turn gate must keep it from acting again.
    for _ in 0..10 { h.frame(None); }
    let after_idle = h.world.transforms.get(&rat_id).map(|t| t.position);
    assert_eq!(after_idle, after_one_turn, "a rat must not act again until the next real player turn, no matter how many idle frames pass");
}

#[test]
fn two_adjacent_rats_each_contribute_their_own_damage_in_the_same_resolve() {
    let mut h = TurnHarness::load(FLOOR2);
    let rats = all_ids_tagged(&h, "enemy");
    assert!(rats.len() >= 2, "floor2 must place at least 2 rats for this test to mean anything");

    let player_pos = Vec2::new(50.0, 10.0); // open floor inside room B
    h.world.transforms.get_mut(&h.player_id()).unwrap().position = player_pos;
    h.world.transforms.get_mut(&rats[0]).unwrap().position = Vec2::new(player_pos.x - 1.0, player_pos.y);
    h.world.transforms.get_mut(&rats[1]).unwrap().position = Vec2::new(player_pos.x, player_pos.y - 1.0);

    h.turn(Key::Space); // turn 1: both rats wake (adjacent = trivial line of sight) and publish their attack
    h.turn(Key::Space); // turn 2: player.rhai resolves BOTH rats' turn-1 attacks in one pass

    let hp = h.persistent.get("hp").and_then(|d| d.as_int().ok());
    assert_eq!(
        hp, Some(8),
        "two adjacent rats (2 dmg each) must both land: 12 - 2 - 2 = 8 — not 12 - 2 = 10, which would mean one rat's atk_turn_<id>/atk_dmg_<id> write silently clobbered the other's (they're different keys, so this pins that they stay different keys)"
    );
}

#[test]
fn stairs_are_locked_while_the_boss_is_alive_and_unlock_after_it_dies() {
    // This test is about the MECHANIC (bump-attack kills the boss in
    // exactly 5 hits; stairs lock/unlock around its death), not about
    // whether a player can survive the boss's counter-attacks with no
    // potions — that's a difficulty-balance question the user validates
    // by actually playing (see docs/HANDOFF.md's quaff-amendment note:
    // the real fight is intentionally tight enough to need a potion or
    // two, confirmed by an actual successful playthrough). Pre-seeding a
    // generously high hp here decouples "does the attack/kill/unlock
    // mechanic work" from "what are today's exact balance numbers" — the
    // latter can reasonably change again without invalidating this test.
    let mut h = TurnHarness::load(FLOOR3);
    let mut persistent: std::collections::HashMap<String, rhai::Dynamic> = std::collections::HashMap::new();
    persistent.insert("hp".into(), rhai::Dynamic::from(999_i64));
    persistent.insert("hp_max".into(), rhai::Dynamic::from(999_i64));
    persistent.insert("potions".into(), rhai::Dynamic::from(0_i64));
    persistent.insert("gold".into(), rhai::Dynamic::from(0_i64));
    persistent.insert("depth".into(), rhai::Dynamic::from(1_i64));
    persistent.insert("turns_taken".into(), rhai::Dynamic::from(0_i64));
    h.persistent = persistent;

    h.turn(Key::Space); // let stairs.rhai's on_update run at least once

    let boss_id = first_id_tagged(&h, "boss");
    let stairs_id = h.world.find_by_tag("stairs").expect("stairs should exist");
    assert!(h.world.colliders.get(&stairs_id).unwrap().locked, "stairs must be locked while the boss is alive");

    let boss_pos = h.world.transforms.get(&boss_id).unwrap().position;
    h.world.transforms.get_mut(&h.player_id()).unwrap().position = Vec2::new(boss_pos.x - 1.0, boss_pos.y);

    // 15 hp, 3 dmg/hit -> dead on the 5th hit.
    for i in 0..5 {
        let triggered = h.turn(Key::D);
        assert!(triggered, "hit #{} must consume a turn", i + 1);
    }
    assert!(!h.world.sprites.contains_key(&boss_id), "the boss must be dead after 5 hits (15 hp, 3 dmg/hit)");

    h.turn(Key::Space); // give stairs.rhai a turn to notice count_by_tag("boss") == 0
    assert!(!h.world.colliders.get(&stairs_id).unwrap().locked, "stairs must unlock once the boss is dead");
}

#[test]
fn identical_input_sequences_produce_identical_state_across_independent_instances() {
    // Two independent PlayState/World/ScriptEngine stacks, each with their
    // own (process-randomized) HashMap iteration order, driven by the
    // exact same key sequence — this is what actually catches
    // script-execution-order dependence, unlike re-running the same
    // instance twice.
    let mut h1 = TurnHarness::load(FLOOR2);
    let mut h2 = TurnHarness::load(FLOOR2);

    let sequence = [
        Key::D, Key::D, Key::D, Key::S, Key::S, Key::A, Key::W,
        Key::Space, Key::D, Key::S, Key::Space, Key::A, Key::A, Key::W, Key::W,
    ];
    for &key in &sequence {
        h1.turn(key);
        h2.turn(key);
    }

    assert_eq!(h1.player_pos(), h2.player_pos(), "same input sequence must produce the same player position");
    let hp = |h: &TurnHarness| h.persistent.get("hp").and_then(|d| d.as_int().ok());
    let gold = |h: &TurnHarness| h.persistent.get("gold").and_then(|d| d.as_int().ok());
    assert_eq!(hp(&h1), hp(&h2), "same input sequence must produce the same hp");
    assert_eq!(gold(&h1), gold(&h2), "same input sequence must produce the same gold");

    let rat_positions = |h: &TurnHarness| -> Vec<(i64, i64)> {
        let mut v: Vec<(i64, i64)> = h.world.tags.iter()
            .filter(|(_, t)| t.name == "enemy")
            .filter_map(|(&id, _)| h.world.transforms.get(&id).map(|t| (t.position.x as i64, t.position.y as i64)))
            .collect();
        v.sort();
        v
    };
    assert_eq!(rat_positions(&h1), rat_positions(&h2), "rats must end up in the same positions across independently-run instances");
}
