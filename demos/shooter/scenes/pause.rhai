// demos/shooter/scenes/pause.rhai — Escape: resume, restart, turn screen
// shake on or off, or quit. A project that ships `scenes/pause.rhai`
// replaces the engine's built-in pause menu.
//
// The shake option outlives the run: it's written with `save_data` (the
// player script reads it back with `load_data` when the arena loads), and
// set as the "shake" global for the rest of this run.

fn on_start(id, ctx) {
    open(id, ctx, ctx.get_global("shake"));
}

fn open(id, ctx, shake) {
    let items = ["Resume", "Restart", "Screen shake: " + if shake { "ON" } else { "OFF" }, "Quit"];
    ctx.set_var(id, "menu", ctx.menu_open(items, #{ title: "PAUSED" }));
}

fn on_update(id, ctx) {
    if !ctx.has_var(id, "menu") { return; }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let pick = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    if pick == 1 {
        ctx.pop_scene();
        ctx.load_level("arena.level");
    } else if pick == 2 {
        // The global write lands after this pass, so the reopened menu is
        // labelled from the new value directly.
        let shake = !ctx.get_global("shake");
        ctx.set_global("shake", shake);
        ctx.save_data("ember_assault_options.sav", #{ shake: shake });
        open(id, ctx, shake);
    } else if pick == 3 {
        ctx.quit_game();
    } else {
        ctx.pop_scene();
    }
}
