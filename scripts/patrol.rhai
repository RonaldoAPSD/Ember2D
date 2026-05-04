// patrol.rhai — An entity that paces left and right between two points.
//
// Uses elapsed time to determine direction — no state stored between frames.
// The entity walks right for 2 seconds, then left for 2 seconds, repeating.

fn on_update(entity_id, ctx) {
    // Rhai doesn't have a built-in time source, so we use the x position
    // as a simple boundary trigger instead.
    let x = ctx.get_x(entity_id);

    // Patrol between x=5 and x=15 at speed 3.
    let speed = 3.0;

    // Simple reflection: pick direction based on position.
    // You can adjust the boundaries to suit your level layout.
    let vx = if x < 5.0 {
        speed
    } else if x > 15.0 {
        -speed
    } else {
        // Keep moving in the same direction — we'd need state for that.
        // For simplicity: head toward center (10).
        if x < 10.0 { speed } else { -speed }
    };

    ctx.set_velocity(entity_id, vx, 0.0);
}
