// demos/roguelike/scripts/dungeon.rhai — builds each floor of the dungeon.
//
// `dungeon.level` is an empty 80x43 level with this script on one
// invisible tile. Every time the level loads — a new game, or the stairs
// from the floor above — `on_start` reads the run's depth and seed from
// persistent state and generates that floor:
//
//   1. rooms and corridors, the classic way (rogueliketutorials.com, part
//      3): up to 30 rooms of random size dropped at random spots, any that
//      overlap thrown away, each new room joined to the previous one by an
//      L-shaped corridor — so every room is connected;
//   2. monsters and items in each room, picked from tables by depth
//      (`monster_table`, `item_table` below — deeper floors, nastier
//      monsters, better loot);
//   3. the stairs down in the last room (floor 20: the Ember Drake and the
//      Amulet instead), the player in the first, and the first field of view.
//
// Its `on_update` draws the HUD every frame — the bottom panel (HP bar,
// stats, gear, the message log), mouse-look, and the death and victory
// screens — from persistent state, which is where player.rhai keeps the run.
// (The HUD lives here rather than in player.rhai only to keep each file to
// one job; any script could draw it.)
//
// The floor is a function of (run seed, depth) only: `set_random_seed`
// restarts the random stream before anything is rolled, so floor 10 of a
// run is the same floor 10 however the run got there (and a test can check
// floors 1, 10 and 20 of a seed directly). Tiles are placed with the tile
// API (`tile_def`/`tile_fill`), not as entities: a floor is 3,440 cells.

fn map_w() { 80 }
fn map_h() { 43 }
fn last_depth() { 20 }

