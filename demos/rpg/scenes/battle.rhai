// demos/rpg/scenes/battle.rhai — a turn-based battle, as a scene.
//
// Pushed by the player script (a random monster in tall grass) or the
// cyclops (`npc.rhai`), with `data` naming the enemy:
//   ctx.push_scene("battle", #{ data: #{ enemy: "slime", boss: false } })
// It pauses the map underneath (scenes do by default), puts its own
// backdrop and fighters on screen as sprites, and runs the fight:
//   each living party member picks from a menu -> what happens is shown in
//   the dialogue box -> the enemy attacks -> repeat,
// until the enemy falls (gold), the party runs, or the party falls (they
// wake up at the inn). The party itself is PERSISTENT state (`party`,
// `potions`, `gold`), so damage and spent potions carry on afterwards.

fn enemy_stats(name) {
    switch name {
        "bat" => #{ name: "Bat", region: "bat", hp: 8, atk: 3, gold: 3, size: 3 },
        "spider" => #{ name: "Spider", region: "spider", hp: 14, atk: 4, gold: 6, size: 3 },
        "ghost" => #{ name: "Ghost", region: "ghost", hp: 12, atk: 5, gold: 8, size: 3 },
        "cyclops" => #{ name: "Cyclops", region: "cyclops", hp: 70, atk: 7, gold: 60, size: 6 },
        _ => #{ name: "Slime", region: "slime", hp: 10, atk: 3, gold: 4, size: 3 },
    }
}

fn on_start(id, ctx) {
    let data = ctx.scene_data();
    let e = enemy_stats(data.enemy);
    ctx.set_var(id, "enemy", e);
    ctx.set_var(id, "boss", data.boss);
    if data.boss { ctx.set_var(id, "boss_id", data.boss_id); }
    ctx.set_var(id, "who", 0);

    // The screen is 80x24 glyph cells; a 16x16 world cell is two glyph
    // cells wide, so the visible world is half as many columns.
    let cx = ctx.get_camera_x();
    let cy = ctx.get_camera_y();
    let vw = ctx.get_viewport_width() / 2.0;
    let vh = ctx.get_viewport_height();
    let bg = ctx.spawn_entity(" ", cx, cy, "battle");
    ctx.set_sprite(bg, "extras", "battle_bg");
    ctx.set_size(bg, vw, vh);
    ctx.set_layer_order(bg, 200);
    let foe = ctx.spawn_entity("?", cx + vw * 0.60, cy + vh * 0.42 - e.size, "battle");
    ctx.set_sprite(foe, "dungeon", e.region);
    ctx.set_size(foe, e.size, e.size);
    ctx.set_layer_order(foe, 210);
    let ash = ctx.spawn_entity("@", cx + vw * 0.18, cy + vh * 0.30, "battle");
    ctx.play_project_clip(ash, "hero_walk");
    ctx.set_size(ash, 2, 2);
    ctx.set_flip(ash, true, false);
    ctx.set_layer_order(ash, 210);
    let lyra = ctx.spawn_entity("m", cx + vw * 0.12, cy + vh * 0.52, "battle");
    ctx.play_project_clip(lyra, "mage_idle");
    ctx.set_size(lyra, 2, 2);
    ctx.set_layer_order(lyra, 210);
    let fx = ctx.spawn_entity(" ", cx + vw * 0.60, cy + vh * 0.42 - e.size, "battle");
    ctx.set_layer_order(fx, 220);
    ctx.set_visible(fx, false);
    ctx.set_var(id, "foe", foe);
    ctx.set_var(id, "fx", fx);
    ctx.set_var(id, "sprites", [bg, foe, ash, lyra, fx]);
    ctx.set_var(id, "state", "intro");
}

