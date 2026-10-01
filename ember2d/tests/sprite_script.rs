// tests/sprite_script.rs — Step 9-7 (docs/ember2d-master-plan.md §5.8):
// `set_size`, `set_flip`, `set_y_sort`, `set_sprite` and
// `play_project_clip`, driven through `Simulation` against a small project
// on disk (a tileset and a clip in `assets/`, found the way level load finds
// them).

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::components::SpriteSource;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::BTreeMap;

const TILESET: &str = r#"(
    name: "hero", image: "hero.png", cell_w: 16, cell_h: 16, margin: 0, spacing: 0,
    columns: 2, rows: 1,
    regions: [
        (name: "stand", col: 0, row: 0, w: 1, h: 1),
        (name: "walk", col: 1, row: 0, w: 1, h: 1),
    ],
)"#;
const CLIP: &str = r#"(name: "walk", tileset: "hero", frames: ["stand", "walk"], fps: 4.0, looping: true)"#;

struct H {
    sim: Simulation,
    world: World,
    persistent: BTreeMap<String, rhai::Dynamic>,
    logs: Vec<LogEntry>,
}

/// A project in a temp folder whose player runs `script` every update.
fn project(tag: &str, script: &str) -> H {
    let dir = common::test_temp_dir().join(format!("sprite_script_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("assets/tilesets")).unwrap();
    std::fs::create_dir_all(dir.join("assets/clips")).unwrap();
    std::fs::write(dir.join("assets/tilesets/hero.ron"), TILESET).unwrap();
    std::fs::write(dir.join("assets/clips/walk.ron"), CLIP).unwrap();
    std::fs::write(dir.join("player.rhai"), script).unwrap();
    let mut data = LevelData::empty(20, 10);
    data.path = dir.join("level.level").to_string_lossy().into_owned();
    data.player.script = Some("player.rhai".to_string());
    let mut sim = Simulation::new(data);
    sim.set_level_source(Box::new(FsLevelSource));
    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let logs = sim.on_start(&mut world, 40, 20, &mut persistent);
    H { sim, world, persistent, logs }
}

impl H {
    fn step(&mut self) {
        let out = self.sim.step(
            &mut self.world,
            StepInput {
                input: &InputSnapshot::default(),
                mouse: MouseSnapshot::default(),
                gamepad: &GamepadSnapshot::default(),
                external_commands: &[],
                animating: &[],
                camera_origin: Vec2::ZERO,
                sim_dt: 1.0 / 60.0,
                elapsed: 0.0,
                viewport_w: 40,
                viewport_h: 20,
            },
            &mut self.persistent,
        );
        self.logs.extend(out.logs);
    }
    fn player(&self) -> EntityId {
        self.world.find_by_tag("player").expect("player")
    }
    fn warnings(&self, about: &str) -> usize {
        self.logs.iter().filter(|l| l.level == LogLevel::Warning && l.text.contains(about)).count()
    }
}

#[test]
fn size_flip_and_y_sort_change_the_sprite_and_the_world() {
    let mut h = project(
        "plain",
        "fn on_update(id, ctx) { ctx.set_size(id, 2, 1.5); ctx.set_flip(id, true, false); ctx.set_y_sort(true); }",
    );
    h.step();
    let sp = &h.world.sprites[&h.player()];
    assert_eq!(sp.size, Some(Vec2::new(2.0, 1.5)));
    assert!(sp.flip_x && !sp.flip_y);
    assert!(h.world.y_sort);
    assert!(!h.logs.iter().any(|l| l.level == LogLevel::Error), "{:?}", h.logs);
}

#[test]
fn set_sprite_draws_a_named_region_and_a_missing_one_warns_once() {
    let mut h = project(
        "region",
        r#"fn on_update(id, ctx) {
            ctx.set_sprite(id, "hero", "walk");
            if ctx.has_global("t") { ctx.set_sprite(id, "hero", "fly"); }
            let _n = ctx.add_global("t", 1);
        }"#,
    );
    h.step();
    match &h.world.sprites[&h.player()].source {
        SpriteSource::Texture { path, src } => {
            assert!(path.ends_with("hero.png"), "{path}");
            assert_eq!(*src, Some(Rect::new(16.0, 0.0, 16.0, 16.0)));
        }
        other => panic!("expected the region's texture, got {other:?}"),
    }
    assert_eq!(h.world.sprites[&h.player()].size, Some(Vec2::new(1.0, 1.0)), "one cell");
    assert_eq!(h.world.sprites[&h.player()].tint, Color::White, "drawn as the art is, not tinted");
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(h.warnings("fly"), 1, "{:?}", h.logs);
}

#[test]
fn play_project_clip_loads_a_clip_no_tile_uses() {
    let mut h = project(
        "clip",
        r#"fn on_update(id, ctx) {
            if !ctx.has_global("go") { ctx.set_global("go", 1); ctx.play_project_clip(id, "walk"); ctx.play_project_clip(id, "swim"); }
        }"#,
    );
    h.step();
    h.step();
    assert!(h.sim.clips().contains_key("walk"), "the clip was loaded");
    let p = h.player();
    assert!(matches!(&h.world.sprites[&p].source, SpriteSource::Clip { name } if name == "walk"));
    assert!(h.world.animators[&p].playing);
    assert_eq!(h.warnings("swim"), 1, "a missing clip says so: {:?}", h.logs);
    assert!(!h.logs.iter().any(|l| l.level == LogLevel::Error), "{:?}", h.logs);
}
