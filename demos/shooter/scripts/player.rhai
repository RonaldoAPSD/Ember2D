// demos/shooter/scripts/player.rhai — the player: WASD to move, the mouse
// to aim, hold the left button to fire; the pickups (medkits and the three
// weapon powerups); the top HUD; the death screen and the high score.
//
// The run's numbers are GLOBALS (hp, score, kills, weapon...): a restart is
// just `load_level("arena.level")`, and globals start empty on every load.
// The two things that outlive a run are kept on disk with `save_data`: the
// best score (`ember_assault_best.sav`) and the options the pause menu sets
// (`ember_assault_options.sav`), read back with `load_data` in `on_start`.
//
// DIRECTIONS WITHOUT TRIGONOMETRY. A spread shot needs the aim direction
// turned by a few degrees. `sin`/`cos` would do it, but they come from the
// platform's maths library, which can differ by a bit between machines —
// and the simulation must be identical everywhere (CLAUDE.md). So the few
// angles used are written down once (`turn6`, `turn12`, `turn18`) and a
// direction is turned by plain multiplication with them.

fn hp_max()        { 12 }
fn move_speed()    { 12.0 }   // cells/second
fn bullet_speed()  { 34.0 }
fn powerup_time()  { 12.0 }   // seconds a weapon powerup lasts
fn medkit_heal()   { 4 }

fn on_start(id, ctx) {
    ctx.set_global("hp", hp_max());
    ctx.set_global("score", 0);
    ctx.set_global("kills", 0);
    ctx.set_global("fire_ready", 0.0);
    ctx.set_global("hurt_until", 0.0);
    ctx.set_global("weapon", "pistol");
    ctx.set_global("weapon_until", 0.0);
    let best = ctx.load_data("ember_assault_best.sav");
    ctx.set_global("best", if type_of(best) == "()" { 0 } else { best });
    let options = ctx.load_data("ember_assault_options.sav");
    ctx.set_global("shake", if type_of(options) == "map" && options.contains("shake") { options.shake } else { true });
    // The view never leaves the arena.
    ctx.set_camera_bounds(0, 0, 160, 60);
}

fn on_input(id, ctx) {
    if ctx.get_global("hp") <= 0 || ctx.has_global("won") {
        ctx.set_velocity(id, 0.0, 0.0);
        if ctx.just_pressed("r") { ctx.load_level("arena.level"); }
        return;
    }
    let vx = 0.0;
    let vy = 0.0;
    if ctx.is_held("a") || ctx.is_held("left") { vx -= 1.0; }
    if ctx.is_held("d") || ctx.is_held("right") { vx += 1.0; }
    if ctx.is_held("w") || ctx.is_held("up") { vy -= 1.0; }
    if ctx.is_held("s") || ctx.is_held("down") { vy += 1.0; }
    if vx != 0.0 && vy != 0.0 { vx *= 0.70710678; vy *= 0.70710678; }
    ctx.set_velocity(id, vx * move_speed(), vy * move_speed());
    if ctx.mouse_left_held() { shoot(id, ctx); }
}

// ── Shooting ─────────────────────────────────────────────────────────────

/// (dx, dy) turned by the angle whose cosine and sine are `c` and `s`.
fn turn(d, c, s) { [d[0] * c - d[1] * s, d[0] * s + d[1] * c] }
fn turn6(d, sign) { turn(d, 0.9945219, 0.1045285 * sign) }
fn turn12(d, sign) { turn(d, 0.9781476, 0.2079117 * sign) }
fn turn18(d, sign) { turn(d, 0.9510565, 0.3090170 * sign) }

fn shoot(id, ctx) {
    let now = ctx.get_elapsed();
    if now < ctx.get_global("fire_ready") { return; }
    let px = ctx.get_x(id);
    let py = ctx.get_y(id);
    let dx = ctx.get_mouse_world_x() - px;
    let dy = ctx.get_mouse_world_y() - py;
    let d2 = dx * dx + dy * dy;
    if d2 < 0.01 { return; }
    let inv = 1.0 / d2.sqrt();
    let aim = [dx * inv, dy * inv];

    let weapon = ctx.get_global("weapon");
    if weapon != "pistol" && now > ctx.get_global("weapon_until") {
        weapon = "pistol";
        ctx.set_global("weapon", weapon);
    }
    if weapon == "spread" {
        for d in [turn12(aim, -1.0), aim, turn12(aim, 1.0)] { bullet(ctx, px, py, d, "*", "Yellow"); }
        ctx.set_global("fire_ready", now + 0.18);
        ctx.play_sound("audio/shoot.ogg");
    } else if weapon == "shotgun" {
        let fan = [turn18(aim, -1.0), turn12(aim, -1.0), turn6(aim, -1.0), aim, turn6(aim, 1.0), turn12(aim, 1.0), turn18(aim, 1.0)];
        for d in fan { pellet(ctx, px, py, d, now); }
        ctx.set_global("fire_ready", now + 0.42);
        ctx.play_sound("audio/shotgun.ogg");
    } else {
        bullet(ctx, px, py, aim, "*", "White");
        ctx.set_global("fire_ready", now + if weapon == "rapid" { 0.07 } else { 0.15 });
        ctx.play_sound("audio/shoot.ogg");
    }
}

