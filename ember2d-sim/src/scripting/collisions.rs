// scripting/collisions.rs — ScriptEngine::run_collisions: the on_collide
// dispatch pass, moved out of engine.rs at Step 7.5-8 (docs/ember2d-master-
// plan.md §5.6) once that file crossed CLAUDE.md's 750-line hard limit
// again (the D22 timer fix's own doc comments were what pushed it over this
// time). Same second-`impl ScriptEngine`-in-a-sibling-file pattern
// `apply.rs`/`lifecycle.rs` already established — pure relocation, nothing
// about this method changed, only location.

use std::collections::{BTreeMap, HashMap};

use crate::command::{GamepadSnapshot, InputSnapshot, MouseSnapshot};
use crate::components::AnimationClip;
use crate::world::{EntityId, World};

use super::api::ScriptCtx;
use super::engine::ScriptEngine;
use super::state::ScriptState;
use super::types::*;

impl ScriptEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn run_collisions(
        &mut self,
        world: &mut World,
        pairs: &[(EntityId, EntityId)],
        log: &mut Vec<LogEntry>,
        delta_time: f32,
        elapsed: f32,
        spawns: &[(String, f32, f32)],
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        camera_pos: crate::math::Vec2,
        viewport_size: (usize, usize),
    ) -> ScriptUpdateResult {
        let scripted_paths: HashMap<EntityId, String> =
            world.scripts.iter().map(|(id, s)| (*id, s.path.clone())).collect();
        let mut calls: Vec<(i64, i64, String)> = Vec::new();
        for &(a, b) in pairs {
            if let Some(p) = scripted_paths.get(&a) {
                calls.push((a as i64, b as i64, p.clone()));
            }
            if let Some(p) = scripted_paths.get(&b) {
                calls.push((b as i64, a as i64, p.clone()));
            }
        }

        // Phase 6 Step 5 (docs/ember2d-phase6-plan.md): `calls` is built
        // BEFORE the `WorldSnapshot` a `ScriptState` would carry, specifically
        // so this can bail out here — with no `on_collide` about to run,
        // there's nothing for a snapshot to back, and building one is a full
        // O(entities) pass (`WorldSnapshot::build`'s own doc comment) for a
        // return value nothing would read. On most turn-resolving steps a
        // colliding pair doesn't involve a scripted entity at all, so this
        // is the common case, not an edge case — this is deliberately NOT
        // done by sharing `step`'s own snapshot instead: `late_step` calls
        // `resolve_solid_collision` on `world` between building `pairs` and
        // calling this function, so a snapshot taken before that would carry
        // stale positions.
        //
        // `globals`/`clips`/`persistent` still have to come straight back
        // out unlanded, exactly what `apply_ctx` would return from a pass
        // that ran zero scripts — every other field here is that same
        // pass's quiescent default (nothing spawned, despawned, drawn, or
        // submitted).
        if calls.is_empty() {
            return ScriptUpdateResult {
                pending_level: None,
                pending_save: None,
                pending_load: None,
                globals,
                clips,
                persistent: std::mem::take(persistent),
                camera_override: None,
                shake_state: None,
                clear_hud: false,
                particles: Vec::new(),
                commands: BTreeMap::new(),
                act_cost: None,
                despawned: Vec::new(),
                animations: Vec::new(),
            };
        }

        let mut ctx_state = ScriptState::from_world(
            world,
            &self.layers,
            delta_time,
            elapsed,
            InputSnapshot::default(),
            MouseSnapshot::default(),
            GamepadSnapshot::default(),
            spawns,
            globals,
            clips,
            std::mem::take(persistent),
            camera_pos,
            BTreeMap::new(),
            0,
            viewport_size,
        );
        ctx_state.timers = std::mem::take(&mut self.timers);
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, other_id, path) in calls {
            if self.disabled_scripts.contains(&path) {
                continue;
            }
            let Some(ast) = self.ast_cache.get(&path) else { continue };
            let scope = self.scopes.entry(entity_id as EntityId).or_default();
            let entity_ctx = ctx.with_entity(entity_id);
            if let Err(e) = self.engine.call_fn::<()>(
                scope,
                ast,
                "on_collide",
                (entity_id, other_id, entity_ctx),
            ) {
                if !Self::is_missing_optional_fn(&e, "on_collide") {
                    log.push(LogEntry::error(format!("on_collide '{}': {}", path, e)));
                    self.disabled_scripts.insert(path.clone());
                }
            }
        }
        self.apply_ctx(ctx, world, log)
    }
}
