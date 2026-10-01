// tests/roguelike_items.rs — Step 9.5-4 (docs/ember2d-master-plan.md
// §5.8.5): the roguelike's items and progression, driven by real key
// presses through the menus and the aiming cursor — the inventory (I),
// drop (X), the three scrolls, equipment, the level-up choice, the
// character sheet, and winning with the Amulet.

mod common;

use common::TurnHarness;
use ember2d::prelude::*;
use ember2d_sim::scripting::LogLevel;
use std::collections::BTreeMap;
use std::rc::Rc;

const DUNGEON: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/dungeon.level");

fn item(kind: &str, name: &str) -> rhai::Dynamic {
    let mut m = rhai::Map::new();
    m.insert("kind".into(), kind.to_string().into());
    m.insert("name".into(), name.to_string().into());
    rhai::Dynamic::from(m)
}

fn gear(kind: &str, name: &str, bonus: i64) -> rhai::Dynamic {
    let mut m = item(kind, name).cast::<rhai::Map>();
    m.insert("bonus".into(), bonus.into());
    rhai::Dynamic::from(m)
}

/// Depth `depth` of seed 7, the player carrying `pack`.
fn floor_with(depth: i64, pack: Vec<rhai::Dynamic>) -> TurnHarness {
    let mut p = BTreeMap::new();
    for (k, v) in [
        ("run_seed", 7),
        ("depth", depth),
        ("hp", 30),
        ("max_hp", 30),
        ("power", 2),
        ("defense", 1),
        ("xp", 0),
        ("level", 1),
        ("kills", 0),
    ] {
        p.insert(k.to_string(), rhai::Dynamic::from(v));
    }
    p.insert("inventory".into(), rhai::Dynamic::from(pack));
    p.insert("weapon".into(), gear("dagger", "dagger", 1));
    p.insert("armour".into(), gear("none", "no armour", 0));
    p.insert("log".into(), rhai::Dynamic::from(rhai::Array::new()));
    let mut h = TurnHarness::continue_run(LevelData::load(DUNGEON).unwrap(), p);
    h.sim.set_ai_turns_per_step(256);
    h.frame(None);
    h
}

fn int(h: &TurnHarness, key: &str) -> i64 {
    let v = &h.persistent[key];
    v.as_int().unwrap_or_else(|_| v.as_float().map(|f| f as i64).unwrap_or(i64::MIN))
}

fn pack_kinds(h: &TurnHarness) -> Vec<String> {
    h.persistent["inventory"]
        .clone()
        .into_array()
        .unwrap()
        .into_iter()
        .map(|m| m.cast::<rhai::Map>()["kind"].to_string())
        .collect()
}

fn var(h: &TurnHarness, id: EntityId, key: &str) -> i64 {
    h.world
        .vars
        .get(&id)
        .and_then(|v| v.values.get(key))
        .and_then(|d| d.as_int().ok())
        .unwrap_or(i64::MIN)
}

fn set_var(h: &mut TurnHarness, id: EntityId, key: &str, value: i64) {
    h.world.vars.get_mut(&id).unwrap().values.insert(key.into(), rhai::Dynamic::from(value));
}

/// The first monster, made tough enough to survive what's thrown at it,
/// and the player put in the floor cell just west of it (east if west is a
/// wall), looking around from there. Returns the monster and the key that
/// steps toward it.
fn face_a_monster(h: &mut TurnHarness) -> (EntityId, &'static str) {
    let (m, at) = h
        .world
        .tags
        .iter()
        .filter(|(_, t)| t.name == "monster")
        .map(|(&id, _)| {
            let p = h.world.get_global_position(id);
            (id, (p.x as i32, p.y as i32))
        })
        .next()
        .expect("a monster");
    set_var(h, m, "hp", 100);
    let map = h.world.tilemaps.values().next().unwrap().clone();
    let (cell, key) = if !map.solid_at(at.0 - 1, at.1, 0) {
        ((at.0 - 1, at.1), "right")
    } else {
        ((at.0 + 1, at.1), "left")
    };
    let p = h.player_id();
    h.world.transforms.get_mut(&p).unwrap().position = Vec2::new(cell.0 as f32, cell.1 as f32);
    Rc::make_mut(h.world.fov.as_mut().unwrap()).compute(Some(&map), cell.0, cell.1, 8);
    (m, key)
}

/// Opens the inventory and picks entry `index` (Down that many times).
fn use_item(h: &mut TurnHarness, index: usize) {
    h.frame(Some("i"));
    h.frame(None);
    for _ in 0..index {
        h.frame(Some("down"));
    }
    h.frame(Some("enter"));
    settle(h);
}

fn settle(h: &mut TurnHarness) {
    for _ in 0..4 {
        h.frame(None);
    }
}

