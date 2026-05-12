// save.rs — Save/Load system for Ember2D.

use std::collections::HashMap;
use std::fs;
use serde::{Serialize, Deserialize};
use crate::world::World;

/// Encapsulates the entire serializable state of a game session.
#[derive(Serialize, Deserialize)]
pub struct SaveState {
    /// The current state of the ECS world.
    pub world: World,
    /// Persistent script variables.
    pub persistent: HashMap<String, rhai::Dynamic>,
    /// Path to the level file this session belongs to.
    pub level_path: String,
}

impl SaveState {
    /// Create a new SaveState from the current engine components.
    pub fn new(world: World, persistent: HashMap<String, rhai::Dynamic>, level_path: String) -> Self {
        SaveState { world, persistent, level_path }
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
