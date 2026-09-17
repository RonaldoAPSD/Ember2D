// world_tests.rs — World unit tests.
//
// Split out of world.rs (via #[path] in that file's own `mod tests;`
// declaration) at Step 7.5-9 (docs/ember2d-master-plan.md §5.6) — world.rs
// was at 742/750 lines (CLAUDE.md's hard limit) before this step's own
// LevelSource/Diagnostic/set_parent/despawn changes, which would have
// pushed it over. Same second-file-via-`#[path]` pattern the scripting
// module already uses repeatedly (engine_tests.rs, timer_tests.rs, etc.) —
// pure relocation for everything below `hierarchy_tests` in this file;
// nothing about the pre-existing tests changed, only location.

use super::*;
use crate::components::Animator;

#[test]
fn a_world_with_animators_round_trips_through_ron() {
    let mut world = World::new();
    let id = world.spawn();
    world.animators.insert(id, Animator::new("flicker"));

    let ron = ron::to_string(&world).expect("World must serialize");
    let restored: World = ron::from_str(&ron).expect("World must deserialize");
    assert_eq!(restored.animators.get(&id).map(|a| a.clip.as_str()), Some("flicker"));
}

#[test]
fn a_saved_world_from_before_the_animators_store_existed_still_loads() {
    // Step 3c added `animators` to an already-shipped serialized type;
    // #[serde(default)] is what keeps an old save (missing the field
    // entirely) loading instead of erroring out.
    let pre_step_3c_ron = "(next_id:1,transforms:{},sprites:{},colliders:{},tags:{},scripts:{})";
    let restored: World = ron::from_str(pre_step_3c_ron)
        .expect("a World RON with no `animators` key must still deserialize");
    assert!(restored.animators.is_empty());
}

#[test]
fn a_world_with_vars_round_trips_through_ron() {
    let mut world = World::new();
    let id = world.spawn();
    world.vars.insert(
        id,
        Vars { values: BTreeMap::from([("hp".to_string(), rhai::Dynamic::from(6_i64))]) },
    );

    let ron = ron::to_string(&world).expect("World must serialize");
    let restored: World = ron::from_str(&ron).expect("World must deserialize");
    assert_eq!(
        restored.vars.get(&id).and_then(|v| v.values.get("hp")).and_then(|d| d.as_int().ok()),
        Some(6)
    );
}

#[test]
fn a_saved_world_from_before_the_vars_store_existed_still_loads() {
    // Step 7.5-3 added `vars` to an already-shipped serialized type;
    // #[serde(default)] is what keeps an old save (missing the field
    // entirely) loading instead of erroring out — same convention
    // `animators`'s own test above pins.
    let pre_step_7_5_3_ron = "(next_id:1,transforms:{},sprites:{},colliders:{},tags:{},\
         scripts:{},animators:{},actors:{})";
    let restored: World = ron::from_str(pre_step_7_5_3_ron)
        .expect("a World RON with no `vars` key must still deserialize");
    assert!(restored.vars.is_empty());
}

#[test]
fn despawn_removes_the_entitys_animator() {
    let mut world = World::new();
    let id = world.spawn();
    world.animators.insert(id, Animator::new("flicker"));
    world.despawn(id);
    assert!(
        !world.animators.contains_key(&id),
        "despawn must clean up the animators store like every other component store"
    );
}

#[test]
fn despawn_removes_the_entitys_actor() {
    let mut world = World::new();
    let id = world.spawn();
    world.add_actor(id, crate::components::Actor::ai(100));
    world.despawn(id);
    assert!(
        !world.actors.contains_key(&id),
        "despawn must clean up the actors store like every other component store"
    );
}

#[test]
fn despawn_removes_the_entitys_vars() {
    let mut world = World::new();
    let id = world.spawn();
    world.add_vars(id, Vars::default());
    world.despawn(id);
    assert!(
        !world.vars.contains_key(&id),
        "despawn must clean up the vars store like every other component store"
    );
}

