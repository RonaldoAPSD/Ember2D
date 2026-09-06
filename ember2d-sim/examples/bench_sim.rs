// examples/bench_sim.rs — headless performance baseline for Phase 6
// (docs/ember2d-phase6-plan.md Step 1).
//
// AN EXAMPLE, NOT A `[[bench]]` OR CRITERION TARGET. Three reasons, same
// shape as the decision `examples/gen_roguelike.rs` (in `ember2d`) already
// documents for itself: (1) no dev-dependency exists anywhere in this
// workspace today, and criterion would be the first, pulling in ~30
// transitive crates for a job a plain binary does fine; (2) `cargo bench`'s
// built-in harness needs a nightly toolchain; (3) this machine's Application
// Control policy intermittently blocks freshly-built *test* binaries — an
// example is compiled by `cargo build`/`cargo test` but never *executed* by
// either, so it never becomes a target for that policy to block. Living in
// `ember2d-sim` (not `ember2d`) is also load-bearing: this crate has four
// dependencies, so `cargo run --release -p ember2d-sim --example bench_sim`
// builds in seconds instead of pulling in wgpu/winit/kira, and it
// structurally proves the thing being measured is the sim, not the renderer.
//
// WHAT THIS MEASURES: `Simulation::step`/`late_step` cost and allocation
// count, both scaling with entity count (synthetic levels at several sizes)
// and against the real shipped content (`roguelike/floor1|2|3.level`). The
// counting global allocator below turns "no per-frame allocation
// proportional to entity count" (Phase 6's actual done-when) into a number
// you can read off a table, not an impression from playing the game.
//
// HOW TO RUN (from the repo root — `cargo run`/`--example` does NOT shift
// CWD to the package directory the way `cargo test` does, so paths below
// are plain repo-root-relative, not `../`-prefixed like `tests/common/
// mod.rs`'s `CARGO_MANIFEST_DIR` dance):
//   cargo run --release -p ember2d-sim --example bench_sim
//   cargo run --release -p ember2d-sim --example bench_sim -- --level roguelike/floor2.level --steps 300
// (always --release: this crate's own code runs unoptimized otherwise, per
// the [profile.dev] comment in the workspace root Cargo.toml, and that gap
// dwarfs anything this file is trying to measure.)
//
// Per-phase attribution deliberately does NOT instrument `Simulation`
// itself — timing hooks inside the hot path would perturb what they
// measure and add permanent complexity to production code for a
// measurement tool's benefit. Instead, `bench_phases` clones a world at a
// representative mid-run state and times `WorldSnapshot::build` and
// `World::detect_collisions` directly, in isolation, with zero engine
// changes.

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::color::Color;
use ember2d_sim::event::EventBus;
use ember2d_sim::layers::LayerRegistry;
use ember2d_sim::level::{ActorRecord, LevelData, TileRecord};
use ember2d_sim::math::Vec2;
use ember2d_sim::scripting::WorldSnapshot;
use ember2d_sim::simulation::{Simulation, StepInput};
use ember2d_sim::world::World;

// ── Counting global allocator ───────────────────────────────────────────────
//
// Affects only this binary — `#[global_allocator]` is process-wide but this
// process exists solely to run this bench, so `ember2d-sim`'s own code is
// untouched by this file's existence. Counts every `alloc`/`dealloc` call;
// `realloc`'s default `GlobalAlloc` impl is expressed in terms of these two,
// so growth reallocations are counted too (as an alloc + a dealloc, which
// slightly overcounts vs. a "true" realloc — acceptable for a relative
// comparison across entity-count scales, which is all this needs to prove).
// The ~2 relaxed atomic ops per allocation make absolute times a few percent
// pessimistic; relative comparisons (500 vs. 2000 entities, before vs. after
// a Phase 6 step) are what this bench is actually for.
struct CountingAlloc;

static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn alloc_snapshot() -> (usize, usize) {
    (ALLOC_COUNT.load(Ordering::Relaxed), ALLOC_BYTES.load(Ordering::Relaxed))
}

