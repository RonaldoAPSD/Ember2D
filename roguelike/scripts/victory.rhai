// roguelike/scripts/victory.rhai — the finale level's player script. Not
// player.rhai: there's no combat, no pickups, and nothing else on this
// level ever acts, so there's no reason for turn-based gating — reusing
// player.rhai's whole turn/hp/attack machinery here would just be dead
// weight. Free-roam movement only, plus victory music and a summary of the
// run pulled from PERSISTENT stats. Persistent, not globals, is exactly
// what survives the level transition into this one — see player.rhai's own
// header comment for why.
fn on_update(id, ctx) {
    if !ctx.has_global("music_started") {
        ctx.play_music("roguelike/audio/victory.ogg");
        ctx.set_global("music_started", true);
    }

    let dx = 0.0;
    let dy = 0.0;
    if ctx.just_pressed("w") || ctx.just_pressed("up") { dy = -1.0; }
    else if ctx.just_pressed("s") || ctx.just_pressed("down") { dy = 1.0; }
    else if ctx.just_pressed("a") || ctx.just_pressed("left") { dx = -1.0; }
    else if ctx.just_pressed("d") || ctx.just_pressed("right") { dx = 1.0; }
    if dx != 0.0 || dy != 0.0 {
        let tx = ctx.get_x(id) + dx;
        let ty = ctx.get_y(id) + dy;
        if !ctx.is_solid_at(tx, ty) { ctx.set_position(id, tx, ty); }
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
