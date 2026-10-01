// demos/roguelike/scripts/player.rhai — the hero: controls, combat, the
// pack, and the HUD (stats, the message log, mouse-look, the death screen).
//
// TURNS. The project is turn-based: `on_input` turns a key into a command
// (`submit`), and `on_turn` carries it out when the scheduler gives the
// player the turn. An action that costs a turn calls `act`; one that
// doesn't (bumping a wall, picking up nothing) returns without it, and the
// player is simply asked again. Every monster then takes its turn
// (monster.rhai) — all of them in the same frame, thanks to the project's
// `ai_turns_per_step`.
//
// STATE. Everything about the run lives in PERSISTENT state, so it carries
// down the stairs (each floor is a fresh load of dungeon.level) and into a
// save: hp, max_hp, power, defense, xp, level, depth, run_seed, the pack
// ("inventory", an array of maps), "log" (the message log) and the
// equipped "weapon"/"armour". `new_run` seeds it — here when the dungeon
// is started straight from the editor, or by the title screen's New Game.
//
// MESSAGES. A function that has something to say returns its lines and the
// caller writes them to the log once (`say`): persistent writes are
// deferred, so two writes in one pass would lose the first.

fn sight() { 8 }   // field-of-view radius

fn new_run(ctx, seed) {
    ctx.set_persistent("run_seed", seed);
    ctx.set_persistent("depth", 1);
    ctx.set_persistent("hp", 30);
    ctx.set_persistent("max_hp", 30);
    ctx.set_persistent("power", 2);
    ctx.set_persistent("defense", 1);
    ctx.set_persistent("xp", 0);
    ctx.set_persistent("level", 1);
    ctx.set_persistent("inventory", [#{ kind: "potion", name: "healing potion" }]);
    ctx.set_persistent("log", []);
    ctx.set_persistent("kills", 0);
}

fn on_start(id, ctx) {
    if !ctx.has_persistent("max_hp") { new_run(ctx, 1); }
    camera(ctx);
    ctx.play_music("audio/music.ogg");
}

/// A loaded save: the camera bounds aren't part of it.
fn on_load(id, ctx) {
    camera(ctx);
}

/// Let the view run eight rows past the bottom of the 80x43 map, so the
/// bottom panel covers empty space rather than the last rows of the map.
fn camera(ctx) {
    ctx.set_camera_bounds(0, 0, 80, 51);
}

// ── Input ────────────────────────────────────────────────────────────────

fn on_input(id, ctx) {
    if ctx.get_persistent("hp") <= 0 {
        if ctx.just_pressed("enter") { ctx.submit(id, "title", []); }
        return;
    }
    let dx = 0;
    let dy = 0;
    if ctx.just_pressed("up") || ctx.just_pressed("w") { dy = -1; }
    else if ctx.just_pressed("down") || ctx.just_pressed("s") { dy = 1; }
    else if ctx.just_pressed("left") || ctx.just_pressed("a") { dx = -1; }
    else if ctx.just_pressed("right") || ctx.just_pressed("d") { dx = 1; }
    if dx != 0 || dy != 0 { ctx.submit(id, "move", [dx * 1.0, dy * 1.0]); return; }
    if ctx.just_pressed("space") { ctx.submit(id, "wait", []); return; }
    if ctx.just_pressed("enter") { ctx.submit(id, "stairs", []); return; }
    if ctx.just_pressed("g") { ctx.submit(id, "pickup", []); return; }
    if ctx.just_pressed("q") { ctx.submit(id, "quaff", []); return; }
}

// ── Actions ──────────────────────────────────────────────────────────────

fn on_turn(id, ctx) {
    let action = ctx.command_action();
    if ctx.get_persistent("hp") <= 0 {
        if action == "title" { ctx.load_level("title.level"); }
        return;
    }
    let r = if action == "move" { do_move(id, ctx) }
        else if action == "wait" { #{ turn: true, msgs: [] } }
        else if action == "stairs" { do_stairs(id, ctx) }
        else if action == "pickup" { do_pickup(id, ctx) }
        else if action == "quaff" { do_quaff(id, ctx) }
        else { #{ turn: false, msgs: [] } };
    say(ctx, r.msgs);
    if r.turn { ctx.act(100.0); }
}

/// The first entity tagged `tag` in cell (x, y), or -1. (`get_entity_at`
/// alone would answer with whatever has the lowest id there — an item
/// lying under a monster, say.)
fn tagged_at(ctx, x, y, tag) {
    for e in ctx.find_entities_in_rect(x + 0.25, y + 0.25, 0.5, 0.5) {
        if ctx.has_tag(e, tag) { return e; }
    }
    -1
}

fn do_move(id, ctx) {
    let tx = ctx.get_x(id) + ctx.command_param(0);
    let ty = ctx.get_y(id) + ctx.command_param(1);
    let target = tagged_at(ctx, tx, ty, "monster");
    if target >= 0 { return attack(ctx, target); }
    if ctx.is_solid_at(tx, ty) { return #{ turn: false, msgs: [] }; }
    ctx.set_position(id, tx, ty);
    ctx.compute_fov(tx, ty, sight());
    let msgs = [];
    let item = tagged_at(ctx, tx, ty, "item");
    if item >= 0 { msgs.push(["You see a " + ctx.get_var(item, "name") + " here. (G to pick up)", "#c0c0c0"]); }
    if tagged_at(ctx, tx, ty, "stairs") >= 0 { msgs.push(["There are stairs down here. (Enter)", "#c0c0c0"]); }
    #{ turn: true, msgs: msgs }
}

fn attack(ctx, target) {
    let name = ctx.get_var(target, "name");
    let damage = ctx.get_persistent("power") + bonus(ctx, "weapon") - ctx.get_var(target, "defense");
    if damage <= 0 {
        return #{ turn: true, msgs: [["You attack the " + name + " but do no damage.", "#c0c0c0"]] };
    }
    let msgs = [["You attack the " + name + " for " + damage + " hit points.", "#ffffff"]];
    let hp = ctx.get_var(target, "hp") - damage;
    ctx.set_var(target, "hp", hp);
    ctx.emit_particles(ctx.get_x(target), ctx.get_y(target), "*", "Red");
    ctx.play_sound("audio/hurt.ogg");
    if hp <= 0 {
        let xp = ctx.get_var(target, "xp");
        msgs.push(["The " + name + " dies! You gain " + xp + " experience.", "#ffa040"]);
        let corpse = ctx.spawn_entity("%", ctx.get_x(target), ctx.get_y(target), "corpse");
        ctx.set_tint(corpse, "#a03030", "Reset");
        ctx.set_layer_order(corpse, 2);
        ctx.set_var(corpse, "name", "remains of a " + name);
        ctx.despawn(target);
        ctx.add_persistent("xp", xp);
        ctx.add_persistent("kills", 1);
    }
    #{ turn: true, msgs: msgs }
}

fn do_stairs(id, ctx) {
    if tagged_at(ctx, ctx.get_x(id), ctx.get_y(id), "stairs") < 0 {
        return #{ turn: false, msgs: [["There are no stairs here.", "#808080"]] };
    }
    ctx.set_persistent("depth", ctx.get_persistent("depth") + 1);
    ctx.load_level("dungeon.level");
    #{ turn: true, msgs: [] }
}

fn do_pickup(id, ctx) {
    let item = tagged_at(ctx, ctx.get_x(id), ctx.get_y(id), "item");
    if item < 0 { return #{ turn: false, msgs: [["There is nothing here to pick up.", "#808080"]] }; }
    let pack = ctx.get_persistent("inventory");
    if pack.len() >= 26 { return #{ turn: false, msgs: [["Your pack is full.", "#ff8080"]] }; }
    let name = ctx.get_var(item, "name");
    pack.push(#{ kind: ctx.get_var(item, "kind"), name: name });
    ctx.set_persistent("inventory", pack);
    ctx.despawn(item);
    #{ turn: true, msgs: [["You pick up the " + name + ".", "#80ff80"]] }
}

/// Drink the first healing potion in the pack.
fn do_quaff(id, ctx) {
    let pack = ctx.get_persistent("inventory");
    let at = -1;
    for i in 0..pack.len() {
        if at < 0 && (pack[i].kind == "potion" || pack[i].kind == "big_potion") { at = i; }
    }
    if at < 0 { return #{ turn: false, msgs: [["You have no healing potions.", "#808080"]] }; }
    let hp = ctx.get_persistent("hp");
    let max_hp = ctx.get_persistent("max_hp");
    if hp >= max_hp { return #{ turn: false, msgs: [["You are already at full health.", "#808080"]] }; }
    let heal = if pack[at].kind == "big_potion" { 30 } else { 12 };
    if hp + heal > max_hp { heal = max_hp - hp; }
    ctx.set_persistent("hp", hp + heal);
    pack.remove(at);
    ctx.set_persistent("inventory", pack);
    ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Green");
    #{ turn: true, msgs: [["You drink the potion and recover " + heal + " hit points.", "#80ff80"]] }
}

/// What an equipped item adds (`weapon` power, `armour` defense).
fn bonus(ctx, slot) {
    if !ctx.has_persistent(slot) { return 0; }
    let item = ctx.get_persistent(slot);
    if item.contains("bonus") { item.bonus } else { 0 }
}

fn say(ctx, msgs) {
    if msgs.len() == 0 { return; }
    let log = if ctx.has_persistent("log") { ctx.get_persistent("log") } else { [] };
    for m in msgs { log.push(m); }
    while log.len() > 60 { log.remove(0); }
    ctx.set_persistent("log", log);
}

// ── The HUD ──────────────────────────────────────────────────────────────

fn on_update(id, ctx) {
    let vw = ctx.get_viewport_width();
    let vh = ctx.get_viewport_height();
    let top = vh - 7;
    ctx.fill_rect(0, top, vw, 7, " ", "White", "#0c0c14");
    draw_stats(ctx, top + 1);
    draw_log(ctx, top + 1, vw);
    draw_look(id, ctx, top);
    let keys = " Arrows move/attack  G get  Q quaff  Enter stairs  Esc menu ";
    ctx.draw_hud(vw - keys.len() - 1, top, keys, "#707084", "#0c0c14");
    if ctx.get_persistent("hp") <= 0 { draw_death(ctx, vw, vh); }
}

fn draw_stats(ctx, row) {
    let hp = ctx.get_persistent("hp").to_int();
    let max_hp = ctx.get_persistent("max_hp").to_int();
    let shown = if hp < 0 { 0 } else { hp };
    let fill = shown * 20 / max_hp;
    ctx.fill_rect(1, row, 20, 1, " ", "White", "#401414");
    if fill > 0 { ctx.fill_rect(1, row, fill, 1, " ", "White", "#b02828"); }
    ctx.draw_hud(2, row, "HP " + shown + "/" + max_hp, "White", "#b02828");
    let level = ctx.get_persistent("level").to_int();
    let xp = ctx.get_persistent("xp").to_int();
    ctx.draw_hud(1, row + 1, "Level " + level + "  XP " + xp + "/" + xp_to_next(level), "#e0d080", "#0c0c14");
    ctx.draw_hud(1, row + 2, "Depth " + ctx.get_persistent("depth").to_int(), "#a0c0ff", "#0c0c14");
    let power = ctx.get_persistent("power") + bonus(ctx, "weapon");
    let defense = ctx.get_persistent("defense") + bonus(ctx, "armour");
    ctx.draw_hud(1, row + 3, "Power " + power.to_int() + "  Defense " + defense.to_int(), "#c0c0c0", "#0c0c14");
}

fn xp_to_next(level) { 200 + level * 150 }

/// The newest messages, wrapped to the panel's width, newest at the
/// bottom, each in its own colour — as many lines as fit (six).
fn draw_log(ctx, row, vw) {
    let log = if ctx.has_persistent("log") { ctx.get_persistent("log") } else { [] };
    let lines = [];
    let i = log.len() - 1;
    while i >= 0 && lines.len() < 6 {
        let wrapped = ctx.wrap_text(log[i][0], vw - 26);
        let j = wrapped.len() - 1;
        while j >= 0 && lines.len() < 6 {
            lines.insert(0, [wrapped[j], log[i][1]]);
            j -= 1;
        }
        i -= 1;
    }
    for k in 0..lines.len() {
        ctx.draw_hud(24, row + k, lines[k][0], lines[k][1], "#0c0c14");
    }
}

/// Mouse-look: name whatever is under the mouse, if it's in view.
fn draw_look(id, ctx, row) {
    let x = ctx.get_mouse_world_x().floor();
    let y = ctx.get_mouse_world_y().floor();
    if !ctx.is_in_fov(x, y) { return; }
    let names = [];
    for e in ctx.find_entities_in_rect(x + 0.25, y + 0.25, 0.5, 0.5) {
        if ctx.has_var(e, "name") { names.push(ctx.get_var(e, "name")); }
        else if ctx.has_tag(e, "player") { names.push("you"); }
        else if ctx.has_tag(e, "stairs") { names.push("stairs down"); }
    }
    if names.len() == 0 { return; }
    let text = " " + names[0];
    for i in 1..names.len() { text += ", " + names[i]; }
    ctx.draw_hud(1, row, text + " ", "#ffffa0", "#0c0c14");
}

fn draw_death(ctx, vw, vh) {
    let x = vw / 2 - 20;
    let y = vh / 2 - 6;
    ctx.fill_rect(x, y, 40, 7, " ", "White", "#400808");
    ctx.draw_box(x, y, 40, 7, "#ff6060", "#400808");
    ctx.draw_hud(x + 15, y + 2, "YOU DIED", "#ffff60", "#400808");
    let killer = if ctx.has_persistent("killer") { ctx.get_persistent("killer") } else { "something" };
    let line = "Killed by a " + killer + " on depth " + ctx.get_persistent("depth").to_int();
    ctx.draw_hud(x + 20 - line.len() / 2, y + 3, line, "White", "#400808");
    ctx.draw_hud(x + 3, y + 5, "Press Enter to return to the title", "#c0c0c0", "#400808");
}
