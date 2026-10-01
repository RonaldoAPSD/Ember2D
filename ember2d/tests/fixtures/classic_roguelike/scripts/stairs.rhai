// demos/roguelike/scripts/stairs.rhai — shared by every floor's stairs-down tile.
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
        // add_persistent (docs/ember2d-master-plan.md §5.6, 7.5-2) reads
        // "depth"'s current value — falling back to 0 itself if the key has
        // never been set — and adds 1, so this no longer needs its own
        // uninitialized-key guard the way a hand-rolled
        // `set_persistent("depth", get_persistent("depth") + 1)` would.
        ctx.add_persistent("depth", 1);
        // demos/roguelike/audio has nothing that fits descending stairs — left
        // silent rather than reusing a thematically-wrong clip.
    }
}
