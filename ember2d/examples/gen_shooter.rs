// examples/gen_shooter.rs — generates the top-down shooter demo's
// `shooter/arena.level` and `shooter/project.ron`.
//
// ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
//
// The roguelike demo (examples/gen_roguelike.rs) proves the engine can build a
// turn-based grid game. It proves nothing about the other half of the engine:
// `GameplayLoop::RealTime`, velocity integration, continuous input, and
// runtime entity spawning. This demo is the second genre — a wave-based
// twin-stick arena shooter — built to exercise exactly that path with zero
// Rust gameplay code, same as the roguelike.
//
// Same generator rationale as gen_roguelike.rs: a level is hundreds of RON
// tile records, so this file (not the generated RON) is the reviewable source
// of truth for the arena layout. Same `examples/` rather than `src/bin/`
// reasoning too — see that file's header for the Application Control policy
// detail.
//
// ── WHAT THE REALTIME PATH FORCED, THAT TURN-BASED DIDN'T ────────────────────
//
// Three engine facts shaped this demo's architecture. They're recorded here
// because each one is invisible until you try to build a realtime game, and
// each one would otherwise look like an arbitrary scripting choice:
//
//  1. `Simulation::late_step` resolves solid collisions ONLY for the local
//     player (it filters on `is_local_player`). Walls therefore stop the
//     player for free, but bullets and enemies pass straight through unless a
//     script says otherwise. director.rhai does that work explicitly.
//
//  2. Rhai can spawn entities but cannot attach a script to one — there's no
//     `set_script` in the API, and `apply_ctx`'s spawn queue only builds a
//     transform/sprite/collider/tag. So a wave shooter cannot give each
//     spawned enemy its own behavior script. Instead ONE always-present
//     director entity drives every enemy and bullet from its own `on_update`.
//     This is a genuine engine limitation, not a stylistic preference.
//
//  3. `Simulation::rebuild_scheduler` inserts every `Actor` into the
//     `TurnScheduler` regardless of `GameplayLoop`, and `Simulation::step`
//     only runs `on_input` for the actor at the scheduler's front. The player
//     is unconditionally `Actor::local(0)`, so as long as it stays the ONLY
//     actor, it's always at the front and `on_input` runs every step — which
//     is what a realtime controller needs. Giving an enemy an `ActorRecord`
//     here would silently start alternating turns with the player and stall
//     input. Hence: no `actor` field on anything in this file.
//
// ── RUN ──────────────────────────────────────────────────────────────────────
//   cargo run --example gen_shooter
//
// Regenerates `shooter/arena.level` and `shooter/project.ron` from scratch.

use ember2d::prelude::*;

const PLAYER_SCRIPT: &str = "shooter/scripts/player.rhai";
const DIRECTOR_SCRIPT: &str = "shooter/scripts/director.rhai";

// The arena is exactly the launch viewport (`Engine::new(80, 24, ...)` in
// ember2d-app/src/main.rs), so the whole playfield is visible at once with no
// camera scrolling — the genre expects to see every incoming enemy, unlike the
// roguelike's deliberately larger-than-viewport floors.
const W: usize = 80;
const H: usize = 24;

// Fixed, not OS entropy — same reproducibility reasoning as gen_roguelike.rs's
// per-floor seeds: this seeds the script `random_*` stream that picks enemy
// spawn positions and medkit drops, so a given run is replayable.
const SEED: u64 = 0x5407E2;

/// A wall/floor grid for the arena.
///
/// Inverted relative to gen_roguelike.rs's `Map`, which carves floor out of an
/// all-wall canvas: an arena is overwhelmingly open, so this starts all-floor
/// and stamps walls into it. The safety property that mattered there (no cell
/// left unplaced, so `is_solid_at` can't report a gap in the boundary as open
/// ground) is preserved differently here — `ring()` writes the entire
/// perimeter unconditionally, so the boundary is complete by construction
/// rather than by careful carving.
struct Arena {
    wall: Vec<Vec<bool>>,
}

impl Arena {
    fn new() -> Self {
        Arena { wall: vec![vec![false; W]; H] }
    }

    /// The unbroken perimeter wall. Nothing can leave the arena, which is also
    /// what lets bullets rely on hitting *something* rather than needing a
    /// lifetime timer (director.rhai keeps an out-of-bounds check anyway, as a
    /// cheap belt-and-braces against a future opening in this ring).
    fn ring(&mut self) {
        for x in 0..W {
            self.wall[0][x] = true;
            self.wall[H - 1][x] = true;
        }
        for y in 0..H {
            self.wall[y][0] = true;
            self.wall[y][W - 1] = true;
        }
    }

