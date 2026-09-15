// scripting/registry.rs — the Rhai function-registration table (Rhai name
// -> ScriptCtx method) for `ScriptEngine::new`.
//
// Split out of engine.rs at 7A-10 (docs/ember2d-master-plan.md §5.1, R43):
// 7A-9's `cargo fmt --all` alone (no logic change) pushed engine.rs to
// 790/750 lines (CLAUDE.md's hard limit). This ~140-line sequence is pure
// mechanical wiring, the lowest-risk piece to pull into its own file — same
// calls, same order, same section comments, nothing renamed or resequenced.

use rhai::Engine;

use super::api::ScriptCtx;

// 7.5-1 (docs/ember2d-master-plan.md §5.6, R31): Rhai dispatches to a
// registered function by EXACT argument type — it never coerces between
// `i64` and `f64` the way a script author might expect from a dynamic
// language, so a function registered with an `f64` parameter rejects an
// integer-literal call (`set_position(id, 5, 5)`) with a plain "function
// not found," and vice versa. Every coordinate/size/layer-order function
// below is registered TWICE under the same Rhai name — once for its
// original type, once for a small wrapper (suffixed `_i`/`_f` in
// `ScriptCtx`'s own Rust name only; the registered Rhai name is identical)
// that casts every numeric argument to the other type and calls straight
// through. Rhai's overload resolution picks whichever registration matches
// the call's actual argument types, the same mechanism `spawn_entity`/
// `spawn_entity_full` already used to pick an overload by ARITY. A script
// must still write a single call's numeric literals in ONE consistent
// style (all int or all float) — this doesn't accept freely mixed types
// within one call, only either style consistently.
pub(super) fn register_all(engine: &mut Engine) {
    engine.register_type_with_name::<ScriptCtx>("Ctx");
    engine.register_fn("get_x", ScriptCtx::get_x);
    engine.register_fn("get_y", ScriptCtx::get_y);
    engine.register_fn("get_position", ScriptCtx::get_position);
    engine.register_fn("get_vel_x", ScriptCtx::get_vel_x);
    engine.register_fn("get_vel_y", ScriptCtx::get_vel_y);
    engine.register_fn("get_velocity", ScriptCtx::get_velocity);
    engine.register_fn("get_tag", ScriptCtx::get_tag);
    engine.register_fn("set_tag", ScriptCtx::set_tag);
    engine.register_fn("has_tag", ScriptCtx::has_tag);
    engine.register_fn("get_glyph", ScriptCtx::get_glyph);
    engine.register_fn("get_color", ScriptCtx::get_color);
    engine.register_fn("get_texture", ScriptCtx::get_texture);
    engine.register_fn("find_by_tag", ScriptCtx::find_by_tag);
    engine.register_fn("find_all_by_tag", ScriptCtx::find_all_by_tag);
    engine.register_fn("is_held", ScriptCtx::is_held);
    engine.register_fn("just_pressed", ScriptCtx::just_pressed);
    engine.register_fn("get_spawn_point", ScriptCtx::get_spawn_point);
    engine.register_fn("get_delta", ScriptCtx::get_delta);
    engine.register_fn("get_elapsed", ScriptCtx::get_elapsed);
    engine.register_fn("set_velocity", ScriptCtx::set_velocity);
    engine.register_fn("set_velocity", ScriptCtx::set_velocity_i);
    engine.register_fn("set_position", ScriptCtx::set_position);
    engine.register_fn("set_position", ScriptCtx::set_position_i);
    engine.register_fn("set_glyph", ScriptCtx::set_glyph);
    engine.register_fn("set_tint", ScriptCtx::set_tint);
    engine.register_fn("set_texture", ScriptCtx::set_texture);
    engine.register_fn("despawn", ScriptCtx::despawn);
    engine.register_fn("spawn_entity", ScriptCtx::spawn_entity);
    engine.register_fn("spawn_entity", ScriptCtx::spawn_entity_i);
    engine.register_fn("spawn_entity", ScriptCtx::spawn_entity_full);
    engine.register_fn("spawn_entity", ScriptCtx::spawn_entity_full_i);
    engine.register_fn("load_level", ScriptCtx::load_level);
    engine.register_fn("log", ScriptCtx::log);
    engine.register_fn("draw_hud", ScriptCtx::draw_hud);
    engine.register_fn("draw_hud", ScriptCtx::draw_hud_f);
    engine.register_fn("draw_menu", ScriptCtx::draw_menu);
    engine.register_fn("draw_menu", ScriptCtx::draw_menu_f);
    engine.register_fn("draw_panel", ScriptCtx::draw_panel);
    engine.register_fn("draw_panel", ScriptCtx::draw_panel_f);
    engine.register_fn("play_sound", ScriptCtx::play_sound);
    engine.register_fn("play_sound_at", ScriptCtx::play_sound_at);
    engine.register_fn("play_sound_at", ScriptCtx::play_sound_at_i);
    engine.register_fn("play_music", ScriptCtx::play_music);
    engine.register_fn("stop_music", ScriptCtx::stop_music);
    engine.register_fn("emit_particles", ScriptCtx::emit_particles);
    engine.register_fn("emit_particles", ScriptCtx::emit_particles_i);
    engine.register_fn("set_global", ScriptCtx::set_global);
    engine.register_fn("get_global", ScriptCtx::get_global);
    engine.register_fn("has_global", ScriptCtx::has_global);
    engine.register_fn("remove_global", ScriptCtx::remove_global);
    engine.register_fn("add_global", ScriptCtx::add_global);
    engine.register_fn("add_global", ScriptCtx::add_global_i);
    engine.register_fn("random_int", ScriptCtx::random_int);
    engine.register_fn("random_float", ScriptCtx::random_float);
    engine.register_fn("random_bool", ScriptCtx::random_bool);
    engine.register_fn("random_choice", ScriptCtx::random_choice);
    engine.register_fn("get_entity_at", ScriptCtx::get_entity_at);
    engine.register_fn("get_entity_at", ScriptCtx::get_entity_at_i);
    engine.register_fn("is_solid_at", ScriptCtx::is_solid_at);
    engine.register_fn("is_solid_at", ScriptCtx::is_solid_at_i);
    engine.register_fn("find_entities_in_rect", ScriptCtx::find_entities_in_rect);
    engine.register_fn("find_entities_in_rect", ScriptCtx::find_entities_in_rect_i);
    engine.register_fn("get_distance", ScriptCtx::get_distance);
    engine.register_fn("get_angle_to", ScriptCtx::get_angle_to);
    engine.register_fn("entity_exists", ScriptCtx::entity_exists);
    engine.register_fn("count_by_tag", ScriptCtx::count_by_tag);
    engine.register_fn("get_collider_w", ScriptCtx::get_collider_w);
    engine.register_fn("get_collider_h", ScriptCtx::get_collider_h);
    engine.register_fn("set_collider_size", ScriptCtx::set_collider_size);
    engine.register_fn("set_collider_size", ScriptCtx::set_collider_size_i);
    engine.register_fn("is_collider_solid", ScriptCtx::is_collider_solid);
    engine.register_fn("set_collider_solid", ScriptCtx::set_collider_solid);
    engine.register_fn("is_visible", ScriptCtx::is_visible);
    engine.register_fn("set_visible", ScriptCtx::set_visible);
    engine.register_fn("get_layer_order", ScriptCtx::get_layer_order);
    engine.register_fn("set_layer_order", ScriptCtx::set_layer_order);
    engine.register_fn("set_layer_order", ScriptCtx::set_layer_order_f);
    engine.register_fn("get_mouse_x", ScriptCtx::get_mouse_x);
    engine.register_fn("get_mouse_y", ScriptCtx::get_mouse_y);
    engine.register_fn("mouse_left_pressed", ScriptCtx::mouse_left_pressed);
    engine.register_fn("mouse_right_pressed", ScriptCtx::mouse_right_pressed);
    engine.register_fn("mouse_left_held", ScriptCtx::mouse_left_held);
    engine.register_fn("mouse_right_held", ScriptCtx::mouse_right_held);
    engine.register_fn("get_mouse_world_x", ScriptCtx::get_mouse_world_x);
    engine.register_fn("get_mouse_world_y", ScriptCtx::get_mouse_world_y);
    engine.register_fn("get_camera_x", ScriptCtx::get_camera_x);
    engine.register_fn("get_camera_y", ScriptCtx::get_camera_y);
    engine.register_fn("set_camera", ScriptCtx::set_camera);
    engine.register_fn("set_camera", ScriptCtx::set_camera_i);
    engine.register_fn("shake_camera", ScriptCtx::shake_camera);
    engine.register_fn("set_persistent", ScriptCtx::set_persistent);
    engine.register_fn("get_persistent", ScriptCtx::get_persistent);
    engine.register_fn("has_persistent", ScriptCtx::has_persistent);
    engine.register_fn("clear_persistent", ScriptCtx::clear_persistent);
    engine.register_fn("clear_all_persistent", ScriptCtx::clear_all_persistent);
    engine.register_fn("add_persistent", ScriptCtx::add_persistent);
    engine.register_fn("add_persistent", ScriptCtx::add_persistent_i);
    engine.register_fn("draw_box", ScriptCtx::draw_box);
    engine.register_fn("draw_box", ScriptCtx::draw_box_f);
    engine.register_fn("fill_rect", ScriptCtx::fill_rect);
    engine.register_fn("fill_rect", ScriptCtx::fill_rect_f);
    engine.register_fn("clear_hud", ScriptCtx::clear_hud);
    engine.register_fn("save_game", ScriptCtx::save_game);
    engine.register_fn("load_game", ScriptCtx::load_game);
    engine.register_fn("get_collider_layer", ScriptCtx::get_collider_layer);
    engine.register_fn("set_collider_layer", ScriptCtx::set_collider_layer);
    engine.register_fn("is_collider_locked", ScriptCtx::is_collider_locked);
    engine.register_fn("set_collider_locked", ScriptCtx::set_collider_locked);
    engine.register_fn("get_collider_mask", ScriptCtx::get_collider_mask);
    engine.register_fn("set_collider_mask", ScriptCtx::set_collider_mask);
    engine.register_fn("raycast", ScriptCtx::raycast);
    engine.register_fn("raycast", ScriptCtx::raycast_i);
    engine.register_fn("get_path", ScriptCtx::get_path);
    engine.register_fn("get_path", ScriptCtx::get_path_i);
    engine.register_fn("get_viewport_width", ScriptCtx::get_viewport_width);
    engine.register_fn("get_viewport_height", ScriptCtx::get_viewport_height);
    engine.register_fn("start_timer", ScriptCtx::start_timer);
    engine.register_fn("timer_done", ScriptCtx::timer_done);
    engine.register_fn("cancel_timer", ScriptCtx::cancel_timer);
    engine.register_fn("set_var", ScriptCtx::set_var);
    engine.register_fn("get_var", ScriptCtx::get_var);
    engine.register_fn("has_var", ScriptCtx::has_var);
    engine.register_fn("remove_var", ScriptCtx::remove_var);
    engine.register_fn("add_var", ScriptCtx::add_var);
    engine.register_fn("add_var", ScriptCtx::add_var_i);
    engine.register_fn("get_parent", ScriptCtx::get_parent);
    engine.register_fn("set_parent", ScriptCtx::set_parent);
    engine.register_fn("set_parent_keep_world", ScriptCtx::set_parent_keep_world);
    engine.register_fn("get_world_x", ScriptCtx::get_world_x);
    engine.register_fn("get_world_y", ScriptCtx::get_world_y);

    // V0.5 Gamepad Extensions
    engine.register_fn("gp_is_held", ScriptCtx::gp_is_held);
    engine.register_fn("gp_just_pressed", ScriptCtx::gp_just_pressed);
    engine.register_fn("gp_axis", ScriptCtx::gp_axis);

    // Phase 3: named animation clips
    engine.register_fn("register_clip", ScriptCtx::register_clip);
    engine.register_fn("play_clip", ScriptCtx::play_clip);
    engine.register_fn("play_clip_once", ScriptCtx::play_clip_once);
    engine.register_fn("stop_clip", ScriptCtx::stop_clip);
    engine.register_fn("set_clip_speed", ScriptCtx::set_clip_speed);
    engine.register_fn("get_frame", ScriptCtx::get_frame);
    engine.register_fn("set_frame", ScriptCtx::set_frame);
    engine.register_fn("clip_finished", ScriptCtx::clip_finished);

    // Phase 3 Step 3e: lets a script (or its author) detect which
    // breaking-change generation of the API it's running against.
    engine.register_fn("api_version", ScriptCtx::api_version);

    // Step 5e: the command boundary (docs/ember2d-phase5-plan.md) —
    // `submit` is meaningful only inside `on_input`; `command_action`/
    // `command_param` read back whatever the entity's own `on_input`
    // pass queued for it, once `on_update` runs.
    engine.register_fn("submit", ScriptCtx::submit);
    engine.register_fn("command_action", ScriptCtx::command_action);
    engine.register_fn("command_param", ScriptCtx::command_param);

    // Step 5f: the turn scheduler (docs/ember2d-phase5-plan.md) —
    // `ctx.trigger_turn` is gone, replaced by `act`.
    engine.register_fn("act", ScriptCtx::act);
    engine.register_fn("get_turn_number", ScriptCtx::get_turn_number);
    engine.register_fn("get_speed", ScriptCtx::get_speed);
    engine.register_fn("set_speed", ScriptCtx::set_speed);

    // Step 7.5-4 (docs/ember2d-master-plan.md §5.6): data-driven actor
    // stats/tint — `TileRecord.actor.stats`/`tint_aware`/`tint_asleep`,
    // read at runtime so one shared `enemy.rhai` can serve every role.
    engine.register_fn("get_stat", ScriptCtx::get_stat);
    engine.register_fn("get_tint_aware", ScriptCtx::get_tint_aware);
    engine.register_fn("get_tint_asleep", ScriptCtx::get_tint_asleep);

    // Phase 5.5 Part 3: the animation queue (docs/ember2d-phase5.5-plan.md).
    engine.register_fn("animate_move", ScriptCtx::animate_move);
    engine.register_fn("animate_move", ScriptCtx::animate_move_i);
    engine.register_fn("animate_flash", ScriptCtx::animate_flash);
    engine.register_fn("animate_shake", ScriptCtx::animate_shake);
    engine.register_fn("is_animating", ScriptCtx::is_animating);
}
