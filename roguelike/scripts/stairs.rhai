// roguelike/scripts/stairs.rhai — shared by every floor's stairs-down tile.
//
// The engine still owns the actual level transition (`TileRecord.next_level`
// + `Collider.locked`, resolved in `play.rs::late_update`); this script only
// decides whether the stairs are currently locked (boss floors only — the
// count is always 0 on floors with no boss, so this is a harmless no-op
// there) and gives them a visual tint to match.

fn on_update(id, ctx) {
    let boss_alive = ctx.count_by_tag("boss") > 0;
    ctx.set_collider_locked(id, boss_alive);
    ctx.set_tint(id, if boss_alive { "DarkGrey" } else { "Yellow" }, "Reset");
}

fn on_collide(id, other, ctx) {
    if ctx.has_tag(other, "player") && !ctx.is_collider_locked(id) {
        // or_zero guards a get_* racing its own same-frame lazy-init (see
        // player.rhai's header comment) — "depth" is always visible by now
        // in practice, since player.rhai's on_update runs earlier in the
        // same engine frame, but the guard costs nothing.
        let depth = ctx.get_persistent("depth");
        if depth == () { depth = 0; }
        ctx.set_persistent("depth", depth + 1);
        // roguelike/audio has nothing that fits descending stairs — left
        // silent rather than reusing a thematically-wrong clip.
    }
}
