// demos/shooter/scripts/director.rhai — the whole game except the player and
// bullets: the ten-wave state machine, enemy spawning and steering, contact
// damage, scoring, and the wave banner.
//
// Attached to one invisible, collider-less tile at (0, 0) (see
// ember2d/examples/gen_shooter.rs's `director()`), so its `on_update` runs
// every simulation step for the life of the level.
//
// ── WHY THIS SCRIPT STILL DRIVES EVERY ENEMY, EVEN THOUGH BULLETS DON'T
//    NEED IT ANYMORE ──────────────────────────────────────────────────────
//
// Before Step 7.5-5 (docs/ember2d-master-plan.md §5.6), Rhai could spawn
// entities (`ctx.spawn_entity`) but not attach a script to one — no
// `set_script` in the API, so a spawned enemy or bullet had no `on_update`/
// `on_collide` of its own, and everything either needed to DO had to be
// driven from a script that already existed, iterating them by tag. That's
// this file's own origin. `set_script` closed that gap; `bullet.rhai` (see
// `demos/shooter/scripts/player.rhai`'s `do_shoot`, which attaches it) is
// what a bullet's own hit detection and kill resolution moved into —
// `update_bullets`/`resolve_hits` used to live here and are gone.
//
// Enemies stay here BY CHOICE, not because `set_script` couldn't reach them
// too: `contact_damage` below is a "take the single largest hit touching the
// player this step, not the sum of all of them" rule, gated by one shared
// cooldown ("hurt_until"). That's a genuinely CENTRAL operation — spreading
// it across each enemy's own `on_update` would either silently stop
// stacking correctly (two enemies' `ctx.set_global("hp", ...)` in the same
// pass both read the same pre-pass "hp"; deferred writes mean neither sees
// the other's, so whichever runs last simply overwrites the first — not a
// sum, not even reliably the larger of the two) or need a per-enemy cooldown
// instead of one shared one, which would measurably raise damage taken when
// surrounded by multiple enemies — a real difficulty change, not a neutral
// refactor. `steer_enemies`/`steer_group` stay here too, mostly for the same
// "this would just be per-enemy busywork with no correctness upside" reason,
// though nothing stops a future step from moving pure per-enemy steering out
// if a reason to actually needs it shows up.
//
// ── THE ONE RULE THAT SHAPES EVERY RUNNING TOTAL HERE ─────────────────────────
//
// A `get_global` never observes a `set_global` from the same script pass —
// writes queue and apply only once every script has run. A running total
// accumulated by hand (`set_global(k, get_global(k) + d)`) breaks the moment
// two writes to the same key land in one pass: both read the same pre-pass
// value and the second write silently erases the first.
//
// `add_global`/`add_persistent`/`add_var` (docs/ember2d-master-plan.md §5.6,
// 7.5-2/7.5-3) exist precisely to remove that hazard: each call reads the
// CURRENT value — this pass's own already-queued write if there's one, the
// resolved store otherwise — and adds to it, so any number of calls to the
// same key in one pass land as that many additions. Every running total in
// this file (score, kills via `add_global`) uses one now — as does
// `bullet.rhai`'s own `resolve_hit` (Step 7.5-5), the file that used to hand-
// tally duplicate hits on one enemy here before a single read-modify-write
// specifically to dodge this hazard; it now just applies each hit as its own
// call, since a bullet resolving its own hit means there's no batch left to
// dedupe across. Per-enemy HP also used to live in a `"ehp_" + id` global —
// real per-entity `Vars` state (7.5-3) replaced that too; see
// `spawn_enemy`'s own comment.

// ── Tuning knobs ─────────────────────────────────────────────────────────────
// Functions, not top-level `const`: `Engine::call_fn` evaluates only the named
// function, so top-level statements in the AST never run and a `const` there
// would not reliably be in scope. Same reasoning as player.rhai.

fn total_waves()  { 10 }
fn intro_delay()  { 3.0 }   // seconds before wave 1
fn breather()     { 3.0 }   // seconds between waves

