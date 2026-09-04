// scripting/state.rs — ScriptState: the per-frame snapshot + write-queue
// scripts see and mutate through `ScriptCtx`.
//
// Split out of engine.rs (sibling-file convention, matching play.rs's
// mod render;/mod spawn; split) once engine.rs crossed the project's
// 600-line hard limit (CLAUDE.md) — Step 3c's clip/animator wiring pushed
// it over. Pure relocation: nothing here changed behavior, only location.

use std::collections::{HashMap, HashSet};

use crate::world::World;
use crate::input::InputManager;
use crate::components::{AnimationClip, SpriteSource};
use crate::renderer::color::Color;

use super::types::*;

pub(super) struct ScriptState {
    pub(super) positions:        HashMap<i64, (f32, f32)>,
    pub(super) velocities:       HashMap<i64, (f32, f32)>,
    pub(super) parents:          HashMap<i64, i64>,
    /// (width, height, solid, layer, mask, locked)
    pub(super) colliders:        HashMap<i64, (f32, f32, bool, String, Vec<String>, bool)>,
    pub(super) tags:             HashMap<i64, String>,
    pub(super) glyphs:           HashMap<i64, char>,
    pub(super) colors:           HashMap<i64, (String, String)>,
    pub(super) textures:         HashMap<i64, String>,
    pub(super) tag_to_id:        HashMap<String, i64>,
    pub(super) tag_to_ids:       HashMap<String, Vec<i64>>,
    pub(super) visibility:       HashMap<i64, bool>,
    pub(super) z_orders:         HashMap<i64, i32>,
    pub(super) delta_time:       f32,
    pub(super) elapsed:          f32,
    pub(super) next_spawn_id:    crate::world::EntityId,
    pub(super) held_keys:        HashSet<String>,
    pub(super) just_pressed_keys: HashSet<String>,
    pub(super) extra_spawns:     HashMap<String, (f32, f32)>,
    pub(super) mouse_pos:        (f32, f32),
    pub(super) mouse_held:       (bool, bool),
    pub(super) mouse_pressed:    (bool, bool),

    pub(super) gamepad_held:     HashSet<(usize, String)>,
    pub(super) gamepad_pressed:  HashSet<(usize, String)>,
    pub(super) gamepad_axes:     HashMap<(usize, String), f32>,