/// A player bullet: it flies until `bullet.rhai`'s `on_collide` sees it hit
/// an enemy or a wall. Its collider is on the "pbullet" layer and only
/// tests against "enemy" and "solid", so bullets never collide with each
/// other or with the player — hundreds can be in the air cheaply.
fn bullet(ctx, px, py, d, glyph, colour) {
    let b = ctx.spawn_entity(glyph, px + d[0] * 0.8, py + d[1] * 0.8, "bullet", colour, "Reset", 12, false, 0.4, 0.4, "pbullet");
    ctx.set_collider_mask(b, ["enemy", "solid"]);
    ctx.set_velocity(b, d[0] * bullet_speed(), d[1] * bullet_speed());
    ctx.set_script(b, "scripts/bullet.rhai");
}

/// A shotgun pellet: short-lived (`pellet.rhai` times it out).
fn pellet(ctx, px, py, d, now) {
    let b = ctx.spawn_entity(".", px + d[0] * 0.8, py + d[1] * 0.8, "bullet", "#ffc060", "Reset", 12, false, 0.4, 0.4, "pbullet");
    ctx.set_collider_mask(b, ["enemy", "solid"]);
    ctx.set_velocity(b, d[0] * 30.0, d[1] * 30.0);
    ctx.set_var(b, "until", now + 0.35);
    ctx.set_script(b, "scripts/pellet.rhai");
}

// ── Pickups ──────────────────────────────────────────────────────────────

fn on_collide(id, other, ctx) {
    if !ctx.has_tag(other, "pickup") || ctx.get_global("hp") <= 0 { return; }
    let kind = ctx.get_var(other, "kind");
    if kind == "medkit" {
        let hp = ctx.get_global("hp") + medkit_heal();
        ctx.set_global("hp", if hp > hp_max() { hp_max() } else { hp });
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Green");
    } else {
        ctx.set_global("weapon", kind);
        ctx.set_global("weapon_until", ctx.get_elapsed() + powerup_time());
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "*", "Cyan");
    }
    ctx.play_sound("audio/powerup.ogg");
    ctx.despawn(other);
}

// ── HUD, death, the high score ───────────────────────────────────────────

fn on_update(id, ctx) {
    let hp = ctx.get_global("hp");
    if hp <= 0 && !ctx.has_global("over") { game_over(ctx); }
    draw_stats(ctx);
    if hp <= 0 { draw_death(ctx); }
}

/// Once, when the run ends: keep the score if it's the best yet.
fn game_over(ctx) {
    ctx.set_global("over", true);
    let score = ctx.get_global("score");
    if score > ctx.get_global("best") {
        ctx.set_global("best", score);
        ctx.set_global("new_best", true);
        ctx.save_data("ember_assault_best.sav", score);
    }
}

fn draw_stats(ctx) {
    let hp = ctx.get_global("hp");
    let bar = "";
    for i in 0..hp_max() { bar += if i < hp { "|" } else { "." }; }
    let colour = if hp <= 3 { "Red" } else if hp <= 6 { "Yellow" } else { "Green" };
    // Only as wide as the stats: the right of row 0 is where F3's debug
    // bar (the frame rate) shows.
    ctx.fill_rect(0, 0, 56, 1, " ", "White", "#101018");
    ctx.draw_hud(1, 0, "HP", "Grey", "#101018");
    ctx.draw_hud(4, 0, bar, colour, "#101018");
    let s = "SCORE " + ctx.get_global("score").to_int();
    s += "  BEST " + ctx.get_global("best").to_int();
    ctx.draw_hud(18, 0, s, "White", "#101018");
    let weapon = ctx.get_global("weapon");
    if weapon != "pistol" {
        let left = (ctx.get_global("weapon_until") - ctx.get_elapsed()).to_int();
        if left >= 0 { ctx.draw_hud(42, 0, weapon.to_upper() + " " + left, "Cyan", "#101018"); }
    }
    let vh = ctx.get_viewport_height();
    ctx.draw_hud(1, vh - 1, "WASD move   Mouse aim   Hold click: fire   R restart   Esc menu", "DarkGrey", "Reset");
}

fn draw_death(ctx) {
    let x = ctx.get_viewport_width() / 2 - 16;
    ctx.fill_rect(x, 8, 32, 8, " ", "White", "#400808");
    ctx.draw_box(x, 8, 32, 8, "#ff6060", "#400808");
    ctx.draw_hud(x + 12, 10, "YOU DIED", "Yellow", "#400808");
    let s = "Score " + ctx.get_global("score").to_int() + "  Wave " + ctx.get_global("wave");
    ctx.draw_hud(x + 16 - s.len() / 2, 11, s, "White", "#400808");
    if ctx.has_global("new_best") { ctx.draw_hud(x + 8, 12, "A NEW BEST SCORE!", "#ffd040", "#400808"); }
    ctx.draw_hud(x + 7, 14, "Press R to restart", "#c0c0c0", "#400808");
}
