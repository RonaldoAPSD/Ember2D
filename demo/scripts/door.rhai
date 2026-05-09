fn on_update(id, ctx) {
    let items_left = ctx.count_by_tag("item");
    
    if items_left > 0 {
        ctx.set_visible(id, false);
        ctx.set_collider_layer(id, "locked");
    } else {
        ctx.set_visible(id, true);
        ctx.set_collider_layer(id, "exit");
        
        // Highlight effect when unlocked
        if (ctx.get_elapsed() * 4.0) % 2.0 < 1.0 {
            ctx.set_color(id, "Yellow", "Reset");
        } else {
            ctx.set_color(id, "White", "Reset");
        }
    }
}
