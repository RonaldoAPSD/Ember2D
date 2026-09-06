// shooter/scripts/player.rhai — twin-stick player controller for the
// realtime arena demo: WASD moves, the mouse aims and left-click shoots, and
// this script owns the player's HP, the stats bar, and the death/restart
// screen.
//
// Companion to director.rhai, which owns everything else (waves, enemies,
// bullet hit resolution). The split is not stylistic: Rhai can spawn entities
// but cannot attach a script to one, so a spawned bullet or enemy has no
// behavior of its own and something permanently present has to drive them.
// See ember2d/examples/gen_shooter.rs's header for the full reasoning.
//
// ── WHY THIS SCRIPT LOOKS DIFFERENT FROM roguelike/scripts/player.rhai ───────
//
// STATE LIVES IN GLOBALS, NOT PERSISTENT. Globals are level-scoped and reset
// on every level load. This demo is a single level whose restart IS a reload
// of that same level, so "reset on load" is exactly the desired behavior and
// `ctx.load_level` alone is a complete restart — no `clear_persistent` book-
// keeping. The roguelike needed `persistent` only because a run spans four
// level files.
//
// INIT HAPPENS IN on_start, NOT LAZILY IN on_update. The roguelike deliberately
// avoids `on_start` because its save/load path (`Simulation::is_loading_save`)
// never runs `on_start` scripts, so a loaded save would start uninitialized.
// This demo has no save/load, so `on_start` is available and is the right
// place: it runs once, before any step, which means on_input can read "hp" on
// step 1 without racing its own initializer. (That race is real — a deferred
// `set_global` is not visible to a `get_global` in the same pass — and it is
// what forces the roguelike's `or_zero()` guards everywhere.)
//
// NOTHING IS SUBMITTED AND ctx.act IS NEVER CALLED. `submit`/`on_turn`/`act`
// are the turn-based command boundary; in realtime the player just writes
// velocity directly from on_input and lets `World::integrate_physics` carry
// it. Not calling `act` also keeps this actor at the front of `TurnScheduler`
// forever, which is precisely what makes `on_input` run every single step —
// see gen_shooter.rs fact (3).
//
// WALLS ARE FREE, EVERYTHING ELSE IS NOT. `Simulation::late_step` resolves
// solid collisions only for the local player, so the arena walls stop this
// entity with no script involvement at all. The same is not true of bullets
// or enemies; director.rhai handles those by hand.

// ── Tuning knobs ─────────────────────────────────────────────────────────────
// Plain functions rather than top-level `const`: `ScriptEngine` invokes these
// files through `Engine::call_fn`, which evaluates the named function only —
// top-level statements in the AST are never re-run, so a top-level `const`
// would not reliably be in scope inside a call.

fn hp_max()        { 10 }
fn move_speed()    { 11.0 }   // cells/second
fn fire_cooldown() { 0.16 }   // seconds between shots
fn bullet_speed()  { 30.0 }   // cells/second
fn medkit_heal()   { 3 }

/// Below this the mouse is considered "on top of the player" and aim direction
/// is left as whatever it last was, rather than normalising a near-zero vector
/// (which would amplify floating-point noise into a direction that visibly
/// jitters at the muzzle).
fn min_aim_dist_sq() { 0.01 }

/// Diagonal input normaliser (1/sqrt(2)), for movement only now — aim is a
/// continuous direction toward the mouse, not a snapped 8-way key
/// combination, so it needs no such correction.
fn diag() { 0.70710678 }

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Runs once when the level loads, before any simulation step — see the header
/// note on why this demo can use `on_start` where the roguelike can't.
///
/// "score"/"kills" are written here but owned by director.rhai; they're
/// initialised in one place so a restart can't leave a stale value behind for
/// whichever script happens to read first.
fn on_start(id, ctx) {
    ctx.set_global("hp", hp_max());
    ctx.set_global("score", 0);
    ctx.set_global("kills", 0);
    ctx.set_global("fire_ready", 0.0);
    ctx.set_global("hurt_until", 0.0);
}

/// The only place this script reads raw key state. In realtime that's every
/// step, for as long as this actor stays at the scheduler's front.
fn on_input(id, ctx) {
    if is_dead(ctx) {
        // Stop dead rather than coasting: velocity persists in `Transform`
        // until something overwrites it, so skipping this would leave the
        // corpse drifting into a wall.
        ctx.set_velocity(id, 0.0, 0.0);
        if ctx.just_pressed("r") { ctx.load_level("shooter/arena.level"); }
        return;
    }
    do_move(id, ctx);
    do_shoot(id, ctx);
}

/// Presentation only — the stats bar and, if it applies, the death screen.
/// Gameplay state is mutated in on_input (this script) and on_update
/// (director.rhai), never here.
fn on_update(id, ctx) {
    draw_stats(ctx);
    if is_dead(ctx) { draw_death_screen(ctx); }
}

/// Medkits are the one pickup, and this is the only `on_collide` in the demo.
///
/// It works precisely because the PLAYER is scripted: `ScriptEngine::run_collisions`
/// dispatches `on_collide` to whichever side of a colliding pair has a script,
/// and a director-spawned medkit has none. A medkit is spawned non-solid, so
/// it's a trigger — overlapping it never pushes the player.
fn on_collide(id, other, ctx) {
    if !ctx.has_tag(other, "medkit") { return; }

    // Reads the post-damage value: director.rhai's contact damage lands during
    // the on_update pass, which is fully applied before the collision pass
    // runs. Two medkits touched in the SAME pass would heal only once (both
    // reads see the same pre-pass "hp", last write wins) — harmless here given
    // the cap below, and not worth a per-pass accumulator for a 1x1 pickup.
    let healed = ctx.get_global("hp") + medkit_heal();
    if healed > hp_max() { healed = hp_max(); }
    ctx.set_global("hp", healed);
    ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Green");
    ctx.despawn(other);
}

