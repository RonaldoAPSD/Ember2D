// roguelike/scripts/pickup.rhai — gold and potion tiles handle their own
// pickup. Shared by every pickup tile across every floor; which resource a
// given tile grants comes from its own authored `tag` ("gold" or
// "potion" — see examples/gen_roguelike.rs), never from a tag string in
// engine code (Phase 4, docs/ember2d-refactor-plan.md).
//
// Read-modify-write on `ctx.get_persistent("gold"/"potions")` is safe here
// only because the player moves one cell per turn and so can touch at most
// one item per turn — two items picked up in the same pass would otherwise
// risk the same lost-update hazard the player script's damage model works
// around.
//
// or_zero guards against reading "gold"/"potions" before player.rhai has
// ever lazy-inited them — should not happen in practice (player.rhai's
// on_update always runs earlier in the same engine frame than any
// on_collide, so its lazy-init is already visible by the time this fires),
// but costs nothing and removes the dependency on that ordering holding.

fn on_collide(id, other, ctx) {
    if !ctx.has_tag(other, "player") { return; }

    let kind = ctx.get_tag(id);
    if kind == "gold" { ctx.set_persistent("gold", or_zero(ctx.get_persistent("gold")) + 1); }
    else if kind == "potion" { ctx.set_persistent("potions", or_zero(ctx.get_persistent("potions")) + 1); }

    ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Yellow");
    // roguelike/audio (hurt.ogg, music.ogg, victory.ogg) has nothing that
    // fits a pickup chime — left silent rather than reusing a
    // thematically-wrong clip.
    ctx.despawn(id);
}

/// See player.rhai's header comment for why this guard exists: a get_*
/// never observes a set_* from earlier in the same script pass.
fn or_zero(v) {
    if v == () { 0 } else { v }
}
