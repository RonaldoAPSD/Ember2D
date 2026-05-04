// goblin.rhai — A simple chasing enemy script.
//
// This script runs every frame for any entity that has it attached.
// It finds the player entity and moves toward them horizontally.
//
// HOW TO ATTACH THIS SCRIPT:
//   In your .level file, add `script: Some("scripts/goblin.rhai")` to any tile.
//   The entity must also have a Collider so it can bump into things.
//
// RHAI NOTES:
//   - Variables are declared with `let` (no type annotation needed)
//   - i64 integers, f32 floats (use 3.0 not just 3)
//   - ctx.find_by_tag returns -1 if the entity is not found
//   - entity_id is passed in as i64 (the ECS EntityId cast to i64)

fn on_update(entity_id, ctx) {
    // Find the player. Returns -1 if there is no entity tagged "player".
    let player = ctx.find_by_tag("player");
    if player < 0 {
        return; // no player found — do nothing
    }

    let my_x     = ctx.get_x(entity_id);
    let player_x = ctx.get_x(player);
    let my_y     = ctx.get_y(entity_id);
    let player_y = ctx.get_y(player);

    // Chase speed in cells per second.
    let speed = 4.0;

    // Move horizontally toward the player.
    let vx = if player_x > my_x { speed } else { -speed };

    // Also move vertically toward the player (simple 8-directional chase).
    let vy = if player_y > my_y { speed } else { -speed };

    // Stop moving on an axis if close enough (half-cell dead zone).
    if (player_x - my_x).abs() < 0.5 { vx = 0.0; }
    if (player_y - my_y).abs() < 0.5 { vy = 0.0; }

    ctx.set_velocity(entity_id, vx, vy);
}
