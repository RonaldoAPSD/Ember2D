// scripting/api_ext.rs — ScriptCtx's V0.4/V0.5 extension methods: global
// state, randomness, entity/collider queries, mouse input, camera control,
// cross-level persistence, HUD/draw utilities, timers, collision layers &
// masks, hierarchy, gamepad, named animation clips, and `api_version`.
//
// Split out of api.rs at 7A-10 (docs/ember2d-master-plan.md §5.1, R42):
// 7A-9's `cargo fmt --all` alone (no logic change) pushed api.rs to 771/750
// lines (CLAUDE.md's hard limit). This is everything that used to live
// under api.rs's own "── V0.4 Extensions ───" marker through `api_version`,
// moved verbatim into a third sibling `impl ScriptCtx` block — the same
// pattern api_animation.rs and api_spatial.rs already established for the
// same reason. Nothing here changed behavior, only location.

use rand::Rng;
use rhai::{Array, Dynamic};

use super::api::ScriptCtx;
use super::types::*;

impl ScriptCtx {
    // 1. Shared Global State
    pub fn set_global(&mut self, key: String, value: Dynamic) {
        self.inner.borrow_mut().pending_globals.insert(key, PendingWrite::Set(value));
    }
    pub fn get_global(&mut self, key: String) -> Dynamic {
        self.inner.borrow_mut().globals.get(&key).cloned().unwrap_or(Dynamic::UNIT)
    }
    pub fn has_global(&mut self, key: String) -> bool {
        self.inner.borrow_mut().globals.contains_key(&key)
    }
    /// R32 (7.5-1, docs/ember2d-master-plan.md §5.6): queues a real
    /// `PendingWrite::Remove` now, not `Dynamic::UNIT` — see that type's
    /// own doc comment for why the old sentinel made this indistinguishable
    /// from `set_global(key, ())`.
    pub fn remove_global(&mut self, key: String) {
        self.inner.borrow_mut().pending_globals.insert(key, PendingWrite::Remove);
    }

    // 2. Randomness
    /// R2 (7A-1): `gen_range` panics on an empty range (`max < min`) —
    /// swapping the bounds first means every argument order a script passes
    /// produces a value, same as if it had asked correctly.
    pub fn random_int(&mut self, min: i64, max: i64) -> i64 {
        let (min, max) = if max < min { (max, min) } else { (min, max) };
        self.rng.borrow_mut().gen_range(min..=max)
    }
    pub fn random_float(&mut self) -> f64 {
        self.rng.borrow_mut().gen()
    }
    /// R3 (7A-1): `chance.clamp(0.0, 1.0)` doesn't rescue a NaN `chance`
    /// (comparisons against NaN are always false, so `clamp` returns it
    /// unchanged) — `gen_bool` asserts its argument is in `0.0..=1.0` and
    /// panics otherwise. Rejecting non-finite input before the clamp closes
    /// that gap; `0.0` is the same "never" fallback `is_finite` guards use
    /// elsewhere in api.rs (`set_position`).
    pub fn random_bool(&mut self, chance: f64) -> bool {
        let chance = if chance.is_finite() { chance.clamp(0.0, 1.0) } else { 0.0 };
        self.rng.borrow_mut().gen_bool(chance)
    }
    pub fn random_choice(&mut self, arr: Array) -> Dynamic {
        if arr.is_empty() {
            Dynamic::UNIT
        } else {
            arr[self.rng.borrow_mut().gen_range(0..arr.len())].clone()
        }
    }

    // get_entity_at/is_solid_at/find_entities_in_rect/get_distance/
    // get_angle_to moved to api_spatial.rs (Phase 6 Step 2,
    // docs/ember2d-phase6-plan.md) — a second `impl ScriptCtx` block in a
    // sibling file, same pattern api_animation.rs already established.

