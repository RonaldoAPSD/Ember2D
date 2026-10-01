// tests/roguelike_dungeon.rs — Step 9.5-3 (docs/ember2d-master-plan.md
// §5.8.5): the roguelike demo's generated floors (`demos/roguelike/
// dungeon.level` + `scripts/dungeon.rhai`) — floors 1, 10 and 20 of a seed
// are well-formed and fully connected, the same seed always makes the same
// floor, and a fight, the stairs and death play out from real key presses.

mod common;

use common::TurnHarness;
use ember2d::prelude::*;
use ember2d_sim::scripting::LogLevel;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const DUNGEON: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/dungeon.level");

/// A run's persistent state at `depth`, as the title screen's New game
/// seeds it (`new_run` in title.rhai).
fn run(seed: i64, depth: i64) -> BTreeMap<String, rhai::Dynamic> {
    let mut p = BTreeMap::new();
    for (k, v) in [
        ("run_seed", seed),
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
    p.insert("inventory".into(), rhai::Dynamic::from(rhai::Array::new()));
    p.insert("log".into(), rhai::Dynamic::from(rhai::Array::new()));
    p
}

fn floor(seed: i64, depth: i64) -> TurnHarness {
    let data = LevelData::load(DUNGEON).expect("dungeon.level");
    let mut h = TurnHarness::continue_run(data, run(seed, depth));
    h.sim.set_ai_turns_per_step(256); // as the project sets it
    h.frame(None); // the on_start pass's writes (monster scripts) settle
    h
}

/// Every entity tagged `tag`, with its cell.
fn tagged(h: &TurnHarness, tag: &str) -> Vec<(EntityId, (i32, i32))> {
    h.world
        .tags
        .iter()
        .filter(|(_, t)| t.name == tag)
        .map(|(&id, _)| {
            let p = h.world.get_global_position(id);
            (id, (p.x as i32, p.y as i32))
        })
        .collect()
}

/// Every walkable cell reachable from the player, by flood fill over the
/// generated tilemap.
fn reachable(h: &TurnHarness) -> BTreeSet<(i32, i32)> {
    let map = h.world.tilemaps.values().next().expect("the generated map");
    let start = h.player_pos();
    let start = (start.x as i32, start.y as i32);
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some((x, y)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (x + dx, y + dy);
            if map.index(n.0, n.1).is_some() && !map.solid_at(n.0, n.1, 0) && seen.insert(n) {
                queue.push_back(n);
            }
        }
    }
    seen
}

fn floor_cells(h: &TurnHarness) -> usize {
    let map = h.world.tilemaps.values().next().unwrap();
    map.iter_tiles().filter(|(_, x, y, _)| !map.solid_at(*x, *y, 0)).count()
}

fn problems(h: &TurnHarness) -> Vec<String> {
    h.logs.iter().filter(|l| l.level != LogLevel::Info).map(|l| l.text.clone()).collect()
}

