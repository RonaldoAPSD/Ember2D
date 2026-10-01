// scripting/collisions.rs — ScriptEngine::run_collisions: the on_collide
// dispatch pass, moved out of engine.rs at Step 7.5-8 (docs/ember2d-master-
// plan.md §5.6) once that file crossed CLAUDE.md's 750-line hard limit
// again (the D22 timer fix's own doc comments were what pushed it over this
// time). Same second-`impl ScriptEngine`-in-a-sibling-file pattern
// `apply.rs`/`lifecycle.rs` already established — pure relocation, nothing
// about this method changed, only location.

use std::collections::{BTreeMap, HashMap};

use crate::world::{EntityId, World};

use super::api::ScriptCtx;
use super::engine::ScriptEngine;
use super::state::{PassArgs, ScriptState};
use super::types::*;

impl ScriptEngine {
    pub fn run_collisions(
        &mut self,
        world: &mut World,
        pairs: &[(EntityId, EntityId)],
        log: &mut Vec<LogEntry>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
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
                globals: args.globals,
                clips: args.clips,
                persistent: std::mem::take(persistent),
                shake_state: None,
                clear_hud: false,
                particles: Vec::new(),
                commands: BTreeMap::new(),
                act_cost: None,
                despawned: Vec::new(),
                animations: Vec::new(),
                ..Default::default()
            };
        }

        let mut ctx_state = ScriptState::from_world(world, &self.layers, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx(); // Step 9-1
        ctx_state.camera_view = self.camera_view; // Step 9-2
        ctx_state.ui = self.ui_ctx(); // Step 9-3
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, other_id, path) in calls {
            let entity_ctx = ctx.with_entity(entity_id);
            self.call_lifecycle_fn(
                &path,
                "on_collide",
                "on_collide",
                (entity_id, other_id, entity_ctx),
                log,
            );
        }
        self.apply_ctx(ctx, world, log)
    }
}
