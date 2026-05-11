fn on_start(id, ctx) {
    ctx.log("2D Sprite Demo Started!");
}

fn on_update(id, ctx) {
    if ctx.just_pressed("space") {
        ctx.log("SPACE PRESSED! Changing glyph to 'S' and clearing texture.");
        ctx.set_glyph(id, "S");
        ctx.set_texture(id, "");
    }
}
