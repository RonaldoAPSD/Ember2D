// demos/roguelike/scenes/pause.rhai — Escape in the dungeon: keep going,
// save and quit to the title, or quit without saving. A project that ships
// `scenes/pause.rhai` replaces the engine's built-in pause menu.
//
// "Save and quit" pops this scene FIRST, in the same pass as the save: a
// scene pop is applied before the save is written, so the save doesn't
// come back with the menu still open. The title screen's Continue loads it.

fn on_start(id, ctx) {
    ctx.set_var(id, "menu", ctx.menu_open(["Continue", "Save and quit", "Quit without saving"], #{ title: "PAUSED" }));
}

fn on_update(id, ctx) {
    if !ctx.has_var(id, "menu") { return; }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let pick = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    ctx.pop_scene();
    if pick == 1 {
        ctx.save_game("depths.sav");
        ctx.load_level("title.level");
    } else if pick == 2 {
        ctx.load_level("title.level");
    }
}
