// ember2d/tests/ui_widgets.rs — Step 9-3 (docs/ember2d-master-plan.md
// §5.8): engine-owned menus and dialogue, driven through `Simulation` with a
// level script that opens them and records what it reads back.

mod common;

use ember2d::prelude::*;
use ember2d_sim::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use ember2d_sim::save::SaveState;
use ember2d_sim::scripting::{LogEntry, LogLevel};
use ember2d_sim::simulation::{Simulation, StepInput};
use std::collections::{BTreeMap, BTreeSet};

struct H {
    world: World,
    sim: Simulation,
    persistent: BTreeMap<String, rhai::Dynamic>,
    logs: Vec<LogEntry>,
}

impl H {
    fn new(tag: &str, script: &str) -> Self {
        let path = common::test_temp_dir().join(format!("ui_widgets_{tag}.rhai"));
        std::fs::write(&path, script).unwrap();
        let mut data = LevelData::empty(20, 10);
        data.player.script = Some(path.to_string_lossy().into_owned());
        let mut sim = Simulation::new(data);
        let mut world = World::new();
        let mut persistent = BTreeMap::new();
        let logs = sim.on_start(&mut world, 40, 20, &mut persistent);
        H { world, sim, persistent, logs }
    }

    fn step(&mut self, keys: &[&str]) {
        let k: BTreeSet<String> = keys.iter().map(|s| s.to_string()).collect();
        let input = InputSnapshot { held: k.clone(), pressed: k };
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
                viewport_w: 40,
                viewport_h: 20,
            },
            &mut self.persistent,
        );
        self.logs.append(&mut out.logs);
    }

    fn global(&self, k: &str) -> rhai::Dynamic {
        self.sim.globals().get(k).cloned().unwrap_or(rhai::Dynamic::UNIT)
    }

    fn int(&self, k: &str) -> i64 {
        self.global(k).as_int().unwrap_or(i64::MIN)
    }

    fn no_errors(&self) {
        let e: Vec<_> = self.logs.iter().filter(|l| l.level == LogLevel::Error).collect();
        assert!(e.is_empty(), "{e:?}");
    }
}

