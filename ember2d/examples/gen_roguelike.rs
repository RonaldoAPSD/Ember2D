// examples/gen_roguelike.rs — generates the roguelike demo's `.level` files
// and `project.ron`.
//
// ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
//
// Phase 4 of docs/ember2d-refactor-plan.md moves all gameplay logic out of
// `PlayState` and into Rhai scripts. The original six-level demo (now
// archived at docs/archive/demo/) couldn't prove that — only two of its six
// levels had a player script attached at all — so instead we build a small
// turn-based roguelike from scratch. Its
// `.level` files are generated rather than hand-written: a single floor is
// already a few hundred RON tile records, far past anything worth hand-
// editing or reviewing line-by-line. This file — not the generated RON — is
// the reviewable source of truth for the dungeon layouts.
//
// ── WHY `examples/`, NOT `src/bin/` ──────────────────────────────────────────
//
// Cargo auto-generates and *runs* a test target for every `[[bin]]`, and this
// environment already can't execute the `main.rs` test binary (see
// docs/HANDOFF.md — "An Application Control policy has blocked this file").
// Adding a second bin would add a second blocked target under `cargo test`.
// Examples compile under `cargo test` but are never executed by it.
//
// ── RUN ──────────────────────────────────────────────────────────────────────
//   cargo run --example gen_roguelike
//
// Regenerates every file under `roguelike/*.level` and `roguelike/project.ron`
// from scratch. Re-run after editing a floor layout below; the output is
// committed to git same as hand-authored content would be.

use ember2d::prelude::*;

// ── Script paths ──────────────────────────────────────────────────────────────
// Every tile of a given kind across every floor shares one script file —
// Rhai here is built with `no_module` (see Cargo.toml), so scripts can't
// import shared code; one file per *role*, attached to many tiles, is the
// only sharing mechanism available.

const PLAYER_SCRIPT: &str = "roguelike/scripts/player.rhai";
const PICKUP_SCRIPT: &str = "roguelike/scripts/pickup.rhai";
const STAIRS_SCRIPT: &str = "roguelike/scripts/stairs.rhai";
const ENEMY_RAT_SCRIPT: &str = "roguelike/scripts/enemy_rat.rhai";
const ENEMY_BOSS_SCRIPT: &str = "roguelike/scripts/enemy_boss.rhai";
const VICTORY_SCRIPT: &str = "roguelike/scripts/victory.rhai";

// Fixed per floor so every run of this generator — and every play of the
// game — draws `random_*` (particles, etc.) from the same sequence.
// `LevelGrid::new` (the editor's own level constructor) picks this from OS
// entropy; a generator must not, or the fixture would be non-reproducible
// and every regeneration would produce a different diff.
const SEED_FLOOR1: u64 = 0xF10021;
const SEED_FLOOR2: u64 = 0xF10022;
const SEED_FLOOR3: u64 = 0xF10023;
const SEED_VICTORY: u64 = 0xF10024;

// ── Map: a carved wall/floor grid ─────────────────────────────────────────────
//
// Deliberately not `editor::grid::LevelGrid` — that stores tiles in a
// HashMap for the editor's O(1) point lookups, which has no defined
// iteration order. Building the tile list by hand here means every cell is
// visited in a fixed, known order (see `build_level`), and — just as
// important — every interior cell ends up EITHER wall OR floor, never
// unplaced. An unplaced cell has no collider at all, so `is_solid_at` would
// report it as open ground: a single missing wall tile would let the player
// walk clean out of the level. Carving from an all-wall canvas makes that
// impossible by construction rather than by careful placement.
struct Map {
    w: usize,
    h: usize,
    floor: Vec<Vec<bool>>,
}

impl Map {
    fn new(w: usize, h: usize) -> Self {
        Map { w, h, floor: vec![vec![false; w]; h] }
    }

    /// Carve an open rectangle of floor. Callers must leave at least a
    /// 1-cell margin from the map edge so the outer wall ring stays intact.
    fn room(&mut self, x: usize, y: usize, w: usize, h: usize) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.floor[yy][xx] = true;
            }
        }
    }

    fn corridor_h(&mut self, x1: usize, x2: usize, y: usize) {
        let (lo, hi) = (x1.min(x2), x1.max(x2));
        for xx in lo..=hi { self.floor[y][xx] = true; }
    }

    fn corridor_v(&mut self, y1: usize, y2: usize, x: usize) {
        let (lo, hi) = (y1.min(y2), y1.max(y2));
        for yy in lo..=hi { self.floor[yy][x] = true; }
    }
}