// ── Movement and shooting ────────────────────────────────────────────────────

/// Eight-directional movement from WASD. Writes velocity and nothing else —
/// `World::integrate_physics` does the moving, and the engine's own solid
/// resolution does the wall stopping.
fn do_move(id, ctx) {
    let vx = 0.0;
    let vy = 0.0;
    if ctx.is_held("a") { vx -= 1.0; }
    if ctx.is_held("d") { vx += 1.0; }
    if ctx.is_held("w") { vy -= 1.0; }
    if ctx.is_held("s") { vy += 1.0; }
    if vx != 0.0 && vy != 0.0 { vx *= diag(); vy *= diag(); }
    ctx.set_velocity(id, vx * move_speed(), vy * move_speed());
}

/// Aims at the mouse's world position and fires left-click, on a cooldown.
///
/// `mouse_left_held`, not `mouse_left_pressed`: a shooter wants to hold the
/// button down and keep firing, and the cooldown (not the click rate) is what
/// sets the actual rate of fire.
///
/// `get_mouse_world_x/y` add the mouse's screen cell to the camera's own
/// position (see that function's doc comment in
/// docs/ember2d-scripting-api.md) — since this arena is exactly
/// viewport-sized, the camera never scrolls, so that's the one case where the
/// "not replay-safe" warning attached to camera-derived reads doesn't apply:
/// a value that never leaves a single fixed point can't diverge between runs.
/// A scrolling level using this same pattern would need to reason about that.
///
/// The cooldown is kept as an absolute deadline in "fire_ready" rather than a
/// counter decremented each step, because a decrement would have to read its
/// own previous value — and a `get_global` never observes a `set_global` from
/// the same pass, so a value written and re-read across passes is fine but a
/// running total accumulated within one is not. Deadlines sidestep the whole
/// question.
fn do_shoot(id, ctx) {
    if !ctx.mouse_left_held() { return; }
    let now = ctx.get_elapsed();
    if now < ctx.get_global("fire_ready") { return; }

    let px = ctx.get_x(id);
    let py = ctx.get_y(id);
    let dx = ctx.get_mouse_world_x() - px;
    let dy = ctx.get_mouse_world_y() - py;
    let d2 = dx * dx + dy * dy;
    // The cursor sitting exactly on the player has no meaningful direction —
    // skip the shot entirely rather than fire toward (0, 0)/normalise noise
    // into a jittering muzzle.
    if d2 < min_aim_dist_sq() { return; }
    let inv = 1.0 / d2.sqrt();
    dx *= inv;
    dy *= inv;

    // Spawned 0.8 cells out so the muzzle clears the player's own 0.7 collider
    // — otherwise director.rhai's hit query would find the player on the
    // bullet's very first step.
    //
    // The extended `spawn_entity` overload (defect D10's fix) sets colour,
    // z-order, solidity and size at spawn time; the four-argument version
    // would hardcode white/z=2/1x1/non-solid and need three follow-up setters.
    // Non-solid matters: a solid bullet would be shoved out of walls by the
    // physics resolver instead of being destroyed by director.rhai.
    let b = ctx.spawn_entity("*", px + dx * 0.8, py + dy * 0.8,
                             "bullet", "White", "Reset", 12, false, 0.4, 0.4, "bullet");
    ctx.set_velocity(b, dx * bullet_speed(), dy * bullet_speed());
    ctx.set_global("fire_ready", now + fire_cooldown());
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// `has_global` first, so a step that somehow ran before `on_start` would read
/// as alive rather than as instantly dead. Cheap insurance against the single
/// worst failure mode this script has.
fn is_dead(ctx) {
    ctx.has_global("hp") && ctx.get_global("hp") <= 0
}

// ── HUD ──────────────────────────────────────────────────────────────────────

/// The stats bar on row 0. Drawn over the top wall row, which is why the arena
/// can be exactly viewport-sized without losing playfield to chrome.
fn draw_stats(ctx) {
    let hp = ctx.get_global("hp");
    if hp < 0 { hp = 0; }

    // Built with `+=` rather than one long `a + b + c + ...` chain: Rhai parses
    // the latter as a single deeply-nested expression tree and trips its
    // max-complexity guard at compile time. The roguelike's HUD hit exactly
    // this and the fix is the same.
    let bar = "";
    for i in 0..hp_max() {
        if i < hp { bar += "|"; } else { bar += "."; }
    }

    let colour = "Green";
    if hp <= 3 { colour = "Red"; }
    else if hp <= 6 { colour = "Yellow"; }

    ctx.draw_hud(2, 0, "HP ", "Grey", "Reset");
    ctx.draw_hud(5, 0, bar, colour, "Reset");

    let s = "SCORE " + ctx.get_global("score");
    s += "   KILLS " + ctx.get_global("kills");
    ctx.draw_hud(20, 0, s, "White", "Reset");

    let vh = ctx.get_viewport_height();
    ctx.draw_hud(2, vh - 1, "WASD move   Mouse aim   Click shoot   R restart   Esc pause", "DarkGrey", "Reset");
}

fn draw_death_screen(ctx) {
    ctx.fill_rect(25, 8, 30, 7, " ", "White", "DarkRed");
    ctx.draw_box(25, 8, 30, 7, "White", "DarkRed");
    ctx.draw_hud(35, 10, "YOU DIED", "Yellow", "DarkRed");
    let s = "Score " + ctx.get_global("score");
    s += "  Kills " + ctx.get_global("kills");
    ctx.draw_hud(30, 11, s, "White", "DarkRed");
    ctx.draw_hud(30, 13, "Press R to restart", "White", "DarkRed");
}
