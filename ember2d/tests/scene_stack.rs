// ember2d/tests/scene_stack.rs — Step 9-1 (docs/ember2d-master-plan.md
// §5.8): the script-visible scene stack, driven through `Simulation`
// directly. Each test writes a level script and its scene scripts
// (`scenes/<name>.rhai`, beside the level) to a temp folder.

mod common;

use ember2d::level_source::FsLevelSource;
use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::save::SaveState;
use ember2d_sim::scripting::{FlowRequest, HudDraw, LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput, StepOutcome};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

struct Harness {
    dir: PathBuf,
    world: World,
    sim: Simulation,
    persistent: BTreeMap<String, rhai::Dynamic>,
    logs: Vec<LogEntry>,
    viewport: (usize, usize),
}

fn dir_for(tag: &str) -> PathBuf {
    let dir = common::test_temp_dir().join(format!("scene_stack_{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("scenes")).unwrap();
    dir
}

impl Harness {
    /// A 20x10 level whose player runs `level` (may be empty), with each
    /// `(name, source)` written to `scenes/<name>.rhai`.
    fn new(tag: &str, level: &str, scenes: &[(&str, &str)]) -> Self {
        Self::with_viewport(tag, level, scenes, (80, 24))
    }

    fn with_viewport(
        tag: &str,
        level: &str,
        scenes: &[(&str, &str)],
        viewport: (usize, usize),
    ) -> Self {
        let dir = dir_for(tag);
        for (name, src) in scenes {
            std::fs::write(dir.join("scenes").join(format!("{name}.rhai")), src).unwrap();
        }
        let script = dir.join("level.rhai");
        std::fs::write(&script, level).unwrap();
        let mut data = LevelData::empty(20, 10);
        data.path = dir.join("level.level").to_string_lossy().into_owned();
        data.player.script = Some(script.to_string_lossy().into_owned());
        Self::start(dir, Simulation::new(data), World::new(), BTreeMap::new(), viewport)
    }

    fn start(
        dir: PathBuf,
        mut sim: Simulation,
        mut world: World,
        mut persistent: BTreeMap<String, rhai::Dynamic>,
        viewport: (usize, usize),
    ) -> Self {
        sim.set_level_source(Box::new(FsLevelSource));
        let logs = sim.on_start(&mut world, viewport.0, viewport.1, &mut persistent);
        Harness { dir, world, sim, persistent, logs, viewport }
    }

    fn step(&mut self, pressed: &[&str]) -> StepOutcome {
        let keys: BTreeSet<String> = pressed.iter().map(|k| k.to_string()).collect();
        let input = InputSnapshot { held: keys.clone(), pressed: keys };
        let mut out = self.sim.step(
            &mut self.world,
            StepInput {
                input: &input,
                mouse: MouseSnapshot::default(),
                gamepad: &GamepadSnapshot::default(),
                external_commands: &[],
                animating: &[],
                camera_origin: Vec2::ZERO,
                sim_dt: 1.0 / 60.0,
                elapsed: 0.0,
                viewport_w: self.viewport.0,
                viewport_h: self.viewport.1,
            },
            &mut self.persistent,
        );
        self.logs.append(&mut out.logs);
        out
    }

    fn int(&self, key: &str) -> i64 {
        self.sim
            .globals()
            .get(key)
            .map(|v| v.as_int().unwrap_or(v.as_float().unwrap_or(-1.0) as i64))
            .unwrap_or(0)
    }

    fn assert_no_errors(&self) {
        let errors: Vec<_> = self.logs.iter().filter(|l| l.level == LogLevel::Error).collect();
        assert!(errors.is_empty(), "script errors: {errors:?}");
    }
}

const COUNTING_LEVEL: &str = r#"
fn on_update(id, ctx) {
    ctx.add_global("ticks", 1);
    if !ctx.has_global("pushed") {
        ctx.set_global("pushed", true);
        ctx.push_scene("menu", #{ data: 42 });
    }
    ctx.set_global("level_sees", ctx.current_scene());
}
"#;

const MENU: &str = r#"
fn on_start(id, ctx) { let _n = ctx.add_global("menu_starts", 1); }
fn on_input(id, ctx) { if ctx.just_pressed("enter") { ctx.pop_scene(); } }
fn on_update(id, ctx) {
    ctx.add_global("menu_ticks", 1);
    ctx.set_global("menu_data", ctx.scene_data());
    ctx.set_global("menu_sees", ctx.current_scene());
}
"#;

#[test]
fn a_pushed_scene_pauses_the_level_and_popping_resumes_it() {
    let mut h = Harness::new("pause_resume", COUNTING_LEVEL, &[("menu", MENU)]);
    h.step(&[]); // the level pushes "menu"
    assert_eq!(h.int("ticks"), 1);
    assert_eq!(h.sim.scene_names(), vec!["menu"]);
    assert!(h.sim.world_paused(), "scenes pause the world by default");

    h.step(&[]);
    h.step(&[]);
    assert_eq!(h.int("ticks"), 1, "the level's on_update is held while the menu is open");
    assert_eq!(h.int("menu_starts"), 1, "on_start runs once");
    // R110: a scene's first on_update is the step AFTER its on_start.
    assert_eq!(h.int("menu_ticks"), 1);
    assert_eq!(h.int("menu_data"), 42, "scene_data() returns the pushed data");
    assert_eq!(h.sim.globals().get("menu_sees").unwrap().to_string(), "menu");

    h.step(&["enter"]); // the menu pops itself
    assert!(h.sim.scene_names().is_empty());
    assert_eq!(h.int("ticks"), 1, "the step that popped the scene still held the level");
    h.step(&[]);
    assert_eq!(h.int("ticks"), 2, "the level resumes the step after");
    assert_eq!(h.sim.globals().get("level_sees").unwrap().to_string(), "");
    h.assert_no_errors();
}

#[test]
fn a_scene_that_does_not_pause_the_world_runs_alongside_the_level() {
    let level = r#"
fn on_update(id, ctx) {
    ctx.add_global("ticks", 1);
    if !ctx.has_global("pushed") {
        ctx.set_global("pushed", true);
        ctx.push_scene("hud", #{ pauses_world: false });
    }
}
"#;
    let hud = r#"fn on_update(id, ctx) { let _n = ctx.add_global("hud_ticks", 1); }"#;
    let mut h = Harness::new("overlay", level, &[("hud", hud)]);
    for _ in 0..4 {
        h.step(&[]);
    }
    assert!(!h.sim.world_paused());
    assert_eq!(h.int("ticks"), 4, "the level never stopped");
    // Pushed in step 1, started in step 2 (R110: no on_update that step),
    // updated in steps 3 and 4.
    assert_eq!(h.int("hud_ticks"), 2, "the overlay scene ran every step after it started");
    h.assert_no_errors();
}

#[test]
fn the_key_that_opens_the_pause_scene_does_not_also_close_it() {
    let mut h = Harness::new("esc", "", &[]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&["escape"]); // the same Esc press that opened it
    assert_eq!(h.sim.scene_names(), vec!["pause"], "still open");
    h.step(&["escape"]);
    assert!(h.sim.scene_names().is_empty(), "a second Esc closes it");
    h.assert_no_errors();
}

fn pause_menu_options(h: &Harness) -> Vec<String> {
    // Step 9-3: the built-in pause scene's list is an engine menu.
    let (_, menu) = h.sim.ui().active_menu().expect("the pause scene opens a menu");
    assert_eq!(menu.title, "PAUSED");
    menu.items.clone()
}

#[test]
fn the_builtin_pause_menu_offers_back_to_editor_only_in_an_editor_preview() {
    let mut h = Harness::new("builtin", "", &[]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    assert_eq!(pause_menu_options(&h), vec!["Resume", "Quit Game"]);

    let mut h = Harness::new("builtin_preview", "", &[]);
    h.sim.set_editor_preview(true);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    assert_eq!(pause_menu_options(&h), vec!["Resume", "Back to Editor", "Quit Game"]);
    h.step(&["down"]);
    let out = h.step(&["enter"]);
    assert_eq!(out.flow, Some(FlowRequest::ToEditor));

    let mut h = Harness::new("builtin_quit", "", &[]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&["up"]); // wraps from Resume to the last row
    let out = h.step(&["enter"]);
    assert_eq!(out.flow, Some(FlowRequest::Quit));
    h.assert_no_errors();
}

// R86's tiny-window case (carried over from the deleted Rust pause menu)
// is now a layout test of the menu widget itself: `ember2d/src/play/
// ui_draw.rs`'s `r86_a_menu_never_underflows_on_a_screen_smaller_than_itself`.

#[test]
fn resume_closes_the_pause_scene_and_its_menu() {
    let mut h = Harness::new("resume", "", &[]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&["enter"]); // Resume is the first row
    assert!(h.sim.scene_names().is_empty());
    assert!(h.sim.ui().active_menu().is_none(), "the popped scene's menu went with it");
    h.assert_no_errors();
}

#[test]
fn a_project_pause_scene_replaces_the_builtin_one() {
    let own = r#"fn on_update(id, ctx) { ctx.draw_hud(1, 1, "MY PAUSE", "White", "Reset"); }"#;
    let mut h = Harness::new("own_pause", "", &[("pause", own)]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]); // on_start
    h.step(&[]); // first on_update (R110)
    let texts: Vec<_> = h
        .sim
        .scene_hud_draws()
        .iter()
        .filter_map(|d| match d {
            HudDraw::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["MY PAUSE"]);
}

#[test]
fn a_paused_levels_hud_stays_under_the_scene_hud() {
    let level = r#"fn on_update(id, ctx) { ctx.draw_hud(0, 0, "LEVEL", "White", "Reset"); }"#;
    let pause = r#"fn on_update(id, ctx) { ctx.draw_hud(0, 1, "SCENE", "White", "Reset"); }"#;
    let mut h = Harness::new("hud", level, &[("pause", pause)]);
    h.step(&[]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&[]);
    let level_text = h
        .sim
        .pending_hud_draws()
        .iter()
        .any(|d| matches!(d, HudDraw::Text { text, .. } if text == "LEVEL"));
    assert!(level_text, "the level's last HUD is still drawn while paused");
    let scene_text = h
        .sim
        .scene_hud_draws()
        .iter()
        .any(|d| matches!(d, HudDraw::Text { text, .. } if text == "SCENE"));
    assert!(scene_text, "and the scene's own HUD draws in its own layer above it");
}

#[test]
fn an_unknown_scene_warns_and_pushes_nothing() {
    let level = r#"fn on_update(id, ctx) { if !ctx.has_global("p") { ctx.set_global("p", 1); ctx.push_scene("nowhere"); } }"#;
    let mut h = Harness::new("unknown", level, &[]);
    h.step(&[]);
    assert!(h.sim.scene_names().is_empty());
    assert!(h.logs.iter().any(|l| l.level == LogLevel::Warning && l.text.contains("nowhere")));
}

#[test]
fn a_save_made_with_a_scene_open_reopens_it_on_load() {
    let menu = r#"
fn on_update(id, ctx) {
    if !ctx.has_global("saved") { ctx.set_global("saved", 1); ctx.save_game(ctx.scene_data()); }
}
"#;
    let dir = dir_for("save");
    let save_path = dir.join("game.sav").to_string_lossy().replace('\\', "/");
    let level = format!(
        r#"fn on_update(id, ctx) {{ if !ctx.has_global("p") {{ ctx.set_global("p", 1); ctx.push_scene("menu", #{{ data: "{save_path}" }}); }} }}"#
    );
    let mut h = Harness::new("save", &level, &[("menu", menu)]);
    h.step(&[]);
    h.step(&[]);
    h.step(&[]);
    h.assert_no_errors();

    let state = SaveState::load_from_file(&save_path).expect("saved");
    assert_eq!(state.scenes.len(), 1);
    assert_eq!(state.scenes[0].name, "menu");
    let level_data = LevelData::load(&h.sim.level().path).unwrap_or_else(|_| {
        let mut d = LevelData::empty(20, 10);
        d.path = h.sim.level().path.clone();
        d
    });
    let mut sim = Simulation::from_save(
        level_data,
        state.globals.clone(),
        state.clips.clone(),
        state.turn_number,
        state.scheduler.clone(),
    );
    sim.set_saved_scenes(state.scenes.clone());
    let loaded = Harness::start(h.dir.clone(), sim, state.world, state.persistent, (80, 24));
    assert_eq!(loaded.sim.scene_names(), vec!["menu"]);
    assert!(loaded.sim.world_paused());
    loaded.assert_no_errors();
}

// ── R110 / R111 (Phase 9 gate pass) ─────────────────────────────────────

/// R110: a scene's first `on_update` used to run in the same step as its
/// `on_start`, against a snapshot taken before `on_start` — so a var
/// `on_start` set read back as `()`.
#[test]
fn r110_a_scene_sees_the_vars_its_on_start_set() {
    let scene = r#"
fn on_start(id, ctx) { ctx.set_var(id, "menu", 7); }
fn on_update(id, ctx) { ctx.set_global("seen", ctx.get_var(id, "menu") + 1); }
"#;
    let mut h = Harness::new("r110", "", &[("pause", scene)]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&[]);
    assert_eq!(h.int("seen"), 8);
    h.assert_no_errors();
}

/// R111: a world-pausing scene whose script fails is closed, not left on
/// the stack forever with the game frozen beneath it.
#[test]
fn r111_a_scene_whose_script_fails_is_closed_and_the_level_resumes() {
    let level = r#"fn on_update(id, ctx) { let _n = ctx.add_global("ticks", 1); }"#;
    let broken = r#"fn on_update(id, ctx) { ctx.no_such_function(); }"#;
    let mut h = Harness::new("r111", level, &[("pause", broken)]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&[]); // on_update fails; the scene is closed
    assert!(h.sim.scene_names().is_empty(), "{:?}", h.sim.scene_names());
    assert!(h.logs.iter().any(|l| l.level == LogLevel::Error && l.text.contains("closed")));
    let before = h.int("ticks");
    h.step(&[]);
    assert_eq!(h.int("ticks"), before + 1, "the level runs again");
}

/// A pause menu that opens an inventory scene with data; closing the
/// inventory's dialogue pops it and hands the keyboard back to the menu.
#[test]
fn a_nested_scene_closes_back_to_the_menu_beneath_it() {
    let pause = r#"
fn on_start(id, ctx) { ctx.set_var(id, "menu", ctx.menu_open(["Continue", "Inventory"])); }
fn on_update(id, ctx) {
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    if ctx.menu_selection(m) == 1 {
        ctx.push_scene("inventory", #{ data: #{ gold: 12 } });
        ctx.set_var(id, "menu", ctx.menu_open(["Continue", "Inventory"], #{ selected: 1 }));
    } else {
        ctx.pop_scene();
    }
}
"#;
    let inventory = r#"
fn on_start(id, ctx) { ctx.set_var(id, "d", ctx.draw_dialogue("Gold: " + ctx.scene_data().gold, "Bag")); }
fn on_update(id, ctx) { if ctx.dialogue_done(ctx.get_var(id, "d")) { ctx.pop_scene(); } }
"#;
    let mut h = Harness::new("nested", "", &[("pause", pause), ("inventory", inventory)]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&["down"]);
    h.step(&["enter"]);
    assert_eq!(h.sim.scene_names(), vec!["pause", "inventory"]);
    h.step(&[]);
    assert_eq!(h.sim.ui().open_dialogue().map(|d| d.pages[0][0].clone()), Some("Gold: 12".into()));
    h.step(&["enter"]); // closes the dialogue; the inventory pops itself
    assert_eq!(h.sim.scene_names(), vec!["pause"]);
    h.step(&["up"]);
    h.step(&["enter"]); // Continue
    h.step(&[]);
    assert!(h.sim.scene_names().is_empty());
    h.assert_no_errors();
}

/// R112 (Phase 9 gate pass): the HUD the last scene drew used to stay on
/// screen after it popped — the scene HUD queue was only cleared when a
/// scene's on_update ran, and with the stack empty none does.
#[test]
fn r112_a_popped_scenes_hud_goes_with_it() {
    let scene = r#"
fn on_input(id, ctx) { if ctx.just_pressed("enter") { ctx.pop_scene(); } }
fn on_update(id, ctx) { ctx.draw_hud(1, 1, "SCENE HUD", "White", "Reset"); }
"#;
    let mut h = Harness::new("r112", "", &[("pause", scene)]);
    h.sim.request_pause(&mut h.world, &mut h.logs);
    h.step(&[]);
    h.step(&[]);
    assert!(!h.sim.scene_hud_draws().is_empty(), "the scene drew its HUD");
    h.step(&["enter"]);
    h.step(&[]);
    assert!(h.sim.scene_names().is_empty());
    assert_eq!(h.sim.scene_hud_draws().len(), 0, "the popped scene's HUD is gone");
    h.assert_no_errors();
}
