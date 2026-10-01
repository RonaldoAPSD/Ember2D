// demos/rpg/scripts/player.rhai — the hero on the town, inn and field maps:
// walking one tile at a time, facing, talking to whoever is in front, and
// meeting monsters in tall grass.
//
// The party, gold and potions live in PERSISTENT state (they survive level
// changes and save files); `new_game` below seeds them when a level is
// started directly from the editor (F5) rather than from the title screen —
// the same seeding `title.rhai` does. Rhai here has no modules (master plan
// decision 7.4), so the two copies are kept identical by hand.

fn step_time() { 0.14 }        // seconds per tile while an arrow is held
fn encounter_odds() { 7 }      // 1 in this many steps on tall grass

fn new_game(ctx) {
    ctx.set_persistent("party", [
        #{ name: "Ash", hp: 24, max: 24, atk: 5, def: 1, mag: 0 },
        #{ name: "Lyra", hp: 16, max: 16, atk: 2, def: 0, mag: 6 },
    ]);
    ctx.set_persistent("gold", 20);
    ctx.set_persistent("potions", 2);
    ctx.set_persistent("quest", "");
    ctx.set_persistent("boss_done", false);
}

fn on_start(id, ctx) {
    ctx.play_project_clip(id, "hero_walk");
    ctx.set_layer_order(id, 10);   // the same layer as trees, houses and NPCs...
    ctx.set_y_sort(true);          // ...so whoever is lower on screen is in front
    ctx.set_var(id, "fx", 0);
    ctx.set_var(id, "fy", 1);
    ctx.set_var(id, "wait", 0.0);
    if !ctx.has_persistent("party") {
        new_game(ctx);
    }
}

fn on_update(id, ctx) {
    draw_status(ctx);
    // Someone is talking (an NPC script sets this) — stand still.
    if ctx.has_global("talking") { return; }

    let wait = ctx.get_var(id, "wait") - ctx.get_delta();
    ctx.set_var(id, "wait", wait);

    // Talk to whoever is in front: Space, Enter or Z.
    if ctx.just_pressed("space") || ctx.just_pressed("enter") || ctx.just_pressed("z") {
        let tx = ctx.get_x(id) + ctx.get_var(id, "fx");
        let ty = ctx.get_y(id) + ctx.get_var(id, "fy");
        let e = ctx.get_entity_at(tx, ty);
        let talkers = ["elder", "shop", "inn", "sign", "boss"];
        if e >= 0 && !ctx.is_tilemap(e) && talkers.contains(ctx.get_tag(e)) {
            ctx.set_global("talk", e);
            ctx.set_global("talking", true);
            return;
        }
    }

    let dx = 0;
    let dy = 0;
    if ctx.is_held("left") || ctx.is_held("a") { dx = -1; }
    else if ctx.is_held("right") || ctx.is_held("d") { dx = 1; }
    else if ctx.is_held("up") || ctx.is_held("w") { dy = -1; }
    else if ctx.is_held("down") || ctx.is_held("s") { dy = 1; }
    if dx == 0 && dy == 0 { return; }

    // Turn to face the way pressed even when blocked (so you can face an
    // NPC across a counter and talk).
    ctx.set_var(id, "fx", dx);
    ctx.set_var(id, "fy", dy);
    if dx != 0 { ctx.set_flip(id, dx < 0, false); }
    if wait > 0.0 { return; }

    let nx = ctx.get_x(id) + dx;
    let ny = ctx.get_y(id) + dy;
    if ctx.is_solid_at(nx, ny) { return; }
    ctx.set_position(id, nx, ny);
    ctx.set_var(id, "wait", step_time());

    if ctx.get_tile_tag(nx, ny) == "tallgrass" && ctx.random_int(1, encounter_odds()) == 1 {
        let foes = ["slime", "slime", "bat", "spider", "ghost"];
        ctx.push_scene("battle", #{ data: #{ enemy: ctx.random_choice(foes), boss: false } });
    }
}

// The always-on status line, and a short message another script left
// (`toast`, e.g. "Game saved").
fn draw_status(ctx) {
    let gold = ctx.get_persistent("gold");
    let potions = ctx.get_persistent("potions");
    ctx.draw_hud(1, 0, " Gold " + gold + "   Potions " + potions + " ", "Yellow", "#202028");
    ctx.draw_hud(1, 23, " Arrows: walk   Space: talk   Esc: menu ", "Grey", "#202028");
    if ctx.has_global("toast") {
        ctx.draw_hud(30, 2, " " + ctx.get_global("toast") + " ", "Black", "Yellow");
        let t = ctx.add_global("toast_t", ctx.get_delta());
        if t > 2.0 {
            ctx.remove_global("toast");
            ctx.remove_global("toast_t");
        }
    }
}
