// demos/roguelike/scripts/monster.rhai — every monster's turn.
//
// `dungeon.rhai` spawns each monster with this script, its stats in vars
// (`hp`, `max_hp`, `power`, `defense`, `xp`, `name`) and `make_actor`, so
// the turn scheduler gives it an `on_turn` after every player turn (the
// project's `ai_turns_per_step` lets all of them answer in the same frame).
//
// A monster sleeps until it sees the player. Sight is symmetric, so "the
// monster sees the player" is "the monster's cell is in the player's field
// of view" — `is_in_fov`, no separate line-of-sight check. Once awake it
// walks the shortest path toward the player (`get_path`) and attacks when
// next to them. A confused monster (scroll of confusion) stumbles about
// for a few turns instead.
//
// Messages go to the log in persistent state. A monster's turn is its own
// script pass, so its one log write can't race another monster's.

fn on_turn(id, ctx) {
    if !ctx.has_var(id, "hp") || ctx.get_var(id, "hp") <= 0 { return; }
    let player = ctx.find_by_tag("player");
    if player < 0 || ctx.get_persistent("hp") <= 0 { return; }
    let mx = ctx.get_x(id);
    let my = ctx.get_y(id);
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    let msgs = [];

    if ctx.has_var(id, "confused") && ctx.get_var(id, "confused") > 0 {
        let left = ctx.get_var(id, "confused") - 1;
        ctx.set_var(id, "confused", left);
        let dirs = [[1, 0], [-1, 0], [0, 1], [0, -1]];
        let d = dirs[ctx.random_int(0, 3)];
        let nx = mx + d[0];
        let ny = my + d[1];
        if !(nx == px && ny == py) && !blocked(ctx, nx, ny, id) { ctx.set_position(id, nx, ny); }
        if left == 0 { msgs.push(["The " + ctx.get_var(id, "name") + " is no longer confused.", "#c080ff"]); }
        say(ctx, msgs);
        return;
    }

    if !ctx.has_var(id, "aware") {
        if !ctx.is_in_fov(mx, my) { return; }
        ctx.set_var(id, "aware", true);
    }

    if (px - mx).abs() + (py - my).abs() == 1.0 {
        msgs = attack(id, ctx);
    } else {
        let path = ctx.get_path(mx, my, px, py, []);
        if path.len() > 0 && path.len() < 30 {
            let nx = path[0][0];
            let ny = path[0][1];
            if !blocked(ctx, nx, ny, id) { ctx.set_position(id, nx, ny); }
        }
    }
    say(ctx, msgs);
}

/// Is (x, y) a wall or another monster's cell? (Every entity in the cell
/// is checked — `get_entity_at` would only name the lowest id, perhaps an
/// item lying under a monster.)
fn blocked(ctx, x, y, me) {
    if ctx.is_solid_at(x, y) { return true; }
    for e in ctx.find_entities_in_rect(x + 0.25, y + 0.25, 0.5, 0.5) {
        if e != me && ctx.has_tag(e, "monster") { return true; }
    }
    false
}

/// Hits the player; returns the log lines it makes (Rhai passes arrays by
/// value, so a function can't push into its caller's).
fn attack(id, ctx) {
    let msgs = [];
    let name = ctx.get_var(id, "name");
    let defense = ctx.get_persistent("defense") + bonus(ctx, "armour");
    let damage = ctx.get_var(id, "power") - defense;
    if damage <= 0 {
        msgs.push(["The " + name + " attacks you but does no damage.", "#c0c0c0"]);
        return msgs;
    }
    let hp = ctx.get_persistent("hp") - damage;
    ctx.set_persistent("hp", hp);
    ctx.shake_camera(1.5, 0.15);
    ctx.play_sound("audio/hurt.ogg");
    msgs.push(["The " + name + " attacks you for " + damage + " hit points.", "#ff8080"]);
    if hp <= 0 {
        msgs.push(["You died! Killed by a " + name + " on depth " + ctx.get_persistent("depth") + ".", "#ff4040"]);
        ctx.set_persistent("killer", name);
        // Permadeath: the save becomes this moment, so the title's
        // Continue can't take it back (it loads straight to the death
        // screen). Written after this pass's writes, so it holds hp <= 0.
        ctx.save_game("depths.sav");
    }
    msgs
}

/// The defense (or power) an equipped item adds — `armour`/`weapon` are
/// maps in persistent state, set by equipping (player.rhai).
fn bonus(ctx, slot) {
    if !ctx.has_persistent(slot) { return 0; }
    let item = ctx.get_persistent(slot);
    if item.contains("bonus") { item.bonus } else { 0 }
}

/// Append `msgs` to the message log (persistent "log", newest last, the
/// last 60 kept). One write per pass: a second `say` in the same pass
/// would read the log as it was before the first (writes are deferred),
/// so callers collect their lines and say them once.
fn say(ctx, msgs) {
    if msgs.len() == 0 { return; }
    let log = if ctx.has_persistent("log") { ctx.get_persistent("log") } else { [] };
    for m in msgs { log.push(m); }
    while log.len() > 60 { log.remove(0); }
    ctx.set_persistent("log", log);
}
