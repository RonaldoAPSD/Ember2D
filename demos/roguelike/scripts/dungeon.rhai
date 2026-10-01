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
        "rat" => ["r", "#b09070", 4, 2, 0, 10, 100, "rat"],
        "kobold" => ["k", "#c0a040", 7, 3, 0, 25, 100, "kobold"],
        "goblin" => ["g", "#60c040", 10, 4, 1, 40, 100, "goblin"],
        "orc" => ["o", "#40a040", 16, 5, 1, 70, 100, "orc"],
        "troll" => ["T", "#20c020", 26, 8, 2, 140, 100, "troll"],
        "ogre" => ["O", "#c0a070", 36, 10, 3, 220, 80, "ogre"],
        "wraith" => ["W", "#a0a0ff", 28, 11, 3, 260, 150, "wraith"],
        "whelp" => ["d", "#ff6040", 42, 13, 4, 380, 100, "dragon whelp"],
        "drake" => ["D", "#ff4000", 140, 18, 6, 1500, 100, "Ember Drake"],
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
