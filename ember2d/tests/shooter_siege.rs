// tests/shooter_siege.rs — Step 9.5-5 (docs/ember2d-master-plan.md §5.8.5):
// the expanded shooter demo (`demos/shooter/`: the 160x60 arena, twelve
// waves, gunners, the boss, weapon powerups, the saved best score) and its
// stress level, driven headlessly in real time — the same per-step
// sequence the engine runs (step, physics, collisions, late step).

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::event::EventBus;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::{BTreeMap, BTreeSet};

const DT: f32 = 1.0 / 60.0;
const ARENA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/shooter/arena.level");
const STRESS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../demos/shooter/stress.level");

struct H {
    world: World,
    sim: Simulation,
    persistent: BTreeMap<String, rhai::Dynamic>,
    elapsed: f32,
    logs: Vec<LogEntry>,
}

impl H {
    fn load(path: &str) -> Self {
        let data = LevelData::load(path).unwrap_or_else(|e| panic!("load {path}: {e}"));
        let mut world = World::new();
        let mut persistent = BTreeMap::new();
        let mut sim = Simulation::new(data);
        sim.set_level_source(Box::new(FsLevelSource));
        let logs = sim.on_start(&mut world, 80, 24, &mut persistent);
        H { world, sim, persistent, elapsed: 0.0, logs }
    }

    fn step_with(&mut self, held: &[&str], mouse: MouseSnapshot) {
        let prev = self.world.snapshot_positions();
        let keys: BTreeSet<String> = held.iter().map(|k| k.to_string()).collect();
        let input = InputSnapshot { held: keys.clone(), pressed: keys };
        let out = self.sim.step(
            &mut self.world,
            StepInput {
                input: &input,
                mouse,
                gamepad: &GamepadSnapshot::default(),
                external_commands: &[],
                animating: &[],
                camera_origin: Vec2::ZERO,
                sim_dt: DT,
                elapsed: self.elapsed,
                viewport_w: 80,
                viewport_h: 24,
            },
            &mut self.persistent,
        );
        self.logs.extend(out.logs);
        self.world.integrate_physics(DT);
        let mut events = EventBus::new();
        self.world.detect_collisions(&mut events);
        let late = self.sim.late_step(
            &mut self.world,
            &events,
            &prev,
            Vec2::ZERO,
            DT,
            self.elapsed,
            80,
            24,
            &mut self.persistent,
        );
        self.logs.extend(late.logs);
        self.elapsed += DT;
    }

    fn run(&mut self, steps: usize) {
        for _ in 0..steps {
            self.step_with(&[], MouseSnapshot::default());
        }
    }

    /// Steps with the left button held, aimed at world point `aim` (the
    /// harness camera sits at the origin, so the mouse cell is the world
    /// point).
    fn fire_at(&mut self, steps: usize, aim: (f32, f32)) {
        let mouse = MouseSnapshot { cell: aim, held: (true, false), pressed: (true, false) };
        for _ in 0..steps {
            self.step_with(&[], mouse);
        }
    }

    fn count(&self, tag: &str) -> usize {
        self.world.tags.values().filter(|t| t.name == tag).count()
    }

    fn global(&self, key: &str) -> rhai::Dynamic {
        self.sim.globals().get(key).cloned().unwrap_or(rhai::Dynamic::UNIT)
    }

    fn num(&self, key: &str) -> f64 {
        let v = self.global(key);
        v.as_float().or(v.as_int().map(|i| i as f64)).unwrap_or(f64::NAN)
    }

    fn set_global(&mut self, key: &str, v: impl Into<rhai::Dynamic>) {
        self.sim.globals_mut().insert(key.to_string(), v.into());
    }

    fn player(&self) -> (EntityId, Vec2) {
        let p = self.world.find_by_tag("player").unwrap();
        (p, self.world.get_global_position(p))
    }

    /// Jump straight to wave `w` (the director starts the next wave as soon
    /// as the breather it thinks it's in runs out).
    fn start_wave(&mut self, w: i64) {
        self.run(2); // past the countdown arming
        self.set_global("wave", w - 1);
        self.set_global("phase", "breather");
        self.set_global("phase_until", 0.0);
    }

    fn clean(&self) {
        let bad: Vec<_> = self
            .logs
            .iter()
            .filter(|l| l.level != LogLevel::Info)
            .map(|l| l.text.clone())
            .collect();
        assert!(bad.is_empty(), "{bad:#?}");
    }
}

#[test]
fn the_arena_is_big_layered_and_starts_quietly() {
    let mut h = H::load(ARENA);
    let map = h.world.tilemaps.values().next().expect("the baked arena");
    assert_eq!((map.width, map.height), (160, 60));
    for layer in ["enemy", "pbullet", "ebullet", "player", "pickup"] {
        assert!(h.sim.layers().bit_for(layer) != 0, "collision layer {layer} is registered");
    }
    assert_eq!(h.count("director"), 1);
    h.run(60);
    assert_eq!(h.count("grunt"), 0, "the countdown runs first");
    h.clean();
}

#[test]
fn wave_one_trickles_in_away_from_the_player() {
    let mut h = H::load(ARENA);
    h.run(260);
    assert_eq!(h.count("grunt"), 8, "wave 1: eight grunts");
    let (_, me) = h.player();
    for (&id, t) in &h.world.tags {
        if t.name == "grunt" {
            let p = h.world.get_global_position(id);
            let d2 = (p.x - me.x).powi(2) + (p.y - me.y).powi(2);
            assert!(d2 > 100.0, "a grunt arrived well away from the player (d^2 {d2})");
        }
    }
    h.clean();
}