fn grunt_speed()   { 3.4 }  // cells/second
fn swarmer_speed() { 6.2 }
fn brute_speed()   { 1.9 }

fn grunt_hp()   { 2 }
fn swarmer_hp() { 1 }
fn brute_hp()   { 9 }

/// Arena interior, in cells — the wall ring occupies row/column 0 and
/// row 23 / column 79 (gen_shooter.rs `Arena::ring`).
fn min_x() { 1.0 }
fn max_x() { 78.0 }
fn min_y() { 1.0 }
fn max_y() { 22.0 }

/// Per-wave enemy counts. Index is `wave - 1`.
fn grunts_for(w)   { let t = [4, 6, 4, 6, 4, 8,  6, 10,  8, 10]; t[w - 1] }
fn swarmers_for(w) { let t = [0, 0, 6, 6, 4, 6, 10,  8, 12, 14]; t[w - 1] }
fn brutes_for(w)   { let t = [0, 0, 0, 0, 1, 1,  2,  2,  3,  4]; t[w - 1] }

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// `phase_until` is armed to -1.0 rather than to a real deadline because
/// `ScriptEngine::run_on_start_all` builds its `ScriptState` with `elapsed`
/// hardcoded to 0.0 — `ctx.get_elapsed()` inside `on_start` returns 0.0, not
/// the engine's actual clock. Computing `0.0 + intro_delay()` here would work
/// on a cold start and then break on every restart, because a restart happens
/// at whatever the real elapsed time has reached and the deadline would
/// already be in the past. The sentinel defers the arithmetic to the first
/// `on_update`, where `elapsed` is real.
fn on_start(id, ctx) {
    ctx.set_global("wave", 0);
    ctx.set_global("phase", "breather");
    ctx.set_global("phase_until", -1.0);
}

fn on_update(id, ctx) {
    let now = ctx.get_elapsed();
    let player = ctx.find_by_tag("player");

    // Arm the opening countdown on the first real step. See on_start.
    if ctx.get_global("phase_until") < 0.0 {
        ctx.set_global("phase_until", now + intro_delay());
        return;
    }

    // Enemies are driven in every phase, not just "fighting": medkits
    // dropped by the final kill should still be collectable, and a wave's
    // last live bullet (each now driven by its own bullet.rhai, Step
    // 7.5-5) should still be able to land after the wave ends regardless of
    // what this script does here.
    steer_enemies(ctx, player);
    contact_damage(ctx, player, now);

    let phase = ctx.get_global("phase");
    if phase == "won" {
        draw_centre(ctx, 11, "ARENA CLEARED", "Green");
        let s = "All " + total_waves();
        s += " waves survived - press R to play again";
        draw_centre(ctx, 12, s, "White");
    } else if phase == "breather" {
        let left = ctx.get_global("phase_until") - now;
        if left <= 0.0 { begin_next_wave(ctx); }
        else { draw_breather(ctx, left); }
    } else {
        draw_wave_status(ctx);
        // `count_by_tag` reads the `WorldSnapshot` built at the top of this
        // step, so an enemy a bullet's own on_update despawns this same step
        // (bullet.rhai's `resolve_hit`, Step 7.5-5) is still counted here —
        // this test only passes on the FOLLOWING step, a one-step lag, not a
        // defect: that kill needs this step to finish landing before the
        // wave can be declared cleared.
        if enemies_left(ctx) == 0 { end_wave(ctx, now); }
    }
}

// ── Wave state machine ───────────────────────────────────────────────────────

fn enemies_left(ctx) {
    ctx.count_by_tag("grunt") + ctx.count_by_tag("swarmer") + ctx.count_by_tag("brute")
}

fn end_wave(ctx, now) {
    ctx.set_global("phase", "breather");
    ctx.set_global("phase_until", now + breather());
}