    pub(super) globals:          HashMap<String, rhai::Dynamic>,
    /// Script-registered animation definitions (Step 3c), threaded through
    /// the same in/out-per-frame pattern as `globals` since — like
    /// globals — these live on `PlayState`, not `World`.
    pub(super) clips:            HashMap<String, AnimationClip>,
    /// Read-only per-entity `Animator::frame` snapshot backing `get_frame`.
    pub(super) animator_frames:  HashMap<i64, usize>,
    /// Entities whose `Animator` reached the last frame of a non-looping
    /// run on the tick this snapshot was taken from — what
    /// `clip_finished(id)` reads. Populated from `Animator::just_finished`,
    /// which is itself only ever true for the one tick that set it.
    pub(super) clip_finished:    HashSet<i64>,
    pub(super) persistent:       HashMap<String, rhai::Dynamic>,
    pub(super) camera_pos:       (f32, f32),
    pub(super) viewport_size:    (usize, usize),
    pub(super) pending_velocities: Vec<(i64, f32, f32)>,
    pub(super) pending_positions:  Vec<(i64, f32, f32)>,
    pub(super) pending_parents:    Vec<(i64, i64, bool)>,
    pub(super) pending_glyphs:     Vec<(i64, char)>,
    pub(super) pending_colors:     Vec<(i64, String, String)>,
    pub(super) pending_animations: Vec<(i64, Vec<char>, f32)>,
    pub(super) pending_textures:   Vec<(i64, Option<String>)>,
    pub(super) pending_hud_draws:  Vec<HudDraw>,
    pub(super) pending_particles:  Vec<ParticleRequest>,
    pub(super) clear_hud:          bool,
    pub(super) despawn_queue:      Vec<i64>,
    pub(super) spawn_queue:        Vec<SpawnRequest>,
    pub(super) pending_level:      Option<String>,
    pub(super) pending_save:       Option<String>,
    pub(super) pending_load:       Option<String>,
    pub(super) pending_logs:       Vec<String>,
    pub(super) pending_sounds:     Vec<String>,
    pub(super) pending_spatial_sounds: Vec<(String, f32, f32)>,
    pub(super) pending_music:      Option<String>,
    pub(super) stop_music:         bool,
    pub(super) pending_globals:    HashMap<String, rhai::Dynamic>,
    /// `register_clip` writes here rather than into `clips` directly, so a
    /// clip a script registers this frame only becomes visible (to that
    /// script or any other) starting next frame — the same "writes settle
    /// at frame end" convention every other pending_* queue follows.
    pub(super) pending_clip_defs:   Vec<(String, AnimationClip)>,
    /// (id, clip name, oneshot) — `play_clip`/`play_clip_once`.
    pub(super) pending_play_clip:   Vec<(i64, String, bool)>,
    pub(super) pending_stop_clip:   Vec<i64>,
    pub(super) pending_clip_speed:  Vec<(i64, f32)>,
    pub(super) pending_set_frame:   Vec<(i64, usize)>,
    pub(super) pending_persistent: HashMap<String, rhai::Dynamic>,
    pub(super) pending_camera:     Option<crate::math::Vec2>,
    pub(super) pending_shake:      Option<crate::play::ShakeState>,
    pub(super) pending_visibility: Vec<(i64, bool)>,
    pub(super) pending_z_order:    Vec<(i64, i32)>,
    pub(super) pending_tags:       Vec<(i64, String)>,
    pub(super) pending_collider_size:  Vec<(i64, f32, f32)>,
    pub(super) pending_collider_solid: Vec<(i64, bool)>,
    pub(super) pending_collider_layer: Vec<(i64, String)>,
    pub(super) pending_collider_locked: Vec<(i64, bool)>,
    pub(super) pending_collider_mask:  Vec<(i64, Vec<String>)>,
    pub(super) pending_timers:     Vec<(crate::world::EntityId, String, f64)>,
    pub(super) timers:             HashMap<crate::world::EntityId, HashMap<String, f64>>,
    pub(super) pending_turn:       bool,
}