fn on_start(id, ctx) {
    let depth = if ctx.has_persistent("depth") { ctx.get_persistent("depth") } else { 1 };
    let seed = if ctx.has_persistent("run_seed") { ctx.get_persistent("run_seed") } else { 1 };
    ctx.set_random_seed(seed * 1000 + depth);

    ctx.tile_def("wall", #{ glyph: "#", fg: "#8a8a9a", bg: "#20202a", solid: true, tag: "wall" });
    ctx.tile_def("floor", #{ glyph: ".", fg: "#68708a", tag: "floor" });
    ctx.tilemap_resize(map_w(), map_h());
    ctx.tile_fill(0, 0, map_w(), map_h(), "wall");

    let rooms = [];
    for attempt in 0..30 {
        let w = ctx.random_int(6, 10);
        let h = ctx.random_int(6, 10);
        let x = ctx.random_int(0, map_w() - w - 1);
        let y = ctx.random_int(0, map_h() - h - 1);
        let room = [x, y, w, h];
        let clear = true;
        for other in rooms {
            if overlaps(room, other) { clear = false; }
        }
        if !clear { continue; }
        ctx.tile_fill(x + 1, y + 1, w - 1, h - 1, "floor");
        if rooms.len() > 0 {
            let prev = rooms[rooms.len() - 1];
            tunnel(ctx, centre(prev), centre(room));
        }
        rooms.push(room);
    }

    let start = centre(rooms[0]);
    let player = ctx.find_by_tag("player");
    ctx.set_position(player, start[0], start[1]);

    // Everything placed is remembered so two things never share a cell.
    let taken = #{};
    taken[key(start[0], start[1])] = true;
    let end_room = rooms[rooms.len() - 1];
    let end = centre(end_room);
    if depth < last_depth() {
        let stairs = ctx.spawn_entity(">", end[0] * 1.0, end[1] * 1.0, "stairs");
        ctx.set_tint(stairs, "White", "Reset");
        taken[key(end[0], end[1])] = true;
    } else {
        // The last floor: the Ember Drake guards the Amulet of Ember, and
        // the way you came in is the way out (`<`, where you start).
        let up = ctx.spawn_entity("<", start[0] * 1.0, start[1] * 1.0, "upstairs");
        ctx.set_tint(up, "White", "Reset");
        spawn_item(ctx, "amulet", end[0], end[1]);
        taken[key(end[0], end[1])] = true;
        spawn_monster(ctx, "drake", end[0] + 1, end[1]);
        taken[key(end[0] + 1, end[1])] = true;
    }

    let max_monsters = if depth < 4 { 2 } else if depth < 9 { 3 } else if depth < 15 { 4 } else { 5 };
    let max_items = if depth < 4 { 1 } else { 2 };
    for i in 1..rooms.len() {
        let room = rooms[i];
        let n = ctx.random_int(0, max_monsters);
        for m in 0..n {
            let at = taken.free_spot(ctx, room);
            if at.len() > 0 { spawn_monster(ctx, pick(ctx, monster_table(), depth), at[0], at[1]); }
        }
        let n = ctx.random_int(0, max_items);
        for k in 0..n {
            let at = taken.free_spot(ctx, room);
            if at.len() > 0 { spawn_item(ctx, pick(ctx, item_table(), depth), at[0], at[1]); }
        }
    }

    ctx.compute_fov(start[0], start[1], 8);
    let msg = if depth == 1 { "You enter the Depths of Ember. Find the Amulet on the 20th floor." }
        else if depth == last_depth() { "The air burns. Something huge sleeps on this floor." }
        else { "You descend to depth " + depth + "." };
    let log = if ctx.has_persistent("log") { ctx.get_persistent("log") } else { [] };
    log.push([msg, if depth == last_depth() { "#ff8040" } else { "#a0c0ff" }]);
    ctx.set_persistent("log", log);
}

// ── Layout ───────────────────────────────────────────────────────────────

fn overlaps(a, b) {
    a[0] <= b[0] + b[2] && a[0] + a[2] >= b[0] && a[1] <= b[1] + b[3] && a[1] + a[3] >= b[1]
}

fn centre(r) { [r[0] + r[2] / 2, r[1] + r[3] / 2] }

fn key(x, y) { "" + x + "," + y }

/// An L-shaped corridor from `a` to `b`: horizontal first or vertical
/// first, at random.
fn tunnel(ctx, a, b) {
    let x1 = a[0];
    let y1 = a[1];
    let x2 = b[0];
    let y2 = b[1];
    let lo_x = if x1 < x2 { x1 } else { x2 };
    let lo_y = if y1 < y2 { y1 } else { y2 };
    let dx = (x2 - x1).abs() + 1;
    let dy = (y2 - y1).abs() + 1;
    if ctx.random_int(0, 1) == 0 {
        ctx.tile_fill(lo_x, y1, dx, 1, "floor");
        ctx.tile_fill(x2, lo_y, 1, dy, "floor");
    } else {
        ctx.tile_fill(x1, lo_y, 1, dy, "floor");
        ctx.tile_fill(lo_x, y2, dx, 1, "floor");
    }
}

/// A random empty floor cell inside `room`, or `[]` after a few misses —
/// called on the `taken` map (`taken.free_spot(ctx, room)`), which marks
/// the cell taken. Method style matters: Rhai passes a map argument by
/// value, but `this` by reference, so the mark sticks.
fn free_spot(ctx, room) {
    for tries in 0..10 {
        let x = ctx.random_int(room[0] + 1, room[0] + room[2] - 1);
        let y = ctx.random_int(room[1] + 1, room[1] + room[3] - 1);
        if !this.contains(key(x, y)) {
            this[key(x, y)] = true;
            return [x, y];
        }
    }
    []
}

// ── What lives down here ─────────────────────────────────────────────────

/// `[name, weight, from depth, to depth]` — a weighted pick among the rows
/// whose depth range covers the floor.
fn monster_table() {
    [
        ["rat", 80, 1, 5], ["kobold", 60, 1, 7], ["goblin", 50, 2, 10],
        ["orc", 60, 4, 13], ["troll", 40, 7, 17], ["ogre", 30, 10, 20],
        ["wraith", 30, 13, 20], ["whelp", 25, 16, 20],
    ]
}

fn item_table() {
    [
        ["potion", 70, 1, 20], ["lightning", 25, 2, 20], ["confusion", 20, 3, 20],
        ["fireball", 20, 6, 20], ["sword", 8, 3, 9], ["axe", 6, 8, 20],
        ["leather", 8, 2, 8], ["chainmail", 6, 6, 15], ["plate", 4, 12, 20],
        ["big_potion", 20, 9, 20],
    ]
}

fn pick(ctx, table, depth) {
    let total = 0;
    for row in table { if depth >= row[2] && depth <= row[3] { total += row[1]; } }
    let roll = ctx.random_int(1, total);
    for row in table {
        if depth >= row[2] && depth <= row[3] {
            roll -= row[1];
            if roll <= 0 { return row[0]; }
        }
    }
    table[0][0]
}

/// `[glyph, colour, hp, power, defense, xp, speed, display name]`.
fn monster_stats(kind) {
    switch kind {
        "rat" => ["r", "#b09070", 4, 2, 0, 15, 100, "rat"],
        "kobold" => ["k", "#c0a040", 7, 3, 0, 30, 100, "kobold"],
        "goblin" => ["g", "#60c040", 10, 4, 1, 50, 100, "goblin"],
        "orc" => ["o", "#40a040", 16, 5, 1, 80, 100, "orc"],
        "troll" => ["T", "#20c020", 26, 8, 2, 160, 100, "troll"],
        "ogre" => ["O", "#c0a070", 36, 10, 3, 250, 80, "ogre"],
        "wraith" => ["W", "#a0a0ff", 28, 11, 3, 300, 150, "wraith"],
        "whelp" => ["d", "#ff6040", 42, 13, 4, 450, 100, "dragon whelp"],
        "drake" => ["D", "#ff4000", 140, 18, 6, 2000, 100, "Ember Drake"],
        _ => ["?", "White", 1, 0, 0, 0, 100, kind],
    }
}

fn spawn_monster(ctx, kind, x, y) {
    let s = monster_stats(kind);
    let m = ctx.spawn_entity(s[0], x * 1.0, y * 1.0, "monster");
    ctx.set_tint(m, s[1], "Reset");
    ctx.set_layer_order(m, 5);
    ctx.set_var(m, "kind", kind);
    ctx.set_var(m, "name", s[7]);
    ctx.set_var(m, "hp", s[2]);
    ctx.set_var(m, "max_hp", s[2]);
    ctx.set_var(m, "power", s[3]);
    ctx.set_var(m, "defense", s[4]);
    ctx.set_var(m, "xp", s[5]);
    ctx.set_script(m, "scripts/monster.rhai");
    ctx.make_actor(m, s[6]);
}

/// `[glyph, colour, display name]`.
fn item_look(kind) {
    switch kind {
        "potion" => ["!", "#e040e0", "healing potion"],
        "big_potion" => ["!", "#ff80ff", "greater healing potion"],
        "lightning" => ["?", "#ffff40", "scroll of lightning"],
        "confusion" => ["?", "#c080ff", "scroll of confusion"],
        "fireball" => ["?", "#ff6020", "scroll of fireball"],
        "sword" => ["/", "#c0c0ff", "sword"],
        "axe" => ["/", "#ffc080", "war axe"],
        "leather" => ["[", "#a07040", "leather armour"],
        "chainmail" => ["[", "#c0c0c0", "chain mail"],
        "plate" => ["[", "#e0e0ff", "plate armour"],
        "amulet" => ["\"", "#ffd040", "Amulet of Ember"],
        _ => ["?", "White", kind],
    }
}

fn spawn_item(ctx, kind, x, y) {
    let look = item_look(kind);
    let it = ctx.spawn_entity(look[0], x * 1.0, y * 1.0, "item");
    ctx.set_tint(it, look[1], "Reset");
    ctx.set_layer_order(it, 3);
    ctx.set_var(it, "kind", kind);
    ctx.set_var(it, "name", look[2]);
}

// ── The HUD ──────────────────────────────────────────────────────────────

fn on_update(id, ctx) {
    if !ctx.has_persistent("max_hp") { return; }
    let vw = ctx.get_viewport_width();
    let vh = ctx.get_viewport_height();
    let top = vh - 7;
    ctx.fill_rect(0, top, vw, 7, " ", "White", "#0c0c14");
    draw_stats(ctx, top + 1);
    draw_log(ctx, top + 1, vw);
    draw_look(ctx, top);
    let player = ctx.find_by_tag("player");
    let hint = if player >= 0 && ctx.has_var(player, "aim") { " Arrows/mouse aim  Enter/click cast  Backspace cancel " }
        else { " Arrows move/attack  G get  I use  X drop  C you  Enter stairs  Esc menu " };
    ctx.draw_hud(vw - hint.len() - 1, top, hint, "#707084", "#0c0c14");
    if ctx.get_persistent("hp") <= 0 { draw_death(ctx, vw, vh); }
    else if ctx.has_persistent("won") { draw_victory(ctx, vw, vh); }
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
    let need = 100 + level * 150;
    ctx.draw_hud(1, row + 1, "Level " + level + "  XP " + ctx.get_persistent("xp").to_int() + "/" + need, "#e0d080", "#0c0c14");
    ctx.draw_hud(1, row + 2, "Depth " + ctx.get_persistent("depth").to_int(), "#a0c0ff", "#0c0c14");
    let weapon = gear(ctx, "weapon");
    let armour = gear(ctx, "armour");
    let power = ctx.get_persistent("power") + weapon.bonus;
    let defense = ctx.get_persistent("defense") + armour.bonus;
    ctx.draw_hud(1, row + 3, "Power " + power.to_int() + "  Defense " + defense.to_int(), "#c0c0c0", "#0c0c14");
    ctx.draw_hud(1, row + 4, weapon.name + ", " + armour.name, "#808898", "#0c0c14");
}

/// An equipment slot, or bare hands for a run that has none yet.
fn gear(ctx, slot) {
    if ctx.has_persistent(slot) { ctx.get_persistent(slot) } else { #{ kind: "none", name: "nothing", bonus: 0 } }
}

/// The newest messages, wrapped to the panel's width, newest at the
/// bottom, each in its own colour — as many lines as fit (six).
fn draw_log(ctx, row, vw) {
    let log = ctx.get_persistent("log");
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
fn draw_look(ctx, row) {
    let x = ctx.get_mouse_world_x().floor();
    let y = ctx.get_mouse_world_y().floor();
    if !ctx.is_in_fov(x, y) { return; }
    let names = [];
    for e in ctx.find_entities_in_rect(x + 0.25, y + 0.25, 0.5, 0.5) {
        if ctx.has_var(e, "name") { names.push(ctx.get_var(e, "name")); }
        else if ctx.has_tag(e, "player") { names.push("you"); }
        else if ctx.has_tag(e, "stairs") { names.push("stairs down"); }
        else if ctx.has_tag(e, "upstairs") { names.push("stairs up"); }
    }
    if names.len() == 0 { return; }
    let text = " " + names[0];
    for k in 1..names.len() { text += ", " + names[k]; }
    ctx.draw_hud(1, row, text + " ", "#ffffa0", "#0c0c14");
}

fn draw_death(ctx, vw, vh) {
    let x = vw / 2 - 20;
    let y = vh / 2 - 6;
    ctx.fill_rect(x, y, 40, 7, " ", "White", "#400808");
    ctx.draw_box(x, y, 40, 7, "#ff6060", "#400808");
    ctx.draw_hud(x + 16, y + 2, "YOU DIED", "#ffff60", "#400808");
    let killer = if ctx.has_persistent("killer") { ctx.get_persistent("killer") } else { "something" };
    let line = "Killed by a " + killer + " on depth " + ctx.get_persistent("depth").to_int();
    ctx.draw_hud(x + 20 - line.len() / 2, y + 3, line, "White", "#400808");
    ctx.draw_hud(x + 3, y + 5, "Press Enter to return to the title", "#c0c0c0", "#400808");
}

fn draw_victory(ctx, vw, vh) {
    let x = vw / 2 - 25;
    let y = vh / 2 - 7;
    ctx.fill_rect(x, y, 50, 9, " ", "White", "#302008");
    ctx.draw_box(x, y, 50, 9, "#ffd040", "#302008");
    ctx.draw_hud(x + 17, y + 2, "V I C T O R Y", "#ffd040", "#302008");
    ctx.draw_hud(x + 4, y + 4, "You escaped the Depths with the Amulet of Ember", "White", "#302008");
    let line = "Level " + ctx.get_persistent("level").to_int() + ", " + ctx.get_persistent("kills").to_int() + " monsters slain";
    ctx.draw_hud(x + 25 - line.len() / 2, y + 5, line, "#e0d080", "#302008");
    ctx.draw_hud(x + 8, y + 7, "Press Enter to return to the title", "#c0c0c0", "#302008");
}
