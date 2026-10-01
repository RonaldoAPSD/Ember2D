// demos/shooter/scripts/director.rhai — the game around the player: twelve
// waves, every enemy's movement and shooting, the boss, contact damage,
// the wave HUD, victory — and, on the stress level, a flood of enemies and
// bullets to measure the engine with.
//
// It lives on one tile of the arena (the top-left corner, under the wall),
// so its `on_update` runs every step. Tagged "stress" (stress.level), it
// runs the stress test instead of the waves.
//
// ENEMIES are spawned entities with no script of their own: their stats
// live in vars ("hp", "points", "colour", ...), and this script steers all
// of them each step, kind by kind (`steer`). One script looping over a
// hundred enemies is far cheaper than a hundred scripts. Bullets are the
// opposite case — each has its own tiny `on_collide`-only script (see
// bullet.rhai) because a hit is per-bullet news.
//
//   grunt   g  walks at you              swarmer s  fast, fragile
//   brute   B  slow, 2x2, hits hard      gunner  G  keeps its distance, shoots
//   boss       an armoured core (wave 12) that fires rings of shots
//
// A wave's enemies don't appear all at once: they queue ("queue") and
// three arrive per step at random spots well away from the player.

fn total_waves() { 12 }

/// `[grunts, swarmers, brutes, gunners]` per wave.
fn wave_plan(w) {
    let t = [[8, 0, 0, 0], [12, 8, 0, 0], [12, 10, 0, 2], [16, 14, 1, 3], [16, 20, 2, 4], [20, 24, 2, 6],
             [24, 30, 3, 6], [26, 36, 4, 8], [30, 40, 4, 10], [34, 48, 6, 10], [40, 56, 6, 12], [16, 24, 2, 6]];
    t[w - 1]
}

/// `[glyph, colour, hp, speed, size, points, contact damage]`.
fn stats(kind) {
    switch kind {
        "grunt" => ["g", "#ff5050", 2, 3.6, 1.0, 10, 1],
        "swarmer" => ["s", "#ffe040", 1, 6.4, 1.0, 5, 1],
        "brute" => ["B", "#e060ff", 9, 2.0, 2.0, 50, 3],
        "gunner" => ["G", "#40e0ff", 3, 2.6, 1.0, 25, 1],
        _ => ["O", "#ff4000", 150, 1.6, 1.0, 1000, 3],
    }
}

fn on_start(id, ctx) {
    ctx.set_global("wave", 0);
    ctx.set_global("phase", "breather");
    ctx.set_global("phase_until", -1.0);
    ctx.set_global("queue", []);
    if ctx.get_tag(id) == "stress" { ctx.set_global("stress", true); }
}

fn on_update(id, ctx) {
    let now = ctx.get_elapsed();
    let player = ctx.find_by_tag("player");
    if player < 0 { return; }
    // Arm the opening countdown on the first real step (`on_start` sees
    // `get_elapsed()` as 0 whatever the clock says).
    if ctx.get_global("phase_until") < 0.0 {
        ctx.set_global("phase_until", now + 3.0);
        return;
    }
    if ctx.has_global("stress") { stress(ctx, player, now); }
    else { waves(ctx, now); }
    steer(ctx, player, now);
    contact_damage(ctx, player, now);
}

// ── Waves ────────────────────────────────────────────────────────────────

fn waves(ctx, now) {
    let phase = ctx.get_global("phase");
    spawn_queued(ctx, 3);
    if phase == "won" {
        centre(ctx, 10, " ARENA CLEARED ", "Black", "#60ff60");
        centre(ctx, 12, "Final score " + ctx.get_global("score").to_int() + "  -  press R to play again", "White", "Reset");
    } else if phase == "breather" {
        let left = ctx.get_global("phase_until") - now;
        if left <= 0.0 { next_wave(ctx); }
        else {
            let n = ctx.get_global("wave") + 1;
            let title = if n == total_waves() { "THE CORE AWAKENS" } else { "WAVE " + n + " OF " + total_waves() };
            centre(ctx, 10, " " + title + " ", "Black", "#40c0ff");
            centre(ctx, 11, "in " + (left + 1.0).to_int(), "#40c0ff", "Reset");
        }
    } else {
        wave_hud(ctx);
        if ctx.get_global("queue").len() == 0 && enemies_left(ctx) == 0 && !ctx.has_global("boss_alive") {
            ctx.set_global("phase", "breather");
            ctx.set_global("phase_until", now + 3.0);
        }
    }
}

