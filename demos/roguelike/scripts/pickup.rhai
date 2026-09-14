// demos/roguelike/scripts/pickup.rhai — gold and potion tiles handle their own
// pickup. Shared by every pickup tile across every floor; which resource a
// given tile grants comes from its own authored `tag` ("gold" or
// "potion" — see examples/gen_roguelike.rs), never from a tag string in
// engine code (Phase 4, docs/ember2d-refactor-plan.md).
//
// `add_persistent` (docs/ember2d-master-plan.md §5.6, 7.5-2) makes the
// write below correct regardless of how many items get touched in one
// pass — it reads the current value (this pass's own pending write if
// there is one, else the resolved store) and adds the delta, so two
// pickups colliding in the same pass would both land instead of the
// second one silently overwriting the first the way a hand-rolled
// `set_persistent(k, get_persistent(k) + 1)` would. In practice the player
// still moves one cell per turn and can only ever touch one item per
// pass, so that safety margin isn't load-bearing today — but it costs
// nothing and removes the dependency on that ordering holding, the same
// reasoning this file used to give for its own `or_zero()` guard (gone as
// of 7.5-2: `add_persistent` also makes an uninitialized key read as 0
// without a script-level guard).

fn on_collide(id, other, ctx) {
    if !ctx.has_tag(other, "player") { return; }

    let kind = ctx.get_tag(id);
    if kind == "gold" { ctx.add_persistent("gold", 1); }
    else if kind == "potion" { ctx.add_persistent("potions", 1); }

    ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Yellow");
    // demos/roguelike/audio (hurt.ogg, music.ogg, victory.ogg) has nothing that
    // fits a pickup chime — left silent rather than reusing a
    // thematically-wrong clip.
    ctx.despawn(id);
}
