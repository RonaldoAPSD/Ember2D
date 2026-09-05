// roguelike/scripts/enemy_rat.rhai — a weak melee enemy: chases the player
// with greedy dominant-axis stepping and bump-attacks when adjacent.
//
// TURN-GATING (Phase 4, docs/ember2d-refactor-plan.md; full reasoning in the
// plan file at C:\Users\ronal\.claude\plans\memoized-discovering-glacier.md):
// on_update runs every frame, but a rat must act at most once per player
// turn. It compares its own "acted_<id>" global against player.rhai's
// shared "turn" global — deferred writes mean this script only ever sees
// the LAST frame's committed "turn" value, so a rat naturally acts one
// frame after the turn counter changes (the same frame TurnHarness::turn's
// follow-up input-less frame exists for).
//
// DAMAGE MODEL (single-writer — see player.rhai's header comment): this
// script never writes the player's hp directly. It publishes
// "atk_turn_<id>"/"atk_dmg_<id>" (which only it ever writes); player.rhai's
// resolve_incoming_damage sums whichever of those are newer than its own
// "last_resolved_turn" once per turn. Two adjacent rats therefore both
// land their hit in the same turn, instead of one clobbering the other's
// write.
//
// MOVEMENT: greedy dominant-axis stepping toward the player, not
// ctx.get_path — get_path rescans every collider for every neighbor of
// every A* node (up to 2000 nodes), fine for one call but not for every
// enemy every turn.
//
// LINE OF SIGHT: sticky aggro, matching classic roguelikes (Rogue/NetHack)
// — a rat starts asleep and does nothing until it either gets an
// unobstructed ctx.raycast to the player, or takes a hit (you can't be
// attacked without being adjacent, so getting hit always reveals the
// attacker even through a raycast quirk). Once awake it stays awake for
// the rest of the level — it doesn't go back to sleep if the player breaks
// line of sight later, it just keeps hunting toward wherever it last knew
// the player to be. No separate "search last known position" behavior;
// dominant-axis stepping toward the player's CURRENT position doubles as
// that, since it's recomputed fresh every turn regardless of visibility.
//
// ACCEPTED LIMITATION: two rats can step onto the same cell in one turn,
// since neither sees the other's pending move (both read the same
// frame-start snapshot). A cosmetic overlap for one turn — real
// arbitration needs Phase 5's scheduler.
//
// HP INIT LIVES IN on_start, NOT on_update — found the hard way. Both this
// script's own "hp_<id>" lazy-init and player.rhai's attack() write that
// exact same key, and on_update runs every frame regardless of whose
// "turn" it is. If the player's very first attack on a rat lands on the
// same frame the rat's on_update has never run before (a real possibility
// if the player reaches a rat before its first frame — e.g. if it spawned
// closer, or a future floor spawns one right next to the player), BOTH
// scripts write "hp_<id>" in that one pass: the rat's init (resetting to
// 6) and the player's damage (subtracting 3) race for the same key, and
// whichever entity's script happens to run later in that frame's
// HashMap-ordered iteration wins outright — silently discarding the other
// write, no error, no log. `on_start` runs in its own earlier pass, fully
// committed before the level's first real engine frame — by the time any
// on_update ever runs (the player's included), "hp_<id>" already has its
// real value, so on_update's own fallback branch below never has anything
// left to initialize in the normal case.
fn on_start(id, ctx) {
    ctx.set_global("hp_" + id, 6);
}