// ── Tests: Step 5b deterministic iteration (docs/ember2d-phase5-plan.md
// §5.2 H1) — component stores are BTreeMap, not HashMap, specifically so
// iteration order is a property of the type and can't regress back to
// HashMap's per-process randomness by accident. ─────────────────────────

#[test]
fn component_store_iteration_is_sorted_by_entity_id_regardless_of_insertion_order() {
    let mut world = World::new();
    // Insert out of order — a HashMap would happily accept this and
    // still iterate in its own (unspecified, per-process-random) order;
    // a BTreeMap must always yield ascending key order regardless.
    let ids: Vec<EntityId> = [30u64, 10, 20].iter().map(|&x| x).collect();
    for &id in &ids {
        world.transforms.insert(id, crate::components::Transform::new(0.0, 0.0));
        world.colliders.insert(id, crate::components::Collider::unit());
        world.tags.insert(id, crate::components::Tag::new("thing"));
    }

    let observed: Vec<EntityId> = world.transforms.keys().copied().collect();
    assert_eq!(observed, vec![10, 20, 30], "transforms must iterate in ascending EntityId order");
    let observed: Vec<EntityId> = world.colliders.keys().copied().collect();
    assert_eq!(observed, vec![10, 20, 30], "colliders must iterate in ascending EntityId order");
    let observed: Vec<EntityId> = world.tags.keys().copied().collect();
    assert_eq!(observed, vec![10, 20, 30], "tags must iterate in ascending EntityId order");
}

#[test]
fn find_by_tag_deterministically_returns_the_lowest_id_when_multiple_entities_share_a_tag() {
    let mut world = World::new();
    // Insert the higher id first — if find_by_tag were still driven by
    // HashMap iteration order, insertion order (or process hash state)
    // could change which one comes back.
    world.tags.insert(50, crate::components::Tag::new("enemy"));
    world.tags.insert(5, crate::components::Tag::new("enemy"));
    world.tags.insert(25, crate::components::Tag::new("enemy"));
    assert_eq!(
        world.find_by_tag("enemy"),
        Some(5),
        "find_by_tag must deterministically return the lowest EntityId sharing the tag"
    );
}

#[test]
fn entity_ids_is_sorted_and_deduplicated_across_stores() {
    let mut world = World::new();
    world.transforms.insert(3, crate::components::Transform::new(0.0, 0.0));
    world.sprites.insert(1, crate::components::Sprite::simple('@', crate::color::Color::White));
    world.colliders.insert(2, crate::components::Collider::unit());
    // 3 appears in both transforms and tags — entity_ids must not list it twice.
    world.tags.insert(3, crate::components::Tag::new("dup"));

    assert_eq!(world.entity_ids(), vec![1, 2, 3]);
}

#[test]
fn entity_ids_includes_entities_whose_only_component_is_a_script_animator_or_actor() {
    // Phase 6 Step 11 (docs/ember2d-phase6-plan.md): entity_ids() used
    // to union only transforms/sprites/colliders/tags — an entity with
    // nothing but a Script, Animator, or Actor component was silently
    // invisible to it. Pins the fix directly rather than trusting the
    // union list by inspection alone.
    let mut world = World::new();
    let script_only = world.spawn();
    world.add_script(script_only, crate::components::Script::new("x.rhai"));
    let animator_only = world.spawn();
    world.add_animator(animator_only, crate::components::Animator::new("clip"));
    let actor_only = world.spawn();
    world.add_actor(actor_only, crate::components::Actor::ai(100));
    let vars_only = world.spawn();
    world.add_vars(vars_only, Vars::default());

    let ids = world.entity_ids();
    assert!(ids.contains(&script_only), "an entity with only a Script component must be listed");
    assert!(
        ids.contains(&animator_only),
        "an entity with only an Animator component must be listed"
    );
    assert!(ids.contains(&actor_only), "an entity with only an Actor component must be listed");
    assert!(ids.contains(&vars_only), "an entity with only a Vars component must be listed");
}

