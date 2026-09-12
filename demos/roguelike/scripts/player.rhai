// roguelike/scripts/player.rhai — player controller: turn-gated grid
// movement, bump-to-attack, quaffing, and the HUD.
//
// Phase 4 (docs/ember2d-refactor-plan.md) moves all of this out of Rust —
// see the Phase 4 plan for the staged steps this script is part of.
//
// STATE OWNERSHIP:
//   Rhai's per-entity `let` does NOT persist between on_update/on_turn/
//   on_input calls — ScriptEngine calls Engine::call_fn, whose default
//   CallFnOptions rewinds the scope after every call. So:
//     - GLOBALS (ctx.set_global/get_global) are level-scoped: they reset to
//       empty on every level load.
//     - PERSISTENT (ctx.set_persistent/get_persistent) survives level
//       transitions — this run's stats live here.
//
// PERSISTENT this run's stats (survive next_level; reset on death/restart):
//   "hp", "hp_max", "gold", "potions", "depth", "turns_taken"
//
// STEP 5e (docs/ember2d-phase5-plan.md) SPLIT INPUT FROM ACTION: on_input
// runs first each step and is the only place this script reads raw key
// state (ctx.is_held/just_pressed) — it just translates keys into a
// ctx.submit()'d Command ("move" with [dx, dy], "wait", "quaff", or
// "restart"), never touching persistent/global state itself.
//
// on_input submits "restart" purely from whether "r" is down — it has no
// idea whether the player is alive. on_turn (below) is what decides
// "restart" means anything, and only inside its hp<=0 branch. A command
// on_turn doesn't recognize for the player's current state (a stray
// "restart" while alive, or an empty "" when nothing was pressed) falls
// through to the same "no turn consumed" outcome as no input at all — see
// the `else if action != "wait"` guard near the bottom of on_turn.
//
// STEP 5f (docs/ember2d-phase5-plan.md) SPLIT ACTION FROM HOUSEKEEPING:
// on_turn now does move/attack/quaff/restart — called exactly once, only
// when TurnScheduler hands this actor the turn, so there's no more need for
// the old "turn"/"acted_<id>" bookkeeping enemy scripts used to gate
// themselves to once per player turn. on_update keeps only what must run
// every single step regardless of whose turn it is: lazy init, the HUD, and
// the death screen.
//
// DAMAGE MODEL CHANGED WITH IT: an enemy's on_turn now writes this script's
// own "hp" persistent value DIRECTLY (see enemy_rat.rhai/enemy_boss.rhai),
// instead of publishing an atk_turn_<id>/atk_dmg_<id> pair for this script
// to sum up later. That indirection existed only because the old model ran
// every entity's on_update in the SAME pass, where two attackers writing
// "hp" in the same pass would silently cost only one hit (a get_* never
// observes a set_* from earlier in the same pass) — under the scheduler,
// each actor's on_turn is its own separate pass, fully committed before the
// next one starts, so a direct write from whichever enemy just acted is
// already visible by the time the next actor (enemy or player) reads "hp".
// This is also what deleted the "last_resolved_turn" scanning this file
// used to do every single frame.
//
// POTIONS WERE COLLECTIBLE BUT UNUSABLE UNTIL PHASE 4 — found by the user
// actually playing through to floor 3 at 2 hp with potions sitting unused
// in inventory. "Q" quaffs one (if any) for +6 hp (capped at hp_max),
// costing a turn like waiting does.
//
// THE SAME-PASS LAZY-INIT HAZARD (fixed, keep the fix — this WILL recur in
// every future script with a lazy-init block): `ctx.set_persistent("hp",
// 12)` doesn't make `ctx.get_persistent("hp")` return 12 later in this SAME
// call — it still returns whatever it was before until apply_ctx folds the
// write in at the end of the pass. `or_zero()` below is the general guard
// for reads that might race their own lazy-init this way; `hp` in
// on_update gets a stronger fix (computed directly during init, never
// defaulted to 0) because a wrong default there isn't just cosmetic — 0
// reads as "already dead".