impl ScriptState {
    pub(super) fn from_world(
        world: &World, delta_time: f32, elapsed: f32,
        input: Option<&InputManager>, mouse: Option<&crate::mouse::MouseState>,
        gamepad: Option<&crate::gamepad::GamepadState>,
        spawns: &[(String, f32, f32)], globals: HashMap<String, rhai::Dynamic>,
        clips: HashMap<String, AnimationClip>,
        persistent: HashMap<String, rhai::Dynamic>, camera_pos: crate::math::Vec2,
        viewport_size: (usize, usize),
    ) -> Self {
        let mut positions  = HashMap::new();
        let mut velocities = HashMap::new();
        let mut parents    = HashMap::new();
        let mut colliders  = HashMap::new();
        let mut tags       = HashMap::new();
        let mut glyphs     = HashMap::new();
        let mut colors     = HashMap::new();
        let mut textures   = HashMap::new();
        let mut tag_to_id  = HashMap::new();
        let mut tag_to_ids: HashMap<String, Vec<i64>> = HashMap::new();
        let mut visibility = HashMap::new();
        let mut z_orders   = HashMap::new();

        for (id, tf) in &world.transforms {
            let eid = *id as i64;
            positions.insert(eid,  (tf.position.x, tf.position.y));
            velocities.insert(eid, (tf.velocity.x, tf.velocity.y));
            if let Some(pid) = tf.parent { parents.insert(eid, pid as i64); }
            if let Some(sp) = world.sprites.get(id) {
                // bg only means something for a Glyph source; Texture/Clip
                // sprites report Reset ("no override"), matching how
                // draw_texture already ignored background entirely.
                let bg = match &sp.source {
                    SpriteSource::Glyph { ch, bg } => {
                        let glyph = if sp.frames.is_empty() { *ch }
                        else { let idx = (sp.frame_timer / sp.frame_rate) as usize % sp.frames.len(); sp.frames[idx] };
                        glyphs.insert(eid, glyph);
                        *bg
                    }
                    SpriteSource::Texture { path, .. } => {
                        textures.insert(eid, path.clone());
                        Color::Reset
                    }
                    SpriteSource::Clip { .. } => Color::Reset,
                };
                colors.insert(eid, (crate::scripting::types::color_to_name(sp.tint), crate::scripting::types::color_to_name(bg)));
                visibility.insert(eid, sp.visible);
                z_orders.insert(eid, sp.layer);
            }
        }
        for (id, col) in &world.colliders { colliders.insert(*id as i64, (col.width, col.height, col.solid, col.layer.clone(), col.mask.clone(), col.locked)); }
        for (id, tag) in &world.tags {
            let eid = *id as i64;
            tags.insert(eid, tag.name.clone());
            tag_to_id.entry(tag.name.clone()).or_insert(eid);
            tag_to_ids.entry(tag.name.clone()).or_default().push(eid);
        }
        let (held_keys, just_pressed_keys) = input.map(snapshot_keys).unwrap_or_default();
        let mouse_pos = mouse.map(|m| (m.cell_x as f32, m.cell_y as f32)).unwrap_or((0.0, 0.0));
        let mouse_held = mouse.map(|m| (m.left_held(), m.right_held())).unwrap_or((false, false));
        let mouse_pressed = mouse.map(|m| (m.left_just_pressed(), m.right_just_pressed())).unwrap_or((false, false));

        let mut gamepad_held = HashSet::new();
        let mut gamepad_pressed = HashSet::new();
        let mut gamepad_axes = HashMap::new();

        if let Some(gp) = gamepad {
            for &(id, btn) in &gp.held { gamepad_held.insert((id, btn.to_string())); }
            for &(id, btn) in &gp.consumed { gamepad_pressed.insert((id, btn.to_string())); }
            for (&(id, ax), &val) in &gp.axes { gamepad_axes.insert((id, ax.to_string()), val); }
        }

        let extra_spawns: HashMap<String, (f32, f32)> = spawns.iter().map(|(name, x, y)| (name.clone(), (*x, *y))).collect();

        let mut animator_frames = HashMap::new();
        let mut clip_finished = HashSet::new();
        for (id, animator) in &world.animators {
            let eid = *id as i64;
            animator_frames.insert(eid, animator.frame);
            if animator.just_finished { clip_finished.insert(eid); }
        }

        ScriptState {
            positions, velocities, parents, colliders, tags, glyphs, colors, textures, tag_to_id, tag_to_ids, visibility, z_orders,
            delta_time, elapsed, next_spawn_id: world.next_id, held_keys, just_pressed_keys, extra_spawns,
            mouse_pos, mouse_held, mouse_pressed,
            gamepad_held, gamepad_pressed, gamepad_axes,
            globals, clips, animator_frames, clip_finished, persistent, camera_pos: (camera_pos.x, camera_pos.y), viewport_size,
            pending_velocities: Vec::new(), pending_positions: Vec::new(), pending_parents: Vec::new(),
            pending_glyphs: Vec::new(), pending_colors: Vec::new(), pending_animations: Vec::new(), pending_textures: Vec::new(),
            pending_hud_draws: Vec::new(), pending_particles: Vec::new(), clear_hud: false, despawn_queue: Vec::new(),
            spawn_queue: Vec::new(), pending_level: None, pending_save: None, pending_load: None, pending_logs: Vec::new(),
            pending_sounds: Vec::new(), pending_spatial_sounds: Vec::new(),
            pending_music: None, stop_music: false, pending_globals: HashMap::new(),
            pending_clip_defs: Vec::new(), pending_play_clip: Vec::new(), pending_stop_clip: Vec::new(),
            pending_clip_speed: Vec::new(), pending_set_frame: Vec::new(),
            pending_persistent: HashMap::new(),
            pending_camera: None, pending_shake: None, pending_visibility: Vec::new(), pending_z_order: Vec::new(), pending_tags: Vec::new(),
            pending_collider_size: Vec::new(), pending_collider_solid: Vec::new(), pending_collider_layer: Vec::new(), pending_collider_mask: Vec::new(),
            pending_collider_locked: Vec::new(),
            pending_timers: Vec::new(), timers: HashMap::new(), pending_turn: false,
        }
    }
}
