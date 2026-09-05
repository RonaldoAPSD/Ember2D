// roguelike/scripts/enemy_boss.rhai — floor 3's boss: the exact same
// turn-scheduled chase-and-bump-attack model as enemy_rat.rhai (see that
// file's header comment for the full reasoning — on_turn vs. on_update,
// why the awareness tint and decision are split, why damage writes the
// player's "hp" directly now, why the old "two rats on one cell" limitation
// no longer applies) — just a bigger health pool (15 hp vs a rat's 6) and
// slightly harder-hitting (3 dmg vs a rat's 2, matching the player's own
// per-hit damage). Deliberately an attrition check, not a damage race:
// with these numbers a full-hp player wins a straight toe-to-toe fight
// with room to spare (confirmed by hand — a 4-dmg boss very nearly killed
// the player first, since it starts landing counter-hits almost as soon as
// the player's own attacks do). No special abilities; kept as a
// straightforward "bigger rat" per this project's own keep-it-simple
// roguelike scope.
//
// Tagged "boss", not "enemy" — player.rhai's attack() already handles both
// tags identically (written that way back in 4b, before either enemy
// script existed). stairs.rhai already locks any level's stairs while
// `ctx.count_by_tag("boss") > 0`; placing this tile on floor 3 IS the
// entire boss-gate mechanism — no stairs.rhai or engine changes needed to
// wire it up.
fn on_start(id, ctx) {
    ctx.set_global("hp_" + id, 15);
}

/// See enemy_rat.rhai's own on_update for the full reasoning (death check
/// + tint only; the awareness decision lives in on_turn below).
fn on_update(id, ctx) {
    let hp;
    if !ctx.has_global("hp_" + id) {
        hp = 15;
        ctx.set_global("hp_" + id, hp);
    } else {
        hp = ctx.get_global("hp_" + id);
    }

    if hp <= 0 {
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "*", "DarkMagenta");
        ctx.despawn(id);
        return;
    }

    if ctx.get_global("aware_" + id) == true { ctx.set_tint(id, "Magenta", "Reset"); }
    else { ctx.set_tint(id, "DarkMagenta", "Reset"); }
}

/// See enemy_rat.rhai's own on_turn for the full reasoning.
fn on_turn(id, ctx) {
    let hp = or_zero(ctx.get_global("hp_" + id));
    if hp <= 0 { return; }

    let player = ctx.find_by_tag("player");
    if !ctx.entity_exists(player) { return; }

    let ex = ctx.get_x(id);
    let ey = ctx.get_y(id);
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);

    if !update_awareness(id, ctx, hp, ex, ey, px, py) { return; }

    let dx = px - ex;
    let dy = py - ey;

    let adjacent = (dx == 0.0 && dy.abs() == 1.0) || (dy == 0.0 && dx.abs() == 1.0);
    if adjacent {
        ctx.set_persistent("hp", or_zero(ctx.get_persistent("hp")) - 3);
        ctx.shake_camera(2.0, 0.2);
        ctx.play_sound("roguelike/audio/hurt.ogg");
    } else {
        let step_x = 0.0;
        let step_y = 0.0;
        if dx.abs() >= dy.abs() && dx != 0.0 { step_x = if dx > 0.0 { 1.0 } else { -1.0 }; }
        else if dy != 0.0 { step_y = if dy > 0.0 { 1.0 } else { -1.0 }; }

        let tx = ex + step_x;
        let ty = ey + step_y;
        if (step_x != 0.0 || step_y != 0.0) && !ctx.is_solid_at(tx, ty) {
            ctx.set_position(id, tx, ty);
        }
    }
}

/// See player.rhai's header comment for why this guard exists: a get_*
/// never observes a set_* from earlier in the same script pass.
fn or_zero(v) {
    if v == () { 0 } else { v }
}

/// See enemy_rat.rhai's own update_awareness for the full design rationale
/// (sticky aggro, why hp < max forces awareness, why the raycast mask is
/// ["solid"], and why this lives in its own function instead of inlined in
/// on_turn — the same max-expression-complexity guard applies here too).
fn update_awareness(id, ctx, hp, ex, ey, px, py) {
    let aware = ctx.get_global("aware_" + id) == true;
    if aware { return true; }

    if hp < 15 { aware = true; }
    else {
        let hit = ctx.raycast(ex, ey, px, py, ["solid"]);
        if hit.is_empty() { aware = true; }
    }

    if aware { ctx.set_global("aware_" + id, true); }
    aware
}