    /// A solid block of cover. Kept away from the centre so the player's spawn
    /// is never inside one.
    fn block(&mut self, x: usize, y: usize, w: usize, h: usize) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.wall[yy][xx] = true;
            }
        }
    }
}

/// Turn the arena grid plus its feature tiles into a seeded, sorted `LevelData`.
///
/// Sorted for the same reason gen_roguelike.rs sorts: entity ids are handed out
/// in tile order by `Simulation::do_on_start`, so an unstable order would mean
/// two runs of this generator produce different ids for the same tile.
///
/// **Floor cells are deliberately not emitted as tiles.** A tile that is
/// neither `solid` nor `trigger` gets no collider at all (see
/// `Simulation::do_on_start`), so an unwritten floor cell and a written one
/// behave identically to `is_solid_at`, physics, and collision detection —
/// unlike the roguelike, where the equivalent omission would have punched a
/// hole in a *wall*. Emitting all 1,920 cells would quadruple the file for
/// pure decoration. Instead a sparse dot grid is laid down as a movement
/// reference, which reads better for a shooter than a fully tiled floor.
fn build_level(map: &Arena, features: Vec<TileRecord>) -> LevelData {
    let mut data = LevelData::empty(W, H);
    data.name = "Arena".to_string();
    data.seed = SEED;
    data.spawn_point = (40.0, 12.0);

    for y in 0..H {
        for x in 0..W {
            if map.wall[y][x] {
                data.tiles.push(TileRecord::new(
                    x as i32, y as i32, 1, '#',
                    Color::DarkCyan, Color::Reset,
                    true, false, "wall",
                ));
            } else if x % 6 == 0 && y % 3 == 0 {
                data.tiles.push(TileRecord::new(
                    x as i32, y as i32, 0, '.',
                    Color::DarkGrey, Color::Reset,
                    false, false, "floor",
                ));
            }
        }
    }

    data.tiles.extend(features);
    data.tiles.sort_by_key(|t| (t.layer, t.y, t.x));
    data
}

/// The wave director: a single invisible, collider-less entity whose
/// `on_update` runs every step and drives the entire game — waves, enemy
/// steering, bullet hit resolution, scoring. See fact (2) in this file's
/// header for why one entity does all of it rather than each enemy carrying
/// its own script.
///
/// Placed at (0, 0), inside the wall ring, with a space glyph on layer 0: the
/// wall tile at the same cell is on layer 1 and therefore draws over it, so
/// the director is never visible. It is neither solid nor a trigger, so it has
/// no collider and can't be hit by a bullet or returned by a spatial query.
fn director() -> TileRecord {
    let mut t = TileRecord::new(0, 0, 0, ' ', Color::Reset, Color::Reset, false, false, "director");
    t.script = Some(DIRECTOR_SCRIPT.to_string());
    t
}

fn arena() -> LevelData {
    let mut map = Arena::new();
    map.ring();

    // Six cover blocks, symmetric about both axes so no spawn corner is
    // favoured, and all well clear of the (40, 12) player spawn.
    map.block(14, 5, 4, 2);
    map.block(62, 5, 4, 2);
    map.block(14, 17, 4, 2);
    map.block(62, 17, 4, 2);
    map.block(38, 4, 4, 2);
    map.block(38, 18, 4, 2);

    let mut data = build_level(&map, vec![director()]);

    data.player.script = Some(PLAYER_SCRIPT.to_string());
    data.player.camera_follow = true;
    data.player.glyph = '@';
    data.player.fg = Color::Green;
    // Slightly under one cell so the player can slip through a one-cell gap
    // without catching on the corners of the blocks above.
    data.player.collider_w = 0.7;
    data.player.collider_h = 0.7;

    data
}

fn main() {
    let out_dir = std::path::Path::new("shooter");
    std::fs::create_dir_all(out_dir).expect("create shooter/ directory");

    let path = out_dir.join("arena.level");
    arena().save(path.to_str().unwrap()).expect("save level");
    println!("wrote {}", path.display());

    // RealTime is the whole point of this demo — it's the branch of
    // `Engine::run` the roguelike never touches (see this file's header).
    let mut project = ProjectData::new("Ember Assault", VisualStyle::ClassicASCII, GameplayLoop::RealTime);
    project.start_level = Some("arena.level".to_string());
    project.save(out_dir.to_str().unwrap()).expect("save project.ron");
    println!("wrote shooter/project.ron");
}