    // 4. Entity Utility Queries
    pub fn entity_exists(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().positions.contains_key(&id)
    }
    pub fn count_by_tag(&mut self, tag: String) -> i64 {
        self.inner.borrow_mut().tag_to_ids.get(tag.as_str()).map(|v| v.len()).unwrap_or(0) as i64
    }
    pub fn get_collider_w(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().colliders.get(&id).map(|c| c.0 as f64).unwrap_or(0.0)
    }
    pub fn get_collider_h(&mut self, id: i64) -> f64 {
        self.inner.borrow_mut().colliders.get(&id).map(|c| c.1 as f64).unwrap_or(0.0)
    }
    pub fn set_collider_size(&mut self, id: i64, w: f64, h: f64) {
        self.inner.borrow_mut().pending_collider_size.push((id, w as f32, h as f32));
    }
    /// `i64` overload — see `registry.rs`'s own note on why every
    /// coordinate/size/layer-order function gets one (7.5-1, R31).
    pub fn set_collider_size_i(&mut self, id: i64, w: i64, h: i64) {
        self.set_collider_size(id, w as f64, h as f64)
    }
    pub fn is_collider_solid(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().colliders.get(&id).map(|c| c.2).unwrap_or(false)
    }
    pub fn set_collider_solid(&mut self, id: i64, solid: bool) {
        self.inner.borrow_mut().pending_collider_solid.push((id, solid));
    }
    pub fn is_visible(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().visibility.get(&id).copied().unwrap_or(false)
    }
    pub fn set_visible(&mut self, id: i64, visible: bool) {
        self.inner.borrow_mut().pending_visibility.push((id, visible));
    }
    /// Step 3e: was `get_z_order`/`set_z_order` — renamed to match `Sprite`'s
    /// own field, `layer` (itself renamed from `z_order` in Step 3b).
    pub fn get_layer_order(&mut self, id: i64) -> i64 {
        self.inner.borrow_mut().z_orders.get(&id).copied().unwrap_or(0) as i64
    }
    pub fn set_layer_order(&mut self, id: i64, z: i64) {
        self.inner.borrow_mut().pending_z_order.push((id, z as i32));
    }
    /// `f64` overload — same reasoning as `set_collider_size_i` above.
    pub fn set_layer_order_f(&mut self, id: i64, z: f64) {
        self.set_layer_order(id, z as i64)
    }

    // 5. Mouse Input
    pub fn get_mouse_x(&mut self) -> f64 {
        self.inner.borrow_mut().mouse_pos.0 as f64
    }
    pub fn get_mouse_y(&mut self) -> f64 {
        self.inner.borrow_mut().mouse_pos.1 as f64
    }

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

    pub fn mouse_left_pressed(&mut self) -> bool {
        self.inner.borrow_mut().mouse_pressed.0
    }
    pub fn mouse_right_pressed(&mut self) -> bool {
        self.inner.borrow_mut().mouse_pressed.1
    }
    pub fn mouse_left_held(&mut self) -> bool {
        self.inner.borrow_mut().mouse_held.0
    }
    pub fn mouse_right_held(&mut self) -> bool {
        self.inner.borrow_mut().mouse_held.1
    }

    // 6. Camera Control
    pub fn get_camera_x(&mut self) -> f64 {
        self.inner.borrow_mut().camera_pos.0 as f64
    }
    pub fn get_camera_y(&mut self) -> f64 {
        self.inner.borrow_mut().camera_pos.1 as f64
    }
    pub fn set_camera(&mut self, x: f64, y: f64) {
        self.inner.borrow_mut().pending_camera = Some(crate::math::Vec2::new(x as f32, y as f32));
    }
    /// `i64` overload — same reasoning as `set_collider_size_i` above.
    pub fn set_camera_i(&mut self, x: i64, y: i64) {
        self.set_camera(x as f64, y as f64)
    }
    pub fn shake_camera(&mut self, intensity: f64, duration: f64) {
        self.inner.borrow_mut().pending_shake =
            Some(ShakeState { intensity: intensity as f32, duration: duration as f32 });
    }

