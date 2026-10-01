// demos/rpg/scripts/npc.rhai — every character (and the sign) in the RPG
// demo runs this one script; its TAG says who it is: "elder", "shop",
// "inn", "sign" or "boss".
//
// The player starts a conversation by facing a character and pressing
// Space, which sets the globals "talk" (that character's id) and "talking"
// (so the player stands still). The character whose id it is then steps
// through its lines here — one `step` per dialogue box or menu, waiting for
// each to close before going on — and ends the conversation with `done`.
// One shared script with a branch per tag is how the demo shares code:
// Rhai has no modules here (master plan decision 7.4).

fn on_start(id, ctx) {
    // A beaten cyclops stays beaten when the field is entered again.
    if ctx.get_tag(id) == "boss" && boss_beaten(ctx) {
        ctx.despawn(id);
    }
}

fn on_update(id, ctx) {
    if !ctx.has_global("talk") || ctx.get_global("talk") != id { return; }
    let tag = ctx.get_tag(id);
    if tag == "elder" { elder(id, ctx); }
    else if tag == "shop" { shop(id, ctx); }
    else if tag == "inn" { inn(id, ctx); }
    else if tag == "boss" { boss(id, ctx); }
    else { sign(id, ctx); }
}

// ── Conversation plumbing ────────────────────────────────────────────────

fn step(id, ctx) {
    if ctx.has_var(id, "step") { ctx.get_var(id, "step") } else { 0 }
}

/// Shows `text` (once) and returns true once it has been read.
fn said(id, ctx, text, who) {
    if !ctx.has_var(id, "dlg") {
        ctx.set_var(id, "dlg", ctx.draw_dialogue(text, who));
        return false;
    }
    if ctx.dialogue_done(ctx.get_var(id, "dlg")) {
        ctx.remove_var(id, "dlg");
        return true;
    }
    false
}

