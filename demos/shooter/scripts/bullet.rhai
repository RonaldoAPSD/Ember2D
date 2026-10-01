// demos/shooter/scripts/bullet.rhai — a player bullet. It has no `on_update`
// at all: physics flies it (its velocity), and the only moment it needs a
// script is when it touches something — `on_collide`. Its collider tests
// only the "enemy" and "solid" layers (player.rhai sets the mask), so that
// means an enemy or a wall. A script with no `on_update` costs nothing per
// frame (since Step 9.5-5 the engine skips a lifecycle function a script
// doesn't define), which is what lets hundreds of bullets fly at once.

fn on_collide(id, other, ctx) {
    if ctx.is_tilemap(other) {
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), ".", "DarkGrey");
        ctx.despawn(id);
        return;
    }
    // A boss part passes the hit to the boss itself (its parent).
    let target = if ctx.has_tag(other, "boss_part") { ctx.get_parent(other) } else { other };
    if target < 0 || !ctx.has_var(target, "hp") { return; }
    ctx.despawn(id);
    hit(ctx, target);
}

/// One point of damage; at exactly zero the enemy dies. (`add_var`
/// returns the new value and is safe when several bullets hit the same
/// enemy in one pass — each sees its own result — so only the hit that
/// takes it to zero does the killing, once.)
fn hit(ctx, e) {
    let hp = ctx.add_var(e, "hp", -1);
    if hp != 0 { return; }
    ctx.add_global("score", ctx.get_var(e, "points"));
    ctx.add_global("kills", 1);
    let x = ctx.get_x(e);
    let y = ctx.get_y(e);
    ctx.emit_particles(x, y, "*", ctx.get_var(e, "colour"));
    if ctx.has_tag(e, "boss") {
        ctx.set_global("boss_dead", true);
        ctx.play_sound("audio/boss_explode.ogg");
        for k in 0..6 { ctx.emit_particles(x - 2.0 + k, y - 1.0 + k % 3, "*", "#ff8020"); }
    } else {
        ctx.play_sound("audio/explode.ogg");
    }
    drop_loot(ctx, e, x, y);
    ctx.despawn(e);
}

/// Brutes always leave a medkit; gunners sometimes leave a weapon; the
/// rest rarely leave a medkit.
fn drop_loot(ctx, e, x, y) {
    let kind = "";
    if ctx.has_tag(e, "brute") { kind = "medkit"; }
    else if ctx.has_tag(e, "gunner") && ctx.random_bool(0.3) { kind = ctx.random_choice(["spread", "shotgun", "rapid"]); }
    else if ctx.random_bool(0.03) { kind = "medkit"; }
    if kind == "" { return; }
    let look = if kind == "medkit" { ["+", "Green"] } else if kind == "spread" { ["S", "Yellow"] } else if kind == "shotgun" { ["G", "#ffc060"] } else { ["R", "Cyan"] };
    let p = ctx.spawn_entity(look[0], x, y, "pickup", look[1], "#202030", 6, false, 1.0, 1.0, "pickup");
    ctx.set_var(p, "kind", kind);
}
