fn on_start(me, ctx) {
    // Animation frames: * (small), + (medium), # (large).
    // set_animation was removed in Step 3e — a named, looping clip plus
    // play_clip is the replacement (fps = 1 / old per-frame rate).
    ctx.register_clip("torch_flicker", "*+#", 1.0 / 0.15, true);
    ctx.play_clip(me, "torch_flicker");
    ctx.set_tint(me, "Yellow", "Reset");
}

fn on_update(me, ctx) {
    // Occasional flicker particles
    if ctx.random_bool(0.05) {
        ctx.emit_particles(ctx.get_x(me), ctx.get_y(me), ".", "Red");
    }
}
