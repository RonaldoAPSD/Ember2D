// demos/shooter/scripts/ebullet.rhai — an enemy shot (a gunner's, or one of
// the boss's rings). Like the player's bullets it only has `on_collide`;
// its collider tests "player" and "solid" only.
//
// Damage uses `add_global` — several shots landing in the same pass each
// subtract, where `set_global(get_global(...) - 1)` would lose all but one
// (writes are deferred). A short invulnerability after a hit ("hurt_until",
// shared with contact damage in director.rhai) keeps a ring of shots from
// melting the player in one frame.

fn on_collide(id, other, ctx) {
    if ctx.is_tilemap(other) { ctx.despawn(id); return; }
    if !ctx.has_tag(other, "player") { return; }
    ctx.despawn(id);
    let now = ctx.get_elapsed();
    if now < ctx.get_global("hurt_until") || ctx.get_global("hp") <= 0 { return; }
    let _hp = ctx.add_global("hp", -1);
    ctx.set_global("hurt_until", now + 0.35);
    ctx.emit_particles(ctx.get_x(other), ctx.get_y(other), "!", "Red");
    ctx.play_sound("audio/hurt.ogg");
    if ctx.get_global("shake") { ctx.shake_camera(1.5, 0.12); }
}
