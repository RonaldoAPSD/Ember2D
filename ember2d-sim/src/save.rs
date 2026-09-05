// save.rs — Save/Load system for Ember2D.

use std::collections::BTreeMap;
use std::fs;
use serde::{Serialize, Deserialize};
use crate::components::AnimationClip;
use crate::world::World;

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
    /// Per-entity script state kept via `ctx.set_global`/`get_global` — the
    /// roguelike's whole combat model (`hp_<id>`, `aware_<id>`, …) lives
    /// here. Level-scoped in normal play (resets on
    /// every level load), but must survive a *mid-run* save/load or that
    /// state silently vanishes. **Defect D17** (docs/ember2d-refactor-plan.md
    /// §3), fixed in Phase 5 Step 5c (docs/ember2d-phase5-plan.md) — before
    /// this field existed, `SaveState` held only `world` + `persistent`, so
    /// globals were dropped on every save. `#[serde(default)]` means a save
    /// file written before this field existed still loads, as an empty map
    /// — the same starting point a script sees on a fresh level load.
    #[serde(default)]
    pub globals: BTreeMap<String, rhai::Dynamic>,
    /// Script-registered animation clip definitions — see
    /// `PlayState::clips`'s doc comment. Lost on save/load for the same
    /// reason `globals` was, fixed the same way and in the same step.
    #[serde(default)]
    pub clips: BTreeMap<String, AnimationClip>,
    /// Path to the level file this session belongs to.
    pub level_path: String,
}

impl SaveState {
    /// Create a new SaveState from the current engine components.
    pub fn new(world: World, persistent: BTreeMap<String, rhai::Dynamic>, globals: BTreeMap<String, rhai::Dynamic>, clips: BTreeMap<String, AnimationClip>, level_path: String) -> Self {
        SaveState { world, persistent, globals, clips, level_path }
    }

    /// Serialize the state to a RON string.
    pub fn to_ron(&self) -> Result<String, String> {
        let config = ron::ser::PrettyConfig::new()
            .depth_limit(4)
            .new_line("\n".to_string());
        ron::ser::to_string_pretty(self, config).map_err(|e| e.to_string())
    }

    /// Save the state to a file.
    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let ron = self.to_ron()?;
        fs::write(path, ron).map_err(|e| e.to_string())
    }

    /// Load a state from a RON string.
    pub fn from_ron(ron_str: &str) -> Result<Self, String> {
        ron::de::from_str(ron_str).map_err(|e| e.to_string())
    }

    /// Load a state from a file.
    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_ron(&content)
    }
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
        let restored: SaveState = ron::de::from_str(pre_step_5c_ron).expect("a SaveState RON with no globals/clips keys must still deserialize");
        assert!(restored.globals.is_empty());
        assert!(restored.clips.is_empty());
    }
}