// ── Tests: Step 7.5-9 (docs/ember2d-master-plan.md §5.6) — set_parent
// rejects a cycle up front, despawn clears children's parent, and
// get_global_position's safety net still fires for data that bypasses
// set_parent entirely ─────────────────────────────────────────────────────

#[test]
fn set_parent_rejects_a_direct_cycle() {
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();
    world.add_transform(a, crate::components::Transform::new(0.0, 0.0));
    world.add_transform(b, crate::components::Transform::new(1.0, 1.0));

    world.set_parent(b, Some(a), false);
    world.set_parent(a, Some(b), false); // would close a 2-entity loop

    assert_eq!(
        world.transforms[&a].parent, None,
        "a reparent that would create a cycle must be rejected as a no-op"
    );
}

#[test]
fn set_parent_rejects_an_indirect_cycle_through_a_longer_chain() {
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();
    let c = world.spawn();
    for id in [a, b, c] {
        world.add_transform(id, crate::components::Transform::new(0.0, 0.0));
    }
    world.set_parent(b, Some(a), false); // a <- b
    world.set_parent(c, Some(b), false); // a <- b <- c
    world.set_parent(a, Some(c), false); // would close a 3-entity loop

    assert_eq!(
        world.transforms[&a].parent, None,
        "a reparent that would close a longer cycle must also be rejected"
    );
}

#[test]
fn set_parent_rejects_an_entity_being_its_own_parent() {
    let mut world = World::new();
    let a = world.spawn();
    world.add_transform(a, crate::components::Transform::new(0.0, 0.0));
    world.set_parent(a, Some(a), false);
    assert_eq!(world.transforms[&a].parent, None, "an entity can never be its own parent");
}

#[test]
fn despawn_clears_a_childs_parent_link_and_preserves_its_world_position() {
    let mut world = World::new();
    let parent = world.spawn();
    let child = world.spawn();
    world.add_transform(parent, crate::components::Transform::new(10.0, 5.0));
    world.add_transform(child, crate::components::Transform::new(1.0, 1.0));
    world.set_parent(child, Some(parent), false);

    // Child's world position right now: parent's (10, 5) + child's own
    // local (1, 1) = (11, 6).
    let world_pos_before = world.get_global_position(child);
    assert_eq!(world_pos_before, crate::math::Vec2::new(11.0, 6.0));

    world.despawn(parent);

    let tf = world.transforms.get(&child).expect("the child itself must survive its parent's despawn");
    assert_eq!(tf.parent, None, "despawning the parent must clear the child's dangling parent link");
    assert_eq!(
        tf.position, world_pos_before,
        "the child must keep its own world position, not jump once its parent's own \
         contribution disappears"
    );
}

#[test]
fn get_global_position_diagnostic_surfaces_for_a_cycle_that_bypasses_set_parent() {
    // set_parent itself can no longer create a cycle (the tests above) —
    // this constructs one by writing `Transform.parent` directly, the way
    // hand-edited save/level data could, to confirm the safety net still
    // catches THAT case and records a Diagnostic instead of eprintln-ing.
    let mut world = World::new();
    let a = world.spawn();
    let b = world.spawn();
    let mut ta = crate::components::Transform::new(0.0, 0.0);
    ta.parent = Some(b);
    let mut tb = crate::components::Transform::new(0.0, 0.0);
    tb.parent = Some(a);
    world.add_transform(a, ta);
    world.add_transform(b, tb);

    let _ = world.get_global_position(a);

    let messages: Vec<String> =
        world.diagnostics.borrow().iter().map(|d| d.message.clone()).collect();
    assert!(
        messages.iter().any(|m| m.contains("cycle")),
        "a hierarchy cycle bypassing set_parent must still be recorded as a Diagnostic, got: {messages:?}"
    );
}
