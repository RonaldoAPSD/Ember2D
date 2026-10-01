// tests/fov_script.rs — Step 9.5-2 (docs/ember2d-master-plan.md §5.8.5):
// field of view from scripts — `compute_fov`, `is_in_fov`, `is_explored`,
// `fov_reset`, `set_fov_visibility` — driven through `Simulation`, on a
// floor the same script generates with 9.5-1's tile API.

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::fov::FovVisibility;
use ember2d_sim::layers::LayerRegistry;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::BTreeMap;

/// Two 5x5 rooms side by side (x 1..=5 and 9..=13, y 1..=5) joined by a
/// corridor along y = 3, inside a wall-filled 15x7 map; the player starts
/// in the left room. `extra` runs at the end of `on_start`; `update` is the
/// body of `on_update`.
fn script(extra: &str, update: &str) -> String {
    format!(
        r##"fn on_start(id, ctx) {{
            ctx.tile_def("wall", #{{ glyph: "#", solid: true, layer: 1 }});
            ctx.tile_def("floor", #{{ glyph: "." }});
            ctx.tilemap_resize(15, 7);
            ctx.tile_fill(0, 0, 15, 7, "wall");
            for r in [[1, 1], [9, 1]] {{
                ctx.tile_clear_rect_layer(r[0], r[1], 5, 5, 1);
                ctx.tile_fill(r[0], r[1], 5, 5, "floor");
            }}
            ctx.tile_clear_rect_layer(6, 3, 3, 1, 1);
            ctx.tile_fill(6, 3, 3, 1, "floor");
            ctx.set_position(id, 2, 2);
            ctx.compute_fov(2, 2, 10);
            {extra}
        }}
        fn on_update(id, ctx) {{ let n = ctx.add_global("n", 1); {update} }}"##
    )
}

struct H {
    sim: Simulation,
    world: World,
    persistent: BTreeMap<String, rhai::Dynamic>,
    logs: Vec<LogEntry>,
}

fn project(tag: &str, src: &str) -> H {
    let dir = common::test_temp_dir().join(format!("fov_script_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("player.rhai"), src).unwrap();
    let mut data = LevelData::empty(15, 7);
    data.tiles.clear();
    data.seed = 9;
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
    fn global(&self, key: &str) -> String {
        self.sim.globals().get(key).map(|v| v.to_string()).unwrap_or_default()
    }
    fn clean(&self) {
        let bad: Vec<_> = self.logs.iter().filter(|l| l.level != LogLevel::Info).collect();
        assert!(bad.is_empty(), "{bad:#?}");
    }
}

#[test]
fn a_view_computed_in_on_start_sees_the_floor_carved_in_the_same_pass() {
    let h = project("start", &script("", ""));
    h.clean();
    let fov = h.world.fov.as_ref().expect("compute_fov turned fog of war on");
    assert_eq!((fov.origin, fov.width, fov.height), ((0, 0), 15, 7), "over the level's tilemap");
    assert!(fov.is_visible(2, 2) && fov.is_visible(5, 5), "the player's room");
    assert!(fov.is_visible(0, 2), "its wall");
    assert!(!fov.is_visible(11, 1), "the other room, round the corner of the corridor");
}

#[test]
fn reads_follow_the_view_and_answer_true_while_there_is_no_fog() {
    let src = script(
        "",
        r#"if n == 1 {
                ctx.set_global("near", ctx.is_in_fov(4, 4));
                ctx.set_global("far", ctx.is_in_fov(11, 1));
                ctx.set_global("far_explored", ctx.is_explored(11.0, 1.0));
                // Walk to the corridor's far end and look again.
                // (`get_x` would still read the old spot this pass — the
                // move is deferred — so the new one is given directly, as
                // floats with an int radius: the mixed overload.)
                ctx.set_position(id, 9, 3);
                ctx.compute_fov(9.0, 3.0, 10);
            } else if n == 2 {
                ctx.set_global("now_far", ctx.is_in_fov(11, 1));
                ctx.set_global("old_room", ctx.is_in_fov(2, 2));
                ctx.set_global("old_explored", ctx.is_explored(2, 2));
                ctx.fov_reset();
            } else if n == 3 {
                ctx.set_global("reset", ctx.is_in_fov(11, 1) && ctx.is_explored(0, 6));
            }"#,
    );
    let mut h = project("reads", &src);
    for _ in 0..4 {
        h.step();
    }
    h.clean();
    assert_eq!(h.global("near"), "true");
    assert_eq!(h.global("far"), "false");
    assert_eq!(h.global("far_explored"), "false");
    assert_eq!(h.global("now_far"), "true", "from the corridor's end the right room is in view");
    assert_eq!(h.global("old_explored"), "true", "the left room is remembered");
    assert_eq!(h.global("reset"), "true", "fov_reset: no fog, everything counts as seen");
    assert!(h.world.fov.is_none());
}

#[test]
fn visibility_modes_are_set_by_name_and_a_bad_one_warns() {
    let src = script(
        r#"let rat = ctx.spawn_entity("r", 11.0, 2.0, "rat");
           ctx.set_global("rat", rat);
           ctx.set_fov_visibility(rat, "always");
           ctx.set_fov_visibility(rat, "sometimes");"#,
        "",
    );
    let h = project("modes", &src);
    let rat = h.world.find_by_tag("rat").expect("the rat");
    assert_eq!(h.world.fov_visibility_of(rat), FovVisibility::Always);
    let player = h.world.find_by_tag("player").unwrap();
    assert_eq!(h.world.fov_visibility_of(player), FovVisibility::Always, "the player by default");
    let warned = h
        .logs
        .iter()
        .filter(|l| l.level == LogLevel::Warning && l.text.contains("'sometimes'"))
        .count();
    assert_eq!(warned, 1);
}

#[test]
fn a_new_floor_starts_a_fresh_view_and_a_save_keeps_what_was_explored() {
    let src = script(
        "",
        r#"if n == 1 {
                ctx.tilemap_resize(20, 9);
                ctx.tile_fill(0, 0, 20, 9, "floor");
                ctx.compute_fov(15, 7, 3);
            }"#,
    );
    let mut h = project("fresh", &src);
    let explored_before = h.world.fov.as_ref().unwrap().is_explored(2, 2);
    // A save taken before the new floor keeps the old floor's exploration.
    let saved = ron::to_string(&h.world).expect("serialize");
    h.step();
    let fov = h.world.fov.as_ref().unwrap();
    assert_eq!((fov.width, fov.height), (20, 9), "the view follows the resized map");
    assert!(explored_before && !fov.is_explored(2, 2), "the old floor's exploration is gone");
    assert!(fov.is_visible(15, 7));

    let mut back: World = ron::from_str(&saved).expect("load");
    back.refresh_collider_bits(&LayerRegistry::new(&[]));
    let f = back.fov.as_ref().expect("the view is part of the save");
    assert!(f.is_explored(2, 2) && f.is_visible(4, 4) && !f.is_explored(11, 1));
}
