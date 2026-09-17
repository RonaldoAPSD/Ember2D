// demos/shooter/scripts/bullet.rhai — a fired bullet's own hit detection and
// kill resolution (Step 7.5-5, docs/ember2d-master-plan.md §5.6).
//
// Attached by `demos/shooter/scripts/player.rhai`'s `do_shoot`, via
// `ctx.set_script`, to every bullet it fires — previously impossible (no
// `set_script` in the API), which is why director.rhai used to scan every
// "bullet"-tagged entity by hand each step (`update_bullets`/`resolve_hits`,
// now gone from that file). This file replaces both: hit detection and kill
// resolution collapse into one function since there's no longer a batch of
// bullets to dedupe hits across — this bullet's own on_update runs once,
// finds at most one hit, and resolves it completely right there.
//
// THE FIRST-CALL STALENESS GUARD BELOW ISN'T OPTIONAL. `ctx.set_script`
// attaches immediately, but this entity's `on_update` still runs against
// whichever `WorldSnapshot` this call's `run_scripts` was handed — and when
// `do_shoot` spawns-and-attaches a bullet from `on_input`, that snapshot was
// built at the TOP of this same step, before the spawn landed. `ctx.get_x`/
// `get_y` read that frozen snapshot, not live `World`, and default to 0.0
// for an id they don't recognize — so this bullet's very first on_update
// would see itself at (0.0, 0.0), outside the arena, and self-destruct on
// the spot without ever having moved. `armed` (a `Vars` flag, Step 7.5-3)
// skips exactly one call so real hit detection only starts once a FRESH
// snapshot (built at the top of the NEXT step, by which point this bullet's
// spawn is old news) exists — matching the pre-7.5-5 director.rhai's own
// comment on this exact timing: "a bullet gets its first hit test on the
// step after it exists... by which point physics has moved it off the
// muzzle."
//
// ── Arena bounds (mirrors director.rhai's own copy) ─────────────────────────
// Duplicated, not shared: this engine has no cross-script `use`/import today
// (7.5-13 is the open question on whether Rhai modules are worth enabling
// for that) — every script in this demo already owns its own tuning
// constants as plain functions rather than reaching for another file's.

fn min_x() { 1.0 }
fn max_x() { 78.0 }
fn min_y() { 1.0 }
fn max_y() { 22.0 }

fn is_enemy(ctx, e) {
    ctx.has_tag(e, "grunt") || ctx.has_tag(e, "swarmer") || ctx.has_tag(e, "brute")
}

fn points_for(ctx, e) {
    if ctx.has_tag(e, "brute") { return 50; }
    if ctx.has_tag(e, "grunt") { return 10; }
    5
}

fn on_update(id, ctx) {
    if !ctx.has_var(id, "armed") {
        ctx.set_var(id, "armed", true);
        return;
    }

    let bx = ctx.get_x(id);
    let by = ctx.get_y(id);

    // Safety net only — the unbroken wall ring should always catch a bullet
    // first. Kept so opening the arena boundary later can't leak bullets
    // forever.
    if bx < min_x() || bx > max_x() || by < min_y() || by > max_y() {
        ctx.emit_particles(bx, by, "*", "DarkGrey");
        ctx.despawn(id);
        return;
    }

    let player = ctx.find_by_tag("player");
    // Matches this bullet's own 0.4x0.4 collider, so this query returns
    // exactly what an engine-side collision would have. It also returns the
    // bullet itself and anything harmless it happens to be flying over (a
    // medkit), so both are skipped rather than treated as a hit.
    let hits = ctx.find_entities_in_rect(bx, by, 0.4, 0.4);
    for h in hits {
        if h == id || h == player { continue; }
        if ctx.has_tag(h, "wall") {
            ctx.emit_particles(bx, by, "*", "DarkGrey");
            ctx.despawn(id);
            return;
        }
        if is_enemy(ctx, h) {
            ctx.emit_particles(bx, by, "*", "DarkGrey");
            ctx.despawn(id);
            resolve_hit(ctx, h);
            return;
        }
    }
}

/// Applies this bullet's one point of damage to `e`; awards the kill and
/// drops loot if it lands. `add_var`, not a read-modify-write — accumulates
/// correctly even in the (currently impossible, since the fire cooldown
/// spaces consecutive shots well apart) case of two bullets hitting the same
/// enemy in one pass, same reasoning `demos/roguelike/scripts/enemy.rhai`'s
/// own damage writes give.
fn resolve_hit(ctx, e) {
    let hp = ctx.add_var(e, "hp", -1);
    if hp > 0 { return; }
    ctx.add_global("score", points_for(ctx, e));
    ctx.add_global("kills", 1);
    ctx.emit_particles(ctx.get_x(e), ctx.get_y(e), "x", "Red");
    maybe_drop(ctx, e);
    ctx.despawn(e);
}

/// Brutes always drop a medkit; anything else rarely does. Spawned non-solid
/// so it's a trigger — player.rhai's `on_collide` picks it up.
fn maybe_drop(ctx, e) {
    if !ctx.has_tag(e, "brute") && !ctx.random_bool(0.06) { return; }
    ctx.spawn_entity("+", ctx.get_x(e), ctx.get_y(e), "medkit", "Green", "Reset",
                     6, false, 1.0, 1.0, "pickup");
}