fn enemies_left(ctx) {
    ctx.count_by_tag("grunt") + ctx.count_by_tag("swarmer") + ctx.count_by_tag("brute") + ctx.count_by_tag("gunner") + ctx.count_by_tag("boss")
}

fn next_wave(ctx) {
    let cleared = ctx.get_global("wave");
    if cleared > 0 { ctx.add_global("score", 25 * cleared); }
    if cleared >= total_waves() {
        ctx.set_global("phase", "won");
        ctx.set_global("won", true);
        let score = ctx.get_global("score");
        if score > ctx.get_global("best") {
            ctx.set_global("best", score);
            ctx.save_data("ember_assault_best.sav", score);
        }
        return;
    }
    let w = cleared + 1;
    ctx.set_global("wave", w);
    ctx.set_global("phase", "fighting");
    ctx.play_sound("audio/wave.ogg");
    let plan = wave_plan(w);
    let kinds = ["grunt", "swarmer", "brute", "gunner"];
    let q = [];
    for k in 0..4 { for n in 0..plan[k] { q.push(kinds[k]); } }
    // Shuffle so kinds arrive mixed.
    for i in 0..q.len() {
        let j = ctx.random_int(0, q.len() - 1);
        let t = q[i];
        q[i] = q[j];
        q[j] = t;
    }
    ctx.set_global("queue", q);
    if w == total_waves() { spawn_boss(ctx); }
}

fn spawn_queued(ctx, n) {
    let q = ctx.get_global("queue");
    if q.len() == 0 { return; }
    for i in 0..n {
        if q.len() == 0 { break; }
        spawn_enemy(ctx, q.pop());
    }
    ctx.set_global("queue", q);
}

/// A random open spot 25-60 cells from the player (off screen, but not
/// so far it takes forever to arrive).
fn spawn_spot(ctx) {
    let player = ctx.find_by_tag("player");
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    let x = 0.0;
    let y = 0.0;
    for tries in 0..25 {
        x = ctx.random_int(2, 157).to_float();
        y = ctx.random_int(2, 57).to_float();
        if ctx.is_solid_at(x, y) || ctx.is_solid_at(x + 1.0, y + 1.0) { continue; }
        let d2 = (x - px) * (x - px) + (y - py) * (y - py);
        if d2 > 625.0 && d2 < 3600.0 { return [x, y]; }
    }
    [x, y]
}

fn spawn_enemy(ctx, kind) {
    let s = stats(kind);
    let at = spawn_spot(ctx);
    let e = ctx.spawn_entity(s[0], at[0], at[1], kind, s[1], "Reset", 8, false, s[4], s[4], "enemy");
    ctx.set_var(e, "hp", s[2]);
    ctx.set_var(e, "points", s[5]);
    ctx.set_var(e, "colour", s[1]);
    if kind == "gunner" { ctx.set_var(e, "next_shot", ctx.get_elapsed() + 1.0 + ctx.random_float()); }
    e
}

/// The boss: an armoured core (`O`) ringed by eight parts that move with
/// it (each parented to the core). A bullet hitting a part hurts the core
/// (bullet.rhai passes the hit to the part's parent).
fn spawn_boss(ctx) {
    let core = spawn_enemy(ctx, "boss");
    ctx.set_var(core, "next_ring", ctx.get_elapsed() + 2.0);
    ctx.set_global("boss_alive", true);
    let parts = [[-1, -1, "/"], [0, -1, "-"], [1, -1, "\\"], [-1, 0, "|"], [1, 0, "|"], [-1, 1, "\\"], [0, 1, "-"], [1, 1, "/"]];
    for p in parts {
        let part = ctx.spawn_entity(p[2], p[0] * 1.0, p[1] * 1.0, "boss_part", "#ff8040", "#401008", 8, false, 1.0, 1.0, "enemy");
        ctx.set_parent(part, core);
    }
}

