// tests/tilemap_equivalence.rs — Step 8-1 (docs/ember2d-master-plan.md
// §5.7): the `Tilemap` component's contract is "a wall is a wall, however
// it's stored". Every test here loads the real shipped floor2 twice:
//
//   - TILEMAP mode: exactly as the game loads it — its v4 `tilemap`
//     section becomes one `Tilemap` entity, only interactive tiles spawn.
//   - ENTITY mode: the same level with every static tile forced to stay an
//     entity, i.e. the pre-8-1 world. The lever is `camera_follow = true`
//     on each of them — `TileRecord::is_static` keeps a camera-follow tile
//     an entity, and nothing a query or the physics does ever reads
//     `camera_follow` (it only picks which entity the camera tracks, and
//     the camera position feeds nothing compared below).
//
// and asserts both worlds answer every spatial query, and move a body into
// the walls, identically. Entity ids legitimately differ between the two
// (a wall's id vs. the tilemap's), so every comparison is on positions,
// hit/no-hit, and "is it a wall" — the things a script's behavior depends on.

use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::{BTreeMap, BTreeSet};

const FLOOR2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/roguelike/floor2.level");

/// Writes the probe script to a per-process temp file (7A-8's convention —
/// two `cargo test` processes never share a path) and returns its path.
/// A real file, not an in-memory `LevelSource`: `ScriptEngine::compile`
/// reads a script path directly, it doesn't route through `LevelSource`.
fn probe_path(src: &str) -> String {
    let dir =
        std::env::temp_dir().join(format!("ember2d-{}", std::process::id())).join("tilemap_eq");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("probe.rhai");
    std::fs::write(&path, src).expect("write probe");
    path.to_string_lossy().into_owned()
}

fn floor2(entity_mode: bool) -> LevelData {
    // The demo levels name their scripts by repo-root-relative path
    // ("demos/roguelike/scripts/..."), so this binary needs the repo root
    // as CWD — the same one-liner `tests/common/mod.rs`'s own
    // `ensure_workspace_root_cwd` runs (private there; see CLAUDE.md's
    // workspace-layout note for why `cargo test` starts elsewhere).
    let _ = std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
    let mut data = LevelData::load(FLOOR2).expect("floor2 must load");
    assert_eq!(data.version, ember2d_sim::level::LEVEL_FORMAT_VERSION);
    if entity_mode {
        let path = data.path.clone();
        let mut tiles = data.all_tiles();
        for t in &mut tiles {
            if t.is_static() {
                t.camera_follow = true;
            }
        }
        data.tiles = tiles;
        data.tilemap = None;
        data.path = path;
    }
    data
}

fn start(
    data: LevelData,
    probe: Option<&str>,
) -> (Simulation, World, BTreeMap<String, rhai::Dynamic>) {
    let mut data = data;
    if let Some(src) = probe {
        data.player.script = Some(probe_path(src));
    }
    let mut sim = Simulation::new(data);
    sim.set_level_source(Box::new(ember2d::level_source::FsLevelSource));
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let logs = sim.on_start(&mut world, 80, 24, &mut persistent);
    for l in &logs {
        assert!(
            !matches!(l.level, ember2d_sim::scripting::LogLevel::Error),
            "no script may fail while probing: {}",
            l.text
        );
    }
    (sim, world, persistent)
}

/// Runs in the player's `on_start` and writes one big string global. Every
/// line is id-independent. `is_wall` is the same test `bullet.rhai` uses:
/// true for a tilemap hit (tilemap mode) and for a "wall"-tagged entity
/// (entity mode), false for anything else (an enemy, the stairs, gold).
const PROBE: &str = r##"
fn is_wall(ctx, h) { ctx.is_tilemap(h) || ctx.has_tag(h, "wall") }