#[test]
fn floors_1_10_and_20_are_whole_connected_and_stocked_by_depth() {
    let mut strongest = Vec::new();
    for depth in [1, 10, 20] {
        let h = floor(777, depth);
        let map = h.world.tilemaps.values().next().expect("a tilemap");
        assert_eq!((map.width, map.height), (80, 43), "depth {depth}");
        let reach = reachable(&h);
        assert_eq!(reach.len(), floor_cells(&h), "depth {depth}: every floor cell is reachable");
        assert!(
            reach.len() > 400,
            "depth {depth}: a real dungeon, not a closet ({} cells)",
            reach.len()
        );

        let monsters = tagged(&h, "monster");
        assert!(!monsters.is_empty(), "depth {depth}: monsters");
        let cells: BTreeSet<_> = monsters.iter().map(|m| m.1).collect();
        assert_eq!(cells.len(), monsters.len(), "depth {depth}: one monster per cell");
        for (id, at) in &monsters {
            assert!(reach.contains(at), "depth {depth}: monster {id} at {at:?} is on the floor");
            assert!(h.world.actors.contains_key(id), "depth {depth}: monster {id} takes turns");
        }
        let hp = |id: &EntityId| {
            h.world
                .vars
                .get(id)
                .and_then(|v| v.values.get("hp"))
                .and_then(|d| d.as_int().ok())
                .unwrap_or(0)
        };
        strongest.push(monsters.iter().map(|(id, _)| hp(id)).max().unwrap());

        if depth < 20 {
            let stairs = tagged(&h, "stairs");
            assert_eq!(stairs.len(), 1, "depth {depth}: one way down");
            assert!(reach.contains(&stairs[0].1), "depth {depth}: the stairs can be reached");
        } else {
            assert!(tagged(&h, "stairs").is_empty(), "the bottom has no way further down");
            let amulet = tagged(&h, "item").into_iter().find(|(id, _)| {
                h.world.vars.get(id).and_then(|v| v.values.get("kind")).map(|k| k.to_string())
                    == Some("amulet".into())
            });
            assert!(amulet.is_some_and(|a| reach.contains(&a.1)), "the Amulet lies within reach");
            assert!(monsters.iter().any(|(id, _)| hp(id) >= 100), "and the Ember Drake guards it");
            assert_eq!(tagged(&h, "upstairs").len(), 1, "with the way out where you start");
        }
        assert!(h.world.fov.is_some(), "depth {depth}: fog of war is on");
        assert!(problems(&h).is_empty(), "depth {depth}: {:#?}", problems(&h));
    }
    assert!(
        strongest[0] < strongest[1] && strongest[1] < strongest[2],
        "deeper is deadlier: {strongest:?}"
    );
}

#[test]
fn a_seed_and_depth_always_make_the_same_floor() {
    let layout = |h: &TurnHarness| {
        let map = h.world.tilemaps.values().next().unwrap();
        let cells: Vec<_> = map.iter_tiles().map(|(_, x, y, d)| (x, y, d.name.clone())).collect();
        let mut monsters: Vec<_> = tagged(h, "monster").into_iter().map(|m| m.1).collect();
        monsters.sort();
        (cells, monsters, h.player_pos())
    };
    assert_eq!(layout(&floor(42, 5)), layout(&floor(42, 5)));
    assert_ne!(layout(&floor(42, 5)).0, layout(&floor(43, 5)).0, "another seed, another dungeon");
    assert_ne!(layout(&floor(42, 5)).0, layout(&floor(42, 6)).0, "another depth, another floor");
}

#[test]
fn bumping_a_monster_attacks_it_and_killing_it_pays_experience() {
    let mut h = floor(9, 1);
    let (rat, at) = tagged(&h, "monster")[0];
    // Stand just west of it (a floor cell next to a monster: it's in a
    // room, so its west neighbour is floor unless that's a wall).
    let p = h.player_id();
    let west = (at.0 - 1, at.1);
    let map = h.world.tilemaps.values().next().unwrap().clone();
    let from =
        if !map.solid_at(west.0, west.1, 0) { (west, "right") } else { ((at.0 + 1, at.1), "left") };
    h.world.transforms.get_mut(&p).unwrap().position =
        Vec2::new(from.0 .0 as f32, from.0 .1 as f32);
    h.world.vars.get_mut(&rat).unwrap().values.insert("hp".into(), rhai::Dynamic::from(1_i64));
    h.world.vars.get_mut(&rat).unwrap().values.insert("defense".into(), rhai::Dynamic::from(0_i64));
    h.turn(from.1);
    assert!(!h.world.transforms.contains_key(&rat), "one blow killed it");
    assert!(!tagged(&h, "corpse").is_empty(), "it left remains");
    let xp = h
        .persistent
        .get("xp")
        .and_then(|v| v.as_float().ok().or(v.as_int().ok().map(|i| i as f64)))
        .unwrap();
    assert!(xp > 0.0, "and experience");
    let log = h.persistent["log"].clone().into_array().unwrap();
    assert!(log.iter().any(|m| m.to_string().contains("dies!")), "{log:?}");
    assert!(problems(&h).is_empty(), "{:#?}", problems(&h));
}

