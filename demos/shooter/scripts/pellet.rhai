// demos/shooter/scripts/pellet.rhai — a shotgun pellet: a bullet that also
// burns out after a moment (its "until" var, set when fired), so the
// shotgun only works up close. Same collision rules as bullet.rhai — kept
// in step by hand (Rhai has no modules here, master plan decision 7.4).

fn on_update(id, ctx) {
    if ctx.get_elapsed() > ctx.get_var(id, "until") { ctx.despawn(id); }
}

fn on_collide(id, other, ctx) {
    if ctx.is_tilemap(other) { ctx.despawn(id); return; }
    let target = if ctx.has_tag(other, "boss_part") { ctx.get_parent(other) } else { other };
    if target < 0 || !ctx.has_var(target, "hp") { return; }
    ctx.despawn(id);
    let hp = ctx.add_var(target, "hp", -1);
    if hp != 0 { return; }
    ctx.add_global("score", ctx.get_var(target, "points"));
    ctx.add_global("kills", 1);
    ctx.emit_particles(ctx.get_x(target), ctx.get_y(target), "*", ctx.get_var(target, "colour"));
    if ctx.has_tag(target, "boss") { ctx.set_global("boss_dead", true); }
    ctx.play_sound("audio/explode.ogg");
    if ctx.has_tag(target, "brute") {
        let p = ctx.spawn_entity("+", ctx.get_x(target), ctx.get_y(target), "pickup", "Green", "#202030", 6, false, 1.0, 1.0, "pickup");
        ctx.set_var(p, "kind", "medkit");
    }
    ctx.despawn(target);
}
