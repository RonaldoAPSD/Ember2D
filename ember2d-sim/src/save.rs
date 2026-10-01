// save.rs — Save/Load system for Ember2D.

use crate::components::AnimationClip;
use crate::world::{EntityId, World};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;

/// Encapsulates the entire serializable state of a game session.
#[derive(Serialize, Deserialize)]
pub struct SaveState {
    /// The current state of the ECS world.
    pub world: World,
    /// Persistent script variables. `BTreeMap`, not `HashMap` (Step 5b,
    /// docs/ember2d-phase5-plan.md) — RON serializes a `HashMap` in
    /// whatever order its per-process-random hash state produces, so the
    /// same logical save could write out as different bytes on different
    /// runs. A `BTreeMap` always serializes key-sorted, so a save file's
    /// content is a pure function of the data — old RON saves still load
    /// unchanged either way, since RON's map syntax doesn't encode which
    /// Rust collection produced it.
    pub persistent: BTreeMap<String, rhai::Dynamic>,
    /// Level-scoped script state kept via `ctx.set_global`/`get_global`.
    /// Must survive a *mid-run* save/load or that state silently vanishes.
    /// **Defect D17** (docs/ember2d-refactor-plan.md §3), fixed in Phase 5
    /// Step 5c (docs/ember2d-phase5-plan.md) — before this field existed,
    /// `SaveState` held only `world` + `persistent`, so globals were
    /// dropped on every save. `#[serde(default)]` means a save file written
    /// before this field existed still loads, as an empty map — the same
    /// starting point a script sees on a fresh level load. The roguelike's
    /// per-enemy combat state (`hp`, `aware`) used to live here too, keyed
    /// by `"hp_" + id`/`"aware_" + id` — Step 7.5-3 (docs/ember2d-master-
    /// plan.md §5.6) moved that onto a real `Vars` component instead
    /// (`components/vars.rs`), which needs no entry in `SaveState` at all:
    /// it lives directly on `World`, which already serializes it as part of
    /// the `world` field below.
    #[serde(default)]
    pub globals: BTreeMap<String, rhai::Dynamic>,
    /// Script-registered animation clip definitions — see
    /// `PlayState::clips`'s doc comment. Lost on save/load for the same
    /// reason `globals` was, fixed the same way and in the same step.
    #[serde(default)]
    pub clips: BTreeMap<String, AnimationClip>,
    /// Path to the level file this session belongs to.
    pub level_path: String,
    /// R7 (7A-3, docs/ember2d-master-plan.md): how many turns the local
    /// player had completed at save time — `ctx.get_turn_number()`'s value.
    /// Without this, a load always resumed counting from 0. `#[serde(default)]`
    /// so a save from before this field existed still loads (resuming at 0,
    /// same as it always did).
    #[serde(default)]
    pub turn_number: u64,
    /// R7: the turn scheduler's exact (actor, due) state at save time —
    /// see `TurnScheduler::snapshot`'s own doc comment for why a full
    /// rebuild (`Simulation::rebuild_scheduler`, which resets every actor
    /// to the same due time) isn't a faithful round trip for a save taken
    /// mid-round. `#[serde(default)]` for the same reason `turn_number`
    /// has it — an older save falls back to the pre-7A-3 rebuild-from-
    /// scratch behavior (`Simulation::on_start`'s loading-save branch).
    #[serde(default)]
    pub scheduler: Vec<(EntityId, u64)>,
    /// Step 9-1 (docs/ember2d-master-plan.md §5.8): the scene stack at the
    /// moment of saving (their entities are in `world`). Empty for a save
    /// from before scenes existed.
    #[serde(default)]
    pub scenes: Vec<crate::simulation::scenes::SceneFrame>,
    /// Step 9-3: open menus and the dialogue box at the moment of saving.
    #[serde(default)]
    pub ui: crate::ui::UiModel,
}

impl SaveState {
    /// Create a new SaveState from the current engine components.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world: World,
        persistent: BTreeMap<String, rhai::Dynamic>,
        globals: BTreeMap<String, rhai::Dynamic>,
        clips: BTreeMap<String, AnimationClip>,
        level_path: String,
        turn_number: u64,
        scheduler: Vec<(EntityId, u64)>,
    ) -> Self {
        SaveState {
            world,
            persistent,
            globals,
            clips,
            level_path,
            turn_number,
            scheduler,
            scenes: Vec::new(),
            ui: Default::default(),
        }
    }

    /// Serialize the state to a RON string.
    pub fn to_ron(&self) -> Result<String, String> {
        let config = ron::ser::PrettyConfig::new().depth_limit(4).new_line("\n".to_string());
        ron::ser::to_string_pretty(self, config).map_err(|e| e.to_string())
    }

    /// Save the state to a file.
    ///
    /// Step 7.5-9 (docs/ember2d-master-plan.md §5.6): exempted from
    /// `clippy.toml`'s `disallowed-methods` — this and `load_from_file`
    /// below are the save format's real, permanent load/save entry points,
    /// called only between simulation runs (`ember2d-app`'s save/load
    /// menu), never reachable mid-step from a running `Simulation`, the
    /// actual hazard that lint exists to catch — same exemption
    /// `level.rs`'s `save`/`load` already document.
    #[allow(clippy::disallowed_methods)]
    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let ron = self.to_ron()?;
        fs::write(path, ron).map_err(|e| e.to_string())
    }

    // (Step 9.5-5's `write_data_file` is below, outside this impl.)

    /// Load a state from a RON string.
    pub fn from_ron(ron_str: &str) -> Result<Self, String> {
        ron::de::from_str(ron_str).map_err(|e| e.to_string())
    }

    /// Load a state from a file.
    #[allow(clippy::disallowed_methods)]
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_ron(&content)
    }
}

/// Step 9.5-5: writes one script value (`save_data`) as RON. Exempt from
/// `disallowed-methods` for the same reason `SaveState::save_to_file` is:
/// it's the data format's own save entry point, run between passes.
#[allow(clippy::disallowed_methods)]
pub fn write_data_file(path: &str, value: &rhai::Dynamic) -> Result<(), String> {
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default()).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_save_from_before_globals_and_clips_existed_still_loads() {
        // Defect D17 fix (Phase 5 Step 5c, docs/ember2d-phase5-plan.md)
        // added `globals`/`clips` to SaveState; #[serde(default)] is what
        // keeps an older save file (missing both fields entirely) loading
        // instead of erroring out — same convention as World's own
        // `animators` field (Step 3c, see world.rs's own such test).
        let pre_step_5c_ron = "(world:(next_id:1,transforms:{},sprites:{},colliders:{},tags:{},scripts:{}),persistent:{},level_path:\"x.level\")";
        let restored: SaveState = ron::de::from_str(pre_step_5c_ron)
            .expect("a SaveState RON with no globals/clips keys must still deserialize");
        assert!(restored.globals.is_empty());
        assert!(restored.clips.is_empty());
    }
}
