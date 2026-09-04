fn on_start(me, ctx) {
    // Animation frames: * (small), + (medium), # (large)
    ctx.set_animation(me, "*+#", 0.15);
    ctx.set_color(me, "Yellow", "Reset");
}

fn on_update(me, ctx) {
    // Occasional flicker particles
    if ctx.random_bool(0.05) {
        ctx.emit_particles(ctx.get_x(me), ctx.get_y(me), ".", "Red");
    }
}
