fn on_start(id, ctx) {
    ctx.play_music("demo/audio/victory.ogg");
}
fn on_update(id, ctx) {
    ctx.draw_hud(8, 5,  "  *  *  *  *  *  *  *  *  *  *  *  ", "Yellow", "Reset");
    ctx.draw_hud(8, 6,  "           YOU  WIN!                 ", "White",  "DarkBlue");
    ctx.draw_hud(8, 7,  "  *  *  *  *  *  *  *  *  *  *  *  ", "Yellow", "Reset");
    ctx.draw_hud(8, 9,  "  You found all the treasure and     ", "Grey",   "Reset");
    ctx.draw_hud(8, 10, "  escaped The Lost Dungeon.          ", "Grey",   "Reset");
    ctx.draw_hud(8, 12, "  Press Esc to return to the editor. ", "DarkGrey", "Reset");
}