// ── Synthetic level generator ───────────────────────────────────────────────
//
// Carves a square all-wall canvas the same way `gen_roguelike.rs`'s
// generator does (border + interior, nothing procedurally clever) but
// doesn't call it — that generator writes real dungeon layouts to disk for
// the shipped game, this one only needs a cheap, in-memory way to hit a
// target entity count. Interior cells are solid two-in-three (roughly
// matching floor2's real ~66% collider density, see
// docs/ember2d-phase6-plan.md's research numbers) so the collision phase's
// cost scales realistically, not just the entity count.
//
// Actor tiles reuse the real `roguelike/scripts/enemy_rat.rhai` and the
// player uses the real `roguelike/scripts/player.rhai` — this measures
// actual script-execution cost, not a stand-in. Paths are CWD-relative
// (`resolve_exit_path` checks `Path::new(next).exists()` against CWD first),
// so this must be run from the repo root, same as `gen_roguelike.rs`.
fn synth_level(n_tiles: usize, n_actors: usize, seed: u64) -> LevelData {
    let side = (n_tiles as f64).sqrt().ceil() as i32;
    let mut data = LevelData::empty(side as usize, side as usize);
    data.seed = seed;
    data.name = format!("bench-synthetic-{}", n_tiles);

    let mut floor_cells: Vec<(i32, i32)> = Vec::new();
    for y in 0..side {
        for x in 0..side {
            let border = x == 0 || y == 0 || x == side - 1 || y == side - 1;
            let solid = border || (x + y) % 3 != 0;
            if !solid { floor_cells.push((x, y)); }
            let glyph = if solid { '#' } else { '.' };
            let tag = if solid { "wall" } else { "floor" };
            let mut tile = TileRecord::new(x, y, 1, glyph, Color::White, Color::Reset, solid, false, tag);
            if solid { tile.collider_layer = "solid".to_string(); }
            data.tiles.push(tile);
        }
    }

    // Player spawns on the first floor cell found; actors take the next
    // `n_actors` floor cells and become solid "enemy" tiles with a real
    // script and turn-scheduler eligibility, matching how
    // examples/gen_roguelike.rs's rat()/boss() tiles are authored.
    if let Some(&(sx, sy)) = floor_cells.first() {
        data.spawn_point = (sx as f32, sy as f32);
    }
    data.player.script = Some("roguelike/scripts/player.rhai".to_string());

    for &(ax, ay) in floor_cells.iter().skip(1).take(n_actors) {
        if let Some(tile) = data.tiles.iter_mut().find(|t| t.x == ax && t.y == ay) {
            tile.glyph = 'r';
            tile.solid = true;
            tile.collider_layer = "solid".to_string();
            tile.tag = "enemy".to_string();
            tile.script = Some("roguelike/scripts/enemy_rat.rhai".to_string());
            tile.actor = Some(ActorRecord { speed: 100 });
        }
    }

    data
}

// ── Driver ───────────────────────────────────────────────────────────────────
//
// Duplicates `ember2d/tests/common/mod.rs`'s `TurnHarness::frame` sequence
// (consume input -> Simulation::step -> if a turn resolved, detect
// collisions -> Simulation::late_step) rather than depending on it — that
// type lives in `ember2d`'s own test tree, which would pull wgpu/winit/kira
// into this crate's example build, defeating the reason this file lives in
// `ember2d-sim` at all. Accepted duplication; see that file for the
// production-code twin this mirrors.
const BENCH_DT: f32 = 1.0 / 60.0;

/// One step's cost: wall-clock time and allocator deltas around the same
/// `Simulation::step` (+ `late_step` when a turn resolves) sequence the real
/// engine runs.
struct StepCost {
    duration: Duration,
    allocs: usize,
    bytes: usize,
}

