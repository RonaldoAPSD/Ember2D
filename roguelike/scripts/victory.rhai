// roguelike/scripts/victory.rhai — the finale level's player script. Not
// player.rhai: there's no combat, no pickups, and nothing else on this
// level ever acts, so there's no reason for turn-based gating — reusing
// player.rhai's whole turn/hp/attack machinery here would just be dead
// weight. Free-roam movement only, plus victory music and a summary of the
// run pulled from PERSISTENT stats. Persistent, not globals, is exactly
// what survives the level transition into this one — see player.rhai's own
// header comment for why.
//
// Step 5e (docs/ember2d-phase5-plan.md): movement input moved to on_input,
// same split as player.rhai — see that script's header comment for why.
//
// Step 5f: applying the move itself moved on again, from on_update to
// on_turn. This level isn't turn-gated in spirit — no ctx.act() call below,
// so a move never advances TurnScheduler or costs a "turn" — but on_turn is
// still the only pass that reliably sees this step's just-submitted
// command: play.rs only calls it (and therefore only fires this level's
// single `Local` actor's turn at all) on a step where on_input actually
// queued one, and PlayState::apply_script_result overwrites `self.commands`
// wholesale after *every* script pass — including a step's on_turn call
// even when, as here, the script defines no such function to catch it —
// so command_action() would already read back "" by the time a still-in-
// on_update version of this got to it.
fn on_input(id, ctx) {
    let dx = 0.0;
    let dy = 0.0;
    if ctx.just_pressed("w") || ctx.just_pressed("up") { dy = -1.0; }
    else if ctx.just_pressed("s") || ctx.just_pressed("down") { dy = 1.0; }
    else if ctx.just_pressed("a") || ctx.just_pressed("left") { dx = -1.0; }
    else if ctx.just_pressed("d") || ctx.just_pressed("right") { dx = 1.0; }
    if dx != 0.0 || dy != 0.0 { ctx.submit(id, "move", [dx, dy]); }
}

fn on_turn(id, ctx) {
    if ctx.command_action() == "move" {
        let tx = ctx.get_x(id) + ctx.command_param(0);
        let ty = ctx.get_y(id) + ctx.command_param(1);
        if !ctx.is_solid_at(tx, ty) { ctx.set_position(id, tx, ty); }
    }
}

fn on_update(id, ctx) {
    if !ctx.has_global("music_started") {
        ctx.play_music("roguelike/audio/victory.ogg");
        ctx.set_global("music_started", true);
    }

    let gold = or_zero(ctx.get_persistent("gold"));
    let potions = or_zero(ctx.get_persistent("potions"));
    let turns = or_zero(ctx.get_persistent("turns_taken"));

    // Banner only covers the top of the 20x10 level, leaving the lower
    // rows free to walk around in rather than filling the whole screen.
    ctx.fill_rect(1, 1, 18, 4, " ", "White", "DarkBlue");
    ctx.draw_box(1, 1, 18, 4, "Yellow", "DarkBlue");
    ctx.draw_hud(5, 2, "VICTORY!", "Yellow", "DarkBlue");

    let s = "Gold: " + gold;
    s += "  Potions: " + potions;
    s += "  Turns: " + turns;
    ctx.draw_hud(2, 3, s, "White", "DarkBlue");
}

fn or_zero(v) {
    if v == () { 0 } else { v }
}
