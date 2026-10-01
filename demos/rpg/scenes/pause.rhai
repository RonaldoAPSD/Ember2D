// demos/rpg/scenes/pause.rhai — the RPG's own pause menu (Esc). A project
// that ships `scenes/pause.rhai` replaces the engine's built-in one.
//
// Party shows everyone's health, Items uses a potion on someone, Save
// writes `emberfall.sav` (the title screen's Continue loads it), Title
// returns to the title screen. Save pops this scene FIRST, in the same
// pass: a scene pop is applied before the save is written, so the save
// doesn't come back with the menu still open.

fn main_items() { ["Party", "Items", "Save", "Title screen", "Close"] }

fn on_start(id, ctx) {
    ctx.set_var(id, "menu", ctx.menu_open(main_items(), #{ title: "MENU" }));
}

fn on_update(id, ctx) {
    if ctx.has_var(id, "dlg") {
        if ctx.dialogue_done(ctx.get_var(id, "dlg")) {
            ctx.remove_var(id, "dlg");
            ctx.set_var(id, "menu", ctx.menu_open(main_items(), #{ title: "MENU" }));
        }
        return;
    }
    if ctx.has_var(id, "items") { use_potion(id, ctx); return; }
    if !ctx.has_var(id, "menu") { return; }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let sel = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    if sel == 0 {
        let lines = "";
        for p in ctx.get_persistent("party") {
            lines += p.name + ": HP " + p.hp + "/" + p.max + ", attack " + p.atk;
            if p.mag > 0 { lines += ", magic " + p.mag; }
            lines += ".  ";
        }
        lines += "Gold " + ctx.get_persistent("gold") + ", potions " + ctx.get_persistent("potions") + ".";
        ctx.set_var(id, "dlg", ctx.draw_dialogue(lines, "Party"));
    } else if sel == 1 {
        if ctx.get_persistent("potions") > 0 {
            let names = [];
            for p in ctx.get_persistent("party") { names.push("Potion on " + p.name + " (" + p.hp + "/" + p.max + ")"); }
            names.push("Back");
            ctx.set_var(id, "items", ctx.menu_open(names, #{ title: "Potions: " + ctx.get_persistent("potions") }));
        } else {
            ctx.set_var(id, "dlg", ctx.draw_dialogue("You have no potions. The stall in town sells them.", ""));
        }
    } else if sel == 2 {
        ctx.pop_scene();
        ctx.save_game("emberfall.sav");
        ctx.set_global("toast", "Game saved");
    } else if sel == 3 {
        ctx.load_level("title.level");
    } else {
        ctx.pop_scene();
    }
}

fn use_potion(id, ctx) {
    let m = ctx.get_var(id, "items");
    if !ctx.menu_closed(m) { return; }
    let sel = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "items");
    let party = ctx.get_persistent("party");
    if sel >= 0 && sel < party.len() {
        let gained = party[sel].max - party[sel].hp;
        if gained > 12 { gained = 12; }
        party[sel].hp += gained;
        ctx.set_persistent("party", party);
        ctx.set_persistent("potions", ctx.get_persistent("potions") - 1);
        ctx.set_var(id, "dlg", ctx.draw_dialogue(party[sel].name + " recovers " + gained + " HP.", ""));
    } else {
        ctx.set_var(id, "menu", ctx.menu_open(main_items(), #{ title: "MENU" }));
    }
}
