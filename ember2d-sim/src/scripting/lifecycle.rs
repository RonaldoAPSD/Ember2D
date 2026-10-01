// scripting/lifecycle.rs — ScriptEngine::run_on_start_all/run_on_load_all:
// the two lifecycle passes called exactly once each by `Simulation::on_start`
// (fresh spawn vs. loaded save), moved out of engine.rs at Step 7.5-5
// (docs/ember2d-master-plan.md §5.6) once that file crossed CLAUDE.md's
// 750-line hard limit. Same second-`impl ScriptEngine`-in-a-sibling-file
// pattern `apply.rs` established (Phase 6 Step 2) — pure relocation, nothing
// about either method changed, only location.

use std::collections::BTreeMap;

use crate::world::World;

use super::api::ScriptCtx;
use super::engine::ScriptEngine;
use super::state::{PassArgs, ScriptState};
use super::types::*;

impl ScriptEngine {
    /// Called once, only on a fresh spawn (`Simulation::on_start`'s
    /// `!is_loading_save` branch) — never on a loaded save, where
    /// `run_on_load_all` below runs instead. Kept as two separate methods
    /// rather than one parameterized by function name: they already
    /// diverge in what they mean to a script (`on_start` = fresh gameplay
    /// state; `on_load` = re-derive presentation state without resetting
    /// gameplay state, see `run_on_load_all`'s own doc comment), and every
    /// other lifecycle pass in engine.rs (`run_on_input`/`run_on_turn`) is
    /// already its own method rather than a shared one parameterized by fn
    /// name.
    pub fn run_on_start_all(
        &mut self,
        world: &mut World,
        log: &mut Vec<LogEntry>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
    ) -> ScriptUpdateResult {
        let scripted: Vec<(i64, String)> =
            world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let mut ctx_state =
            ScriptState::from_world(world, &self.layers, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx(); // Step 9-1
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, path) in &scripted {
            let entity_ctx = ctx.with_entity(*entity_id);
            self.call_lifecycle_fn(path, "on_start", "on_start", (*entity_id, entity_ctx), log);
        }
        self.apply_ctx(ctx, world, log)
    }

    /// Step 7.5-5 (docs/ember2d-master-plan.md §5.6): the loaded-save
    /// counterpart to `run_on_start_all` above — called once, only from
    /// `Simulation::on_start`'s `is_loading_save` branch, for every
    /// scripted entity a save deserialized. `on_start` deliberately never
    /// runs on this path (see that branch's own comment) since a script's
    /// `on_start` is where FRESH gameplay state gets seeded — re-running it
    /// on load would reset a run already in progress back to its opening
    /// values. `on_load(id, ctx)` exists so a script still gets a one-time
    /// hook here to re-derive whatever PRESENTATION-only state it keeps
    /// that a save doesn't carry (an animation clip pointer freshly
    /// registered via `register_clip` every level load, say) without
    /// touching `persistent`/`globals`, which the save already restored
    /// faithfully (R7). Missing `on_load` is exactly as fine as a missing
    /// `on_start` — most scripts need neither.
    pub fn run_on_load_all(
        &mut self,
        world: &mut World,
        log: &mut Vec<LogEntry>,
        persistent: &mut BTreeMap<String, rhai::Dynamic>,
        args: PassArgs,
    ) -> ScriptUpdateResult {
        let scripted: Vec<(i64, String)> =
            world.scripts.iter().map(|(id, s)| (*id as i64, s.path.clone())).collect();
        let mut ctx_state =
            ScriptState::from_world(world, &self.layers, std::mem::take(persistent), args);
        ctx_state.timers = std::mem::take(&mut self.timers);
        ctx_state.scene = self.scene_ctx(); // Step 9-1
        let ctx = ScriptCtx::new(ctx_state, self.rng.clone());
        for (entity_id, path) in &scripted {
            let entity_ctx = ctx.with_entity(*entity_id);
            self.call_lifecycle_fn(path, "on_load", "on_load", (*entity_id, entity_ctx), log);
        }
        self.apply_ctx(ctx, world, log)
    }
}

