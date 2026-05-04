fn on_start(id, ctx) {
    ctx.play_music("demo/audio/music.ogg");
}
fn on_update(id, ctx) {
    ctx.draw_hud(1, 1, "Collect all items [ * ] and reach the exit [ + ]", "Yellow", "Reset");
    ctx.draw_hud(1, 2, "Avoid the guards — they send you back to start!", "Red", "Reset");
}