fn grid(ctx, w, h) {
    let s = "";
    for y in 0..h {
        for x in 0..w {
            let fx = x + 0.5;
            let fy = y + 0.5;
            let solid = ctx.is_solid_at(fx, fy);
            let e = ctx.get_entity_at(fx, fy);
            let c = if solid { "#" } else { "." };
            if e != -1 && is_wall(ctx, e) { c += "w"; } else if e != -1 { c += "e"; }
            let r = ctx.find_entities_in_rect(x + 0.3, y + 0.3, 0.4, 0.4);
            let walls = 0;
            for h in r { if is_wall(ctx, h) { walls += 1; } }
            c += if walls > 0 { "W" } else { "-" };
            s += c;
        }
        s += "\n";
    }
    s
}

// One ray per call, split out of `rays` for the same complexity-guard
// reason as the flat arrays below.
fn one_ray(ctx, sx, sy, ex, ey) {
    let hit = ctx.raycast(sx, sy, ex, ey, []);
    let masked = ctx.raycast(sx, sy, ex, ey, ["nonexistent"]);
    let m = `m${masked.len()};`;
    if hit.is_empty() { return "none;" + m; }
    let tag = if is_wall(ctx, hit[0]) { "wall" } else { "other" };
    let hx = hit[1];
    let hy = hit[2];
    tag + "@" + hx + "," + hy + ";" + m
}

fn rays(ctx, sx, sy) {
    let s = "";
    // Two flat arrays, not one array of pairs: a nested literal this size
    // trips Rhai's max-expression-complexity guard at compile time.
    let dxs = [1, -1, 0, 0, 1, -1, 1, -1, 3, -2];
    let dys = [0, 0, 1, -1, 1, 1, -1, -1, 1, 5];
    for i in 0..dxs.len() {
        let ex = sx + dxs[i] * 40.0;
        let ey = sy + dys[i] * 40.0;
        s += one_ray(ctx, sx, sy, ex, ey);
    }
    s
}

fn paths(ctx, sx, sy) {
    let s = "";
    let txs = [5.0, 30.0, 50.0, 10.0, 60.0];
    let tys = [5.0, 10.0, 20.0, 30.0, 5.0];
    for i in 0..txs.len() {
        let p4 = ctx.get_path(sx, sy, txs[i], tys[i], []);
        let p8 = ctx.get_path(sx, sy, txs[i], tys[i], [], true);
        s += `${p4.len()}/${p8.len()};`;
    }
    s
}

fn on_start(id, ctx) {
    let sx = ctx.get_x(id) + 0.5;
    let sy = ctx.get_y(id) + 0.5;
    let out = grid(ctx, 80, 40);
    out += rays(ctx, sx, sy);
    out += paths(ctx, ctx.get_x(id), ctx.get_y(id));
    out += `reach${ctx.reachable_within(id, 6).len()}`;
    ctx.set_global("probe", out);
}
"##;

fn probe(entity_mode: bool) -> String {
    let (sim, world, _) = start(floor2(entity_mode), Some(PROBE));
    if entity_mode {
        assert!(world.tilemaps.is_empty(), "entity mode must really be the pre-8-1 world");
    } else {
        assert_eq!(world.tilemaps.len(), 1, "tilemap mode must really collapse the walls");
    }
    sim.globals().get("probe").map(|d| d.to_string()).expect("the probe must have run")
}

#[test]
fn every_spatial_query_answers_the_same_for_tilemap_cells_as_for_wall_entities() {
    let tilemap = probe(false);
    let entities = probe(true);
    // Sanity: the probe must have actually exercised each query against
    // real walls, or an empty-vs-empty comparison would pass vacuously.
    assert!(tilemap.contains("#wW"), "the grid scan must see solid wall cells");
    assert!(tilemap.contains("wall@"), "at least one ray must hit a wall");
    assert!(!tilemap.contains("reach0"), "reachable_within must find open cells");
    assert_eq!(tilemap, entities);
}