fn clean(h: &TurnHarness) {
    let bad: Vec<_> =
        h.logs.iter().filter(|l| l.level != LogLevel::Info).map(|l| l.text.clone()).collect();
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn a_potion_from_the_pack_heals_and_is_used_up() {
    let mut h = floor_with(1, vec![item("potion", "healing potion")]);
    h.persistent.insert("hp".into(), rhai::Dynamic::from(10_i64));
    use_item(&mut h, 0);
    assert_eq!(int(&h, "hp"), 22);
    assert!(pack_kinds(&h).is_empty());
    clean(&h);
}

#[test]
fn dropping_puts_the_item_on_the_floor() {
    let mut h = floor_with(1, vec![item("potion", "healing potion"), item("sword", "sword")]);
    h.frame(Some("x"));
    h.frame(None);
    h.frame(Some("down"));
    h.frame(Some("enter"));
    settle(&mut h);
    assert_eq!(pack_kinds(&h), vec!["potion"]);
    let me = h.player_pos();
    let dropped = h.world.tags.iter().filter(|(_, t)| t.name == "item").any(|(&id, _)| {
        h.world.get_global_position(id) == me
            && h.world.vars[&id].values.get("kind").map(|k| k.to_string()) == Some("sword".into())
    });
    assert!(dropped, "the sword lies where the player stands");
    clean(&h);
}

#[test]
fn lightning_strikes_the_nearest_monster_in_view() {
    let mut h = floor_with(1, vec![item("lightning", "scroll of lightning")]);
    let (m, _) = face_a_monster(&mut h);
    use_item(&mut h, 0);
    assert_eq!(var(&h, m, "hp"), 80, "20 damage");
    assert!(pack_kinds(&h).is_empty());
    clean(&h);
}

#[test]
fn confusion_is_aimed_with_the_cursor() {
    let mut h = floor_with(1, vec![item("confusion", "scroll of confusion")]);
    let (m, toward) = face_a_monster(&mut h);
    use_item(&mut h, 0); // picks the scroll: now aiming
    let p = h.player_id();
    assert!(h.world.vars[&p].values.contains_key("aim"), "a cursor to aim with");
    assert!(h.world.tags.values().any(|t| t.name == "cursor"));
    h.frame(Some(toward)); // the cursor onto the monster
    h.frame(Some("enter"));
    settle(&mut h);
    assert!(var(&h, m, "confused") >= 9, "confused for ten turns (one already spent)");
    assert!(!h.world.tags.values().any(|t| t.name == "cursor"), "the cursor is gone");
    assert!(pack_kinds(&h).is_empty());
    clean(&h);
}

#[test]
fn a_fireball_burns_everything_in_its_radius_including_you() {
    let mut h = floor_with(1, vec![item("fireball", "scroll of fireball")]);
    let (m, toward) = face_a_monster(&mut h);
    use_item(&mut h, 0);
    h.frame(Some(toward)); // aim at the monster, one cell away: you're in the blast
    h.frame(Some("enter"));
    settle(&mut h);
    assert_eq!(var(&h, m, "hp"), 86, "14 damage");
    assert!(int(&h, "hp") <= 16, "and 14 to the player standing next to it ({})", int(&h, "hp"));
    clean(&h);
}

#[test]
fn equipping_a_sword_swaps_it_for_the_dagger_and_hits_harder() {
    let mut h = floor_with(1, vec![item("sword", "sword")]);
    use_item(&mut h, 0);
    let weapon = h.persistent["weapon"].clone().cast::<rhai::Map>();
    assert_eq!(weapon["kind"].to_string(), "sword");
    assert_eq!(weapon["bonus"].as_int().unwrap(), 3);
    assert_eq!(pack_kinds(&h), vec!["dagger"], "the dagger went back in the pack");
    // Power 2 + sword 3 against defense 0: 5 a blow.
    let (m, toward) = face_a_monster(&mut h);
    set_var(&mut h, m, "defense", 0);
    h.turn(toward);
    assert_eq!(var(&h, m, "hp"), 95);
    clean(&h);
}

#[test]
fn enough_experience_offers_a_level_up_choice() {
    let mut h = floor_with(1, vec![]);
    h.persistent.insert("xp".into(), rhai::Dynamic::from(300_i64));
    h.frame(None); // the choice opens
    h.frame(None);
    h.frame(Some("down")); // Strength
    h.frame(Some("enter"));
    settle(&mut h);
    assert_eq!(int(&h, "level"), 2);
    assert_eq!(int(&h, "power"), 3);
    assert_eq!(int(&h, "xp"), 50, "250 spent on level 2");
    let log = h.persistent["log"].clone().into_array().unwrap();
    assert!(log.iter().any(|m| m.to_string().contains("Welcome to level 2")));
    clean(&h);
}

#[test]
fn the_character_sheet_opens_and_closes() {
    let mut h = floor_with(1, vec![]);
    h.frame(Some("c"));
    h.frame(None);
    let p = h.player_id();
    assert!(h.world.vars[&p].values.contains_key("sheet"));
    for _ in 0..3 {
        h.frame(Some("enter"));
        h.frame(None);
    }
    assert!(!h.world.vars[&p].values.contains_key("sheet"), "Enter closed it");
    clean(&h);
}

#[test]
fn the_amulet_carried_up_the_stairs_wins_the_game() {
    let mut h = floor_with(20, vec![]);
    // The way up is where the player starts.
    h.turn("enter");
    let log = h.persistent["log"].clone().into_array().unwrap();
    assert!(
        log.iter().any(|m| m.to_string().contains("can't leave without")),
        "not without the Amulet"
    );
    assert!(!h.persistent.contains_key("won"));
    h.persistent
        .insert("inventory".into(), rhai::Dynamic::from(vec![item("amulet", "Amulet of Ember")]));
    h.turn("enter");
    assert!(h.persistent.contains_key("won"), "carrying it, you win");
    h.frame(Some("enter"));
    h.frame(None);
    assert!(h.take_pending_level().is_some_and(|l| l.path.ends_with("title.level")));
    clean(&h);
}