/// Pays out the cleared wave's bonus, then either starts the next wave or
/// declares the run won.
fn begin_next_wave(ctx) {
    let cleared = ctx.get_global("wave");
    if cleared > 0 {
        ctx.add_global("score", 25 * cleared);
    }

    if cleared >= total_waves() {
        ctx.set_global("phase", "won");
        return;
    }

    let next = cleared + 1;
    ctx.set_global("wave", next);
    ctx.set_global("phase", "fighting");
    spawn_wave(ctx, next);
}

fn spawn_wave(ctx, wave) {
    for i in 0..grunts_for(wave) {
        spawn_enemy(ctx, "grunt", "g", "Red", grunt_hp(), 1.0, 1.0);
    }
    for i in 0..swarmers_for(wave) {
        spawn_enemy(ctx, "swarmer", "s", "Yellow", swarmer_hp(), 1.0, 1.0);
    }
    for i in 0..brutes_for(wave) {
        spawn_enemy(ctx, "brute", "B", "Magenta", brute_hp(), 2.0, 2.0);
    }
}

/// Spawns one enemy and records its HP.
///
/// Non-solid on purpose: a solid enemy would be pushed out of the player by
/// `late_step`'s resolver (the player is the one entity that resolver acts
/// on), turning every contact into a shove instead of a hit. As a trigger it
/// overlaps freely and `contact_damage` decides what that means.
///
/// HP lives in this enemy's own `Vars` (`ctx.set_var`, Step 7.5-3,
/// docs/ember2d-master-plan.md §5.6) — real per-entity component state that
/// doesn't need the entity to have a script of its own (unlike `globals`/
/// `persistent`, which live on `PlayState`, not `World`, and have no
/// concept of "this entity's own" data at all). This used to be a
/// `"ehp_" + id` global specifically because a spawned, unscripted enemy
/// had no OTHER way to own per-entity state; `Vars` closes that gap
/// directly, so `bullet.rhai`'s own `resolve_hit` (Step 7.5-5) no longer
/// needs to `remove_global` by hand on a kill either — `World::despawn`
/// clears the whole component.
fn spawn_enemy(ctx, tag, glyph, colour, hp, w, h) {
    let p = spawn_pos(ctx);
    let e = ctx.spawn_entity(glyph, p[0], p[1], tag, colour, "Reset", 8, false, w, h, "enemy");
    ctx.set_var(e, "hp", hp);
}

/// A random open cell at least 12 cells from the player, so a wave never
/// materialises on top of them. Falls back to the last candidate after 20
/// tries rather than looping forever — a crowded arena still has to spawn
/// something, and an enemy that starts slightly too close is a far better
/// failure than a hung frame.
fn spawn_pos(ctx) {
    let player = ctx.find_by_tag("player");
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);

    let x = 0.0;
    let y = 0.0;
    for tries in 0..20 {
        x = ctx.random_int(2, 76).to_float();
        y = ctx.random_int(2, 20).to_float();
        if ctx.is_solid_at(x, y) { continue; }
        let dx = x - px;
        let dy = y - py;
        if dx * dx + dy * dy < 144.0 { continue; }
        return [x, y];
    }
    [x, y]
}

// ── Enemies ──────────────────────────────────────────────────────────────────

fn steer_enemies(ctx, player) {
    if player == -1 { return; }
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    steer_group(ctx, "grunt", grunt_speed(), px, py);
    steer_group(ctx, "swarmer", swarmer_speed(), px, py);
    steer_group(ctx, "brute", brute_speed(), px, py);
}

