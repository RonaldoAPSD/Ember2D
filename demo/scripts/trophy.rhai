fn on_start(id, ctx) {
    ctx.play_music("demo/audio/victory.ogg");
}

fn on_update(id, ctx) {
    let deaths = ctx.get_persistent("deaths");
    if deaths == () { deaths = 0; }

    // Clear the script draws to prevent overlapping with tutorial/other UI
    ctx.clear_hud();

    // Full screen overlay
    ctx.fill_rect(0, 0, 80, 40, " ", "Black", "Black");
    ctx.fill_rect(10, 5, 60, 15, " ", "Black", "DarkBlue");
    ctx.draw_box(10, 5, 60, 15, "Yellow", "DarkBlue");

    let messages = [
        "LEGENDARY RUN!",
        "Dungeon Mastered!",
        "Victory is Yours!"
    ];
    let msg = ctx.random_choice(messages);

    ctx.draw_hud(30, 8,  msg, "Yellow", "DarkBlue");
    ctx.draw_hud(25, 10, "You escaped The Lost Dungeon!", "White", "DarkBlue");
    ctx.draw_hud(25, 12, "Total Deaths in this Run: " + deaths, "Red", "DarkBlue");
    
    ctx.draw_hud(25, 16, "Press ESC to return to Editor", "Grey", "DarkBlue");
}
