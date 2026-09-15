// demos/roguelike/scripts/enemy.rhai — the one script every roguelike enemy
// tile uses (Step 7.5-4, docs/ember2d-master-plan.md §5.6), replacing the
// near-duplicate enemy_rat.rhai/enemy_boss.rhai: max hp, per-hit damage, and
// awareness range are no longer hardcoded constants baked into a copy of
// this file per role — they're authored once per role as DATA on the tile's
// own Actor (`ctx.get_stat`, `TileRecord.actor.stats` in `gen_roguelike.rs`'s
// `rat()`/`boss()`), so this script itself no longer knows or cares which
// role it's running as. The aware/asleep tint pair is the same story via
// `ctx.get_tint_aware`/`get_tint_asleep` — kept as separate authored fields,
// not a `stats` entry, since `stats` is numeric-only and a color isn't.
//
// TURN SCHEDULER (carried over unchanged from the old enemy_rat.rhai/
// enemy_boss.rhai): on_turn does all of the real work — chasing, attacking,
// waking up — called exactly once whenever TurnScheduler hands this actor
// the turn. on_update keeps only the death check and the awareness tint —
// see that function's own comment for why even the tint stays split from
// the awareness *decision*.
//
// DAMAGE MODEL: this script writes the player's "hp" persistent value
// DIRECTLY when it lands a hit, via `ctx.add_persistent` — safe because
// each actor's on_turn is its own fully-committed pass under the scheduler
// (apply_ctx has already run) before the next actor's on_turn even starts,
// so two adjacent enemies hitting the player in the same round both land
// (see `docs/ember2d-master-plan.md` §5.6's 7.5-2 write-up for the same
// reasoning applied to `add_global`/`add_persistent` generally).
//
// LINE OF SIGHT: sticky aggro, matching classic roguelikes (Rogue/NetHack)
// — an enemy starts asleep and does nothing until it either takes a hit
// (you can't be attacked without being adjacent, so getting hit always
// reveals the attacker), or gets an unobstructed `ctx.raycast` to the
// player from within its own authored `awareness_range` — NEW with this
// step: the old per-script raycast had no distance limit at all, so any
// unobstructed line of sight of any length woke the enemy regardless of
// range. `dist <= awareness_range` gates the raycast first; `sqrt` is
// IEEE-754-exact on every platform (`math.rs`'s own `Vec2::length`, and
// `api_spatial.rs`'s distance calc, already rely on this), unlike the
// sin/cos/atan2/exp/powf this project avoids in the sim, so computing it
// directly in a script is safe. Once awake, an enemy stays awake for the
// rest of the level — it doesn't go back to sleep if the player breaks line
// of sight later, it just keeps hunting toward wherever it last knew the
// player to be. No separate "search last known position" behavior;
// dominant-axis stepping toward the player's CURRENT position doubles as
// that, since it's recomputed fresh every turn regardless of visibility.
//
// An asleep enemy's on_turn still returns having done nothing — that still
// counts as its turn (see play.rs's run_actor_turn: an AI actor's turn
// always advances the scheduler, whether or not it calls ctx.act, since a
// sleeping monster still "used" its turn doing nothing, same as any other
// roguelike).
//
// HP INIT LIVES IN on_start, NOT on_update — on_start runs in its own
// earlier pass, fully committed before the level's first real frame, so
// there's no same-frame race with anything else reading "hp" this step.
//
// HP/AWARENESS LIVE IN Vars, NOT A "hp_" + id GLOBAL (Step 7.5-3,
// docs/ember2d-master-plan.md §5.6) — real per-entity component state
// (`components/vars.rs`): automatically cleared when this entity despawns,
// with no risk of a naming collision with any other entity's own "hp"/
// "aware" var.
fn on_start(id, ctx) {
    ctx.set_var(id, "hp", ctx.get_stat(id, "hp"));
}