/// A body driven into walls from several directions, stepped through the
/// real detect-collisions → `late_step` resolution sequence, must end every
/// frame at exactly the same position in both worlds — the push-out order
/// and arithmetic (`World::push_out_of`) are the same, so the floats are.
#[test]
fn a_mover_is_pushed_out_of_tilemap_walls_exactly_as_out_of_wall_entities() {
    let run = |entity_mode: bool| -> Vec<(f32, f32)> {
        let (mut sim, mut world, mut persistent) = start(floor2(entity_mode), None);
        let player = world.find_by_tag("player").expect("player");
        let mut trace = Vec::new();
        let velocities =
            [(9.0, 0.0), (0.0, 9.0), (-9.0, 0.0), (0.0, -9.0), (7.0, 7.0), (-7.0, 5.0)];
        for (i, &(vx, vy)) in velocities.iter().cycle().take(240).enumerate() {
            if let Some(tf) = world.transforms.get_mut(&player) {
                tf.velocity = Vec2::new(vx, vy);
            }
            let prev = world.snapshot_positions();
            world.integrate_physics(1.0 / 60.0);
            let mut events = EventBus::new();
            world.detect_collisions(&mut events);
            sim.late_step(
                &mut world,
                &events,
                &prev,
                Vec2::ZERO,
                1.0 / 60.0,
                i as f32 / 60.0,
                80,
                24,
                &mut persistent,
            );
            let p = world.transforms[&player].position;
            trace.push((p.x, p.y));
        }
        trace
    };
    let tilemap = run(false);
    let entities = run(true);
    let moved: BTreeSet<(i64, i64)> =
        tilemap.iter().map(|&(x, y)| ((x * 100.0) as i64, (y * 100.0) as i64)).collect();
    assert!(
        moved.len() > 10,
        "the mover must actually move and collide, or this test proves nothing"
    );
    assert_eq!(tilemap, entities);
}

/// The whole game, not just queries: floor2 played through its real
/// scripts (player.rhai, enemy.rhai, pickups, stairs) for a fixed input
/// sequence must produce the same player position and the same script
/// globals (HP, gold, messages — everything the HUD shows) every step in
/// both worlds.
#[test]
fn floor2_plays_out_identically_with_and_without_the_tilemap() {
    let run = |entity_mode: bool| -> Vec<String> {
        let (mut sim, mut world, mut persistent) = start(floor2(entity_mode), None);
        let player = world.find_by_tag("player").expect("player");
        let keys = ["d", "d", "s", "s", "d", "w", "a", "space", "d", "s"];
        let mut trace = Vec::new();
        for i in 0..160 {
            let mut held = BTreeSet::new();
            held.insert(keys[(i / 3) % keys.len()].to_string());
            let input = InputSnapshot { held: held.clone(), pressed: held };
            let gamepad = GamepadSnapshot::default();
            let outcome = sim.step(
                &mut world,
                StepInput {
                    input: &input,
                    mouse: MouseSnapshot::default(),
                    gamepad: &gamepad,
                    external_commands: &[],
                    animating: &[],
                    camera_origin: Vec2::ZERO,
                    sim_dt: 1.0 / 60.0,
                    elapsed: i as f32 / 60.0,
                    viewport_w: 80,
                    viewport_h: 24,
                },
                &mut persistent,
            );
            if outcome.turn_triggered {
                let prev = world.snapshot_positions();
                let mut events = EventBus::new();
                world.detect_collisions(&mut events);
                sim.late_step(
                    &mut world,
                    &events,
                    &prev,
                    Vec2::ZERO,
                    1.0 / 60.0,
                    i as f32 / 60.0,
                    80,
                    24,
                    &mut persistent,
                );
            }
            let p = world.transforms.get(&player).map(|t| t.position).unwrap_or(Vec2::ZERO);
            trace.push(format!("{:?} {:?} {:?}", p, sim.globals(), persistent));
        }
        trace
    };
    let tilemap = run(false);
    let entities = run(true);
    assert_ne!(tilemap.first(), tilemap.last(), "the player must actually do something");
    assert_eq!(tilemap, entities);
}
