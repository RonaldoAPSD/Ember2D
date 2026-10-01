// tests/tile_script.rs — Step 9.5-1 (docs/ember2d-master-plan.md §5.8.5):
// scripts placing static tiles — `tile_def`, `tilemap_resize`, `tile_set`,
// `tile_fill`, `tile_clear*`, `get_tile` — driven through `Simulation`
// against a small project on disk (a tileset in `assets/`, for a sprite
// tile).

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::components::Tilemap;
use ember2d_sim::layers::LayerRegistry;
use ember2d_sim::level::TileRecord;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::BTreeMap;

const TILESET: &str = r#"(
    name: "dun", image: "dun.png", cell_w: 16, cell_h: 16, margin: 0, spacing: 0,
    columns: 2, rows: 1,
    regions: [(name: "floor", col: 0, row: 0, w: 1, h: 1), (name: "wall", col: 1, row: 0, w: 1, h: 1)],
)"#;

/// The defs every test starts from: a solid wall on layer 1, a floor on 0.
const DEFS: &str = r##"
    ctx.tile_def("wall", #{ glyph: "#", fg: "Grey", solid: true, tag: "wall", layer: 1 });
    ctx.tile_def("floor", #{ glyph: ".", fg: "DarkGrey", tag: "floor" });
"##;

struct H {
    sim: Simulation,
    world: World,
    persistent: BTreeMap<String, rhai::Dynamic>,
    logs: Vec<LogEntry>,
}

/// A 20x10 project whose player runs `script`, with a level that has
/// `painted` static tiles (x, y) baked into it, the way the editor saves.
fn project(tag: &str, script: &str, painted: &[(i32, i32)]) -> H {
    let dir = common::test_temp_dir().join(format!("tile_script_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("assets/tilesets")).unwrap();
    std::fs::write(dir.join("assets/tilesets/dun.ron"), TILESET).unwrap();
    std::fs::write(dir.join("player.rhai"), script).unwrap();
    let mut data = LevelData::empty(20, 10);
    data.tiles.clear();
    for &(x, y) in painted {
        data.tiles.push(TileRecord::new(
            x,
            y,
            1,
            'T',
            Color::Green,
            Color::Reset,
            true,
            false,
            "tree",
        ));
    }
    data.bake_tilemap();
    data.seed = 0x5EED; // `empty` picks a random seed; the tests need one
    data.path = dir.join("level.level").to_string_lossy().into_owned();
    data.player.script = Some("player.rhai".to_string());
    data.set_player_spawn((10.0, 5.0));
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
    fn map(&self) -> &Tilemap {
        assert_eq!(self.world.tilemaps.len(), 1, "one level tilemap");
        self.world.tilemaps.values().next().unwrap()
    }
    fn warnings(&self, about: &str) -> usize {
        self.logs.iter().filter(|l| l.level == LogLevel::Warning && l.text.contains(about)).count()
    }
    fn global(&self, key: &str) -> String {
        self.sim.globals().get(key).map(|v| v.to_string()).unwrap_or_default()
    }
}

#[test]
fn a_script_generates_a_floor_in_on_start() {
    let script = format!(
        r#"fn on_start(id, ctx) {{
            {DEFS}
            ctx.tilemap_resize(30, 12);
            ctx.tile_fill(0, 0, 30, 12, "floor");
            ctx.tile_fill(0, 0, 30, 1, "wall");
            ctx.tile_set(5, 5, "wall");
        }}"#
    );
    let h = project("gen", &script, &[]);
    assert!(h.logs.iter().all(|l| l.level == LogLevel::Info), "{:#?}", h.logs);
    let map = h.map();
    assert_eq!((map.origin, map.width, map.height), ((0, 0), 30, 12));
    assert_eq!(map.tile_count(), 30 * 12 + 30 + 1, "a floor everywhere, walls over it on layer 1");
    assert!(map.solid_at(5, 5, 0) && map.solid_at(29, 0, 0), "walls are solid");
    assert!(!map.solid_at(5, 6, 0), "floor isn't");
    assert_eq!(map.tag_at(5, 5), "wall", "the topmost tagged layer");
    assert_eq!(map.name_at(0, 5, 5), "floor");
    assert_eq!(map.palette.len(), 2, "two defs, however many cells");
}

#[test]
fn reads_see_new_tiles_the_next_step_and_collision_at_once() {
    let script = format!(
        r#"fn on_start(id, ctx) {{ {DEFS} ctx.tile_fill(0, 0, 20, 10, "floor"); }}
        fn on_update(id, ctx) {{
            let n = ctx.add_global("n", 1);
            if n == 1 {{
                ctx.tile_set(11, 5, "wall");
                ctx.set_global("same_step", ctx.is_solid_at(11, 5));
            }} else if n == 2 {{
                ctx.set_global("next_step", ctx.is_solid_at(11, 5));
                ctx.set_global("name", ctx.get_tile(11, 5, 1));
                ctx.set_global("under", ctx.get_tile(11, 5, 0));
                ctx.tile_clear_layer(11, 5, 1);
            }} else if n == 3 {{
                ctx.set_global("cleared", ctx.is_solid_at(11, 5));
                ctx.set_global("still_floor", ctx.get_tile(11.5, 5.5, 0.0));
                // A position from get_x/get_y (floats) with a layer literal.
                ctx.set_global("at_player", ctx.get_tile(ctx.get_x(id), ctx.get_y(id), 0));
                ctx.tile_clear(11, 5);
            }} else if n == 4 {{
                ctx.set_global("empty", ctx.get_tile(11, 5, 0));
            }}
        }}"#
    );
    let mut h = project("reads", &script, &[]);
    for _ in 0..5 {
        h.step();
    }
    assert_eq!(h.global("same_step"), "false", "the pass reads the snapshot taken before it");
    assert_eq!(h.global("next_step"), "true");
    assert_eq!(h.global("name"), "wall");
    assert_eq!(h.global("under"), "floor");
    assert_eq!(h.global("cleared"), "false", "tile_clear_layer took the wall off");
    assert_eq!(h.global("still_floor"), "floor", "...and left the floor (floats floor to a cell)");
    assert_eq!(h.global("empty"), "", "tile_clear empties every layer");
    assert_eq!(h.global("at_player"), "floor", "float coordinates, int layer");
    assert!(h.logs.iter().all(|l| l.level == LogLevel::Info), "{:#?}", h.logs);
}

#[test]
fn a_wall_placed_this_step_blocks_this_steps_physics() {
    // The player is moving right at (10, 5) and a wall appears in front of
    // it in the same step's script pass: the step's collision resolution
    // already stops it.
    let script = format!(
        r#"fn on_start(id, ctx) {{ {DEFS} ctx.set_velocity(id, 30.0, 0.0); }}
        fn on_update(id, ctx) {{ if ctx.add_global("n", 1) == 1 {{ ctx.tile_set(11, 5, "wall"); }} }}"#
    );
    let mut h = project("physics", &script, &[]);
    for _ in 0..10 {
        h.step();
    }
    let p = h.world.find_by_tag("player").unwrap();
    assert!(h.world.get_global_position(p).x < 11.0, "the wall stopped the player");
}

#[test]
fn a_painted_levels_tilemap_grows_to_the_whole_level() {
    // The editor baked only the box its two trees span (3..=4, 2); the
    // script places tiles far outside it.
    let script = format!(
        r#"fn on_start(id, ctx) {{ {DEFS} ctx.tile_set(0, 0, "wall"); ctx.tile_set(19, 9, "wall"); }}"#
    );
    let h = project("grow", &script, &[(3, 2), (4, 2)]);
    let map = h.map();
    assert_eq!((map.origin, map.width, map.height), ((0, 0), 20, 10));
    assert!(map.solid_at(0, 0, 0) && map.solid_at(19, 9, 0), "the new walls");
    assert_eq!(map.tag_at(3, 2), "tree", "the painted trees stayed where they were");
    assert_eq!(map.tag_at(4, 2), "tree");
    assert_eq!(h.warnings(""), 0, "{:#?}", h.logs);
}

#[test]
fn bad_requests_warn_once_and_change_nothing() {
    let script = format!(
        r#"fn on_start(id, ctx) {{
            {DEFS}
            ctx.tile_def("door", #{{ glyph: "+", trigger: true }});
            ctx.tile_def("typo", #{{ glyph: "x", soild: true }});
            ctx.tile_def("odd", #{{ fg: "NotAColour" }});
            ctx.tile_fill(0, 0, 20, 10, "lava");
            ctx.tile_set_layer(1, 1, 300, "wall");
            ctx.tile_fill(500, 500, 3, 3, "wall");
            ctx.tile_fill(-1000000000, -1000000000, 2000000000, 2000000000, "floor");
            ctx.tilemap_resize(0, 5);
            ctx.tilemap_resize(100000, 100000);
            ctx.tile_set(1.0 / 0.0, 2.0, "wall");
        }}"#
    );
    let h = project("bad", &script, &[]);
    assert_eq!(h.warnings("'trigger' needs an entity"), 1);
    assert_eq!(h.warnings("unknown key 'soild'"), 1);
    assert_eq!(h.warnings("'NotAColour' isn't a colour"), 1);
    assert_eq!(h.warnings("no tile named lava"), 1, "one warning for a whole fill");
    assert_eq!(h.warnings("layer 300"), 1);
    assert_eq!(h.warnings("fell outside the tilemap"), 1, "the far rect and the infinite x, once");
    assert_eq!(h.warnings("tilemap_resize("), 2);
    // The huge fill was clipped to the map: every cell is floor, nothing else.
    let map = h.map();
    assert_eq!((map.width, map.height), (20, 10));
    assert_eq!(map.tile_count(), 200);
}

#[test]
fn a_sprite_tile_draws_its_tileset_region() {
    let script = r#"fn on_start(id, ctx) {
        ctx.tile_def("stone", #{ sprite: "dun:wall", solid: true, layer: 1 });
        ctx.tile_def("ghost", #{ sprite: "dun:nope" });
        ctx.tile_set(2, 2, "stone");
        ctx.tile_set(3, 3, "ghost");
    }"#;
    let h = project("sprite", script, &[]);
    let map = h.map();
    let stone = map.get(1, 2, 2).expect("the stone tile");
    assert!(stone.texture.as_deref().is_some_and(|t| t.ends_with("dun.png")), "{stone:?}");
    assert_eq!(stone.src, Some(ember2d_sim::math::Rect::new(16.0, 0.0, 16.0, 16.0)));
    assert_eq!(h.warnings("tile_def(\"ghost\")"), 1, "a missing region warns once...");
    assert!(map.get(0, 3, 3).is_some_and(|d| d.texture.is_none()), "...and draws as a glyph");
}

#[test]
fn generated_tiles_and_their_defs_survive_a_save() {
    let script = format!(
        r#"fn on_start(id, ctx) {{ {DEFS} ctx.tilemap_resize(8, 8); ctx.tile_fill(0, 0, 8, 8, "floor"); ctx.tile_set(4, 4, "wall"); }}"#
    );
    let h = project("save", &script, &[]);
    let text = ron::to_string(&h.world).expect("a world serializes");
    let mut back: World = ron::from_str(&text).expect("and loads");
    back.refresh_collider_bits(&LayerRegistry::new(&[]));
    let map = back.tilemaps.values().next().expect("the generated map");
    assert!(map.solid_at(4, 4, 0), "solidity rebuilt after load");
    assert_eq!(map.name_at(1, 4, 4), "wall");
    assert!(back.tile_defs.contains_key("wall") && back.tile_defs.contains_key("floor"));
}

#[test]
fn the_same_seed_generates_the_same_floor() {
    // A random walk carving floor out of solid rock, twice from the same
    // level seed: cell for cell identical.
    let script = format!(
        r#"fn on_start(id, ctx) {{
            {DEFS}
            ctx.tilemap_resize(20, 10);
            ctx.tile_fill(0, 0, 20, 10, "wall");
            let x = 10; let y = 5;
            for i in 0..200 {{
                ctx.tile_set_layer(x, y, 1, "floor");
                let d = ctx.random_int(0, 3);
                if d == 0 && x > 1 {{ x -= 1; }} else if d == 1 && x < 18 {{ x += 1; }}
                else if d == 2 && y > 1 {{ y -= 1; }} else if d == 3 && y < 8 {{ y += 1; }}
            }}
        }}"#
    );
    let a = project("seed_a", &script, &[]);
    let b = project("seed_b", &script, &[]);
    let cells = |h: &H| {
        h.map().iter_tiles().map(|(l, x, y, d)| (l, x, y, d.name.clone())).collect::<Vec<_>>()
    };
    assert_eq!(cells(&a), cells(&b));
    assert!(
        a.map().iter_tiles().any(|(_, _, _, d)| d.name == "floor"),
        "the walk carved something"
    );
}
