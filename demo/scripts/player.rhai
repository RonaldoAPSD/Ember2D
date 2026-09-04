fn on_start(id, ctx) {
    ctx.play_music("demo/audio/music.ogg");
    ctx.set_global("tutorial_dismissed", false);
    ctx.set_collider_layer(id, "player");
    // Teleport is ready from the start
    ctx.set_global("tp_ready", true);
}

fn on_update(id, ctx) {
    let tutorial_dismissed = ctx.get_global("tutorial_dismissed");
    let vh = ctx.get_viewport_height();

    if !tutorial_dismissed {
        // Draw tutorial overlay
        ctx.fill_rect(5, 5, 40, 10, " ", "Black", "DarkBlue");
        ctx.draw_box(5, 5, 40, 10, "White", "DarkBlue");
        
        ctx.draw_hud(7, 7,  "WELCOME TO EMBER2D V0.4!", "Yellow", "DarkBlue");
        ctx.draw_hud(7, 9,  "WASD/Arrows to Move", "White", "DarkBlue");
        ctx.draw_hud(7, 10, "LEFT/RIGHT Click to TELEPORT (2s CD)", "Cyan", "DarkBlue");
        ctx.draw_hud(7, 12, "Collect all items [*] to unlock the door!", "Green", "DarkBlue");
        ctx.draw_hud(7, 13, "Press ENTER or Move to Start", "Grey", "DarkBlue");

        // Pause state check: if movement or enter pressed, dismiss
        let move_pressed = ctx.is_held("w") || ctx.is_held("a") || ctx.is_held("s") || ctx.is_held("d");
        let arrow_pressed = ctx.is_held("up") || ctx.is_held("down") || ctx.is_held("left") || ctx.is_held("right");
        
        if move_pressed || arrow_pressed || ctx.is_held("enter") {
            ctx.set_global("tutorial_dismissed", true);
            // clear_hud only clears the script draws, so we don't need it every frame
        }
        
        // Stop player movement while tutorial is up
        ctx.set_velocity(id, 0.0, 0.0);
        return;
    }

    // --- Active Gameplay HUD ---
    let items_left = ctx.count_by_tag("item");
    let deaths = ctx.get_persistent("deaths");
    if deaths == () { deaths = 0; }
    
    // Draw on the bottom bar
    ctx.draw_hud(45, vh - 1, "Items Left: " + items_left, "Yellow", "DarkGrey");
    ctx.draw_hud(62, vh - 1, "Total Deaths: " + deaths, "Red", "DarkGrey");

    // --- Teleport Mechanic ---
    if ctx.mouse_left_pressed() || ctx.mouse_right_pressed() {
        if ctx.get_global("tp_ready") || ctx.timer_done("tp_cd") {
             let mx = ctx.get_mouse_world_x();
             let my = ctx.get_mouse_world_y();
             
             if !ctx.is_solid_at(mx, my) {
                 ctx.set_position(id, mx, my);
                 ctx.shake_camera(2.0, 0.2);
                 ctx.play_sound("demo/audio/hurt.ogg");
                 ctx.start_timer("tp_cd", 2.0);
                 ctx.set_global("tp_ready", false);
             }
        }
    }
}