fn wave_hud(ctx) {
    let s = "WAVE " + ctx.get_global("wave") + "/" + total_waves() + "  LEFT " + (enemies_left(ctx) + ctx.get_global("queue").len());
    ctx.draw_hud(ctx.get_viewport_width() - s.len() - 2, 1, s, "#40c0ff", "#101018");
    let core = ctx.find_by_tag("boss");
    if core >= 0 {
        let hp = ctx.get_var(core, "hp");
        let w = 40;
        let x = ctx.get_viewport_width() / 2 - w / 2;
        ctx.fill_rect(x, 2, w, 1, " ", "White", "#401008");
        let fill = (hp * w / 150).to_int();
        if fill > 0 { ctx.fill_rect(x, 2, fill, 1, " ", "White", "#e04010"); }
        ctx.draw_hud(x + 15, 2, " THE CORE ", "White", "#e04010");
    } else if ctx.has_global("boss_alive") {
        ctx.remove_global("boss_alive");
        for p in ctx.find_all_by_tag("boss_part") { ctx.despawn(p); }
    }
}

fn centre(ctx, y, text, fg, bg) {
    ctx.draw_hud((ctx.get_viewport_width() - text.len()) / 2, y, text, fg, bg);
}

// ── Steering and shooting ────────────────────────────────────────────────

/// Every enemy, kind by kind: point it at the player (sliding along walls,
/// since nothing pushes an enemy out of a wall but this), and let gunners
/// and the boss shoot.
fn steer(ctx, player, now) {
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    let wander = ctx.has_global("stress");
    for kind in ["grunt", "swarmer", "brute", "gunner", "boss"] {
        let speed = stats(kind)[3];
        for e in ctx.find_all_by_tag(kind) {
            let ex = ctx.get_x(e);
            let ey = ctx.get_y(e);
            // The stress test's crowd wanders between random goals instead
            // of mobbing the player (so the bullet ring has room to fly).
            let tx = if wander { ctx.get_var(e, "gx") } else { px };
            let ty = if wander { ctx.get_var(e, "gy") } else { py };
            let dx = tx - ex;
            let dy = ty - ey;
            if wander && dx * dx + dy * dy < 4.0 {
                ctx.set_var(e, "gx", ctx.random_int(3, 156).to_float());
                ctx.set_var(e, "gy", ctx.random_int(3, 56).to_float());
            }
            let d2 = dx * dx + dy * dy;
            if d2 < 0.01 { ctx.set_velocity(e, 0.0, 0.0); continue; }
            let d = d2.sqrt();
            dx /= d;
            dy /= d;
            if kind == "gunner" {
                // Hold at 8-13 cells: back off when close, circle when in range.
                if d < 8.0 { dx = -dx; dy = -dy; }
                else if d < 13.0 { let t = dx; dx = -dy; dy = t; }
                if d < 24.0 && now >= ctx.get_var(e, "next_shot") {
                    fire(ctx, ex, ey, (px - ex) / d, (py - ey) / d, 16.0);
                    ctx.set_var(e, "next_shot", now + 1.4 + ctx.random_float() * 0.6);
                }
            } else if kind == "boss" {
                if d < 10.0 { let t = dx; dx = -dy; dy = t; }
                boss_attack(ctx, e, ex, ey, now);
            }
            if dx > 0.0 && ctx.is_solid_at(ex + 1.0, ey) { dx = 0.0; }
            else if dx < 0.0 && ctx.is_solid_at(ex - 1.0, ey) { dx = 0.0; }
            if dy > 0.0 && ctx.is_solid_at(ex, ey + 1.0) { dy = 0.0; }
            else if dy < 0.0 && ctx.is_solid_at(ex, ey - 1.0) { dy = 0.0; }
            ctx.set_velocity(e, dx * speed, dy * speed);
        }
    }
}

/// An enemy shot from (x, y) along the unit direction (dx, dy).
fn fire(ctx, x, y, dx, dy, speed) {
    let b = ctx.spawn_entity("o", x + dx * 0.8, y + dy * 0.8, "ebullet", "#ff7040", "Reset", 11, false, 0.4, 0.4, "ebullet");
    ctx.set_collider_mask(b, ["player", "solid"]);
    ctx.set_velocity(b, dx * speed, dy * speed);
    ctx.set_script(b, "scripts/ebullet.rhai");
}

