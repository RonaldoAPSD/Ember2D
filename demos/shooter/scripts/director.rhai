// shooter/scripts/director.rhai — the whole game except the player: the
// ten-wave state machine, enemy spawning and steering, bullet hit resolution,
// scoring, and the wave banner.
//
// Attached to one invisible, collider-less tile at (0, 0) (see
// ember2d/examples/gen_shooter.rs's `director()`), so its `on_update` runs
// every simulation step for the life of the level.
//
// ── WHY ONE SCRIPT DRIVES EVERY ENEMY ────────────────────────────────────────
//
// Rhai can spawn entities (`ctx.spawn_entity`) but cannot attach a script to
// one — there is no `set_script` in the API, and `ScriptEngine::apply_ctx`'s
// spawn queue only builds a transform, sprite, tag and collider. A spawned
// enemy therefore has no `on_update` of its own and no `on_collide` of its
// own. Everything a wave shooter needs enemies to DO has to be driven from a
// script that already exists, iterating them by tag. That is this file.
//
// The second consequence is that bullets can't detect their own hits either.
// `Simulation::late_step` resolves solid collisions only for the local player,
// so a bullet is not stopped by a wall and never receives an `on_collide`;
// `update_bullets` below does that work explicitly with a rect query.
//
// ── THE ONE RULE THAT SHAPES EVERY WRITE HERE ────────────────────────────────
//
// A `get_global` never observes a `set_global` from the same script pass —
// writes queue and apply only once every script has run. So a running total
// must never be accumulated by read-modify-write more than once per pass: two
// enemies dying in one pass would both read the same pre-pass "score" and the
// second write would silently erase the first. Every such total in this file
// (score, kills, per-enemy HP) is accumulated in a LOCAL first and written
// exactly once. `resolve_hits` is the worked example.
//
// The wave-bonus write in `begin_next_wave` is safe against the same hazard
// for a structural reason worth stating, since it is not obvious: that
// function only ever runs during the "breather" phase, and a breather by
// definition has no enemies alive, so no kill — and therefore no other "score"
// write — can occur in the same pass.

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

    // Enemies and bullets are driven in every phase, not just "fighting": a
    // wave's last bullet should still be able to land after the wave ends, and
    // medkits dropped by the final kill should still be collectable.
    update_bullets(ctx, player);
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
        // step, so an enemy despawned by `update_bullets` a few lines above is
        // still counted here and this test only passes on the FOLLOWING step.
        // That one-step lag is load-bearing, not a defect: it is exactly what
        // guarantees `begin_next_wave`'s "score" write can never share a pass
        // with `resolve_hits`' one (see this file's header).
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
/// declares the run won. Runs only during a breather — see the header note on
/// why that makes the "score" write here safe.
fn begin_next_wave(ctx) {
    let cleared = ctx.get_global("wave");
    if cleared > 0 {
        ctx.set_global("score", ctx.get_global("score") + 25 * cleared);
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
/// HP lives in a `"ehp_<id>"` global because a spawned entity has no script
/// and therefore no per-entity state of its own — see the header.
fn spawn_enemy(ctx, tag, glyph, colour, hp, w, h) {
    let p = spawn_pos(ctx);
    let e = ctx.spawn_entity(glyph, p[0], p[1], tag, colour, "Reset", 8, false, w, h, "enemy");
    ctx.set_global("ehp_" + e, hp);
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

fn points_for(ctx, e) {
    if ctx.has_tag(e, "brute") { return 50; }
    if ctx.has_tag(e, "grunt") { return 10; }
    5
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

// ── Bullets ──────────────────────────────────────────────────────────────────

/// Advances every live bullet's collision state. Bullets move themselves (they
/// carry a velocity), so this only decides what they hit and when they die.
///
/// Bullets spawned during THIS step's `on_input` are not visible here — the
/// `WorldSnapshot` is built once at the top of the step, before `on_input`
/// runs. That is correct rather than a lag to work around: a bullet gets its
/// first hit test on the step after it exists, by which point physics has
/// moved it off the muzzle.
fn update_bullets(ctx, player) {
    let bullets = ctx.find_all_by_tag("bullet");
    if bullets.len() == 0 { return; }

    let spent = [];    // bullets to destroy
    let struck = [];   // one entry per enemy hit, so duplicates are countable

    for b in bullets {
        let bx = ctx.get_x(b);
        let by = ctx.get_y(b);

        // Safety net only — the unbroken wall ring should always catch a
        // bullet first. Kept so that opening the arena boundary later can't
        // leak bullets forever.
        if bx < min_x() || bx > max_x() || by < min_y() || by > max_y() {
            spent.push(b);
            continue;
        }

        // Matches the bullet's own 0.4x0.4 collider, so this query returns
        // exactly what an engine-side collision would have. It also returns
        // the bullet itself, and anything harmless it happens to be flying
        // over (a medkit), so both are skipped rather than treated as a hit.
        let hits = ctx.find_entities_in_rect(bx, by, 0.4, 0.4);
        for h in hits {
            if h == b || h == player { continue; }
            if ctx.has_tag(h, "wall") { spent.push(b); break; }
            if is_enemy(ctx, h) {
                struck.push(h);
                spent.push(b);
                break;
            }
        }
    }

    for b in spent {
        ctx.emit_particles(ctx.get_x(b), ctx.get_y(b), "*", "DarkGrey");
        ctx.despawn(b);
    }
    resolve_hits(ctx, struck);
}

/// Applies this pass's accumulated bullet damage, one write per enemy and one
/// write each for "score" and "kills".
///
/// The duplicate tally is the point. Two bullets striking the same enemy in
/// one pass is currently impossible — the fire cooldown puts consecutive shots
/// about five cells apart — but resolving each hit with its own
/// read-modify-write would silently drop all but one of them the moment that
/// stops being true (a spread weapon, a faster cadence). Counting first and
/// writing once costs a few lines and removes the trap entirely.
fn resolve_hits(ctx, struck) {
    if struck.len() == 0 { return; }

    let done = [];
    let gained = 0;
    let killed = 0;

    for e in struck {
        if done.contains(e) { continue; }
        done.push(e);

        let n = 0;
        for x in struck {
            if x == e { n += 1; }
        }

        let key = "ehp_" + e;
        let hp = ctx.get_global(key) - n;
        if hp > 0 {
            ctx.set_global(key, hp);
            continue;
        }

        // Dead: drop the HP entry too, or the globals map would grow by one
        // stale key per kill for the whole run.
        ctx.remove_global(key);
        gained += points_for(ctx, e);
        killed += 1;
        ctx.emit_particles(ctx.get_x(e), ctx.get_y(e), "x", "Red");
        maybe_drop(ctx, e);
        ctx.despawn(e);
    }

    if killed > 0 {
        ctx.set_global("score", ctx.get_global("score") + gained);
        ctx.set_global("kills", ctx.get_global("kills") + killed);
    }
}

/// Brutes always drop a medkit; anything else rarely does. Spawned non-solid
/// so it's a trigger — player.rhai's `on_collide` picks it up.
fn maybe_drop(ctx, e) {
    if !ctx.has_tag(e, "brute") && !ctx.random_bool(0.06) { return; }
    ctx.spawn_entity("+", ctx.get_x(e), ctx.get_y(e), "medkit", "Green", "Reset",
                     6, false, 1.0, 1.0, "pickup");
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
