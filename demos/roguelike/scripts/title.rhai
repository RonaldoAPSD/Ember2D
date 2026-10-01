// demos/roguelike/scripts/title.rhai — the title screen (`title.level`,
// the project's start level): New game, Continue, Quit.
//
// New game seeds the run (`new_run`, kept the same as player.rhai's — Rhai
// has no modules here, master plan decision 7.4) with a seed taken from how
// many frames the title has been up: different every time you play, yet a
// recorded game replays exactly (it's input timing, not the clock). Then it
// loads the first floor. Continue loads `depths.sav`, which the pause menu's
// "Save and quit" writes (and a death overwrites — no second chances).

fn new_run(ctx, seed) {
    // Nothing from a finished run (its "won", its "killer") carries over:
    // the clear applies first, then this pass's writes on top.
    ctx.clear_all_persistent();
    ctx.set_persistent("run_seed", seed);
    ctx.set_persistent("depth", 1);
    ctx.set_persistent("hp", 30);
    ctx.set_persistent("max_hp", 30);
    ctx.set_persistent("power", 2);
    ctx.set_persistent("defense", 1);
    ctx.set_persistent("xp", 0);
    ctx.set_persistent("level", 1);
    ctx.set_persistent("kills", 0);
    ctx.set_persistent("inventory", [#{ kind: "potion", name: "healing potion" }]);
    ctx.set_persistent("weapon", #{ kind: "dagger", name: "dagger", bonus: 1 });
    ctx.set_persistent("armour", #{ kind: "none", name: "no armour", bonus: 0 });
    ctx.set_persistent("log", []);
}

fn open_menu(id, ctx) {
    let x = ctx.get_viewport_width() / 2 - 7;
    ctx.set_var(id, "menu", ctx.menu_open(["New game", "Continue", "Quit"], #{ title: "", x: x, y: 16 }));
}

fn on_start(id, ctx) {
    ctx.set_visible(id, false);
    open_menu(id, ctx);
}

fn on_update(id, ctx) {
    let frames = ctx.add_global("frames", 1);
    draw_title(ctx);
    if !ctx.has_var(id, "menu") { return; }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let pick = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    if pick == 0 {
        new_run(ctx, frames.to_int() * 7919 % 999983 + 1);
        ctx.load_level("dungeon.level");
    } else if pick == 1 {
        // With no save this does nothing but log; the note below says so.
        ctx.load_game("depths.sav");
        ctx.set_global("tried_continue", true);
        open_menu(id, ctx);
    } else if pick == 2 {
        ctx.quit_game();
    } else {
        open_menu(id, ctx);
    }
}

fn draw_title(ctx) {
    let vw = ctx.get_viewport_width();
    let x = vw / 2 - 28;
    let art = [
        "###  #### ###  #### #  #  ###     ##  ####    #### #  # ###  #### ### ",
        "#  # #    #  #  ##  #  # #       #  # #       #    #### #  # #    #  #",
        "#  # ###  ###   ##  ####  ##     #  # ###     ###  #  # ###  ###  ### ",
        "#  # #    #     ##  #  #    #    #  # #       #    #  # #  # #    # # ",
        "###  #### #     ##  #  # ###      ##  #       #### #  # ###  #### #  #",
    ];
    let ax = vw / 2 - 35;
    let shades = ["#ffd060", "#ffb040", "#ff9030", "#f07020", "#d05010"];
    for i in 0..art.len() { ctx.draw_hud(ax, 3 + i, art[i], shades[i], "Reset"); }
    ctx.draw_hud(x + 6, 11, "Twenty floors down, the Amulet of Ember waits.", "#c0a080", "Reset");
    ctx.draw_hud(x + 13, 12, "Bring it back up, or die trying.", "#c0a080", "Reset");
    if ctx.has_global("tried_continue") {
        ctx.draw_hud(x + 12, 21, " No saved game yet - start a new one. ", "Black", "#e0c060");
    }
    ctx.draw_hud(x + 10, 23, "Up/Down to choose, Enter to select", "#606070", "Reset");
}