/// Sixteen directions around a circle, written down rather than computed
/// with sin/cos (see player.rhai's header: the simulation must be the same
/// on every machine).
fn ring16() {
    [[1.0, 0.0], [0.9239, 0.3827], [0.7071, 0.7071], [0.3827, 0.9239], [0.0, 1.0], [-0.3827, 0.9239],
     [-0.7071, 0.7071], [-0.9239, 0.3827], [-1.0, 0.0], [-0.9239, -0.3827], [-0.7071, -0.7071], [-0.3827, -0.9239],
     [0.0, -1.0], [0.3827, -0.9239], [0.7071, -0.7071], [0.9239, -0.3827]]
}

fn boss_attack(ctx, e, x, y, now) {
    if now < ctx.get_var(e, "next_ring") { return; }
    let angry = ctx.get_var(e, "hp") < 75;
    for d in ring16() { fire(ctx, x, y, d[0], d[1], 11.0); }
    ctx.play_sound("audio/enemy_shot.ogg");
    ctx.set_var(e, "next_ring", now + if angry { 1.0 } else { 1.8 });
    if angry && ctx.random_bool(0.3) {
        for k in 0..3 { spawn_enemy(ctx, "swarmer"); }
    }
}

/// One contact hit at a time — the largest of whatever touches the player
/// — then a short invulnerability ("hurt_until", shared with enemy shots).
fn contact_damage(ctx, player, now) {
    if ctx.get_global("hp") <= 0 || now < ctx.get_global("hurt_until") { return; }
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    let dmg = 0;
    for e in ctx.find_entities_in_rect(px - 0.3, py - 0.3, 1.3, 1.3) {
        let d = if ctx.has_tag(e, "brute") || ctx.has_tag(e, "boss") || ctx.has_tag(e, "boss_part") { 3 }
            else if ctx.has_var(e, "points") { 1 } else { 0 };
        if d > dmg { dmg = d; }
    }
    if dmg == 0 { return; }
    let _hp = ctx.add_global("hp", -dmg);
    ctx.set_global("hurt_until", now + 0.7);
    ctx.play_sound("audio/hurt.ogg");
    ctx.emit_particles(px, py, "!", "Red");
    if ctx.get_global("shake") { ctx.shake_camera(2.0, 0.15); }
}

// ── The stress test (stress.level) ───────────────────────────────────────

/// Keeps about 240 enemies alive and a ring of player bullets pouring out,
/// with the player unharmed — 300-400 live entities, steadily, to watch the
/// frame rate (F3) and measure the simulation (`bench_sim`).
fn stress(ctx, player, now) {
    ctx.set_global("hp", 12);
    let alive = ctx.count_by_tag("grunt") + ctx.count_by_tag("swarmer");
    for i in 0..4 {
        if alive + i < 240 {
            // Tough enough that the bullet ring doesn't thin them out: the
            // point is a steady crowd, not a fair fight.
            let e = spawn_enemy(ctx, if ctx.random_bool(0.5) { "grunt" } else { "swarmer" });
            ctx.set_var(e, "hp", 50);
            ctx.set_var(e, "gx", ctx.random_int(3, 156).to_float());
            ctx.set_var(e, "gy", ctx.random_int(3, 56).to_float());
        }
    }
    let k = ctx.add_global("stress_tick", 1);
    let dirs = ring16();
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    for j in 0..6 {
        let d = dirs[(k.to_int() * 5 + j * 3) % 16];
        let b = ctx.spawn_entity("*", px + d[0], py + d[1], "bullet", "White", "Reset", 12, false, 0.4, 0.4, "pbullet");
        ctx.set_collider_mask(b, ["enemy", "solid"]);
        ctx.set_velocity(b, d[0] * 34.0, d[1] * 34.0);
        ctx.set_script(b, "scripts/bullet.rhai");
    }
    let n = alive + ctx.count_by_tag("bullet") + ctx.count_by_tag("ebullet");
    ctx.draw_hud(ctx.get_viewport_width() - 32, 1, " STRESS TEST  entities " + n + " ", "Black", "#ffe040");
}
