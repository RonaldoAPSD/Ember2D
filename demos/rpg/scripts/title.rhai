// demos/rpg/scripts/title.rhai — the title screen (the project's start
// level, `title.level`). The level is just scenery; this script hides the
// player, draws the title and offers New Game / Continue / Quit.
//
// New Game seeds the persistent party, gold and potions — the same seeding
// `player.rhai`'s `new_game` does for a level started straight from the
// editor (kept identical by hand: Rhai has no modules here, master plan
// decision 7.4). Continue loads `emberfall.sav`, which the pause menu's
// Save writes; with no save yet it says so.

fn new_game(ctx) {
    ctx.set_persistent("party", [
        #{ name: "Ash", hp: 24, max: 24, atk: 5, def: 1, mag: 0 },
        #{ name: "Lyra", hp: 16, max: 16, atk: 2, def: 0, mag: 6 },
    ]);
    ctx.set_persistent("gold", 20);
    ctx.set_persistent("potions", 2);
    ctx.set_persistent("quest", "");
    ctx.set_persistent("boss_done", false);
}

fn open_menu(id, ctx) {
    ctx.set_var(id, "menu", ctx.menu_open(["New Game", "Continue", "Quit"], #{ title: "", y: 14 }));
}

fn on_start(id, ctx) {
    ctx.set_visible(id, false);
    open_menu(id, ctx);
}

fn on_update(id, ctx) {
    ctx.draw_hud(26, 4, "                            ", "White", "#301810");
    ctx.draw_hud(26, 5, "    E M B E R F A L L       ", "Yellow", "#301810");
    ctx.draw_hud(26, 6, "  a tiny RPG built in Ember2D ", "White", "#301810");
    ctx.draw_hud(26, 7, "                            ", "White", "#301810");
    ctx.draw_hud(16, 22, " Up/Down to choose, Enter to start ", "Grey", "#202028");
    if ctx.has_global("tried_continue") {
        ctx.draw_hud(24, 20, " No save yet - start a New Game. ", "Black", "Yellow");
    }
    if !ctx.has_var(id, "menu") { return; }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let sel = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    if sel == 0 {
        new_game(ctx);
        ctx.load_level("town.level");
    } else if sel == 1 {
        // A missing save leaves us here; the next update reopens the menu.
        ctx.load_game("emberfall.sav");
        ctx.set_global("tried_continue", true);
        open_menu(id, ctx);
    } else if sel == 2 {
        ctx.quit_game();
    } else {
        open_menu(id, ctx);
    }
}
