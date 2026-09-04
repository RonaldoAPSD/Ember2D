fn on_start(id, ctx) {
    ctx.log("AI Test script started");
    ctx.set_collider_layer(id, "enemy");
}

fn on_update(id, ctx) {
    let player = ctx.find_by_tag("player");
    if player == -1 { 
        ctx.draw_hud(2, 2, "AI Error: Player not found!", "Red", "Reset");
        return; 
    }

    let px = ctx.get_x(player);
    let py = ctx.get_y(player);
    let my_x = ctx.get_x(id);
    let my_y = ctx.get_y(id);

    // 1. Raycast test
    let hit = ctx.raycast(my_x, my_y, px, py, ["wall"]);
    if hit.len() > 0 {
        ctx.draw_hud(2, 2, "Raycast: Wall blocked LOS!", "Red", "Reset");
    } else {
        ctx.draw_hud(2, 2, "Raycast: Clear LOS to Player", "Green", "Reset");
    }

    // 2. Pathfinding test
    // We search for a path that avoids "wall" layer entities.
    let path = ctx.get_path(my_x, my_y, px, py, ["wall"]);
    
    if path.len() > 0 {
        ctx.draw_hud(2, 3, "Path found! Steps: " + path.len(), "Cyan", "Reset");
        
        // Move towards the first point in the path.
        let next_point = path[0];
        let target_x = next_point[0];
        let target_y = next_point[1];

        let dx = target_x - my_x;
        let dy = target_y - my_y;

        let mag = (dx*dx + dy*dy).sqrt();
        if mag > 0.1 {
            ctx.set_velocity(id, (dx/mag) * 2.0, (dy/mag) * 2.0);
        } else {
            ctx.set_velocity(id, 0.0, 0.0);
        }
    } else {
        ctx.draw_hud(2, 3, "No path to player", "Red", "Reset");
        ctx.set_velocity(id, 0.0, 0.0);
    }
}