#[test]
fn the_stairs_lead_one_floor_deeper() {
    let mut h = floor(5, 3);
    let (_, at) = tagged(&h, "stairs")[0];
    let p = h.player_id();
    h.world.transforms.get_mut(&p).unwrap().position = Vec2::new(at.0 as f32, at.1 as f32);
    h.turn("enter");
    assert!(
        h.take_pending_level().is_some_and(|l| l.path.ends_with("dungeon.level")),
        "the next floor loads"
    );
    assert_eq!(h.persistent.get("depth").and_then(|v| v.as_int().ok()), Some(4));
}

#[test]
fn dying_shows_the_death_screen_and_enter_returns_to_the_title() {
    let mut h = floor(5, 1);
    h.persistent.insert("hp".into(), rhai::Dynamic::from(0_i64));
    h.frame(None);
    h.frame(Some("enter"));
    h.frame(None);
    assert!(h.take_pending_level().is_some_and(|l| l.path.ends_with("title.level")));
}

/// The direction key for one step from `a` to `b`.
fn key_toward(a: (i32, i32), b: (i32, i32)) -> &'static str {
    match (b.0 - a.0, b.1 - a.1) {
        (1, _) => "right",
        (-1, _) => "left",
        (_, 1) => "down",
        _ => "up",
    }
}

/// The first step of a shortest walk from the player to `goal`, over
/// floor cells.
fn first_step(h: &TurnHarness, goal: (i32, i32)) -> Option<(i32, i32)> {
    let map = h.world.tilemaps.values().next()?;
    let p = h.player_pos();
    let start = (p.x as i32, p.y as i32);
    let mut came: BTreeMap<(i32, i32), (i32, i32)> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut seen = BTreeSet::from([start]);
    while let Some(c) = queue.pop_front() {
        if c == goal {
            let mut cur = c;
            while came.get(&cur) != Some(&start) {
                cur = *came.get(&cur)?;
            }
            return Some(cur);
        }
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = (c.0 + dx, c.1 + dy);
            if map.index(n.0, n.1).is_some() && !map.solid_at(n.0, n.1, 0) && seen.insert(n) {
                came.insert(n, c);
                queue.push_back(n);
            }
        }
    }
    None
}

#[test]
fn a_bot_plays_down_ten_floors_without_a_single_script_problem() {
    // A soak test: real key presses through ten generated floors — every
    // monster turn, attack, kill, pickup and stairway on the way runs the
    // shipped scripts. The bot fights whatever is next to it, otherwise
    // walks the shortest way to the stairs, and is kept alive and given the
    // power a player would have found by then in gear and levels (this
    // tests the scripts, not the bot's tactics).
    let mut h = floor(31337, 1);
    let mut turns = 0;
    for depth in 1..=10 {
        let mut here = 0;
        loop {
            turns += 1;
            here += 1;
            assert!(here < 2000, "stuck on depth {depth}");
            let max = h.persistent["max_hp"].clone();
            h.persistent.insert("hp".into(), max);
            h.persistent.insert("power".into(), rhai::Dynamic::from(2_i64 + depth as i64));
            let p = h.player_pos();
            let me = (p.x as i32, p.y as i32);
            let foe = tagged(&h, "monster")
                .into_iter()
                .find(|(_, at)| (at.0 - me.0).abs() + (at.1 - me.1).abs() == 1);
            if let Some((_, at)) = foe {
                h.turn(key_toward(me, at));
                continue;
            }
            let stairs = tagged(&h, "stairs")[0].1;
            if me == stairs {
                h.turn("enter");
                break;
            }
            let step = first_step(&h, stairs).expect("the stairs are reachable");
            h.turn(key_toward(me, step));
        }
        let next = h.take_pending_level().expect("the stairs load the next floor");
        let problems = problems(&h);
        assert!(problems.is_empty(), "depth {depth}: {problems:#?}");
        let persistent = std::mem::take(&mut h.persistent);
        h = TurnHarness::continue_run(next, persistent);
        h.sim.set_ai_turns_per_step(256);
        h.frame(None);
    }
    assert_eq!(h.persistent.get("depth").and_then(|v| v.as_int().ok()), Some(11));
    let kills = h.persistent.get("kills").map(|v| v.to_string()).unwrap_or_default();
    assert!(problems(&h).is_empty(), "{:#?}", problems(&h));
    println!("bot: {turns} turns, {kills} kills, ten floors");
}