/// Turn a carved `Map` plus a list of feature tiles (items, stairs, enemies)
/// into a seeded, sorted `LevelData`.
///
/// Sorting is mandatory, not cosmetic: `LevelData.tiles` is a `Vec`, but the
/// editor's `LevelGrid::to_level_data` collects it from a HashMap, and this
/// generator builds its own list by iterating `map.floor` — both need an
/// explicit, stable order or two runs of this program would produce
/// different byte streams (and, worse, a different entity-id assignment in
/// `PlayState::do_on_start`, since ids are handed out in tile order).
fn build_level(name: &str, map: &Map, seed: u64, spawn: (f32, f32), features: Vec<TileRecord>) -> LevelData {
    let mut data = LevelData::empty(map.w, map.h);
    data.name = name.to_string();
    data.seed = seed;
    data.spawn_point = spawn;

    for y in 0..map.h {
        for x in 0..map.w {
            if map.floor[y][x] {
                data.tiles.push(TileRecord::new(x as i32, y as i32, 0, '.', Color::DarkGrey, Color::Reset, false, false, "floor"));
            } else {
                data.tiles.push(TileRecord::new(x as i32, y as i32, 1, '#', Color::Grey, Color::Reset, true, false, "wall"));
            }
        }
    }
    data.tiles.extend(features);
    data.tiles.sort_by_key(|t| (t.layer, t.y, t.x));
    data
}

/// A pickup tile — gold or potion. `pickup.rhai` reads its own `tag` at
/// runtime to decide which resource it grants, so the tag string here is
/// the only place that decision lives (never in engine code).
fn item(x: i32, y: i32, glyph: char, fg: Color, tag: &str) -> TileRecord {
    let mut t = TileRecord::new(x, y, 1, glyph, fg, Color::Reset, false, true, tag);
    t.script = Some(PICKUP_SCRIPT.to_string());
    t
}

/// A stairs-down tile. The engine owns the actual level transition
/// (`next_level` + `Collider.locked`, see `play.rs::late_update`);
/// `stairs.rhai` only ever toggles the lock and its tint.
fn stairs(x: i32, y: i32, next_level: &str) -> TileRecord {
    let mut t = TileRecord::new(x, y, 1, '>', Color::Cyan, Color::Reset, false, true, "stairs");
    t.script = Some(STAIRS_SCRIPT.to_string());
    t.next_level = Some(next_level.to_string());
    t
}

/// An enemy_rat.rhai-driven rat. Solid (not a trigger): `Collider::new`
/// hardcodes `solid: true` for the player regardless of `PlayerRecord.solid`
/// (a noted engine fact in the Phase 4 plan file), so a solid rat collider
/// is what lets `is_solid_at` block both the player's and another rat's
/// movement into it — no separate `get_entity_at` check needed on either
/// side. `player.rhai`'s own bump-to-attack path finds it via
/// `get_entity_at` + `has_tag(_, "enemy")` regardless of the solid flag.
fn rat(x: i32, y: i32) -> TileRecord {
    let mut t = TileRecord::new(x, y, 1, 'r', Color::Red, Color::Reset, true, false, "enemy");
    t.script = Some(ENEMY_RAT_SCRIPT.to_string());
    // Step 5f: an Ai actor, so TurnScheduler gives it a turn each round —
    // see level.rs's TileRecord::actor.
    t.actor = Some(ActorRecord::default());
    t
}

/// An enemy_boss.rhai-driven boss — floor 3's finale. Same solid-collider
/// reasoning as `rat()` above. Tagged "boss", not "enemy": stairs.rhai
/// already locks any level's stairs while `ctx.count_by_tag("boss") > 0`,
/// so placing this tile is the entire boss-gate mechanism.
fn boss(x: i32, y: i32) -> TileRecord {
    let mut t = TileRecord::new(x, y, 1, 'B', Color::DarkMagenta, Color::Reset, true, false, "boss");
    t.script = Some(ENEMY_BOSS_SCRIPT.to_string());
    // Step 5f: an Ai actor, same as rat() above.
    t.actor = Some(ActorRecord::default());
    t
}

// ── Floor 1 — movement and pickups, no enemies yet ────────────────────────────

fn floor1() -> LevelData {
    let w = 40usize;
    let h = 20usize;
    let mut map = Map::new(w, h);
    // One open hall: floor 1's job is teaching movement and pickups, not
    // navigation, so there's nothing to gain from multiple rooms yet.
    map.room(2, 2, w - 4, h - 4);

    let spawn = (4.0, 4.0);
    let features = vec![
        item(10, 6, '$', Color::Yellow, "gold"),
        item(20, 10, '$', Color::Yellow, "gold"),
        item(30, 5, '!', Color::Magenta, "potion"),
        stairs((w - 4) as i32, (h - 4) as i32, "roguelike/floor2.level"),
    ];

    let mut data = build_level("Floor 1", &map, SEED_FLOOR1, spawn, features);
    data.player.script = Some(PLAYER_SCRIPT.to_string());
    data.player.camera_follow = true;
    data
}