fn on_update(id, ctx) {
    // Fallback only (see on_start above for why the real init lives
    // there). Still guards against defaulting a missing value to 0 here —
    // a wrong 0 would read as "already dead" and instantly despawn a rat
    // that was never actually hit (same care player.rhai's own hp takes).
    let hp;
    if !ctx.has_global("hp_" + id) {
        hp = 6;
        ctx.set_global("hp_" + id, hp);
    } else {
        hp = ctx.get_global("hp_" + id);
    }

    // Death check runs every frame, not just on the entity's own turn, so
    // a rat killed mid-turn disappears as soon as possible rather than
    // waiting for its next scheduled action.
    if hp <= 0 {
        // No dedicated death sound in roguelike/audio — hurt.ogg already
        // plays on the player's turn when it lands the killing blow (see
        // player.rhai's attack()); nothing thematically right is left for
        // this side, so this stays silent like pickup.rhai/stairs.rhai do.
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "*", "DarkRed");
        ctx.despawn(id);
        return;
    }

    let turn = or_zero(ctx.get_global("turn"));
    let acted_key = "acted_" + id;
    let last_acted = or_zero(ctx.get_global(acted_key));
    if turn == last_acted { return; } // already acted this turn, or no turn has happened yet

    let player = ctx.find_by_tag("player");
    if !ctx.entity_exists(player) { ctx.set_global(acted_key, turn); return; }

    let ex = ctx.get_x(id);
    let ey = ctx.get_y(id);
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);

    let aware = update_awareness(id, ctx, hp, ex, ey, px, py);
    if !aware { ctx.set_global(acted_key, turn); return; } // still asleep

    let dx = px - ex;
    let dy = py - ey;

    let adjacent = (dx == 0.0 && dy.abs() == 1.0) || (dy == 0.0 && dx.abs() == 1.0);
    if adjacent {
        ctx.set_global("atk_turn_" + id, turn);
        ctx.set_global("atk_dmg_" + id, 2);
    } else {
        let step_x = 0.0;
        let step_y = 0.0;
        if dx.abs() >= dy.abs() && dx != 0.0 { step_x = if dx > 0.0 { 1.0 } else { -1.0 }; }
        else if dy != 0.0 { step_y = if dy > 0.0 { 1.0 } else { -1.0 }; }

        let tx = ex + step_x;
        let ty = ey + step_y;
        // is_solid_at already blocks walls, the player (Collider::new
        // hardcodes solid: true regardless of PlayerRecord.solid — see the
        // plan file's "Engine facts" section), and any other rat (rats are
        // solid too) — no separate get_entity_at check needed.
        if (step_x != 0.0 || step_y != 0.0) && !ctx.is_solid_at(tx, ty) {
            ctx.set_position(id, tx, ty);
        }
    }

    ctx.set_global(acted_key, turn);
}

/// See player.rhai's header comment for why this guard exists: a get_*
/// never observes a set_* from earlier in the same script pass.
fn or_zero(v) {
    if v == () { 0 } else { v }
}

/// Sticky aggro (see the header comment): once true, never re-checked
/// again. hp < 6 (this rat's max) means the player has already landed a
/// hit, which requires having been adjacent — treat that as automatic
/// awareness rather than needing a clean raycast on the exact turn the hit
/// landed. mask=["solid"] restricts the raycast to walls/rats (their
/// shared default collider layer — see the Phase 4h generator's `rat()`
/// comment) so the player's own collider, on a different layer, can never
/// itself be mistaken for something blocking sight to... itself. Pulled
/// into its own function (rather than inlined in on_update) because the
/// nested if/else here alone was enough to trip Rhai's default
/// max-expression-complexity guard at compile time — the same class of
/// limit player.rhai's HUD line hit from string concatenation instead.
fn update_awareness(id, ctx, hp, ex, ey, px, py) {
    let aware_key = "aware_" + id;
    let aware = ctx.get_global(aware_key) == true;
    if aware {
        ctx.set_tint(id, "Red", "Reset");
        return true;
    }

    if hp < 6 { aware = true; }
    else {
        let hit = ctx.raycast(ex, ey, px, py, ["solid"]);
        if hit.is_empty() { aware = true; }
    }

    if aware {
        ctx.set_global(aware_key, true);
        ctx.set_tint(id, "Red", "Reset");
    } else {
        ctx.set_tint(id, "DarkRed", "Reset");
    }
    aware
}