const MENU_LEVEL: &str = r#"
fn on_update(id, ctx) {
    if !ctx.has_global("m") {
        ctx.set_global("m", ctx.menu_open(["Attack", "Item", "Run"], #{ title: "Battle" }));
        return;
    }
    let m = ctx.get_global("m");
    ctx.set_global("sel", ctx.menu_selection(m));
    ctx.set_global("closed", ctx.menu_closed(m));
    ctx.set_global("saw_down", ctx.just_pressed("down"));
    ctx.set_global("saw_d", ctx.just_pressed("d"));
}
"#;

#[test]
fn a_menu_moves_with_the_arrows_confirms_and_keeps_its_keys_from_scripts() {
    let mut h = H::new("menu", MENU_LEVEL);
    h.step(&[]); // opens the menu
    h.step(&["down", "d"]);
    assert_eq!(h.int("sel"), 1);
    assert_eq!(h.global("saw_down").as_bool().ok(), Some(false), "the menu took Down");
    assert_eq!(h.global("saw_d").as_bool().ok(), Some(true), "other keys still reach scripts");
    h.step(&["down"]);
    h.step(&["enter"]);
    assert_eq!(h.global("closed").as_bool().ok(), Some(true));
    assert_eq!(h.int("sel"), 2, "Run confirmed");
    h.step(&["down"]);
    assert_eq!(h.global("saw_down").as_bool().ok(), Some(true), "a closed menu takes nothing");
    h.no_errors();
}

#[test]
fn escape_cancels_unless_the_menu_says_otherwise() {
    let mut h = H::new("cancel", MENU_LEVEL);
    h.step(&[]);
    h.step(&["escape"]);
    assert_eq!(h.int("sel"), -1);
    assert_eq!(h.global("closed").as_bool().ok(), Some(true));

    let stuck = MENU_LEVEL.replace("#{ title: \"Battle\" }", "#{ cancelable: false }");
    let mut h = H::new("no_cancel", &stuck);
    h.step(&[]);
    h.step(&["escape"]);
    assert_eq!(h.global("closed").as_bool().ok(), Some(false));
    h.no_errors();
}

#[test]
fn the_newest_menu_has_the_keyboard() {
    let script = r#"
fn on_update(id, ctx) {
    if !ctx.has_global("a") {
        ctx.set_global("a", ctx.menu_open(["1", "2"]));
        ctx.set_global("b", ctx.menu_open(["x", "y"]));
        return;
    }
    ctx.set_global("sa", ctx.menu_selection(ctx.get_global("a")));
    ctx.set_global("sb", ctx.menu_selection(ctx.get_global("b")));
}
"#;
    let mut h = H::new("two", script);
    h.step(&[]);
    h.step(&["down"]);
    assert_eq!((h.int("sa"), h.int("sb")), (0, 1));
    assert_ne!(h.int("a"), h.int("b"), "two opens in one pass get different ids");
    h.no_errors();
}

#[test]
fn dialogue_pages_turn_on_enter_and_report_done() {
    let script = r#"
fn on_update(id, ctx) {
    let text = "Welcome to the village of Emberfall. The well has run dry and the elder wants a word with you. Mind the rats in the cellar; they bite.";
    if !ctx.has_global("d") {
        ctx.set_global("d", ctx.draw_dialogue(text, "Old man"));
        ctx.set_global("lines", ctx.wrap_text(text, 36).len());
        return;
    }
    let d = ctx.get_global("d");
    if ctx.dialogue_open() {
        // Drawing the same text again while it shows is a no-op.
        ctx.set_global("same", ctx.draw_dialogue(text, "Old man") == d);
    }
    ctx.set_global("open", ctx.dialogue_open());
    ctx.set_global("done", ctx.dialogue_done(d));
}
"#;
    let mut h = H::new("dialogue", script);
    h.step(&[]);
    h.step(&[]);
    let pages = h.sim.ui().open_dialogue().expect("showing").pages.len();
    assert!(pages >= 2, "a long text is paged ({pages})");
    assert_eq!(h.global("same").as_bool().ok(), Some(true), "redrawing it while open is a no-op");
    assert_eq!(
        h.int("lines"),
        h.sim.ui().open_dialogue().unwrap().pages.iter().map(|p| p.len()).sum::<usize>() as i64
    );
    for _ in 0..pages {
        h.step(&["enter"]);
    }
    assert_eq!(h.global("done").as_bool().ok(), Some(true));
    assert_eq!(h.global("open").as_bool().ok(), Some(false));
    h.no_errors();
}

#[test]
fn scripts_can_turn_and_close_the_dialogue_themselves() {
    let script = r#"
fn on_update(id, ctx) {
    let n = ctx.add_global("n", 1);
    if n == 1.0 { ctx.draw_dialogue("one two three four five six seven eight nine ten", ""); }
    if n == 3.0 { ctx.dialogue_advance(); }
    if n == 5.0 { ctx.close_dialogue(); }
}
"#;
    let mut h = H::new("advance", script);
    h.step(&[]);
    h.step(&[]);
    assert_eq!(h.sim.ui().open_dialogue().map(|d| d.page), Some(0));
    h.step(&[]);
    h.step(&[]);
    let after_advance = h.sim.ui().dialogue.clone().unwrap();
    assert!(after_advance.page == 1 || after_advance.done);
    h.step(&[]);
    h.step(&[]);
    assert!(h.sim.ui().open_dialogue().is_none(), "closed");
    h.no_errors();
}

#[test]
fn an_open_menu_survives_a_save_and_load() {
    let dir = common::test_temp_dir();
    let save = dir.join("ui_widgets.sav").to_string_lossy().replace('\\', "/");
    let script = format!(
        r#"
fn on_update(id, ctx) {{
    if !ctx.has_global("m") {{ ctx.set_global("m", ctx.menu_open(["Yes", "No"])); return; }}
    if !ctx.has_global("saved") {{ ctx.set_global("saved", 1); ctx.save_game("{save}"); }}
}}
"#
    );
    let mut h = H::new("save", &script);
    h.step(&[]);
    h.step(&["down"]);
    h.step(&[]);
    let state = SaveState::load_from_file(&save).expect("saved");
    let (_, menu) = state.ui.active_menu().expect("the menu is in the save");
    assert_eq!((menu.items.len(), menu.selected), (2, 1));
    h.no_errors();
}