/// Opens `items` (once) and returns the choice once made: -1 while still
/// open, -2 if cancelled with Escape.
fn chose(id, ctx, items, title) {
    if !ctx.has_var(id, "menu") {
        ctx.set_var(id, "menu", ctx.menu_open(items, #{ title: title }));
        return -1;
    }
    let m = ctx.get_var(id, "menu");
    if !ctx.menu_closed(m) { return -1; }
    let sel = ctx.menu_selection(m);
    ctx.menu_close(m);
    ctx.remove_var(id, "menu");
    if sel < 0 { -2 } else { sel }
}

fn next(id, ctx, n) {
    ctx.set_var(id, "step", n);
}

fn done(id, ctx) {
    ctx.remove_var(id, "step");
    ctx.remove_var(id, "kind");
    ctx.remove_global("talk");
    ctx.remove_global("talking");
}

/// The cyclops is beaten. Checked with `has_persistent` first: a
/// character's `on_start` can run before the player's has set up the game.
fn boss_beaten(ctx) {
    ctx.has_persistent("boss_done") && ctx.get_persistent("boss_done") == true
}

fn heal_party(ctx) {
    let party = ctx.get_persistent("party");
    for i in 0..party.len() { party[i].hp = party[i].max; }
    ctx.set_persistent("party", party);
}

// ── The characters ───────────────────────────────────────────────────────

fn sign(id, ctx) {
    if said(id, ctx, "EMBERFALL. East road: the Wild Fields. Mind the tall grass - things live in it.", "Sign") {
        done(id, ctx);
    }
}

fn elder(id, ctx) {
    // Which conversation this is is decided once, when it starts — the
    // quest changing halfway through (saying yes) must not switch it.
    if !ctx.has_var(id, "kind") {
        let kind = if boss_beaten(ctx) { "ending" }
            else if ctx.get_persistent("quest") == "accepted" { "reminder" }
            else { "plea" };
        ctx.set_var(id, "kind", kind);
        return;
    }
    let kind = ctx.get_var(id, "kind");
    let s = step(id, ctx);
    if kind == "ending" {
        if s == 0 && said(id, ctx, "You did it! The cyclops is gone and the fields are safe again. Emberfall will remember you, Ash and Lyra.", "Elder") { next(id, ctx, 1); }
        if s == 1 && said(id, ctx, "THE END - thank you for playing. (You can keep exploring, or rest at the inn.)", "") { done(id, ctx); }
        return;
    }
    if kind == "reminder" {
        if said(id, ctx, "The cyclops waits where the eastern path ends. Buy potions at the stall, and rest at the inn if you're hurt.", "Elder") { done(id, ctx); }
        return;
    }
    if s == 0 && said(id, ctx, "Ah, travellers. A cyclops has taken the Wild Fields east of town, and nobody dares go out there. Will you help us?", "Elder") { next(id, ctx, 1); }
    if s == 1 {
        let c = chose(id, ctx, ["Yes, we'll go", "Not right now"], "Help the elder?");
        if c == 0 {
            ctx.set_persistent("quest", "accepted");
            ctx.set_persistent("potions", ctx.get_persistent("potions") + 2);
            next(id, ctx, 2);
        } else if c == 1 || c == -2 {
            next(id, ctx, 3);
        }
    }
    if s == 2 && said(id, ctx, "Bless you! Take these two potions. Follow the road east - and stay out of the tall grass if you can.", "Elder") { done(id, ctx); }
    if s == 3 && said(id, ctx, "I understand. Come back if you change your mind.", "Elder") { done(id, ctx); }
}

fn shop(id, ctx) {
    let s = step(id, ctx);
    if s == 0 && said(id, ctx, "Potions! Fresh potions! Ten gold apiece.", "Shopkeeper") { next(id, ctx, 1); }
    if s == 1 {
        let c = chose(id, ctx, ["Buy a potion (10 gold)", "Leave"], "Gold: " + ctx.get_persistent("gold"));
        if c == 0 {
            if ctx.get_persistent("gold") >= 10 {
                ctx.set_persistent("gold", ctx.get_persistent("gold") - 10);
                ctx.set_persistent("potions", ctx.get_persistent("potions") + 1);
                next(id, ctx, 2);
            } else {
                next(id, ctx, 3);
            }
        } else if c == 1 || c == -2 {
            done(id, ctx);
        }
    }
    if s == 2 && said(id, ctx, "Thank you kindly! Anything else?", "Shopkeeper") { next(id, ctx, 1); }
    if s == 3 && said(id, ctx, "You're a little short, friend. Monsters in the fields carry gold, they say.", "Shopkeeper") { done(id, ctx); }
}

fn inn(id, ctx) {
    let s = step(id, ctx);
    if s == 0 && said(id, ctx, "Welcome to the Ember Inn! A warm bed is five gold.", "Innkeeper") { next(id, ctx, 1); }
    if s == 1 {
        let c = chose(id, ctx, ["Rest (5 gold)", "No thanks"], "Gold: " + ctx.get_persistent("gold"));
        if c == 0 {
            if ctx.get_persistent("gold") >= 5 {
                ctx.set_persistent("gold", ctx.get_persistent("gold") - 5);
                heal_party(ctx);
                next(id, ctx, 2);
            } else {
                next(id, ctx, 3);
            }
        } else if c == 1 || c == -2 {
            done(id, ctx);
        }
    }
    if s == 2 && said(id, ctx, "You sleep soundly. The whole party is rested and healed.", "Innkeeper") { done(id, ctx); }
    if s == 3 && said(id, ctx, "Oh dear, not enough gold. Come back when you have five.", "Innkeeper") { done(id, ctx); }
}

fn boss(id, ctx) {
    let s = step(id, ctx);
    if boss_beaten(ctx) {
        if said(id, ctx, "(The cyclops is gone. Only its footprints remain.)", "") { done(id, ctx); }
        return;
    }
    if s == 0 && said(id, ctx, "GRAAAH! Little people, in MY field?! I'll grind you into flour!", "Cyclops") {
        done(id, ctx);
        ctx.push_scene("battle", #{ data: #{ enemy: "cyclops", boss: true, boss_id: id } });
    }
}