/// Runs every step regardless of whose turn it is (Step 5f) — the death
/// check (so an enemy killed mid-round disappears as soon as possible, not
/// waiting for its next scheduled turn) and the awareness tint. The tint
/// only *reads* "aware" here — the awareness *decision* (raycast, hp check,
/// actually waking up) happens once per turn in on_turn's update_awareness
/// call, so the color redraws every step but the underlying state only ever
/// changes on this enemy's own turn.
fn on_update(id, ctx) {
    let max_hp = ctx.get_stat(id, "hp");
    let hp;
    if !ctx.has_var(id, "hp") {
        hp = max_hp;
        ctx.set_var(id, "hp", hp);
    } else {
        hp = ctx.get_var(id, "hp");
    }

    if hp <= 0 {
        ctx.emit_particles(ctx.get_x(id), ctx.get_y(id), "*", ctx.get_tint_asleep(id));
        ctx.despawn(id);
        return;
    }

    if ctx.get_var(id, "aware") == true { ctx.set_tint(id, ctx.get_tint_aware(id), "Reset"); }
    else { ctx.set_tint(id, ctx.get_tint_asleep(id), "Reset"); }
}

/// Runs exactly once, only when TurnScheduler hands this actor the turn —
/// wake up (if not already), then chase or attack.
fn on_turn(id, ctx) {
    let hp = ctx.get_var(id, "hp");
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
        let dmg = ctx.get_stat(id, "atk");
        ctx.add_persistent("hp", -dmg);
        ctx.shake_camera(2.0, 0.2);
        ctx.play_sound("demos/roguelike/audio/hurt.ogg");
    } else {
        let step_x = 0.0;
        let step_y = 0.0;
        if dx.abs() >= dy.abs() && dx != 0.0 { step_x = if dx > 0.0 { 1.0 } else { -1.0 }; }
        else if dy != 0.0 { step_y = if dy > 0.0 { 1.0 } else { -1.0 }; }

        let tx = ex + step_x;
        let ty = ey + step_y;
        // is_solid_at already blocks walls, the player (Collider::new
        // hardcodes solid: true regardless of PlayerRecord.solid — see the
        // plan file's "Engine facts" section), and any other enemy (enemies
        // are solid too, and an enemy that already moved this round is
        // already committed by the time this check runs) — no separate
        // get_entity_at check needed.
        if (step_x != 0.0 || step_y != 0.0) && !ctx.is_solid_at(tx, ty) {
            ctx.set_position(id, tx, ty);
            // Phase 5.5 Part 3 (docs/ember2d-phase5.5-plan.md): grid state
            // above already resolved the real move; this is purely the
            // visual — an enemy now visibly slides one cell instead of
            // teleporting. 0.08s (D19, docs/ember2d-refactor-plan.md §3):
            // the scheduler blocks ALL stepping while this drains, and
            // several enemies acting in one round each pay their own
            // animation's duration serially.
            ctx.animate_move(id, tx, ty, 0.08);
        }
    }
}

/// Sticky aggro (see the header comment): once true, never re-checked
/// again. hp below this enemy's own authored max means the player has
/// already landed a hit, which requires having been adjacent — treat that
/// as automatic awareness rather than needing a clean raycast on the exact
/// turn the hit landed. mask=["solid"] restricts the raycast to walls/
/// enemies (their shared default collider layer — see the Phase 4h
/// generator's `rat()` comment) so the player's own collider, on a
/// different layer, can never itself be mistaken for something blocking
/// sight to... itself. Pulled into its own function (rather than inlined in
/// on_turn) because the nested if/else here alone was enough to trip Rhai's
/// default max-expression-complexity guard at compile time — the same class
/// of limit player.rhai's HUD line hit from string concatenation instead.
/// Pulled out of `update_awareness` for the same reason that function is
/// itself pulled out of `on_turn` — Rhai's default max-expression-
/// complexity guard trips at compile time on one function body that inlines
/// too much, and a distance-then-raycast check is enough on top of the
/// existing hp/aware branching to do it.
fn in_awareness_range(ex, ey, px, py, range) {
    let dx = px - ex;
    let dy = py - ey;
    (dx * dx + dy * dy).sqrt() <= range
}

fn update_awareness(id, ctx, hp, ex, ey, px, py) {
    let aware = ctx.get_var(id, "aware") == true;
    if aware { return true; }

    if hp < ctx.get_stat(id, "hp") { aware = true; }
    else if in_awareness_range(ex, ey, px, py, ctx.get_stat(id, "awareness_range")) {
        let hit = ctx.raycast(ex, ey, px, py, ["solid"]);
        if hit.is_empty() { aware = true; }
    }

    if aware { ctx.set_var(id, "aware", true); }
    aware
}
