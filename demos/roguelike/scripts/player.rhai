// demos/roguelike/scripts/player.rhai — the hero: controls and everything
// the hero does — move, fight, pick up, use and drop items, aim scrolls,
// level up, take the stairs. (The HUD that shows it all is drawn by
// dungeon.rhai.)
//
// TURNS. The project is turn-based: `on_input` turns a key into a command
// (`submit`), and `on_turn` carries it out when the scheduler gives the
// player the turn. An action that costs a turn calls `act`; one that
// doesn't (bumping a wall, picking up nothing, choosing a level-up bonus)
// returns without it, and the player is simply asked again. Every monster
// then takes its turn (monster.rhai) — all of them in the same frame,
// thanks to the project's `ai_turns_per_step`.
//
// MENUS AND AIMING happen in `on_input`, while the game waits for the
// player: I (inventory), D (drop) and the level-up choice open a menu; a
// scroll that needs a target (confusion, fireball) puts a cursor on the map
// that the arrows move — or click a cell — and Enter confirms (Backspace
// or a right-click cancels). Only the
// final choice becomes a command, with the item's slot and the target cell
// in its params, so the turn itself happens in `on_turn` like any other —
// and a recorded game replays it exactly.
//
// STATE. Everything about the run lives in PERSISTENT state, so it carries
// down the stairs (each floor is a fresh load of dungeon.level) and into a
// save: hp, max_hp, power, defense, xp, level, depth, run_seed, kills, the
// pack ("inventory", an array of item maps), the equipped "weapon" and
// "armour", and "log" (the message log). `new_run` seeds it — here when the
// dungeon is started straight from the editor, or by the title's New game.
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
    ctx.set_persistent("kills", 0);
    ctx.set_persistent("inventory", [#{ kind: "potion", name: "healing potion" }]);
    ctx.set_persistent("weapon", #{ kind: "dagger", name: "dagger", bonus: 1 });
    ctx.set_persistent("armour", #{ kind: "none", name: "no armour", bonus: 0 });
    ctx.set_persistent("log", []);
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

/// What using an item does. `effect`: heal, lightning, confusion,
/// fireball, equip or amulet (not `use`: a reserved word in Rhai, even as a
/// map key); `amount`: hit points healed or dealt (or turns
/// confused); `slot`/`bonus`: for equipment.
fn item_info(kind) {
    switch kind {
        "potion" => #{ effect: "heal", amount: 12 },
        "big_potion" => #{ effect: "heal", amount: 30 },
        "lightning" => #{ effect: "lightning", amount: 20, range: 5 },
        "confusion" => #{ effect: "confusion", amount: 10, range: 8 },
        "fireball" => #{ effect: "fireball", amount: 14, range: 3 },
        "dagger" => #{ effect: "equip", slot: "weapon", bonus: 1 },
        "sword" => #{ effect: "equip", slot: "weapon", bonus: 3 },
        "axe" => #{ effect: "equip", slot: "weapon", bonus: 5 },
        "leather" => #{ effect: "equip", slot: "armour", bonus: 1 },
        "chainmail" => #{ effect: "equip", slot: "armour", bonus: 3 },
        "plate" => #{ effect: "equip", slot: "armour", bonus: 5 },
        "amulet" => #{ effect: "amulet" },
        _ => #{ effect: "none" },
    }
}

fn xp_to_next(level) { 100 + level * 150 }

// ── Input: keys, menus, aiming ───────────────────────────────────────────

fn on_input(id, ctx) {
    if ctx.get_persistent("hp") <= 0 || ctx.has_persistent("won") {
        if ctx.just_pressed("enter") { ctx.submit(id, "title", []); }
        return;
    }
    if ctx.has_var(id, "menu") { menu_input(id, ctx); return; }
    if ctx.has_var(id, "aim") { aim_input(id, ctx); return; }
    if ctx.has_var(id, "sheet") {
        if ctx.dialogue_done(ctx.get_var(id, "sheet")) { ctx.remove_var(id, "sheet"); }
        return;
    }
    if ctx.get_persistent("xp") >= xp_to_next(ctx.get_persistent("level")) {
        let choices = ["Toughness (+20 max HP)", "Strength (+1 power)", "Agility (+1 defense)"];
        open_menu(id, ctx, "levelup", choices, "Level up! Choose:", false);
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
    if ctx.just_pressed("i") { open_pack(id, ctx, "use", "Use which item?"); return; }
    if ctx.just_pressed("x") { open_pack(id, ctx, "drop", "Drop which item?"); return; }
    if ctx.just_pressed("c") { ctx.set_var(id, "sheet", ctx.draw_dialogue(character_sheet(ctx), "Character")); }
}

fn open_menu(id, ctx, purpose, items, title, cancelable) {
    ctx.set_var(id, "menu", ctx.menu_open(items, #{ title: title, cancelable: cancelable }));
    ctx.set_var(id, "menu_for", purpose);
}

fn open_pack(id, ctx, purpose, title) {
    let pack = ctx.get_persistent("inventory");
    if pack.len() == 0 {
        say(ctx, [["Your pack is empty.", "#808080"]]);
        return;
    }
    let names = [];
    for it in pack { names.push(it.name); }
    open_menu(id, ctx, purpose, names, title, true);
}

/// A menu is open: wait for it, then act on the choice.
fn menu_input(id, ctx) {
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let pick = ctx.menu_selection(m);
    let purpose = ctx.get_var(id, "menu_for");
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    if pick < 0 { return; }
    if purpose == "levelup" { ctx.submit(id, "levelup", [pick * 1.0]); return; }
    if purpose == "drop" { ctx.submit(id, "drop", [pick * 1.0]); return; }
    let kind = ctx.get_persistent("inventory")[pick].kind;
    let info = item_info(kind);
    if info.effect == "confusion" || info.effect == "fireball" {
        // Aim first: a cursor on the player, moved with the arrows.
        let cursor = ctx.spawn_entity("X", ctx.get_x(id), ctx.get_y(id), "cursor");
        ctx.set_tint(cursor, "#ffff00", "#604000");
        ctx.set_layer_order(cursor, 50);
        ctx.set_fov_visibility(cursor, "always");
        ctx.set_var(id, "aim", #{ slot: pick, x: ctx.get_x(id), y: ctx.get_y(id), cursor: cursor });
        let what = if info.effect == "fireball" { "Fireball (radius 3 - it burns you too)" } else { "Confusion" };
        say(ctx, [[what + ": aim with the arrows or the mouse, Enter or click to cast, Backspace to cancel.", "#80c0ff"]]);
        return;
    }
    ctx.submit(id, "use", [pick * 1.0]);
}

/// Aiming a scroll: arrows move the cursor, Enter or a click casts,
/// Backspace or a right-click cancels. (Not Escape: with no menu open, the
/// engine gives Escape to the pause menu.)
fn aim_input(id, ctx) {
    let aim = ctx.get_var(id, "aim");
    if ctx.just_pressed("backspace") || ctx.mouse_right_pressed() {
        ctx.despawn(aim.cursor);
        ctx.remove_var(id, "aim");
        return;
    }
    let tx = aim.x;
    let ty = aim.y;
    let fire = ctx.just_pressed("enter");
    if ctx.mouse_left_pressed() {
        tx = ctx.get_mouse_world_x().floor();
        ty = ctx.get_mouse_world_y().floor();
        fire = true;
    }
    if ctx.just_pressed("up") || ctx.just_pressed("w") { ty -= 1.0; }
    if ctx.just_pressed("down") || ctx.just_pressed("s") { ty += 1.0; }
    if ctx.just_pressed("left") || ctx.just_pressed("a") { tx -= 1.0; }
    if ctx.just_pressed("right") || ctx.just_pressed("d") { tx += 1.0; }
    if fire {
        ctx.despawn(aim.cursor);
        ctx.remove_var(id, "aim");
        ctx.submit(id, "cast", [aim.slot * 1.0, tx, ty]);
        return;
    }
    if tx != aim.x || ty != aim.y {
        aim.x = tx;
        aim.y = ty;
        ctx.set_position(aim.cursor, tx, ty);
        ctx.set_var(id, "aim", aim);
    }
}

fn character_sheet(ctx) {
    let level = ctx.get_persistent("level").to_int();
    let weapon = gear(ctx, "weapon");
    let armour = gear(ctx, "armour");
    let s = "Level " + level + ", experience " + ctx.get_persistent("xp").to_int();
    s += " (next level at " + xp_to_next(level) + ").  Max HP " + ctx.get_persistent("max_hp").to_int();
    s += ", power " + ctx.get_persistent("power").to_int() + " + " + weapon.bonus + " (" + weapon.name + ")";
    s += ", defense " + ctx.get_persistent("defense").to_int() + " + " + armour.bonus + " (" + armour.name + ").  ";
    s += "Depth " + ctx.get_persistent("depth").to_int() + ", " + ctx.get_persistent("kills").to_int() + " monsters slain.";
    s
}

// ── Actions (on the player's turn) ───────────────────────────────────────

fn on_turn(id, ctx) {
    let action = ctx.command_action();
    if ctx.get_persistent("hp") <= 0 || ctx.has_persistent("won") {
        if action == "title" { ctx.load_level("title.level"); }
        return;
    }
    let slot = ctx.command_param(0).to_int();
    let r = if action == "move" { do_move(id, ctx) }
        else if action == "wait" { #{ turn: true, msgs: [] } }
        else if action == "stairs" { do_stairs(id, ctx) }
        else if action == "pickup" { do_pickup(id, ctx) }
        else if action == "use" { do_use(id, ctx, slot, -1.0, -1.0) }
        else if action == "cast" { do_use(id, ctx, slot, ctx.command_param(1), ctx.command_param(2)) }
        else if action == "drop" { do_drop(id, ctx, slot) }
        else if action == "levelup" { do_levelup(ctx, slot) }
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
    if tagged_at(ctx, tx, ty, "upstairs") >= 0 { msgs.push(["The stairs up, back to the surface. (Enter)", "#c0c0c0"]); }
    #{ turn: true, msgs: msgs }
}

/// Deal `amount` to monster `target`; returns the log lines (and kills it,
/// paying its experience, at 0 hit points).
fn hurt(ctx, target, amount, how) {
    let name = ctx.get_var(target, "name");
    let msgs = [[how + " the " + name + " for " + amount + " hit points.", "#ffffff"]];
    let hp = ctx.get_var(target, "hp") - amount;
    ctx.set_var(target, "hp", hp);
    ctx.emit_particles(ctx.get_x(target), ctx.get_y(target), "*", "Red");
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
    msgs
}

fn attack(ctx, target) {
    let damage = ctx.get_persistent("power") + gear(ctx, "weapon").bonus - ctx.get_var(target, "defense");
    ctx.play_sound("audio/hurt.ogg");
    if damage <= 0 {
        return #{ turn: true, msgs: [["You attack the " + ctx.get_var(target, "name") + " but do no damage.", "#c0c0c0"]] };
    }
    #{ turn: true, msgs: hurt(ctx, target, damage, "You attack") }
}

fn do_stairs(id, ctx) {
    let x = ctx.get_x(id);
    let y = ctx.get_y(id);
    if tagged_at(ctx, x, y, "upstairs") >= 0 {
        if !has_item(ctx, "amulet") {
            return #{ turn: false, msgs: [["You can't leave without the Amulet of Ember!", "#ff8080"]] };
        }
        ctx.set_persistent("won", true);
        return #{ turn: false, msgs: [["You climb out of the Depths with the Amulet of Ember. You have won!", "#ffd040"]] };
    }
    if tagged_at(ctx, x, y, "stairs") < 0 {
        return #{ turn: false, msgs: [["There are no stairs here.", "#808080"]] };
    }
    ctx.set_persistent("depth", ctx.get_persistent("depth") + 1);
    ctx.load_level("dungeon.level");
    #{ turn: true, msgs: [] }
}

/// An equipment slot, or bare hands for a run that has none yet.
fn gear(ctx, slot) {
    if ctx.has_persistent(slot) { ctx.get_persistent(slot) } else { #{ kind: "none", name: "nothing", bonus: 0 } }
}

fn has_item(ctx, kind) {
    for it in ctx.get_persistent("inventory") { if it.kind == kind { return true; } }
    false
}

fn do_pickup(id, ctx) {
    let item = tagged_at(ctx, ctx.get_x(id), ctx.get_y(id), "item");
    if item < 0 { return #{ turn: false, msgs: [["There is nothing here to pick up.", "#808080"]] }; }
    let pack = ctx.get_persistent("inventory");
    if pack.len() >= 26 { return #{ turn: false, msgs: [["Your pack is full.", "#ff8080"]] }; }
    let kind = ctx.get_var(item, "kind");
    let name = ctx.get_var(item, "name");
    pack.push(#{ kind: kind, name: name });
    ctx.set_persistent("inventory", pack);
    ctx.despawn(item);
    let msgs = [["You pick up the " + name + ".", "#80ff80"]];
    if kind == "amulet" {
        msgs.push(["It burns with a cold light. Back to the stairs up (<) where you came in!", "#ffd040"]);
    }
    #{ turn: true, msgs: msgs }
}

fn do_drop(id, ctx, slot) {
    let pack = ctx.get_persistent("inventory");
    if slot < 0 || slot >= pack.len() { return #{ turn: false, msgs: [] }; }
    let it = pack[slot];
    let dropped = ctx.spawn_entity(glyph_of(it.kind), ctx.get_x(id), ctx.get_y(id), "item");
    ctx.set_tint(dropped, "#c0c0c0", "Reset");
    ctx.set_layer_order(dropped, 3);
    ctx.set_var(dropped, "kind", it.kind);
    ctx.set_var(dropped, "name", it.name);
    pack.remove(slot);
    ctx.set_persistent("inventory", pack);
    #{ turn: true, msgs: [["You drop the " + it.name + ".", "#c0c0c0"]] }
}

fn glyph_of(kind) {
    let info = item_info(kind);
    if info.effect == "heal" { "!" }
    else if info.effect == "equip" { if info.slot == "weapon" { "/" } else { "[" } }
    else if info.effect == "amulet" { "\"" }
    else { "?" }
}

/// Use pack slot `slot`; (tx, ty) is the aimed cell for a targeted scroll.
fn do_use(id, ctx, slot, tx, ty) {
    let pack = ctx.get_persistent("inventory");
    if slot < 0 || slot >= pack.len() { return #{ turn: false, msgs: [] }; }
    let it = pack[slot];
    let info = item_info(it.kind);
    let r = if info.effect == "heal" { use_heal(id, ctx, info) }
        else if info.effect == "lightning" { use_lightning(id, ctx, info) }
        else if info.effect == "confusion" { use_confusion(ctx, info, tx, ty) }
        else if info.effect == "fireball" { use_fireball(id, ctx, info, tx, ty) }
        else if info.effect == "equip" { use_equip(ctx, it, info) }
        else { #{ turn: false, used: false, msgs: [["The " + it.name + " glows warmly in your hand.", "#ffd040"]] } };
    if r.used {
        pack.remove(slot);
        if info.effect == "equip" {
            // Whatever was in the slot goes back in the pack.
            let old = gear(ctx, info.slot);
            if old.kind != "none" { pack.push(#{ kind: old.kind, name: old.name }); }
        }
        ctx.set_persistent("inventory", pack);
    }
    #{ turn: r.turn, msgs: r.msgs }
}

fn use_heal(id, ctx, info) {
    let hp = ctx.get_persistent("hp");
    let max_hp = ctx.get_persistent("max_hp");
    if hp >= max_hp { return #{ turn: false, used: false, msgs: [["You are already at full health.", "#808080"]] }; }
    let heal = if hp + info.amount > max_hp { max_hp - hp } else { info.amount };
    ctx.set_persistent("hp", hp + heal);
    ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Green");
    #{ turn: true, used: true, msgs: [["You drink the potion and recover " + heal.to_int() + " hit points.", "#80ff80"]] }
}

/// The nearest monster in view, within the scroll's range, takes the bolt.
fn use_lightning(id, ctx, info) {
    let best = -1;
    let best_d = 1000.0;
    for m in ctx.find_all_by_tag("monster") {
        let d = ctx.get_distance(id, m);
        if d <= info.range && d < best_d && ctx.is_in_fov(ctx.get_x(m), ctx.get_y(m)) {
            best = m;
            best_d = d;
        }
    }
    if best < 0 { return #{ turn: false, used: false, msgs: [["No enemy is close enough to strike.", "#808080"]] }; }
    ctx.shake_camera(2.0, 0.2);
    #{ turn: true, used: true, msgs: hurt(ctx, best, info.amount, "Lightning strikes") }
}

fn use_confusion(ctx, info, tx, ty) {
    if !ctx.is_in_fov(tx, ty) { return #{ turn: false, used: false, msgs: [["You can't target what you can't see.", "#808080"]] }; }
    let m = tagged_at(ctx, tx, ty, "monster");
    if m < 0 { return #{ turn: false, used: false, msgs: [["There's no enemy there.", "#808080"]] }; }
    ctx.set_var(m, "confused", info.amount);
    #{ turn: true, used: true, msgs: [["The " + ctx.get_var(m, "name") + "'s eyes go vacant as it starts to stumble around!", "#c080ff"]] }
}

/// Everything within the radius of the aimed cell burns — you included.
fn use_fireball(id, ctx, info, tx, ty) {
    if !ctx.is_in_fov(tx, ty) { return #{ turn: false, used: false, msgs: [["You can't target what you can't see.", "#808080"]] }; }
    let msgs = [["The fireball explodes, burning everything within " + info.range + " tiles!", "#ff8020"]];
    let r = info.range * 1.0;
    for m in ctx.find_entities_in_rect(tx - r, ty - r, r * 2.0 + 1.0, r * 2.0 + 1.0) {
        if !ctx.has_tag(m, "monster") { continue; }
        let dx = ctx.get_x(m) - tx;
        let dy = ctx.get_y(m) - ty;
        if dx * dx + dy * dy <= r * r {
            for line in hurt(ctx, m, info.amount, "The fireball burns") { msgs.push(line); }
        }
    }
    let dx = ctx.get_x(id) - tx;
    let dy = ctx.get_y(id) - ty;
    if dx * dx + dy * dy <= r * r {
        let hp = ctx.get_persistent("hp") - info.amount;
        ctx.set_persistent("hp", hp);
        msgs.push(["You are caught in the blast for " + info.amount + " hit points!", "#ff4040"]);
        if hp <= 0 { ctx.set_persistent("killer", "fireball of your own"); }
    }
    for k in 0..12 { ctx.emit_particles(tx + ctx.random_int(-3, 3), ty + ctx.random_int(-3, 3), "*", "#ff8020"); }
    ctx.shake_camera(3.0, 0.3);
    #{ turn: true, used: true, msgs: msgs }
}

fn use_equip(ctx, it, info) {
    ctx.set_persistent(info.slot, #{ kind: it.kind, name: it.name, bonus: info.bonus });
    #{ turn: true, used: true, msgs: [["You equip the " + it.name + ".", "#a0ffa0"]] }
}

fn do_levelup(ctx, pick) {
    let level = ctx.get_persistent("level");
    ctx.set_persistent("xp", ctx.get_persistent("xp") - xp_to_next(level));
    ctx.set_persistent("level", level + 1);
    let what = if pick == 0 {
        ctx.set_persistent("max_hp", ctx.get_persistent("max_hp") + 20);
        ctx.set_persistent("hp", ctx.get_persistent("hp") + 20);
        "Your health increases."
    } else if pick == 1 {
        ctx.set_persistent("power", ctx.get_persistent("power") + 1);
        "You feel stronger."
    } else {
        ctx.set_persistent("defense", ctx.get_persistent("defense") + 1);
        "Your movements are getting swifter."
    };
    #{ turn: false, msgs: [["Welcome to level " + (level + 1) + "! " + what, "#ffff80"]] }
}

fn say(ctx, msgs) {
    if msgs.len() == 0 { return; }
    let log = if ctx.has_persistent("log") { ctx.get_persistent("log") } else { [] };
    for m in msgs { log.push(m); }
    while log.len() > 60 { log.remove(0); }
    ctx.set_persistent("log", log);
}