#[test]
fn bullets_fly_kill_and_never_pile_up() {
    let mut h = H::load(ARENA);
    let (_, me) = h.player();
    // Fire right for two seconds: the bullets hit the arena's east wall.
    h.fire_at(120, (me.x + 10.0, me.y));
    let flying = h.count("bullet");
    assert!(flying > 0 && flying < 20, "a steady stream, each one gone at the wall ({flying})");
    // A grunt dropped in the line of fire dies, and pays.
    h.start_wave(1);
    h.run(200);
    let (_, me) = h.player();
    let grunt = h.world.tags.iter().find(|(_, t)| t.name == "grunt").map(|(&id, _)| id).unwrap();
    h.world.transforms.get_mut(&grunt).unwrap().position = Vec2::new(me.x + 4.0, me.y);
    h.fire_at(40, (me.x + 10.0, me.y));
    assert!(!h.world.transforms.contains_key(&grunt), "two hits killed it");
    assert!(h.num("score") >= 10.0 && h.num("kills") >= 1.0);
    h.clean();
}

#[test]
fn the_spread_and_the_shotgun_fire_their_patterns() {
    let mut h = H::load(ARENA);
    h.set_global("weapon", "spread");
    h.set_global("weapon_until", 100.0);
    let (_, me) = h.player();
    h.fire_at(1, (me.x, me.y - 10.0));
    h.run(1);
    assert_eq!(h.count("bullet"), 3, "three bullets a shot");
    h.run(60);
    h.set_global("weapon", "shotgun");
    h.fire_at(1, (me.x, me.y - 10.0));
    h.run(1);
    assert_eq!(h.count("bullet"), 7, "seven pellets");
    h.run(30);
    assert_eq!(h.count("bullet"), 0, "pellets burn out (or hit something) fast");
    h.clean();
}

#[test]
fn gunners_shoot_and_a_hit_costs_one_hp_then_a_moment_of_safety() {
    let mut h = H::load(ARENA);
    h.start_wave(3); // twelve grunts, ten swarmers, two gunners
    let mut shots = 0;
    for _ in 0..600 {
        h.run(1);
        shots = shots.max(h.count("ebullet"));
        // Keep the player alive and clear of melee for the test.
        h.set_global("hp", 12);
        for (&id, t) in &h.world.tags.clone() {
            if t.name == "grunt" || t.name == "swarmer" {
                h.world.despawn(id);
            }
        }
    }
    assert!(shots > 0, "the gunners fired");
    h.clean();
}

#[test]
fn the_boss_has_an_armoured_ring_and_dies_with_it() {
    let mut h = H::load(ARENA);
    h.start_wave(12);
    h.run(5);
    let core = h.world.find_by_tag("boss").expect("the boss");
    assert_eq!(h.count("boss_part"), 8, "eight parts");
    let at = h.world.get_global_position(core);
    for (&id, t) in &h.world.tags {
        if t.name == "boss_part" {
            let p = h.world.get_global_position(id);
            assert!(
                (p.x - at.x).abs() <= 1.01 && (p.y - at.y).abs() <= 1.01,
                "parts ring the core"
            );
        }
    }
    // Put it in front of the player with one hit point left, and shoot.
    let (_, me) = h.player();
    h.world.transforms.get_mut(&core).unwrap().position = Vec2::new(me.x + 6.0, me.y);
    h.world.vars.get_mut(&core).unwrap().values.insert("hp".into(), rhai::Dynamic::from(1_i64));
    for _ in 0..90 {
        h.set_global("hp", 12);
        h.fire_at(1, (me.x + 10.0, me.y));
    }
    assert!(h.world.find_by_tag("boss").is_none(), "the core is destroyed");
    h.run(2);
    assert_eq!(h.count("boss_part"), 0, "and its armour with it");
    assert!(h.num("score") >= 1000.0);
    h.clean();
}

#[test]
fn a_death_saves_the_best_score() {
    let mut h = H::load(ARENA);
    let file = "ember_assault_best.sav"; // relative to the working directory, as save_game's are
    let _ = std::fs::remove_file(file);
    h.run(2);
    h.set_global("score", 4321);
    h.set_global("hp", 0);
    h.run(2);
    let saved = std::fs::read_to_string(file).expect("the best score was written");
    assert_eq!(saved.trim(), "4321");
    // A new run reads it back.
    let mut again = H::load(ARENA);
    again.run(1);
    assert_eq!(again.num("best"), 4321.0);
    let _ = std::fs::remove_file(file);
    h.clean();
}

#[test]
fn the_stress_level_holds_hundreds_of_live_entities() {
    let mut h = H::load(STRESS);
    let start = std::time::Instant::now();
    h.run(600);
    let per_step = start.elapsed().as_secs_f64() * 1000.0 / 600.0;
    let live = h.count("grunt") + h.count("swarmer") + h.count("bullet") + h.count("ebullet");
    println!(
        "stress: {live} live entities ({} grunts, {} swarmers, {} bullets), {per_step:.2} ms/step in this build (headless, physics included)",
        h.count("grunt"),
        h.count("swarmer"),
        h.count("bullet")
    );
    assert!(live >= 300, "the stress level keeps 300+ entities alive ({live})");
    h.clean();
}