/// The only place this script reads raw input (see the Step 5e header note
/// above). Priority mirrors on_turn's if/else-if chain exactly: restart
/// beats movement beats waiting beats quaffing, and only the first matching
/// key each step produces a command.
fn on_input(id, ctx) {
    if ctx.just_pressed("r") {
        ctx.submit(id, "restart", []);
        return;
    }

    let dx = 0.0;
    let dy = 0.0;
    if ctx.just_pressed("w") || ctx.just_pressed("up") { dy = -1.0; }
    else if ctx.just_pressed("s") || ctx.just_pressed("down") { dy = 1.0; }
    else if ctx.just_pressed("a") || ctx.just_pressed("left") { dx = -1.0; }
    else if ctx.just_pressed("d") || ctx.just_pressed("right") { dx = 1.0; }
    if dx != 0.0 || dy != 0.0 {
        ctx.submit(id, "move", [dx, dy]);
        return;
    }

    if ctx.just_pressed("space") {
        ctx.submit(id, "wait", []);
        return;
    }

    if ctx.just_pressed("q") {
        ctx.submit(id, "quaff", []);
        return;
    }
}

/// Runs every step regardless of whose turn it is (Step 5f) — lazy init,
/// the HUD, and the death screen. Never mutates gameplay state (that's
/// on_turn's job) except the once-ever lazy-init below.
fn on_update(id, ctx) {
    // ---- lazy run init -------------------------------------------------
    // Never in on_start: PlayState's is_loading_save path never calls
    // on_start scripts at all. Lazy-init here is what lets this script
    // survive both a fresh level load and a save/load without
    // special-casing either.
    let hp;
    if !ctx.has_persistent("hp_max") {
        hp = 12;
        ctx.set_persistent("hp", hp);
        ctx.set_persistent("hp_max", 12);
        ctx.set_persistent("gold", 0);
        ctx.set_persistent("potions", 1);
        ctx.set_persistent("depth", 1);
        ctx.set_persistent("turns_taken", 0);
    } else {
        hp = ctx.get_persistent("hp");
    }
    if !ctx.has_global("music_started") {
        ctx.play_music("roguelike/audio/music.ogg");
        ctx.set_global("music_started", true);
    }

    draw_hud(ctx);
    if hp <= 0 { draw_death_screen(ctx); }
}

/// Runs exactly once, only when TurnScheduler hands this actor the turn
/// (Step 5f) — move/attack/quaff/restart, whichever `ctx.command_action()`
/// (queued by this same step's on_input) names.
fn on_turn(id, ctx) {
    let hp = or_zero(ctx.get_persistent("hp"));

    if hp <= 0 {
        if ctx.command_action() == "restart" {
            ctx.set_persistent("hp", 12);
            ctx.set_persistent("gold", 0);
            ctx.set_persistent("potions", 1);
            ctx.set_persistent("depth", 1);
            ctx.set_persistent("turns_taken", 0);
            ctx.load_level("roguelike/floor1.level");
        }
        return; // dead: never consumes a turn, restart included — the
                // level transition (if any) happens regardless.
    }

    let action = ctx.command_action();
    let hp_max = or_zero(ctx.get_persistent("hp_max"));

    // "move"/"quaff" are their own functions (not inlined here) specifically
    // to keep this if/else-if chain shallow — an earlier version nested
    // each action's own logic directly in these branches and tripped Rhai's
    // "Expression exceeds maximum complexity" guard (see draw_hud's `s +=`
    // chain below for the same class of hazard, just from nesting depth
    // instead of a long `+` chain).
    if action == "move" {
        if !do_move(id, ctx) { return; } // wall bump: no turn consumed
    } else if action == "quaff" {
        if !do_quaff(id, ctx, hp, hp_max) { return; } // nothing to quaff, or already full: no turn consumed
    } else if action != "wait" {
        // No command this step (action == ""), or one this state doesn't
        // recognize (e.g. "restart" while alive) — either way, no turn.
        return;
    }

    ctx.set_persistent("turns_taken", or_zero(ctx.get_persistent("turns_taken")) + 1);
    ctx.act(100.0);
}

