fn on_start(id, ctx) {
    ctx.set_global("patrol_dir_" + id, 1.0);
    ctx.start_timer("patrol", 3.0);
}

fn on_update(id, ctx) {
    let speed = 5.0;
    let dir = ctx.get_global("patrol_dir_" + id);
    if dir == () { dir = 1.0; }

    if ctx.timer_done("patrol") {
        dir = -dir;
        ctx.set_global("patrol_dir_" + id, dir);
        ctx.start_timer("patrol", 3.0);
    }

    ctx.set_velocity(id, speed * dir, 0.0);

    // Spooky flicker effect
    if ctx.random_bool(0.05) {
        ctx.set_visible(id, false);
    } else {
        ctx.set_visible(id, true);
    }
}

fn on_collide(id, other, ctx) {
    if ctx.has_tag(other, "player") {
        let deaths = ctx.get_persistent("deaths");
        if deaths == () { deaths = 0; }
        ctx.set_persistent("deaths", deaths + 1);

        ctx.shake_camera(4.0, 0.3);
        ctx.play_sound("demo/audio/hurt.ogg");

        let pt = ctx.get_spawn_point("start");
        if pt.len() == 2 {
            ctx.set_position(other, pt[0], pt[1]);
        } else {
            ctx.set_position(other, 1.0, 1.0);
        }
    }
}
