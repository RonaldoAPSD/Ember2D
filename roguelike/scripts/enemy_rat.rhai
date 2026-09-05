// roguelike/scripts/enemy_rat.rhai — a weak melee enemy: chases the player
// with greedy dominant-axis stepping and bump-attacks when adjacent.
//
// STEP 5f (docs/ember2d-phase5-plan.md) TURN SCHEDULER: on_turn now does
// all of this — chasing, attacking, waking up — called exactly once
// whenever TurnScheduler hands this actor the turn. The old "acted_<id>"
// global (compared against player.rhai's "turn" global to gate to once per
// player turn, since on_update used to run every frame for every entity)
// is gone: the scheduler itself is what guarantees on_turn fires exactly
// once per this rat's turn, no gating needed. on_update keeps only the
// death check and the awareness tint — see that function's own comment for
// why even the tint stays split from the awareness *decision*.
//
// DAMAGE MODEL CHANGED TOO: this script now writes the player's "hp"
// persistent value DIRECTLY when it lands a hit, instead of publishing an
// "atk_turn_<id>"/"atk_dmg_<id>" pair for player.rhai to sum up later. That
// indirection existed only because the old model ran every entity's
// on_update in the SAME pass — two attackers writing "hp" in the same pass
// would silently cost only one hit (a get_* never observes a set_* from
// earlier in the same pass). Under the scheduler, each actor's on_turn is
// its own separate pass, fully committed (apply_ctx has already run)
// before the next actor's on_turn even starts — so a rat's direct write is
// already visible by the time the player, or the next rat, reads "hp" (or,
// for two adjacent rats, `is_solid_at`; see below).
//
// THE OLD "ACCEPTED LIMITATION" NO LONGER APPLIES: this file used to note
// that two rats could step onto the same cell in one turn, since neither
// saw the other's pending move (both read the same frame-start snapshot
// under the old every-entity-every-frame model). Under the scheduler, rat
// A's move is fully committed (apply_ctx has run) before rat B's on_turn
// even begins, so rat B's own `is_solid_at` check correctly sees rat A's
// new position and won't step onto it.
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
// An asleep rat's on_turn still returns having done nothing — that still
// counts as its turn (see play.rs's run_actor_turn: an AI actor's turn
// always advances the scheduler, whether or not it calls ctx.act, since a
// sleeping monster still "used" its turn doing nothing, same as any other
// roguelike).
//
// HP INIT LIVES IN on_start, NOT on_update — found the hard way, back when
// on_update ran every frame for every entity (Phase 4) and could race the
// player's own attack() write for "hp_<id>" in the same pass. Kept here
// even though on_turn's sequential-pass model would arguably make an
// on_update fallback safe again now — on_start still runs in its own
// earlier pass, fully committed before the level's first real frame, so
// there's no reason to move it and no reason to trust a same-frame race
// that's already been fixed once.
fn on_start(id, ctx) {
    ctx.set_global("hp_" + id, 6);
}

/// Runs every step regardless of whose turn it is (Step 5f) — the death
/// check (so a rat killed mid-round disappears as soon as possible, not
/// waiting for its next scheduled turn) and the awareness tint. The tint
/// only *reads* "aware_<id>" here — the awareness *decision* (raycast, hp
/// check, actually waking up) happens once per turn in on_turn's
/// update_awareness call, so the color redraws every step but the
/// underlying state only ever changes on this rat's own turn.
fn on_update(id, ctx) {
    let hp;
    if !ctx.has_global("hp_" + id) {
        hp = 6;
        ctx.set_global("hp_" + id, hp);
    } else {
        hp = ctx.get_global("hp_" + id);
    }

    if hp <= 0 {
        // No dedicated death sound in roguelike/audio — hurt.ogg already
        // plays when a hit lands (on either side); nothing thematically
        // right is left for this, so this stays silent like pickup.rhai/
        // stairs.rhai do.
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "*", "DarkRed");
        ctx.despawn(id);
        return;
    }

    if ctx.get_global("aware_" + id) == true { ctx.set_tint(id, "Red", "Reset"); }
    else { ctx.set_tint(id, "DarkRed", "Reset"); }
}

/// Runs exactly once, only when TurnScheduler hands this actor the turn —
/// wake up (if not already), then chase or attack.
fn on_turn(id, ctx) {
    let hp = or_zero(ctx.get_global("hp_" + id));
    if hp <= 0 { return; } // on_update should have despawned it already; guard anyway

    let player = ctx.find_by_tag("player");
    if !ctx.entity_exists(player) { return; }

    let ex = ctx.get_x(id);
    let ey = ctx.get_y(id);
    let px = ctx.get_x(player);
    let py = ctx.get_y(player);

    if !update_awareness(id, ctx, hp, ex, ey, px, py) { return; } // still asleep

    let dx = px - ex;
    let dy = py - ey;

    let adjacent = (dx == 0.0 && dy.abs() == 1.0) || (dy == 0.0 && dx.abs() == 1.0);
    if adjacent {
        ctx.set_persistent("hp", or_zero(ctx.get_persistent("hp")) - 2);
        ctx.shake_camera(2.0, 0.2);
        ctx.play_sound("roguelike/audio/hurt.ogg");
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
        // solid too, and — see the header comment — a rat that already
        // moved this round is already committed by the time this check
        // runs) — no separate get_entity_at check needed.
        if (step_x != 0.0 || step_y != 0.0) && !ctx.is_solid_at(tx, ty) {
            ctx.set_position(id, tx, ty);
        }
    }
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
/// into its own function (rather than inlined in on_turn) because the
/// nested if/else here alone was enough to trip Rhai's default
/// max-expression-complexity guard at compile time — the same class of
/// limit player.rhai's HUD line hit from string concatenation instead.
fn update_awareness(id, ctx, hp, ex, ey, px, py) {
    let aware = ctx.get_global("aware_" + id) == true;
    if aware { return true; }

    if hp < 6 { aware = true; }
    else {
        let hit = ctx.raycast(ex, ey, px, py, ["solid"]);
        if hit.is_empty() { aware = true; }
    }

    if aware { ctx.set_global("aware_" + id, true); }
    aware
}