/// Guard for a get_global/get_persistent read that might race its own
/// same-frame lazy-init (see the header comment) — safe to use wherever
/// the value's true initial value is itself 0, or the read only feeds a
/// cosmetic display rather than a comparison/branch.
fn or_zero(v) {
    if v == () { 0 } else { v }
}

/// One step of bump-movement: attacks an adjacent enemy/boss instead of
/// moving onto it, refuses to move into a wall, otherwise moves. Returns
/// `false` only for the wall-bump case — the standard roguelike rule that a
/// bumped wall costs no turn, unlike a bumped enemy (which does).
fn do_move(id, ctx) {
    let dx = ctx.command_param(0);
    let dy = ctx.command_param(1);
    let tx = ctx.get_x(id) + dx;
    let ty = ctx.get_y(id) + dy;
    let target = ctx.get_entity_at(tx, ty);
    if target != -1 && (ctx.has_tag(target, "enemy") || ctx.has_tag(target, "boss")) {
        attack(target, ctx);
        return true;
    }
    if ctx.is_solid_at(tx, ty) { return false; }
    ctx.set_position(id, tx, ty);
    true
}

/// Drink a potion for +6 hp (capped at `hp_max`). Returns `false` (no turn
/// consumed) if there's no potion to drink or the player is already at full
/// hp — guards against a stray "q" press wasting one.
fn do_quaff(id, ctx, hp, hp_max) {
    if or_zero(ctx.get_persistent("potions")) <= 0 || hp >= hp_max { return false; }
    ctx.set_persistent("potions", or_zero(ctx.get_persistent("potions")) - 1);
    let healed = hp + 6;
    if healed > hp_max { healed = hp_max; }
    ctx.set_persistent("hp", healed);
    ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Green");
    true
}

/// The player bump-attacking an enemy/boss — the reverse direction
/// (enemy/boss attacking the player) lives in enemy_rat.rhai/
/// enemy_boss.rhai's own on_turn, writing this script's "hp" persistent
/// value directly (see the header comment's "DAMAGE MODEL CHANGED" note).
fn attack(target, ctx) {
    let key = "hp_" + target;
    let ehp = or_zero(ctx.get_global(key));
    ctx.set_global(key, ehp - 3);
    ctx.emit_particles(ctx.get_x(target), ctx.get_y(target), "*", "Red");
    ctx.shake_camera(1.0, 0.1);
    // roguelike/audio has no dedicated attack sound — hurt.ogg (the only
    // impact-shaped clip available) does double duty for both landing a
    // hit and taking one.
    ctx.play_sound("roguelike/audio/hurt.ogg");
}

fn draw_hud(ctx) {
    let hp = or_zero(ctx.get_persistent("hp"));
    let hp_max = or_zero(ctx.get_persistent("hp_max"));
    let gold = or_zero(ctx.get_persistent("gold"));
    let potions = or_zero(ctx.get_persistent("potions"));
    let depth = or_zero(ctx.get_persistent("depth"));
    let turn = ctx.get_turn_number();

    // Built incrementally rather than as one long `a + b + c + ...` chain:
    // Rhai parses that as a single deeply-nested binary expression tree,
    // and a line with this many concatenations trips the engine's default
    // max-expression-depth guard ("Expression exceeds maximum complexity")
    // at compile time. Each `+=` below is its own small statement instead.
    let s = "HP " + hp;
    s += "/" + hp_max;
    s += "   Gold " + gold;
    s += "   Potions " + potions;
    s += "   Depth " + depth;
    s += "   Turn " + turn;
    ctx.draw_hud(1, 0, s, "White", "Reset");

    let vh = ctx.get_viewport_height();
    ctx.draw_hud(1, vh - 1, "WASD/Arrows: Move   Space: Wait   Q: Potion   Esc: Pause", "Grey", "Reset");
}

fn draw_death_screen(ctx) {
    ctx.fill_rect(10, 8, 30, 6, " ", "White", "DarkRed");
    ctx.draw_box(10, 8, 30, 6, "White", "DarkRed");
    ctx.draw_hud(14, 10, "YOU DIED", "Yellow", "DarkRed");
    ctx.draw_hud(14, 12, "Press R to restart", "White", "DarkRed");
}
