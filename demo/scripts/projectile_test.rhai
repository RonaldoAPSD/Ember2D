fn on_start(id, ctx) {
    ctx.set_collider_layer(id, "projectile");
    // Only collide with "enemy" (not walls or items)
    ctx.set_collider_mask(id, ["enemy"]);
    ctx.set_velocity(id, -5.0, 0.0);
}

fn on_collide(id, other, ctx) {
    let tag = ctx.get_tag(other);
    ctx.log("Projectile hit " + tag);
    ctx.despawn(id);
}
