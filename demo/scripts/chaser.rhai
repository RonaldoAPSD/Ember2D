fn on_update(id, ctx) {
    let player = ctx.find_by_tag("player");
    if player < 0 { return; }

    let dist = ctx.get_distance(id, player);
    
    if dist < 8.0 && dist > 0.5 {
        let dx = ctx.get_x(player) - ctx.get_x(id);
        let dy = ctx.get_y(player) - ctx.get_y(id);
        let spd = 3.0;
        ctx.set_velocity(id, dx / dist * spd, dy / dist * spd);
    } else {
        ctx.set_velocity(id, 0.0, 0.0);
    }
}

fn on_collide(id, other, ctx) {
    if ctx.has_tag(other, "player") {
        // Increment persistent deaths
        let deaths = ctx.get_persistent("deaths");
        if deaths == () { deaths = 0; }
        ctx.set_persistent("deaths", deaths + 1);

        // Impact effect
        ctx.shake_camera(5.0, 0.4);
        ctx.play_sound("demo/audio/hurt.ogg");

        // Respawn player
        let pt = ctx.get_spawn_point("start");
        if pt.len() == 2 {
            ctx.set_position(other, pt[0], pt[1]);
        } else {
            ctx.set_position(other, 1.0, 1.0);
        }
    }
}
