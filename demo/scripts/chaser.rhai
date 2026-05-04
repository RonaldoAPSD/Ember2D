fn on_update(id, ctx) {
    let player = ctx.find_by_tag("player");
    if player < 0 { return; }
    let dx = ctx.get_x(player) - ctx.get_x(id);
    let dy = ctx.get_y(player) - ctx.get_y(id);
    let dist = (dx * dx + dy * dy).sqrt();
    if dist > 0.5 {
        let spd = 3.0;
        ctx.set_velocity(id, dx / dist * spd, dy / dist * spd);
    }
}
fn on_collide(id, other, ctx) {
    if ctx.has_tag(other, "player") {
        let pt = ctx.get_spawn_point("start");
        if pt.len() == 2 {
            ctx.set_position(other, pt[0], pt[1]);
        } else {
            ctx.set_position(other, 1.0, 1.0);
        }
        ctx.play_sound("demo/audio/hurt.ogg");
    }
}
