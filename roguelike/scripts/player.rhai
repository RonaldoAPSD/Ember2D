// roguelike/scripts/player.rhai — player controller: turn-gated grid
// movement, bump-to-attack, damage resolution, and the HUD.
//
// Phase 4 (docs/ember2d-refactor-plan.md) moves all of this out of Rust —
// see the Phase 4 plan for the staged steps this script is part of.
//
// STATE OWNERSHIP:
//   Rhai's per-entity `let` does NOT persist between on_update calls —
//   ScriptEngine calls Engine::call_fn, whose default CallFnOptions rewinds
//   the scope after every call. (An earlier version of
//   docs/ember2d-scripting-api.md claimed scope persists; that was wrong
//   and is being corrected as part of this phase.) So:
//     - GLOBALS (ctx.set_global/get_global) are level-scoped: they reset to
//       empty on every level load, which is exactly what a per-floor turn
//       counter needs.
//     - PERSISTENT (ctx.set_persistent/get_persistent) survives level
//       transitions — this run's stats live here.
//
// GLOBALS this script owns (sole writer):
//   "turn"                — i64, bumped once per player action (move,
//                           attack, or wait). Enemy scripts compare their
//                           own "acted_<id>" against this to act once per
//                           turn instead of once per frame.
//   "last_resolved_turn"  — i64, the highest turn whose enemy attacks have
//                           already been folded into the player's hp.
//
// PERSISTENT this run's stats (survive next_level; reset on death/restart):
//   "hp", "hp_max", "gold", "potions", "depth", "turns_taken"
//
// DAMAGE MODEL (single-writer, see the Phase 4 plan's "deferred writes"
// note): an enemy never writes the player's hp directly — two attackers in
// the same pass would silently cost only one hit, since a get_* never
// observes a set_* from earlier in the same pass. Instead each enemy
// publishes "atk_turn_<id>"/"atk_dmg_<id>" (which only it ever writes), and
// resolve_incoming_damage sums whichever of those are newer than
// "last_resolved_turn" once per turn.
//
// A SECOND DEFERRED-WRITE HAZARD, FOUND DURING STEP 4h (enemy_rat.rhai was
// the first script to ever actually publish an atk_turn_*/atk_dmg_* pair —
// nothing before it had, so this path had never really run): the original
// resolve_incoming_damage used `now == last_resolved_turn` to skip scanning
// on frames where "turn" hadn't changed, and otherwise set
// last_resolved_turn := now unconditionally. But an enemy can only publish
// atk_turn_<id> for turn T no earlier than the same frame T first becomes
// visible to IT — and that write only commits at the END of that frame,
// one frame after player.rhai's own on_update (same pass, same frame) had
// already read "turn" == T and set last_resolved_turn := T right then,
// having found nothing yet (the attack hadn't landed). By the NEXT frame,
// atk_turn(T) is no longer "> last_resolved_turn(T)" — the attack is
// silently dropped, permanently, every time. Confirmed by hand: a player
// standing next to a live rat for 6 turns took zero damage. **Fixed**:
// never skip the scan, and only ever raise last_resolved_turn to the
// highest atk_turn value actually found and applied this call — never to
// the raw "turn" counter — so a published attack stays eligible for
// exactly as many frames as it takes to become visible.
//
// POTIONS WERE COLLECTIBLE BUT UNUSABLE UNTIL NOW — found by the user
// actually playing through to floor 3 at 2 hp with potions sitting unused
// in inventory. "Q" now quaffs one (if any) for +6 hp (capped at hp_max),
// costing a turn like waiting does. This also exposed a related staleness
// bug: resolve_incoming_damage wrote persistent "hp" internally, but this
// function's own local `hp` (captured once at the top) never saw that
// write — a get_* never observes a set_* from earlier in the same pass,
// same as everywhere else in this file, just easy to miss when the write
// happens *inside a helper function* rather than inline. That meant both
// the death check AND (if this fix weren't here) potion-healing math would
// silently use a one-frame-stale hp whenever incoming damage landed the
// same frame — e.g. quaffing back to pre-damage-plus-heal instead of
// post-damage-plus-heal. Fixed: resolve_incoming_damage now returns the
// post-damage hp, and the call site reassigns its own local `hp` to that
// return value, keeping it fresh for every use below it in this same call.
//
// THE SAME HAZARD BIT THIS SCRIPT DURING ITS OWN LAZY INIT (fixed, keep the
// fix — this WILL recur in every future script with a lazy-init block):
//   `ctx.set_global("turn", 0)` doesn't make `ctx.get_global("turn")`
//   return 0 later in this SAME on_update call — it still returns `()`
//   until apply_ctx folds the write in at the end of the frame. On the
//   very first frame, `let turn = ctx.get_global("turn"); ctx.set_global(
//   "turn", turn + 1);` therefore tried to compute `() + 1`, which Rhai has
//   no operator overload for. That error's message happens to contain the
//   substring "Function not found" — the same phrase the engine already
//   uses to mean "this script has no such function, that's fine" — so
//   ScriptEngine::run_scripts's error filter silently swallowed it instead
//   of logging it, and on_update just stopped dead at that line every
//   frame (still doing its position update from earlier in the call, but
//   never reaching trigger_turn() a few lines later). No compile error, no
//   runtime error in the log, no panic — just a script that quietly never
//   finishes. `or_zero()` below is the general guard for reads that might
//   race their own lazy-init this way; `hp` gets a stronger fix (computed
//   directly during init, never defaulted to 0) because a wrong default
//   there isn't just cosmetic — 0 reads as "already dead".
fn on_update(id, ctx) {
    // ---- lazy run init -------------------------------------------------
    // Never in on_start: globals aren't part of SaveState, and PlayState's
    // is_loading_save path never calls on_start scripts at all. Lazy-init
    // here is what lets this script survive both a fresh level load and a
    // (future) save/load without special-casing either.
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
    if !ctx.has_global("turn") {
        ctx.set_global("turn", 0);
        ctx.set_global("last_resolved_turn", 0);
    }
    if !ctx.has_global("music_started") {
        ctx.play_music("roguelike/audio/music.ogg");
        ctx.set_global("music_started", true);
    }

    // or_zero's 0 fallback is safe here specifically because "turn"'s true
    // initial value (set just above) IS 0 — the fallback and the real
    // value coincide, unlike hp's 12.
    let turn = or_zero(ctx.get_global("turn"));

    hp = resolve_incoming_damage(id, ctx, hp);
    draw_hud(ctx);

    if hp <= 0 {
        draw_death_screen(ctx);
        if ctx.just_pressed("r") {
            ctx.set_persistent("hp", 12);
            ctx.set_persistent("gold", 0);
            ctx.set_persistent("potions", 1);
            ctx.set_persistent("depth", 1);
            ctx.set_persistent("turns_taken", 0);
            ctx.load_level("roguelike/floor1.level");
        }
        return;
    }

    let dx = 0.0;
    let dy = 0.0;
    if ctx.just_pressed("w") || ctx.just_pressed("up") { dy = -1.0; }
    else if ctx.just_pressed("s") || ctx.just_pressed("down") { dy = 1.0; }
    else if ctx.just_pressed("a") || ctx.just_pressed("left") { dx = -1.0; }
    else if ctx.just_pressed("d") || ctx.just_pressed("right") { dx = 1.0; }

    let waited = ctx.just_pressed("space");

    // Quaffing is another turn-costing action alternative to moving/waiting,
    // same tier as bump-to-attack — resolved immediately (dx/dy stay 0.0
    // when only "q" is pressed, so it never falls into the movement branch
    // below). Guarded on both "have a potion" and "actually hurt" so a
    // stray press at full hp doesn't waste one.
    let hp_max = or_zero(ctx.get_persistent("hp_max"));
    let quaffed = ctx.just_pressed("q") && or_zero(ctx.get_persistent("potions")) > 0 && hp < hp_max;
    if quaffed {
        ctx.set_persistent("potions", or_zero(ctx.get_persistent("potions")) - 1);
        let healed = hp + 6;
        if healed > hp_max { healed = hp_max; }
        ctx.set_persistent("hp", healed);
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "+", "Green");
    }

    if dx == 0.0 && dy == 0.0 && !waited && !quaffed { return; } // no input this frame -> no turn

    if dx != 0.0 || dy != 0.0 {
        let tx = ctx.get_x(id) + dx;
        let ty = ctx.get_y(id) + dy;
        let target = ctx.get_entity_at(tx, ty);
        if target != -1 && (ctx.has_tag(target, "enemy") || ctx.has_tag(target, "boss")) {
            attack(target, ctx);
        } else if ctx.is_solid_at(tx, ty) {
            return; // wall bump: no turn consumed, matching the standard roguelike rule
        } else {
            ctx.set_position(id, tx, ty);
        }
    }

    ctx.set_global("turn", turn + 1);
    ctx.set_persistent("turns_taken", or_zero(ctx.get_persistent("turns_taken")) + 1);
    ctx.trigger_turn();
}

