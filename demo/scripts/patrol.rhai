fn on_update(id, ctx) {
    let t = ctx.get_elapsed();
    let speed = 5.0;
    if (t % 6.0) < 3.0 {
        ctx.set_velocity(id, speed, 0.0);
    } else {
        ctx.set_velocity(id, -speed, 0.0);
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