/// Points every enemy of one type at the player and writes its velocity;
/// `World::integrate_physics` does the actual moving.
///
/// Re-steered every step rather than once at spawn, which is what makes them
/// track a moving target. It is also cheap: this is a velocity write, not a
/// path search.
fn steer_group(ctx, tag, speed, px, py) {
    let list = ctx.find_all_by_tag(tag);
    for e in list {
        let ex = ctx.get_x(e);
        let ey = ctx.get_y(e);
        let dx = px - ex;
        let dy = py - ey;
        let d2 = dx * dx + dy * dy;
        if d2 < 0.01 {
            ctx.set_velocity(e, 0.0, 0.0);
            continue;
        }
        let inv = 1.0 / d2.sqrt();
        dx *= inv;
        dy *= inv;

        // Wall slide. The engine resolves solid overlap for the player only,
        // so without this an enemy walks straight through a cover block.
        // Testing the adjacent cell on each axis and dropping whichever one is
        // blocked leaves the other component intact, so the enemy slides along
        // the face instead of stopping dead against it.
        //
        // Deliberately a one-cell point probe, not a swept box: it under-tests
        // the 2x2 brute, which can clip a block corner. Visible only if you go
        // looking, and a real fix belongs in the engine's collision resolver
        // rather than in nine lines of Rhai.
        if dx > 0.0 && ctx.is_solid_at(ex + 1.0, ey) { dx = 0.0; }
        else if dx < 0.0 && ctx.is_solid_at(ex - 1.0, ey) { dx = 0.0; }
        if dy > 0.0 && ctx.is_solid_at(ex, ey + 1.0) { dy = 0.0; }
        else if dy < 0.0 && ctx.is_solid_at(ex, ey - 1.0) { dy = 0.0; }

        ctx.set_velocity(e, dx * speed, dy * speed);
    }
}

fn is_enemy(ctx, e) {
    ctx.has_tag(e, "grunt") || ctx.has_tag(e, "swarmer") || ctx.has_tag(e, "brute")
}

fn contact_dmg_for(ctx, e) {
    if ctx.has_tag(e, "brute") { return 3; }
    if is_enemy(ctx, e) { return 1; }
    0
}

/// One contact hit at a time, gated by an invulnerability deadline — without
/// it, standing in a swarmer would deal damage on all 60 steps of a second.
///
/// Takes the single largest damage value among everything touching the player
/// rather than summing, so being surrounded is punishing but not instantly
/// lethal, and so this stays a single "hp" write per pass.
fn contact_damage(ctx, player, now) {
    if player == -1 { return; }
    if !ctx.has_global("hp") { return; }
    let hp = ctx.get_global("hp");
    if hp <= 0 { return; }
    if now < ctx.get_global("hurt_until") { return; }

    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    // The player's collider is 0.7x0.7 anchored at (px, py); widened half a
    // cell each way so a hit registers as the glyphs meet rather than after
    // they visibly overlap.
    let near = ctx.find_entities_in_rect(px - 0.3, py - 0.3, 1.3, 1.3);

    let dmg = 0;
    for e in near {
        let d = contact_dmg_for(ctx, e);
        if d > dmg { dmg = d; }
    }
    if dmg == 0 { return; }

    ctx.set_global("hp", hp - dmg);
    ctx.set_global("hurt_until", now + 0.7);
    ctx.shake_camera(2.0, 0.15);
    ctx.emit_particles(px, py, "!", "Red");
}

// ── HUD ──────────────────────────────────────────────────────────────────────

fn draw_centre(ctx, y, text, fg) {
    let x = (ctx.get_viewport_width() - text.len()) / 2;
    ctx.draw_hud(x, y, text, fg, "Reset");
}

fn draw_breather(ctx, left) {
    let next = ctx.get_global("wave") + 1;
    // +1.0 then truncate, so the countdown reads 3, 2, 1 rather than 2, 1, 0.
    let secs = (left + 1.0).to_int();
    let s = "WAVE " + next;
    s += " OF " + total_waves();
    s += "  IN  " + secs;
    draw_centre(ctx, 11, s, "Cyan");
}

fn draw_wave_status(ctx) {
    let s = "WAVE " + ctx.get_global("wave");
    s += "/" + total_waves();
    s += "   LEFT " + enemies_left(ctx);
    ctx.draw_hud(58, 0, s, "Cyan", "Reset");
}