fn on_update(id, ctx) {
    if !ctx.has_var(id, "state") { return; }
    draw_status(id, ctx);
    let state = ctx.get_var(id, "state");
    let e = ctx.get_var(id, "enemy");
    if state == "intro" {
        if said(id, ctx, if ctx.get_var(id, "boss") { "The Cyclops attacks!" } else { "A " + e.name + " jumps out of the grass!" }) {
            set_state(id, ctx, "choose");
        }
    } else if state == "choose" {
        choose(id, ctx);
    } else if state == "result" {
        if said(id, ctx, ctx.get_var(id, "text")) { after_action(id, ctx); }
    } else if state == "enemy" {
        enemy_turn(id, ctx);
    } else if state == "won" {
        if said(id, ctx, ctx.get_var(id, "text")) { finish(id, ctx); }
    } else if state == "fled" {
        if said(id, ctx, "You got away safely.") { finish(id, ctx); }
    } else if state == "lost" {
        if said(id, ctx, "The party has fallen... You wake up at the inn, with half your gold.") {
            let party = ctx.get_persistent("party");
            for i in 0..party.len() { party[i].hp = party[i].max; }
            ctx.set_persistent("party", party);
            ctx.set_persistent("gold", ctx.get_persistent("gold") / 2);
            finish(id, ctx);
            ctx.load_level("inn.level", "door");
        }
    }
}

// ── Plumbing ─────────────────────────────────────────────────────────────

fn set_state(id, ctx, state) {
    ctx.set_var(id, "state", state);
}

fn said(id, ctx, text) {
    if !ctx.has_var(id, "dlg") {
        ctx.set_var(id, "dlg", ctx.draw_dialogue(text, ""));
        return false;
    }
    if ctx.dialogue_done(ctx.get_var(id, "dlg")) {
        ctx.remove_var(id, "dlg");
        return true;
    }
    false
}

fn finish(id, ctx) {
    for s in ctx.get_var(id, "sprites") { ctx.despawn(s); }
    ctx.remove_var(id, "state");
    ctx.pop_scene();
}

fn draw_status(id, ctx) {
    // The map's own HUD stays on screen underneath, frozen with the map
    // (its status line would show stale potions, its hint "Space: talk"):
    // blank those two rows. A scene's HUD draws above the map's.
    ctx.fill_rect(0, 0, 80, 1, " ", "White", "#101828");
    ctx.fill_rect(0, 23, 80, 1, " ", "White", "#101828");
    ctx.draw_hud(1, 23, " Up/Down: choose   Enter: confirm ", "Grey", "#202028");
    let e = ctx.get_var(id, "enemy");
    ctx.draw_hud(44, 1, " " + e.name + "  HP " + e.hp + " ", "White", "#401818");
    let party = ctx.get_persistent("party");
    for i in 0..party.len() {
        let p = party[i];
        let mark = if ctx.get_var(id, "state") == "choose" && ctx.get_var(id, "who") == i { ">" } else { " " };
        ctx.draw_hud(2, 1 + i, mark + p.name + "  HP " + p.hp + "/" + p.max + " ", if p.hp > 0 { "White" } else { "DarkGrey" }, "#182040");
    }
    ctx.draw_hud(2, 4, " Potions " + ctx.get_persistent("potions") + " ", "Yellow", "#182040");
}

// ── The party's turn ─────────────────────────────────────────────────────

