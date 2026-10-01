// scripting/apply.rs — ScriptEngine::apply_ctx: folds one ScriptCtx's
// pending_* write queues into World (and ScriptEngine's own pending-audio/
// HUD queues), producing the ScriptUpdateResult a caller (run_on_start_all/
// run_on_input/run_on_turn/run_scripts/run_collisions, all in engine.rs)
// applies to its own state.
//
// Split into its own file rather than left in engine.rs: engine.rs was at
// 573/600 lines (CLAUDE.md's hard limit) before this move, and Phase 6
// (docs/ember2d-phase6-plan.md) edits engine.rs directly across five more
// steps — a pure relocation now, matching the same second-`impl
// ScriptEngine`-in-a-sibling-file pattern `api_animation.rs` already
// established for `ScriptCtx` in Phase 5.5. Nothing here changed behavior,
// only location.

use std::collections::BTreeMap;

use crate::command::Command;
use crate::components::{
    AnimationClip, Animator, Collider, Script, Sprite, SpriteSource, Tag, Transform,
};
use crate::world::{EntityId, World};

use super::api::ScriptCtx;
use super::engine::ScriptEngine;
use super::types::*;

impl ScriptEngine {
    pub(super) fn apply_ctx(
        &mut self,
        ctx: ScriptCtx,
        world: &mut World,
        log: &mut Vec<LogEntry>,
    ) -> ScriptUpdateResult {
        let mut state = ctx.inner.borrow_mut();

        // Spawns are applied first, ahead of every other pending_* queue below:
        // a script that spawns an entity and immediately calls a setter on the
        // returned id (e.g. `set_layer_order`) needs that entity to already exist
        // by the time this pass reaches the setter's queue, or the setter
        // silently no-ops against a nonexistent entity until next frame.
        for req in state.spawn_queue.drain(..) {
            world.next_id = req.id + 1;
            let id = req.id;
            world.transforms.insert(id, Transform::new(req.x, req.y));
            world.sprites.insert(id, Sprite::new(req.glyph, req.fg, req.bg, req.z));
            if !req.tag.is_empty() {
                world.add_tag(id, Tag::new(&req.tag));
            }
            let mut col = Collider::new(req.w, req.h);
            col.solid = req.solid;
            col.set_layer(&self.layers, req.layer);
            world.add_collider(id, col);
        }

        for &(id, vx, vy) in &state.pending_velocities {
            if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) {
                tf.velocity.x = vx;
                tf.velocity.y = vy;
            }
        }
        for &(id, x, y) in &state.pending_positions {
            if let Some(tf) = world.transforms.get_mut(&(id as EntityId)) {
                tf.position.x = x;
                tf.position.y = y;
            }
        }
        for &(id, pid, keep_world) in &state.pending_parents {
            let parent = if pid < 0 { None } else { Some(pid as EntityId) };
            world.set_parent(id as EntityId, parent, keep_world);
        }
        // set_glyph only means something for a Glyph-sourced sprite —
        // silently does nothing otherwise, same "setters on the wrong kind
        // of entity are a no-op" convention every other setter here follows.
        for &(id, ch) in &state.pending_glyphs {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                if let SpriteSource::Glyph { ch: c, .. } = &mut sp.source {
                    *c = ch;
                }
            }
        }
        for (id, f, b) in state.pending_colors.drain(..) {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.tint = parse_color(&f);
                if let SpriteSource::Glyph { bg, .. } = &mut sp.source {
                    *bg = parse_color(&b);
                }
            }
        }
        for (id, v) in state.pending_visibility.drain(..) {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.visible = v;
            }
        }
        for (id, z) in state.pending_z_order.drain(..) {
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.layer = z;
            }
        }
        // R10 (7A-1, docs/ember2d-master-plan.md): `set_tag` on a missing
        // entity used to insert into `world.tags` anyway — a "ghost" tag
        // component with no `Transform`/`Sprite` behind it, invisible to
        // `entity_ids()`'s union of every OTHER store but still sitting in
        // this one. Guarded the same way `world.transforms.get_mut` already
        // guards every other setter below: existence checked first, no-op
        // if it's missing.
        for (id, t) in state.pending_tags.drain(..) {
            if world.transforms.contains_key(&(id as EntityId)) {
                world.add_tag(id as EntityId, Tag::new(&t));
            }
        }
        // Step 7.5-3 (docs/ember2d-master-plan.md §5.6): same ghost-
        // component guard `pending_tags` above already established for R10
        // — no `Vars` entry for an entity nothing else spawned this pass.
        for (id, key, write) in state.pending_vars.drain(..) {
            if !world.transforms.contains_key(&(id as EntityId)) {
                continue;
            }
            let entry = world.vars.entry(id as EntityId).or_default();
            match write {
                PendingWrite::Set(v) => {
                    entry.values.insert(key, v);
                }
                PendingWrite::Remove => {
                    entry.values.remove(&key);
                }
            }
        }
        // Step 7.5-5 (docs/ember2d-master-plan.md §5.6): same ghost-
        // component guard `pending_tags`/`pending_vars` above already
        // established for R10 — no `Script` for an entity nothing else
        // spawned this pass. A path that fails to compile is never
        // attached (`compile` already logs the error) — matches the sim
        // boundary's "a script can never crash the editor" rule, not
        // "wrong path attaches anyway and errors every step after." A
        // successful attach queues the entity onto `pending_on_start` so
        // `run_scripts` calls its `on_start` at the very next step — see
        // `pending_set_script`'s own doc comment (state.rs) for why not
        // this same pass.
        for (id, path) in state.pending_set_script.drain(..) {
            if !world.transforms.contains_key(&(id as EntityId)) {
                continue;
            }
            // Step 9-6: project-relative, like the level's own scripts.
            let path = self.resolve_path(&path);
            if self.compile(&path, log) {
                world.scripts.insert(id as EntityId, Script::new(path));
                self.pending_on_start.push(id as EntityId);
            }
        }
        for (id, w, h) in state.pending_collider_size.drain(..) {
            if let Some(col) = world.colliders.get_mut(&(id as EntityId)) {
                col.width = w;
                col.height = h;
            }
        }
        for (id, s) in state.pending_collider_solid.drain(..) {
            if let Some(col) = world.colliders.get_mut(&(id as EntityId)) {
                col.solid = s;
            }
        }
        for (id, l) in state.pending_collider_layer.drain(..) {
            if let Some(col) = world.colliders.get_mut(&(id as EntityId)) {
                col.set_layer(&self.layers, l);
            }
        }
        for (id, l) in state.pending_collider_locked.drain(..) {
            if let Some(col) = world.colliders.get_mut(&(id as EntityId)) {
                col.locked = l;
            }
        }
        for (id, m) in state.pending_collider_mask.drain(..) {
            if let Some(col) = world.colliders.get_mut(&(id as EntityId)) {
                col.set_mask(&self.layers, m);
            }
        }
        for (id, speed) in state.pending_speed.drain(..) {
            if let Some(actor) = world.actors.get_mut(&(id as EntityId)) {
                actor.speed = speed;
            }
        }
        // Clearing (set_texture(id, "") -> None here) has no defined
        // behavior under the SpriteSource model — there's no stored
        // "previous glyph" to revert to, so it's a no-op. Nothing in the
        // demo (or any known script) relies on clear-to-glyph; revisit if
        // real usage needs it.
        for (id, p) in state.pending_textures.drain(..) {
            if let Some(path) = p {
                // Step 9-6: project-relative, like a tile's own texture.
                let path = self.resolve_path(&path);
                if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                    sp.source = SpriteSource::Texture { path, src: None };
                }
            }
        }
        // play_clip/play_clip_once both (re)point the sprite at the clip by
        // name AND (re)start its Animator — the two used to be one call
        // (`set_animation`) before Step 3c split "what to show" from
        // "where playback is", so this is where they get reunited.
        // R10 (7A-1): same ghost-component bug as `pending_tags` above,
        // for `play_clip`/`play_clip_once` — `.entry(id).or_insert_with`
        // would happily create an `Animator` for an entity with no other
        // component at all.
        for (id, name, oneshot) in state.pending_play_clip.drain(..) {
            if !world.transforms.contains_key(&(id as EntityId)) {
                continue;
            }
            let animator = world
                .animators
                .entry(id as EntityId)
                .or_insert_with(|| Animator::new(name.clone()));
            animator.clip = name.clone();
            animator.frame = 0;
            animator.elapsed = 0.0;
            animator.playing = true;
            animator.oneshot = oneshot;
            if let Some(sp) = world.sprites.get_mut(&(id as EntityId)) {
                sp.source = SpriteSource::Clip { name };
            }
        }
        for id in state.pending_stop_clip.drain(..) {
            if let Some(a) = world.animators.get_mut(&(id as EntityId)) {
                a.playing = false;
            }
        }
        // R5 (7A-1): clamped to a finite, bounded range before it ever
        // reaches `Animator.speed` — `Animator::advance`'s own R5 comment
        // covers the loop-bound half of this fix; this half stops an
        // extreme scripted speed (or a NaN one) from getting there at all.
        for (id, speed) in state.pending_clip_speed.drain(..) {
            if let Some(a) = world.animators.get_mut(&(id as EntityId)) {
                a.speed = if speed.is_finite() { speed.clamp(0.0, 64.0) } else { 0.0 };
            }
        }
        for (id, frame) in state.pending_set_frame.drain(..) {
            if let Some(a) = world.animators.get_mut(&(id as EntityId)) {
                a.frame = frame;
                a.elapsed = 0.0;
            }
        }
        let clip_defs: Vec<(String, AnimationClip)> = std::mem::take(&mut state.pending_clip_defs);
        for (name, clip) in clip_defs {
            state.clips.insert(name, clip);
        }
        self.pending_hud_draws.append(&mut state.pending_hud_draws);
        self.pending_sounds.append(&mut state.pending_sounds);
        self.pending_spatial_sounds.append(&mut state.pending_spatial_sounds);
        if state.pending_music.is_some() {
            self.pending_music = state.pending_music.take();
        }
        if state.stop_music {
            self.stop_music = true;
            state.stop_music = false;
        }
        for msg in state.pending_logs.drain(..) {
            log.push(LogEntry::info(msg));
        }
        // `BTreeMap` has no `.drain()` (unlike `HashMap`/`Vec`) — `mem::take`
        // swaps in an empty map and hands back the old one to iterate, same
        // effect as drain-then-clear.
        // R32 (7.5-1, docs/ember2d-master-plan.md §5.6): was `if
        // v.is_unit() { remove } else { insert }` against a raw
        // `rhai::Dynamic` — indistinguishable from a script legitimately
        // calling `set_global("k", ())` to store unit, which this same
        // check silently turned into a delete. `PendingWrite`'s own
        // `Set`/`Remove` variants carry that distinction explicitly now.
        let globals_to_apply: Vec<(String, PendingWrite)> =
            std::mem::take(&mut state.pending_globals).into_iter().collect();
        for (k, v) in globals_to_apply {
            match v {
                PendingWrite::Set(v) => {
                    state.globals.insert(k, v);
                }
                PendingWrite::Remove => {
                    state.globals.remove(&k);
                }
            }
        }

        // R9 (7A-1): `clear_all_persistent`'s real effect now lands here —
        // see `ScriptState::pending_persistent_clear_all`'s own doc comment
        // for why this replaced clearing `pending_persistent` directly.
        // Ordered before `pending_persistent` is applied so a same-pass
        // `set_persistent` call after the clear still lands.
        if state.pending_persistent_clear_all {
            state.persistent.clear();
            state.pending_persistent_clear_all = false;
        }
        // R32 (7.5-1, docs/ember2d-master-plan.md §5.6): same
        // `PendingWrite` fix as `pending_globals` above.
        let persistent_to_apply: Vec<(String, PendingWrite)> =
            std::mem::take(&mut state.pending_persistent).into_iter().collect();
        for (k, v) in persistent_to_apply {
            match v {
                PendingWrite::Set(v) => {
                    state.persistent.insert(k, v);
                }
                PendingWrite::Remove => {
                    state.persistent.remove(&k);
                }
            }
        }
        // Phase 6 Step 9 (docs/ember2d-phase6-plan.md): writes straight into
        // `state.timers` (which holds everything `mem::take`n out of
        // `self.timers` at the top of whichever `run_*` method built this
        // pass — see that field's own doc comment) instead of a `__timer_`-
        // prefixed `Scope` variable. Step 7.5-8 (D22 fix): each `TimerWrite`
        // variant is now its own real transition instead of a magic-number
        // sentinel — see `TimerState`'s own doc comment (types.rs) for the
        // ambiguity this replaces (a cancelled timer and a just-consumed one
        // used to collapse to the same stored value). Collected into an
        // owned `Vec` first, same reason `globals_to_apply`/
        // `persistent_to_apply` above are: `state` is a `RefCell` `RefMut`,
        // so `state.pending_timers.drain(..)` and `state.timers.entry(...)`
        // can't be live at once — the borrow checker can't see the two
        // fields are disjoint through the `DerefMut` boundary the way it
        // can for a plain struct.
        let pending_timers: Vec<(EntityId, String, TimerWrite)> =
            std::mem::take(&mut state.pending_timers);
        for (id, name, write) in pending_timers {
            let entry = state.timers.entry(id).or_default();
            match write {
                TimerWrite::Start(duration) => {
                    entry.insert(name, TimerState::Running(duration));
                }
                TimerWrite::Cancel => {
                    entry.insert(name, TimerState::Cancelled);
                }
                TimerWrite::Consume => {
                    entry.insert(name, TimerState::Consumed);
                }
            }
        }
        // Step 5e: unlike `globals`/`persistent`, commands don't merge with
        // whatever `state.commands` was read from — a fresh set built
        // purely from this pass's `ctx.submit()` calls, keyed by actor id
        // (see `ScriptUpdateResult::commands`'s doc comment).
        let commands: BTreeMap<i64, Command> = std::mem::take(&mut state.pending_commands)
            .into_iter()
            .map(|c| (c.actor as i64, c))
            .collect();
        let act_cost = state.pending_act_cost.take();
        // Phase 6 Step 3 (docs/ember2d-phase6-plan.md): `mem::take`, not
        // `.clone()` — `state` (and therefore `state.globals`/`.clips`/
        // `.persistent`) is dropped a few lines below and never read again,
        // so there's nothing left to preserve a copy for. This is the
        // matching half of every call site's own take (`Simulation::step`/
        // `on_start`/`late_step`, and each `run_*` method's own
        // `persistent` take) — together they turn what used to be 18-24
        // full map clones per step into pointer swaps.
        let result = ScriptUpdateResult {
            pending_level: state.pending_level.take(),
            pending_save: state.pending_save.take(),
            pending_load: state.pending_load.take(),
            globals: std::mem::take(&mut state.globals),
            clips: std::mem::take(&mut state.clips),
            persistent: std::mem::take(&mut state.persistent),
            camera: std::mem::take(&mut state.pending_camera),
            shake_state: state.pending_shake.take(),
            clear_hud: state.clear_hud,
            particles: std::mem::take(&mut state.pending_particles),
            commands,
            act_cost,
            despawned: state.despawn_queue.iter().map(|&id| id as EntityId).collect(),
            animations: std::mem::take(&mut state.pending_animations),
            scene_ops: std::mem::take(&mut state.scene.ops),
            flow: state.scene.flow.take(),
            ui_ops: std::mem::take(&mut state.ui.ops),
            // Step 9-7: size/flip/y-sort land now; regions and project
            // clips go up to `Simulation` to be found.
            sprite_requests: super::sprite::apply_sprite_ops(
                world,
                std::mem::take(&mut state.sprite_ops),
            ),
            // Step 9.5-1: all of them go up to `Simulation`, which has the
            // layer registry and the tileset loader they need.
            tile_ops: std::mem::take(&mut state.tile_ops),
            fov_ops: std::mem::take(&mut state.fov_ops), // Step 9.5-2
        };
        // Phase 6 Step 9: the matching half of every call site's own
        // `ctx_state.timers = std::mem::take(&mut self.timers)` — timers
        // never surface through `ScriptUpdateResult` (they're purely
        // internal `ScriptEngine` state, unlike globals/clips/persistent,
        // which `Simulation` itself owns), so this restore happens here
        // directly rather than via the caller.
        self.timers = std::mem::take(&mut state.timers);
        state.clear_hud = false;
        let despawn_ids = state.despawn_queue.clone();
        drop(state);
        for id in despawn_ids {
            world.despawn(id as EntityId);
            // Step 9: a despawned entity's stale timer would otherwise leak
            // forever (harmless, since ids never get reused within a level,
            // but still dead weight; see `check_hot_reload`'s matching
            // cleanup for the other lifecycle event that must clear this).
            self.timers.remove(&(id as EntityId));
        }
        result
    }
}