    // 7. Cross-Level Persistence
    pub fn set_persistent(&mut self, key: String, value: Dynamic) {
        self.inner.borrow_mut().pending_persistent.insert(key, PendingWrite::Set(value));
    }
    pub fn get_persistent(&mut self, key: String) -> Dynamic {
        self.inner.borrow_mut().persistent.get(&key).cloned().unwrap_or(Dynamic::UNIT)
    }
    pub fn has_persistent(&mut self, key: String) -> bool {
        self.inner.borrow_mut().persistent.contains_key(&key)
    }
    /// R32 (7.5-1, docs/ember2d-master-plan.md §5.6): same `PendingWrite`
    /// fix as `remove_global` above.
    pub fn clear_persistent(&mut self, key: String) {
        self.inner.borrow_mut().pending_persistent.insert(key, PendingWrite::Remove);
    }
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
    pub fn clear_all_persistent(&mut self) {
        self.inner.borrow_mut().pending_persistent_clear_all = true;
    }

    // 8. HUD / Draw Utilities
    pub fn draw_box(&mut self, x: i64, y: i64, w: i64, h: i64, fg: String, bg: String) {
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Box {
            x: x as usize,
            y: y as usize,
            w: w as usize,
            h: h as usize,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
        });
    }
    /// `f64` overload — same reasoning as `set_collider_size_i` above.
    pub fn draw_box_f(&mut self, x: f64, y: f64, w: f64, h: f64, fg: String, bg: String) {
        self.draw_box(x as i64, y as i64, w as i64, h as i64, fg, bg)
    }
    pub fn fill_rect(
        &mut self,
        x: i64,
        y: i64,
        w: i64,
        h: i64,
        ch_str: String,
        fg: String,
        bg: String,
    ) {
        let ch = ch_str.chars().next().unwrap_or(' ');
        self.inner.borrow_mut().pending_hud_draws.push(HudDraw::Fill {
            x: x as usize,
            y: y as usize,
            w: w as usize,
            h: h as usize,
            ch,
            fg: parse_color(&fg),
            bg: parse_color(&bg),
        });
    }
    /// `f64` overload — same reasoning as `set_collider_size_i` above.
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rect_f(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        ch_str: String,
        fg: String,
        bg: String,
    ) {
        self.fill_rect(x as i64, y as i64, w as i64, h as i64, ch_str, fg, bg)
    }
    pub fn clear_hud(&mut self) {
        self.inner.borrow_mut().clear_hud = true;
    }

    pub fn save_game(&mut self, path: String) {
        self.inner.borrow_mut().pending_save = Some(path);
    }
    pub fn load_game(&mut self, path: String) {
        self.inner.borrow_mut().pending_load = Some(path);
    }

    pub fn get_viewport_width(&mut self) -> i64 {
        self.inner.borrow_mut().viewport_size.0 as i64
    }
    pub fn get_viewport_height(&mut self) -> i64 {
        self.inner.borrow_mut().viewport_size.1 as i64
    }

    // raycast/get_path moved to api_spatial.rs (Phase 6 Step 2,
    // docs/ember2d-phase6-plan.md) — see that file's own header comment.

    // 9. Timers
    pub fn start_timer(&mut self, name: String, duration: f64) {
        if self.entity_id != -1 {
            self.inner.borrow_mut().pending_timers.push((
                self.entity_id as crate::world::EntityId,
                name,
                duration,
            ));
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
            self.inner.borrow_mut().pending_timers.push((
                self.entity_id as crate::world::EntityId,
                name,
                -1.0f64,
            ));
        }
    }

    // 10. Collision Layers & Masks
    pub fn get_collider_layer(&mut self, id: i64) -> String {
        self.inner.borrow_mut().colliders.get(&id).map(|c| c.3.clone()).unwrap_or_default()
    }
    pub fn set_collider_layer(&mut self, id: i64, layer: String) {
        self.inner.borrow_mut().pending_collider_layer.push((id, layer));
    }

    pub fn get_collider_mask(&mut self, id: i64) -> Array {
        self.inner
            .borrow_mut()
            .colliders
            .get(&id)
            .map(|c| c.4.iter().map(|s| Dynamic::from(s.clone())).collect())
            .unwrap_or_default()
    }
    pub fn set_collider_mask(&mut self, id: i64, mask: Array) {
        let mask_vec: Vec<String> = mask.into_iter().map(|d| d.to_string()).collect();
        self.inner.borrow_mut().pending_collider_mask.push((id, mask_vec));
    }

    /// Defect D12: exits used to check `get_collider_layer(id) == "locked"`,
    /// smuggling a gameplay gate through the layer field meant for collision
    /// filtering. `locked` is now its own flag — an exit trigger checks this
    /// instead, and a locked collider still detects overlap normally.
    pub fn is_collider_locked(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().colliders.get(&id).map(|c| c.5).unwrap_or(false)
    }
    pub fn set_collider_locked(&mut self, id: i64, locked: bool) {
        self.inner.borrow_mut().pending_collider_locked.push((id, locked));
    }

    // ── V0.5 Hierarchy ────────────────────────────────────────────────────────

    pub fn get_parent(&mut self, id: i64) -> i64 {
        self.inner.borrow_mut().parents.get(&id).copied().unwrap_or(-1)
    }
    pub fn set_parent(&mut self, id: i64, parent_id: i64) {
        self.inner.borrow_mut().pending_parents.push((id, parent_id, false));
    }
    pub fn set_parent_keep_world(&mut self, id: i64, parent_id: i64) {
        self.inner.borrow_mut().pending_parents.push((id, parent_id, true));
    }

    pub fn get_world_x(&mut self, id: i64) -> f64 {
        let mut x = 0.0;
        let mut curr = id;
        let s = self.inner.borrow_mut();
        let mut depth = 0;
        while curr != -1 && depth < 100 {
            if let Some(&(px, _)) = s.positions.get(&curr) {
                x += px as f64;
                curr = s.parents.get(&curr).copied().unwrap_or(-1);
            } else {
                break;
            }
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
            } else {
                break;
            }
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
        self.inner.borrow_mut().gamepad_axes.get(&(gp_id as usize, axis)).copied().unwrap_or(0.0)
            as f64
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
    pub fn play_clip(&mut self, id: i64, name: String) {
        self.inner.borrow_mut().pending_play_clip.push((id, name, false));
    }
    /// Play `name` from frame 0, stopping at its last frame even if the
    /// clip itself was registered as looping.
    pub fn play_clip_once(&mut self, id: i64, name: String) {
        self.inner.borrow_mut().pending_play_clip.push((id, name, true));
    }
    pub fn stop_clip(&mut self, id: i64) {
        self.inner.borrow_mut().pending_stop_clip.push(id);
    }
    pub fn set_clip_speed(&mut self, id: i64, speed: f64) {
        self.inner.borrow_mut().pending_clip_speed.push((id, speed as f32));
    }
    pub fn get_frame(&mut self, id: i64) -> i64 {
        self.inner.borrow_mut().animator_frames.get(&id).copied().unwrap_or(0) as i64
    }
    pub fn set_frame(&mut self, id: i64, frame: i64) {
        self.inner.borrow_mut().pending_set_frame.push((id, frame.max(0) as usize));
    }
    /// True for exactly the frame a non-looping (or `play_clip_once`)
    /// playback reaches its last frame — see `Animator::just_finished`.
    pub fn clip_finished(&mut self, id: i64) -> bool {
        self.inner.borrow_mut().clip_finished.contains(&id)
    }

    // ── Phase 3 Step 3e: API version ─────────────────────────────────────────

    /// The scripting API's breaking-change generation — see
    /// `docs/ember2d-scripting-api.md` §6's changelog table. Bumped whenever
    /// a "Yes" lands there; a script (or its author, mid-migration) can
    /// branch on this instead of guessing from engine version numbers.
    pub fn api_version(&mut self) -> i64 {
        API_VERSION
    }
}
