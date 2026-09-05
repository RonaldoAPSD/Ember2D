fn on_update(me, ctx) {
    // 30% chance per frame to emit a particle
    if ctx.random_bool(0.3) {
        let x = ctx.get_x(me);
        let y = ctx.get_y(me);
        // Random colors
        let colors = ["Cyan", "White", "Yellow", "Green"];
        let col = ctx.random_choice(colors);
        ctx.emit_particles(x, y, "*", col);
    }
}