fn choose(id, ctx) {
    let party = ctx.get_persistent("party");
    let who = ctx.get_var(id, "who");
    if who >= party.len() { set_state(id, ctx, "enemy"); return; }
    let p = party[who];
    if p.hp <= 0 { ctx.set_var(id, "who", who + 1); return; }
    let items = if p.mag > 0 { ["Attack", "Fire spell", "Potion", "Run"] } else { ["Attack", "Potion", "Run"] };
    if !ctx.has_var(id, "menu") {
        ctx.set_var(id, "menu", ctx.menu_open(items, #{ title: p.name, x: 52, y: 13, cancelable: false }));
        return;
    }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return; }
    let pick = items[ctx.menu_selection(m)];
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    act(id, ctx, who, pick);
}

fn act(id, ctx, who, pick) {
    let party = ctx.get_persistent("party");
    let p = party[who];
    let e = ctx.get_var(id, "enemy");
    let text = "";
    if pick == "Attack" || pick == "Fire spell" {
        let dmg = if pick == "Attack" { p.atk + ctx.random_int(0, 2) } else { p.mag + ctx.random_int(1, 4) };
        e.hp = if e.hp > dmg { e.hp - dmg } else { 0 };
        ctx.set_var(id, "enemy", e);
        let fx = ctx.get_var(id, "fx");
        ctx.set_visible(fx, true);
        ctx.play_project_clip_once(fx, "slash");
        ctx.set_size(fx, e.size, e.size);
        text = if pick == "Attack" { p.name + " strikes the " + e.name + " for " + dmg + "!" } else { p.name + " casts Fire! The " + e.name + " takes " + dmg + "." };
    } else if pick == "Potion" {
        let potions = ctx.get_persistent("potions");
        if potions > 0 {
            ctx.set_persistent("potions", potions - 1);
            let heal = if p.max - p.hp < 12 { p.max - p.hp } else { 12 };
            party[who].hp = p.hp + heal;
            ctx.set_persistent("party", party);
            text = p.name + " drinks a potion and recovers " + heal + " HP.";
        } else {
            text = "No potions left!";
        }
    } else {
        if ctx.get_var(id, "boss") {
            text = "There's no running from the Cyclops!";
        } else if ctx.random_int(1, 2) == 1 {
            set_state(id, ctx, "fled");
            return;
        } else {
            text = "Couldn't get away!";
        }
    }
    ctx.set_var(id, "text", text);
    set_state(id, ctx, "result");
}

fn after_action(id, ctx) {
    ctx.set_visible(ctx.get_var(id, "fx"), false);
    let e = ctx.get_var(id, "enemy");
    if e.hp <= 0 {
        ctx.set_persistent("gold", ctx.get_persistent("gold") + e.gold);
        ctx.set_visible(ctx.get_var(id, "foe"), false);
        if ctx.get_var(id, "boss") {
            ctx.set_persistent("boss_done", true);
            ctx.despawn(ctx.get_var(id, "boss_id"));
            ctx.set_var(id, "text", "The Cyclops crashes to the ground! The fields are safe. (+" + e.gold + " gold) Go tell the elder!");
        } else {
            ctx.set_var(id, "text", "The " + e.name + " is defeated! You find " + e.gold + " gold.");
        }
        set_state(id, ctx, "won");
        return;
    }
    ctx.set_var(id, "who", ctx.get_var(id, "who") + 1);
    set_state(id, ctx, "choose");
}

// ── The enemy's turn ─────────────────────────────────────────────────────

fn enemy_turn(id, ctx) {
    if ctx.has_var(id, "dlg") {
        if said(id, ctx, "") {
            let party = ctx.get_persistent("party");
            let alive = 0;
            for p in party { if p.hp > 0 { alive += 1; } }
            ctx.set_var(id, "who", 0);
            set_state(id, ctx, if alive == 0 { "lost" } else { "choose" });
        }
        return;
    }
    let party = ctx.get_persistent("party");
    let living = [];
    for i in 0..party.len() { if party[i].hp > 0 { living.push(i); } }
    let t = ctx.random_choice(living);
    let e = ctx.get_var(id, "enemy");
    let dmg = e.atk + ctx.random_int(-1, 1) - party[t].def;
    if dmg < 1 { dmg = 1; }
    party[t].hp = if party[t].hp > dmg { party[t].hp - dmg } else { 0 };
    ctx.set_persistent("party", party);
    ctx.shake_camera(0.3, 0.25);
    ctx.set_var(id, "dlg", ctx.draw_dialogue("The " + e.name + " hits " + party[t].name + " for " + dmg + "!", ""));
}