// ── Floor 2 — introduces combat (three rats) across a multi-room layout.
// Deliberately larger than the 80×24 viewport (unlike floor 1's single hall)
// so it also exercises the camera clamp against level edges. ─────────────────

fn floor2() -> LevelData {
    let w = 80usize;
    let h = 32usize;
    let mut map = Map::new(w, h);
    map.room(2, 2, 20, 12);       // room A: entry, x:[2,21] y:[2,13]
    map.corridor_h(21, 45, 7);    // A -> B
    map.room(44, 2, 26, 20);      // room B: combat arena, x:[44,69] y:[2,21]
    map.corridor_v(21, 27, 56);   // B -> C
    map.room(50, 26, 20, 4);      // room C: stairs, x:[50,69] y:[26,29]

    let spawn = (5.0, 5.0);
    let features = vec![
        item(10, 8, '$', Color::Yellow, "gold"),
        item(16, 5, '!', Color::Magenta, "potion"),
        rat(48, 6),
        rat(56, 14),
        rat(63, 8),
        item(50, 18, '$', Color::Yellow, "gold"),
        item(66, 4, '$', Color::Yellow, "gold"),
        item(55, 27, '!', Color::Magenta, "potion"),
        stairs(65, 27, "roguelike/floor3.level"),
    ];

    let mut data = build_level("Floor 2", &map, SEED_FLOOR2, spawn, features);
    data.player.script = Some(PLAYER_SCRIPT.to_string());
    data.player.camera_follow = true;
    data
}

// ── Floor 3 — the finale dungeon floor: an entry room, then the boss arena
// (two warm-up rats plus the boss). Stairs here are boss-gated purely by
// stairs.rhai's existing `ctx.count_by_tag("boss") > 0` check — placing a
// `boss()` tile on this level is the entire gating mechanism. ────────────────

fn floor3() -> LevelData {
    let w = 56usize;
    let h = 28usize;
    let mut map = Map::new(w, h);
    map.room(2, 2, 16, 10);      // room A: entry, x:[2,17] y:[2,11]
    map.corridor_h(17, 30, 6);   // A -> B
    map.room(28, 2, 24, 20);     // room B: boss arena, x:[28,51] y:[2,21]

    let spawn = (5.0, 5.0);
    let features = vec![
        item(9, 7, '$', Color::Yellow, "gold"),
        item(9, 9, '!', Color::Magenta, "potion"),
        rat(32, 6),
        rat(45, 15),
        boss(40, 10),
        item(48, 4, '$', Color::Yellow, "gold"),
        stairs(49, 18, "roguelike/victory.level"),
    ];

    let mut data = build_level("Floor 3", &map, SEED_FLOOR3, spawn, features);
    data.player.script = Some(PLAYER_SCRIPT.to_string());
    data.player.camera_follow = true;
    data
}

// ── Victory — the finale: a small, enemy-free room (victory.rhai handles
// free-roam movement and a run summary; no combat, no pickups, no exit). ────

fn victory() -> LevelData {
    let w = 20usize;
    let h = 10usize;
    let mut map = Map::new(w, h);
    map.room(1, 1, w - 2, h - 2);

    let spawn = (w as f32 / 2.0, h as f32 / 2.0);
    let mut data = build_level("Victory!", &map, SEED_VICTORY, spawn, vec![]);
    data.player.script = Some(VICTORY_SCRIPT.to_string());
    data.player.camera_follow = true;
    data
}

fn main() {
    let out_dir = std::path::Path::new("roguelike");
    std::fs::create_dir_all(out_dir).expect("create roguelike/ directory");

    let floors: Vec<(LevelData, &str)> = vec![
        (floor1(), "floor1.level"),
        (floor2(), "floor2.level"),
        (floor3(), "floor3.level"),
        (victory(), "victory.level"),
    ];

    for (data, filename) in floors {
        let path = out_dir.join(filename);
        data.save(path.to_str().unwrap()).expect("save level");
        println!("wrote {}", path.display());
    }

    // TurnBased is the whole point of this demo (see the Phase 4 plan): it
    // exercises the engine's turn-gating path, which no prior demo touched.
    let mut project = ProjectData::new("Roguelike", VisualStyle::ClassicASCII, GameplayLoop::TurnBased);
    project.start_level = Some("floor1.level".to_string());
    project.save(out_dir.to_str().unwrap()).expect("save project.ron");
    println!("wrote roguelike/project.ron");
}