fn run_steps(world: &mut World, sim: &mut Simulation, persistent: &mut BTreeMap<String, rhai::Dynamic>, n_steps: usize, viewport: (usize, usize)) -> Vec<StepCost> {
    // Cycle through a few keys so *some* turn resolves most steps —
    // realistic content has the player acting most frames, not sitting idle.
    let keys = ["w", "d", "s", "a", "space"];
    let mut costs = Vec::with_capacity(n_steps);

    for i in 0..n_steps {
        let mut held = std::collections::BTreeSet::new();
        held.insert(keys[i % keys.len()].to_string());
        let input = InputSnapshot { held: held.clone(), pressed: held };
        let mouse = MouseSnapshot::default();
        let gamepad = GamepadSnapshot::default();

        let (alloc_before, bytes_before) = alloc_snapshot();
        let start = Instant::now();

        let outcome = sim.step(world, StepInput {
            input: &input,
            mouse,
            gamepad: &gamepad,
            external_commands: &[],
            camera_origin: Vec2::ZERO,
            sim_dt: BENCH_DT,
            elapsed: i as f32 * BENCH_DT,
            viewport_w: viewport.0,
            viewport_h: viewport.1,
        }, persistent);

        if outcome.turn_triggered {
            let prev_positions = world.snapshot_positions();
            let mut events = EventBus::new();
            world.detect_collisions(&mut events);
            sim.late_step(world, &events, &prev_positions, Vec2::ZERO, BENCH_DT, i as f32 * BENCH_DT, viewport.0, viewport.1, persistent);
        }

        let duration = start.elapsed();
        let (alloc_after, bytes_after) = alloc_snapshot();
        costs.push(StepCost { duration, allocs: alloc_after - alloc_before, bytes: bytes_after - bytes_before });
    }
    costs
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    if sorted.is_empty() { return Duration::ZERO; }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

fn report(label: &str, mut costs: Vec<StepCost>) {
    let allocs: Vec<usize> = costs.iter().map(|c| c.allocs).collect();
    let bytes: Vec<usize> = costs.iter().map(|c| c.bytes).collect();
    let mut durations: Vec<Duration> = costs.drain(..).map(|c| c.duration).collect();
    durations.sort();

    let avg_allocs = allocs.iter().sum::<usize>() as f64 / allocs.len().max(1) as f64;
    let avg_bytes = bytes.iter().sum::<usize>() as f64 / bytes.len().max(1) as f64;

    println!(
        "{:<32} p50={:>7.3}ms p95={:>7.3}ms  allocs/step={:>8.1}  bytes/step={:>10.0}",
        label,
        percentile(&durations, 0.50).as_secs_f64() * 1000.0,
        percentile(&durations, 0.95).as_secs_f64() * 1000.0,
        avg_allocs,
        avg_bytes,
    );
}

/// Per-phase attribution (Step 1's spec): clone a world at a representative
/// mid-run state and time `WorldSnapshot::build`/`detect_collisions`
/// directly and in isolation. No `Simulation`/engine instrumentation.
fn bench_phases(world: &World, layers: &LayerRegistry, n_iters: usize) {
    let mut snapshot_times = Vec::with_capacity(n_iters);
    for _ in 0..n_iters {
        let w = world.clone();
        let start = Instant::now();
        let _snap = WorldSnapshot::build(&w, layers);
        snapshot_times.push(start.elapsed());
    }
    snapshot_times.sort();

    let mut collision_times = Vec::with_capacity(n_iters);
    for _ in 0..n_iters {
        let w = world.clone();
        let mut events = EventBus::new();
        let start = Instant::now();
        w.detect_collisions(&mut events);
        collision_times.push(start.elapsed());
    }
    collision_times.sort();

    println!(
        "  {:<30} p50={:>7.3}ms p95={:>7.3}ms",
        "WorldSnapshot::build",
        percentile(&snapshot_times, 0.50).as_secs_f64() * 1000.0,
        percentile(&snapshot_times, 0.95).as_secs_f64() * 1000.0,
    );
    println!(
        "  {:<30} p50={:>7.3}ms p95={:>7.3}ms",
        "World::detect_collisions",
        percentile(&collision_times, 0.50).as_secs_f64() * 1000.0,
        percentile(&collision_times, 0.95).as_secs_f64() * 1000.0,
    );
}

fn bench_synthetic(n_tiles: usize, n_actors: usize, n_steps: usize) {
    let data = synth_level(n_tiles, n_actors, 12345);
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let mut sim = Simulation::new(data);
    let viewport = (80, 24);
    sim.on_start(&mut world, viewport.0, viewport.1, &mut persistent);

    let entity_count = world.transforms.len();
    let costs = run_steps(&mut world, &mut sim, &mut persistent, n_steps, viewport);
    report(&format!("synthetic n={} ({} entities)", n_tiles, entity_count), costs);
    bench_phases(&world, sim.layers(), 20);
}

fn bench_real_level(path: &str, n_steps: usize) {
    if !Path::new(path).exists() {
        eprintln!("skipping {} (not found — run from the repo root)", path);
        return;
    }
    let data = match LevelData::load(path) {
        Ok(d) => d,
        Err(e) => { eprintln!("skipping {}: {}", path, e); return; }
    };
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let mut sim = Simulation::new(data);
    let viewport = (80, 24);
    sim.on_start(&mut world, viewport.0, viewport.1, &mut persistent);

    let entity_count = world.transforms.len();
    let costs = run_steps(&mut world, &mut sim, &mut persistent, n_steps, viewport);
    report(&format!("{} ({} entities)", path, entity_count), costs);
    bench_phases(&world, sim.layers(), 20);
}

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("*** WARNING: running an unoptimized debug build. Use --release. ***");
        eprintln!("*** ember2d-sim's own code runs 3-4x slower without it (see the workspace root Cargo.toml's [profile.dev] comment). ***\n");
    }

    let args: Vec<String> = std::env::args().collect();
    let mut single_level: Option<String> = None;
    let mut steps = 300usize;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--level" => { i += 1; single_level = args.get(i).cloned(); }
            "--steps" => { i += 1; steps = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(300); }
            _ => {}
        }
        i += 1;
    }

    if let Some(path) = single_level {
        bench_real_level(&path, steps);
        return;
    }

    println!("--- synthetic levels ---");
    for &n in &[500usize, 2000, 5000, 10000] {
        // A handful of actors regardless of scale, matching "a few monsters
        // in a big level" rather than scaling monster count with tile
        // count — floor2 has 6 rats in ~2,570 tiles, not a fixed fraction.
        bench_synthetic(n, 6, steps.min(120));
    }

    println!("\n--- shipped content ---");
    for path in &["roguelike/floor1.level", "roguelike/floor2.level", "roguelike/floor3.level"] {
        bench_real_level(path, steps);
    }
}