/// Guard for a get_global/get_persistent read that might race its own
/// same-frame lazy-init (see the header comment) — safe to use wherever
/// the value's true initial value is itself 0, or the read only feeds a
/// cosmetic display rather than a comparison/branch.
fn or_zero(v) {
    if v == () { 0 } else { v }
}

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

/// Returns the post-damage hp (see the header comment's "POTIONS WERE
/// COLLECTIBLE BUT UNUSABLE" note for why the caller must reassign its own
/// `hp` to this return value instead of trusting the one it passed in).
fn resolve_incoming_damage(id, ctx, hp) {
    let last = or_zero(ctx.get_global("last_resolved_turn"));
    let highest = last;

    let dmg = 0;
    for e in ctx.find_all_by_tag("enemy") {
        let atk_turn = ctx.get_global("atk_turn_" + e);
        if atk_turn != () && atk_turn > last {
            dmg += or_zero(ctx.get_global("atk_dmg_" + e));
            if atk_turn > highest { highest = atk_turn; }
        }
    }
    for e in ctx.find_all_by_tag("boss") {
        let atk_turn = ctx.get_global("atk_turn_" + e);
        if atk_turn != () && atk_turn > last {
            dmg += or_zero(ctx.get_global("atk_dmg_" + e));
            if atk_turn > highest { highest = atk_turn; }
        }
    }
    ctx.set_global("last_resolved_turn", highest);
    if dmg <= 0 { return hp; }

    let new_hp = hp - dmg;
    ctx.set_persistent("hp", new_hp);
    ctx.shake_camera(2.0, 0.2);
    ctx.play_sound("roguelike/audio/hurt.ogg");
    new_hp
}

fn draw_hud(ctx) {
    let hp = or_zero(ctx.get_persistent("hp"));
    let hp_max = or_zero(ctx.get_persistent("hp_max"));
    let gold = or_zero(ctx.get_persistent("gold"));
    let potions = or_zero(ctx.get_persistent("potions"));
    let depth = or_zero(ctx.get_persistent("depth"));
    let turn = or_zero(ctx.get_global("turn"));

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
