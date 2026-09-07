// scripting/api.rs — ScriptCtx implementation (the Rhai API).

use std::rc::Rc;
use std::cell::RefCell;
use rhai::{Array, Dynamic};
use crate::command::Command;
use super::state::ScriptState;
use super::types::*;
use rand::rngs::SmallRng;
use rand::Rng;

#[derive(Clone)]
pub struct ScriptCtx {
    pub(super) inner: Rc<RefCell<ScriptState>>,
    pub(super) rng:   Rc<RefCell<SmallRng>>,
    pub(super) entity_id: i64,
}

impl ScriptCtx {
    pub(super) fn new(state: ScriptState, rng: Rc<RefCell<SmallRng>>) -> Self {
        ScriptCtx { inner: Rc::new(RefCell::new(state)), rng, entity_id: -1 }
    }

    pub(super) fn with_entity(&self, id: i64) -> Self {
        let mut cloned = self.clone();
        cloned.entity_id = id;
        cloned
    }

    pub fn get_x(&mut self, id: i64) -> f64 { self.inner.borrow_mut().positions.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0) }
    pub fn get_y(&mut self, id: i64) -> f64 { self.inner.borrow_mut().positions.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0) }
    pub fn get_position(&mut self, id: i64) -> Array {
        self.inner.borrow_mut().positions.get(&id).map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)]).unwrap_or_default().into()
    }
    pub fn get_vel_x(&mut self, id: i64) -> f64 { self.inner.borrow_mut().velocities.get(&id).map(|(x, _)| *x as f64).unwrap_or(0.0) }
    pub fn get_vel_y(&mut self, id: i64) -> f64 { self.inner.borrow_mut().velocities.get(&id).map(|(_, y)| *y as f64).unwrap_or(0.0) }
    pub fn get_velocity(&mut self, id: i64) -> Array {
        self.inner.borrow_mut().velocities.get(&id).map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)]).unwrap_or_default().into()
    }

    pub fn get_tag(&mut self, id: i64) -> String { self.inner.borrow_mut().tags.get(&id).map(|t| t.to_string()).unwrap_or_default() }
    pub fn set_tag(&mut self, id: i64, tag: String) { self.inner.borrow_mut().pending_tags.push((id, tag)); }
    pub fn has_tag(&mut self, id: i64, name: String) -> bool { self.inner.borrow_mut().tags.get(&id).map(|t| **t == name).unwrap_or(false) }

    pub fn get_glyph(&mut self, id: i64) -> String { self.inner.borrow_mut().glyphs.get(&id).map(|c| c.to_string()).unwrap_or_default() }
    // Phase 6 Step 4 (docs/ember2d-phase6-plan.md): `colors` stores `Color`
    // now, not a pre-formatted name string — see `WorldSnapshot::colors`'s
    // doc comment. `color_to_name` runs here instead, only for whichever
    // entity a script actually asks about.
    pub fn get_color(&mut self, id: i64) -> Array {
        self.inner.borrow_mut().colors.get(&id).map(|&(fg, bg)| vec![Dynamic::from(color_to_name(fg)), Dynamic::from(color_to_name(bg))]).unwrap_or_default().into()
    }
    pub fn get_texture(&mut self, id: i64) -> String { self.inner.borrow_mut().textures.get(&id).map(|p| p.to_string()).unwrap_or_default() }
    // `Rc<str>: Borrow<str>` (and its Hash/Eq/Ord delegate to `str`'s) is
    // what lets these three keep taking a plain Rhai `String` and looking it
    // up against a map keyed by `Rc<str>` — see `WorldSnapshot::tags`'s doc
    // comment (Phase 6 Step 4).
    pub fn find_by_tag(&mut self, tag: String) -> i64 { self.inner.borrow_mut().tag_to_id.get(tag.as_str()).copied().unwrap_or(-1) }
    pub fn find_all_by_tag(&mut self, tag: String) -> Array {
        self.inner.borrow_mut().tag_to_ids.get(tag.as_str()).cloned().unwrap_or_default().into_iter().map(Dynamic::from).collect()
    }

    /// Kept registered for compatibility (Step 5e, docs/ember2d-phase5-plan.md)
    /// but no longer replay-safe: real key state is engine-side wall-clock
    /// input, not part of a recorded command stream, so a script that reads
    /// it outside `on_input` will diverge between a live run and a replay
    /// of the same commands. `on_input` is the only place a script should
    /// read raw input at all — everywhere else, read `command_action`/
    /// `command_param` instead.
    pub fn is_held(&mut self, key: String) -> bool { self.inner.borrow_mut().input.is_held(&key) }
    pub fn just_pressed(&mut self, key: String) -> bool { self.inner.borrow_mut().input.just_pressed(&key) }

    // ── Step 5e: the command boundary ─────────────────────────────────────────

    /// Queue a `Command` for `actor_id`. Meaningful only inside `on_input`
    /// — anything submitted from `on_update`/`on_collide`/`on_start` is
    /// still collected into this pass's `ScriptUpdateResult.commands`, but
    /// by the time the *next* `on_input` pass runs it will have already
    /// been overwritten, since commands don't accumulate across passes
    /// (see that field's doc comment). `params` accepts either ints or
    /// floats — Rhai integer literals (`1`, not `1.0`) are the common case
    /// in a script's `submit` call, so both are coerced to `f64` here
    /// rather than making every caller write `.0`.
    pub fn submit(&mut self, actor_id: i64, action: String, params: Array) {
        let params: Vec<f64> = params.into_iter()
            .filter_map(|d| d.as_float().ok().or_else(|| d.as_int().ok().map(|i| i as f64)))
            .collect();
        self.inner.borrow_mut().pending_commands.push(Command {
            actor: actor_id as crate::world::EntityId,
            action,
            params,
        });
    }

    /// This entity's command for the current step, or `""` if `on_input`
    /// (this entity's own, or whichever actor's `ctx.submit` named it)
    /// didn't queue one.
    pub fn command_action(&mut self) -> String {
        self.inner.borrow_mut().commands.get(&self.entity_id).map(|c| c.action.clone()).unwrap_or_default()
    }

    /// The `i`-th param of this entity's current command, or `0.0` if
    /// there's no command or the index is out of range.
    pub fn command_param(&mut self, i: i64) -> f64 {
        self.inner.borrow_mut().commands.get(&self.entity_id).and_then(|c| c.params.get(i as usize).copied()).unwrap_or(0.0)
    }

    // ── Step 5f: the turn scheduler ───────────────────────────────────────────

    /// Marks this `on_turn` call as having consumed a turn, at `cost`
    /// energy (`TurnScheduler::advance`'s unit — 100 is a normal turn under
    /// today's `Alternating`-only scheduling). Replaces the removed
    /// `ctx.trigger_turn()`. For a locally-controlled actor, NOT calling
    /// this is how a rejected action (a wall bump, an out-of-potions quaff)
    /// costs nothing — the actor's own next `on_input` gets asked again
    /// immediately, no turn spent. An AI actor's turn always counts whether
    /// or not it calls this (a sleeping monster still "used" its turn doing
    /// nothing, and unconditionally NOT advancing it would wedge the
    /// scheduler forever) — see `PlayState::run_actor_turn`.
    pub fn act(&mut self, cost: f64) {
        self.inner.borrow_mut().pending_act_cost = Some(cost);
    }

    /// How many turns the local player has completed so far this level —
    /// replaces the old "turn" global (see player.rhai's header comment for
    /// what used to live there).
    pub fn get_turn_number(&mut self) -> i64 {
        self.inner.borrow_mut().turn_number
    }

    /// `Actor::speed` — vestigial until a non-`Alternating` scheduling mode
    /// ships (see that field's own doc comment); `0` for an entity with no
    /// `Actor` component at all.
    pub fn get_speed(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().actor_speeds.get(&id).copied().unwrap_or(0) as f64
    }
    pub fn set_speed(&mut self, id: i64, n: f64) {
        self.inner.borrow_mut().pending_speed.push((id, n.max(0.0) as u32));
    }

    pub fn get_spawn_point(&mut self, name: String) -> Array {
        self.inner.borrow_mut().extra_spawns.get(&name).map(|&(x, y)| vec![Dynamic::from(x as f64), Dynamic::from(y as f64)]).unwrap_or_default()
    }

    pub fn get_delta(&mut self) -> f64 { self.inner.borrow_mut().delta_time as f64 }
    pub fn get_elapsed(&mut self) -> f64 { self.inner.borrow_mut().elapsed as f64 }

    pub fn set_velocity(&mut self, id: i64, vx: f64, vy: f64) { self.inner.borrow_mut().pending_velocities.push((id, vx as f32, vy as f32)); }
    /// R6 (7A-1): a non-finite position (NaN from a script's own bad math,
    /// e.g. `0.0 / 0.0`) used to flow straight into `Transform.position`,
    /// where it later broke `detect_collisions`'s sort (see that method's
    /// own R6 comment). Rejecting it here — a no-op, same convention as a
    /// setter targeting a missing entity — stops it at the boundary instead
    /// of chasing it through every downstream consumer.
    pub fn set_position(&mut self, id: i64, x: f64, y: f64) {
        if !x.is_finite() || !y.is_finite() { return; }
        self.inner.borrow_mut().pending_positions.push((id, x as f32, y as f32));
    }
    pub fn set_glyph(&mut self, id: i64, glyph_str: String) {
        if let Some(ch) = glyph_str.chars().next() { self.inner.borrow_mut().pending_glyphs.push((id, ch)); }
    }
    /// Step 3e: was `set_color`. Still takes color names (`"Red"`) or, since
    /// `parse_color` now also reads them, explicit `"#RRGGBB"` hex values —
    /// `color_to_name` already emits that format for `Color::Rgb`, so this
    /// is a read-side addition, not a new wire format.
    ///
    /// R4 (7A-1): both channels are validated with `try_parse_color` before
    /// queueing anything. A byte-sliced-non-ASCII hex string used to panic
    /// here (`parse_color` indexed into the middle of a multi-byte
    /// character); an unrecognized name or malformed hex now just leaves
    /// the tint unchanged instead of silently overwriting it with `Reset` —
    /// the same "setter given garbage is a no-op" convention `set_position`
    /// above follows for a non-finite value. Logged once per distinct bad
    /// string (`ScriptState::logged_bad_colors`) rather than every call, so
    /// a script that repeats the same mistake every frame doesn't flood the
    /// console.
    pub fn set_tint(&mut self, id: i64, fg: String, bg: String) {
        let fg_ok = try_parse_color(&fg).is_some();
        let bg_ok = try_parse_color(&bg).is_some();
        let mut s = self.inner.borrow_mut();
        if !fg_ok { s.log_bad_color_once(&fg); }
        if !bg_ok { s.log_bad_color_once(&bg); }
        if fg_ok && bg_ok { s.pending_colors.push((id, fg, bg)); }
    }

    pub fn set_texture(&mut self, id: i64, path: String) {
        let p = if path.is_empty() { None } else { Some(path) };
        self.inner.borrow_mut().pending_textures.push((id, p));
    }

    pub fn despawn(&mut self, id: i64) { self.inner.borrow_mut().despawn_queue.push(id); }

    /// Spawn an entity with just a glyph, position, and tag. Appearance and
    /// collider fall back to the engine's long-standing defaults: white
    /// glyph, z_order 2, a 1x1 non-solid trigger with no layer. Registered
    /// under the same Rhai name as `spawn_entity_full` (arity picks the
    /// overload), so existing scripts calling this 4-arg form are unaffected.
    pub fn spawn_entity(&mut self, glyph_str: String, x: f64, y: f64, tag: String) -> i64 {
        self.spawn_entity_full(glyph_str, x, y, tag, "White".to_string(), "Reset".to_string(), 2, false, 1.0, 1.0, String::new())
    }

    /// Spawn an entity with full control over appearance and collider
    /// (defect D10 — `spawn_entity` used to hardcode all of this).
    /// `fg`/`bg` are color names as in `set_tint`; `z` is draw order;
    /// `solid` marks a physical obstacle rather than a trigger; `w`/`h` are
    /// the collider size; `layer` is the collision layer (empty = unlabeled,
    /// matching the trigger-layer default from defect D4).
    pub fn spawn_entity_full(&mut self, glyph_str: String, x: f64, y: f64, tag: String, fg: String, bg: String, z: i64, solid: bool, w: f64, h: f64, layer: String) -> i64 {
        let glyph = glyph_str.chars().next().unwrap_or('?');
        let mut s = self.inner.borrow_mut();
        let id = s.next_spawn_id;
        s.next_spawn_id += 1;
        s.spawn_queue.push(SpawnRequest {
            id, glyph, x: x as f32, y: y as f32, tag,
            fg: parse_color(&fg), bg: parse_color(&bg), z: z as i32, solid,
            w: w as f32, h: h as f32, layer,
        });
        id as i64
    }

    pub fn load_level(&mut self, path: String) {
        let mut s = self.inner.borrow_mut();
        if s.pending_level.is_none() { s.pending_level = Some(path); }
    }
    pub fn log(&mut self, msg: String) { self.inner.borrow_mut().pending_logs.push(msg); }
    pub fn draw_hud(&mut self, x: i64, y: i64, text: String, fg: String, bg: String) {
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Text { x: x as usize, y: y as usize, text, fg: parse_color(&fg), bg: parse_color(&bg) });
    }

    pub fn draw_menu(&mut self, x: i64, y: i64, w: i64, options: Array, selected: i64, fg: String, bg: String, sel_fg: String, sel_bg: String) {
        let opts: Vec<String> = options.into_iter().map(|d| d.to_string()).collect();
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Menu { 
            x: x as usize, y: y as usize, w: w as usize, options: opts, selected: selected as usize, 
            fg: parse_color(&fg), bg: parse_color(&bg), sel_fg: parse_color(&sel_fg), sel_bg: parse_color(&sel_bg) 
        });
    }

    pub fn draw_panel(&mut self, x: i64, y: i64, w: i64, h: i64, title: String, fg: String, bg: String) {
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Panel { 
            x: x as usize, y: y as usize, w: w as usize, h: h as usize, title, 
            fg: parse_color(&fg), bg: parse_color(&bg) 
        });
    }

    pub fn play_sound(&mut self, path: String) { self.inner.borrow_mut().pending_sounds.push(path); }
    pub fn play_sound_at(&mut self, path: String, x: f64, y: f64) {
        self.inner.borrow_mut().pending_spatial_sounds.push((path, x as f32, y as f32));
    }
    pub fn play_music(&mut self, path: String) { self.inner.borrow_mut().pending_music = Some(path); }
    pub fn stop_music(&mut self) { self.inner.borrow_mut().stop_music = true; }

    pub fn emit_particles(&mut self, x: f64, y: f64, glyph_str: String, fg: String) {
        let glyph = glyph_str.chars().next().unwrap_or('*');
        let fg_col = parse_color(&fg);
        self.inner.borrow_mut().pending_particles.push(ParticleRequest { x: x as f32, y: y as f32, glyph, fg: fg_col });
    }

    // Phase 5.5 Part 3's animation-queue methods (animate_move/animate_flash/
    // animate_shake/is_animating) live in api_animation.rs, a sibling
    // `impl ScriptCtx` block — see that file's own header comment for why.

    // ── V0.4 Extensions ───────────────────────────────────────────────────────

    // 1. Shared Global State
    pub fn set_global(&mut self, key: String, value: Dynamic) { self.inner.borrow_mut().pending_globals.insert(key, value); }
    pub fn get_global(&mut self, key: String) -> Dynamic { self.inner.borrow_mut().globals.get(&key).cloned().unwrap_or(Dynamic::UNIT) }
    pub fn has_global(&mut self, key: String) -> bool { self.inner.borrow_mut().globals.contains_key(&key) }
    pub fn remove_global(&mut self, key: String) { self.inner.borrow_mut().pending_globals.insert(key, Dynamic::UNIT); }

    // 2. Randomness
    /// R2 (7A-1): `gen_range` panics on an empty range (`max < min`) —
    /// swapping the bounds first means every argument order a script passes
    /// produces a value, same as if it had asked correctly.
    pub fn random_int(&mut self, min: i64, max: i64) -> i64 {
        let (min, max) = if max < min { (max, min) } else { (min, max) };
        self.rng.borrow_mut().gen_range(min..=max)
    }
    pub fn random_float(&mut self) -> f64 { self.rng.borrow_mut().gen() }
    /// R3 (7A-1): `chance.clamp(0.0, 1.0)` doesn't rescue a NaN `chance`
    /// (comparisons against NaN are always false, so `clamp` returns it
    /// unchanged) — `gen_bool` asserts its argument is in `0.0..=1.0` and
    /// panics otherwise. Rejecting non-finite input before the clamp closes
    /// that gap; `0.0` is the same "never" fallback `is_finite` guards use
    /// elsewhere in this file (`set_position`).
    pub fn random_bool(&mut self, chance: f64) -> bool {
        let chance = if chance.is_finite() { chance.clamp(0.0, 1.0) } else { 0.0 };
        self.rng.borrow_mut().gen_bool(chance)
    }
    pub fn random_choice(&mut self, arr: Array) -> Dynamic {
        if arr.is_empty() { Dynamic::UNIT }
        else { arr[self.rng.borrow_mut().gen_range(0..arr.len())].clone() }
    }

    // get_entity_at/is_solid_at/find_entities_in_rect/get_distance/
    // get_angle_to moved to api_spatial.rs (Phase 6 Step 2,
    // docs/ember2d-phase6-plan.md) — a second `impl ScriptCtx` block in a
    // sibling file, same pattern api_animation.rs already established.

    // 4. Entity Utility Queries
    pub fn entity_exists(&mut self, id: i64) -> bool { self.inner.borrow_mut().positions.contains_key(&id) }
    pub fn count_by_tag(&mut self, tag: String) -> i64 { self.inner.borrow_mut().tag_to_ids.get(tag.as_str()).map(|v| v.len()).unwrap_or(0) as i64 }
    pub fn get_collider_w(&mut self, id: i64) -> f64 { self.inner.borrow_mut().colliders.get(&id).map(|c| c.0 as f64).unwrap_or(0.0) }
    pub fn get_collider_h(&mut self, id: i64) -> f64 { self.inner.borrow_mut().colliders.get(&id).map(|c| c.1 as f64).unwrap_or(0.0) }
    pub fn set_collider_size(&mut self, id: i64, w: f64, h: f64) { self.inner.borrow_mut().pending_collider_size.push((id, w as f32, h as f32)); }
    pub fn is_collider_solid(&mut self, id: i64) -> bool { self.inner.borrow_mut().colliders.get(&id).map(|c| c.2).unwrap_or(false) }
    pub fn set_collider_solid(&mut self, id: i64, solid: bool) { self.inner.borrow_mut().pending_collider_solid.push((id, solid)); }
    pub fn is_visible(&mut self, id: i64) -> bool { self.inner.borrow_mut().visibility.get(&id).copied().unwrap_or(false) }
    pub fn set_visible(&mut self, id: i64, visible: bool) { self.inner.borrow_mut().pending_visibility.push((id, visible)); }
    /// Step 3e: was `get_z_order`/`set_z_order` — renamed to match `Sprite`'s
    /// own field, `layer` (itself renamed from `z_order` in Step 3b).
    pub fn get_layer_order(&mut self, id: i64) -> i64 { self.inner.borrow_mut().z_orders.get(&id).copied().unwrap_or(0) as i64 }
    pub fn set_layer_order(&mut self, id: i64, z: i64) { self.inner.borrow_mut().pending_z_order.push((id, z as i32)); }

    // 5. Mouse Input
    pub fn get_mouse_x(&mut self) -> f64 { self.inner.borrow_mut().mouse_pos.0 as f64 }
    pub fn get_mouse_y(&mut self) -> f64 { self.inner.borrow_mut().mouse_pos.1 as f64 }
    
    pub fn get_mouse_world_x(&mut self) -> f64 {
        let s = self.inner.borrow_mut();
        (s.mouse_pos.0 + s.camera_pos.0) as f64
    }
    
    pub fn get_mouse_world_y(&mut self) -> f64 {
        let s = self.inner.borrow_mut();
        // Screen row -> world row: add the camera offset. Used to also
        // subtract a HUD-reserved-row constant here — Phase 2
        // (docs/ember2d-refactor-plan.md) centralized that into
        // `crate::play::HUD_TOP_ROWS`, and Step 5a
        // (docs/ember2d-phase5-plan.md) deleted the constant outright once
        // its value had been 0 (no reserved rows) since Step 4g: both this
        // and `Camera::viewport_origin` were already inert. If a future HUD
        // design needs to reserve rows again, write a nonzero
        // `Camera::viewport_origin` and subtract it here.
        (s.mouse_pos.1 + s.camera_pos.1) as f64
    }

    pub fn mouse_left_pressed(&mut self) -> bool { self.inner.borrow_mut().mouse_pressed.0 }
    pub fn mouse_right_pressed(&mut self) -> bool { self.inner.borrow_mut().mouse_pressed.1 }
    pub fn mouse_left_held(&mut self) -> bool { self.inner.borrow_mut().mouse_held.0 }
    pub fn mouse_right_held(&mut self) -> bool { self.inner.borrow_mut().mouse_held.1 }

    // 6. Camera Control
    pub fn get_camera_x(&mut self) -> f64 { self.inner.borrow_mut().camera_pos.0 as f64 }
    pub fn get_camera_y(&mut self) -> f64 { self.inner.borrow_mut().camera_pos.1 as f64 }
    pub fn set_camera(&mut self, x: f64, y: f64) { self.inner.borrow_mut().pending_camera = Some(crate::math::Vec2::new(x as f32, y as f32)); }
    pub fn shake_camera(&mut self, intensity: f64, duration: f64) {
        self.inner.borrow_mut().pending_shake = Some(ShakeState { intensity: intensity as f32, duration: duration as f32 });
    }

    // 7. Cross-Level Persistence
    pub fn set_persistent(&mut self, key: String, value: Dynamic) { self.inner.borrow_mut().pending_persistent.insert(key, value); }
    pub fn get_persistent(&mut self, key: String) -> Dynamic { self.inner.borrow_mut().persistent.get(&key).cloned().unwrap_or(Dynamic::UNIT) }
    pub fn has_persistent(&mut self, key: String) -> bool { self.inner.borrow_mut().persistent.contains_key(&key) }
    pub fn clear_persistent(&mut self, key: String) { self.inner.borrow_mut().pending_persistent.insert(key, Dynamic::UNIT); }
    /// R9 (7A-1): this used to `.clear()` `pending_persistent` — the
    /// not-yet-applied write *queue* for this pass, which is almost always
    /// empty at the point a script calls this, not `persistent` itself (the
    /// actual store, held on `ScriptState` and rebuilt fresh each pass from
    /// `Simulation`'s copy — see that field's own doc comment). The net
    /// effect was a no-op that silently discarded whatever this same pass
    /// had already queued via `set_persistent`, while the store lived on
    /// untouched. Fixed by requesting the clear on `ScriptState` instead —
    /// `apply_ctx` clears the real store first, then applies any
    /// `pending_persistent` writes this same pass queued on top (so
    /// `clear_all_persistent(); set_persistent("x", 1);` in one pass leaves
    /// exactly `x = 1`, not an empty store).
    pub fn clear_all_persistent(&mut self) { self.inner.borrow_mut().pending_persistent_clear_all = true; }

    // 8. HUD / Draw Utilities
    pub fn draw_box(&mut self, x: i64, y: i64, w: i64, h: i64, fg: String, bg: String) {
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Box { x: x as usize, y: y as usize, w: w as usize, h: h as usize, fg: parse_color(&fg), bg: parse_color(&bg) });
    }
    pub fn fill_rect(&mut self, x: i64, y: i64, w: i64, h: i64, ch_str: String, fg: String, bg: String) {
        let ch = ch_str.chars().next().unwrap_or(' ');
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Fill { x: x as usize, y: y as usize, w: w as usize, h: h as usize, ch, fg: parse_color(&fg), bg: parse_color(&bg) });
    }
    pub fn clear_hud(&mut self) { self.inner.borrow_mut().clear_hud = true; }

    pub fn save_game(&mut self, path: String) { self.inner.borrow_mut().pending_save = Some(path); }
    pub fn load_game(&mut self, path: String) { self.inner.borrow_mut().pending_load = Some(path); }

    pub fn get_viewport_width(&mut self) -> i64 { self.inner.borrow_mut().viewport_size.0 as i64 }
    pub fn get_viewport_height(&mut self) -> i64 { self.inner.borrow_mut().viewport_size.1 as i64 }

    // raycast/get_path moved to api_spatial.rs (Phase 6 Step 2,
    // docs/ember2d-phase6-plan.md) — see that file's own header comment.

    // 9. Timers
    pub fn start_timer(&mut self, name: String, duration: f64) {
        if self.entity_id != -1 {
            self.inner.borrow_mut().pending_timers.push((self.entity_id as crate::world::EntityId, name, duration));
        }
    }
    pub fn timer_done(&mut self, name: String) -> bool {
        let mut s = self.inner.borrow_mut();
        if let Some(entity_timers) = s.timers.get(&(self.entity_id as crate::world::EntityId)) {
            if let Some(&val) = entity_timers.get(&name) {
                if val <= 0.0 && val > -500.0 {
                    // Mark for removal/consumed by setting to a special value
                    s.pending_timers.push((self.entity_id as crate::world::EntityId, name, -999.0)); 
                    return true;
                }
            }
        }
        false
    }
    pub fn cancel_timer(&mut self, name: String) {
        if self.entity_id != -1 {
            self.inner.borrow_mut().pending_timers.push((self.entity_id as crate::world::EntityId, name, -1.0f64));
        }
    }

    // 10. Collision Layers & Masks
    pub fn get_collider_layer(&mut self, id: i64) -> String { self.inner.borrow_mut().colliders.get(&id).map(|c| c.3.clone()).unwrap_or_default() }
    pub fn set_collider_layer(&mut self, id: i64, layer: String) { self.inner.borrow_mut().pending_collider_layer.push((id, layer)); }

    pub fn get_collider_mask(&mut self, id: i64) -> Array {
        self.inner.borrow_mut().colliders.get(&id).map(|c| c.4.iter().map(|s| Dynamic::from(s.clone())).collect()).unwrap_or_default()
    }
    pub fn set_collider_mask(&mut self, id: i64, mask: Array) {
        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();
        self.inner.borrow_mut().pending_collider_mask.push((id, mask_vec));
    }

    /// Defect D12: exits used to check `get_collider_layer(id) == "locked"`,
    /// smuggling a gameplay gate through the layer field meant for collision
    /// filtering. `locked` is now its own flag — an exit trigger checks this
    /// instead, and a locked collider still detects overlap normally.
    pub fn is_collider_locked(&mut self, id: i64) -> bool { self.inner.borrow_mut().colliders.get(&id).map(|c| c.5).unwrap_or(false) }
    pub fn set_collider_locked(&mut self, id: i64, locked: bool) { self.inner.borrow_mut().pending_collider_locked.push((id, locked)); }

    // ── V0.5 Hierarchy ────────────────────────────────────────────────────────

    pub fn get_parent(&mut self, id: i64) -> i64 { self.inner.borrow_mut().parents.get(&id).copied().unwrap_or(-1) }
    pub fn set_parent(&mut self, id: i64, parent_id: i64) { self.inner.borrow_mut().pending_parents.push((id, parent_id, false)); }
    pub fn set_parent_keep_world(&mut self, id: i64, parent_id: i64) { self.inner.borrow_mut().pending_parents.push((id, parent_id, true)); }

    pub fn get_world_x(&mut self, id: i64) -> f64 {
        let mut x = 0.0;
        let mut curr = id;
        let s = self.inner.borrow_mut();
        let mut depth = 0;
        while curr != -1 && depth < 100 {
            if let Some(&(px, _)) = s.positions.get(&curr) {
                x += px as f64;
                curr = s.parents.get(&curr).copied().unwrap_or(-1);
            } else { break; }
            depth += 1;
        }
        x
    }

    pub fn get_world_y(&mut self, id: i64) -> f64 {
        let mut y = 0.0;
        let mut curr = id;
        let s = self.inner.borrow_mut();
        let mut depth = 0;
        while curr != -1 && depth < 100 {
            if let Some(&(_, py)) = s.positions.get(&curr) {
                y += py as f64;
                curr = s.parents.get(&curr).copied().unwrap_or(-1);
            } else { break; }
            depth += 1;
        }
        y
    }

    pub fn gp_is_held(&mut self, gp_id: i64, btn: String) -> bool {
        self.inner.borrow_mut().gamepad_held.contains(&(gp_id as usize, btn))
    }

    pub fn gp_just_pressed(&mut self, gp_id: i64, btn: String) -> bool {
        self.inner.borrow_mut().gamepad_pressed.contains(&(gp_id as usize, btn))
    }

    pub fn gp_axis(&mut self, gp_id: i64, axis: String) -> f64 {
        self.inner.borrow_mut().gamepad_axes.get(&(gp_id as usize, axis)).copied().unwrap_or(0.0) as f64
    }

    // ── Phase 3: named animation clips ──────────────────────────────────────

    /// Define (or redefine) a named clip from a string of glyphs, cycled at
    /// `fps`. `looping` is this clip's own default — `play_clip_once`
    /// overrides it per-play without needing a second registration.
    pub fn register_clip(&mut self, name: String, frames_str: String, fps: f64, looping: bool) {
        let frames: Vec<char> = frames_str.chars().collect();
        let clip = crate::components::AnimationClip {
            frames: crate::components::ClipFrames::Glyphs { frames },
            fps: fps as f32,
            looping,
        };
        self.inner.borrow_mut().pending_clip_defs.push((name, clip));
    }

    /// Play `name` from frame 0, respecting the clip's own `looping` flag.
    pub fn play_clip(&mut self, id: i64, name: String) { self.inner.borrow_mut().pending_play_clip.push((id, name, false)); }
    /// Play `name` from frame 0, stopping at its last frame even if the
    /// clip itself was registered as looping.
    pub fn play_clip_once(&mut self, id: i64, name: String) { self.inner.borrow_mut().pending_play_clip.push((id, name, true)); }
    pub fn stop_clip(&mut self, id: i64) { self.inner.borrow_mut().pending_stop_clip.push(id); }
    pub fn set_clip_speed(&mut self, id: i64, speed: f64) { self.inner.borrow_mut().pending_clip_speed.push((id, speed as f32)); }
    pub fn get_frame(&mut self, id: i64) -> i64 { self.inner.borrow_mut().animator_frames.get(&id).copied().unwrap_or(0) as i64 }
    pub fn set_frame(&mut self, id: i64, frame: i64) { self.inner.borrow_mut().pending_set_frame.push((id, frame.max(0) as usize)); }
    /// True for exactly the frame a non-looping (or `play_clip_once`)
    /// playback reaches its last frame — see `Animator::just_finished`.
    pub fn clip_finished(&mut self, id: i64) -> bool { self.inner.borrow_mut().clip_finished.contains(&id) }

    // ── Phase 3 Step 3e: API version ─────────────────────────────────────────

    /// The scripting API's breaking-change generation — see
    /// `docs/ember2d-scripting-api.md` §6's changelog table. Bumped whenever
    /// a "Yes" lands there; a script (or its author, mid-migration) can
    /// branch on this instead of guessing from engine version numbers.
    pub fn api_version(&mut self) -> i64 { API_VERSION }
}
