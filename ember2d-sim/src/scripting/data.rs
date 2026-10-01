// scripting/data.rs — small named values a game keeps on disk between
// runs: `save_data(path, value)` and `load_data(path)`.
//
// Step 9.5-5 (docs/ember2d-master-plan.md §5.8.5), for the shooter's high
// score and options. `save_game`/`load_game` store a whole game — world,
// scenes, everything — which is the wrong tool for "the best score anyone
// has reached on this machine": that has to survive starting a NEW game.
// These store one script value (a number, a string, a map of them) in its
// own small RON file instead.
//
// Paths work exactly as `save_game`'s do (as given — relative to the
// working directory). Writing happens where `save_game`'s does, after the
// pass (`Simulation::apply_script_result`, through `save::write_data_file`);
// reading happens at once, through the level source the simulation was
// given (`LevelSource::read_to_string`), so this crate still opens no file
// itself. A missing or unreadable file reads as `()`.
//
// Not replay-safe, by nature: a loaded value is whatever is on this disk.
// Use it for what a replay needn't reproduce — a high score, an option.

use rhai::Dynamic;

use super::api::ScriptCtx;

impl ScriptCtx {
    /// Store `value` in the file at `path` (written after this pass).
    pub fn save_data(&mut self, path: String, value: Dynamic) {
        self.inner.borrow_mut().pending_data_saves.push((path, value));
    }

    /// The value stored at `path`, or `()` if there's none (or it can't be
    /// read). Reads the file now, not the version a `save_data` earlier in
    /// this same pass is about to write.
    pub fn load_data(&mut self, path: String) -> Dynamic {
        let source = self.inner.borrow().data_source.clone();
        let Some(source) = source else { return Dynamic::UNIT };
        if !source.exists(&path) {
            return Dynamic::UNIT;
        }
        source
            .read_to_string(&path)
            .ok()
            .and_then(|text| ron::de::from_str::<Dynamic>(&text).ok())
            .unwrap_or(Dynamic::UNIT)
    }
}

pub(super) fn register(engine: &mut rhai::Engine) {
    engine.register_fn("save_data", ScriptCtx::save_data);
    engine.register_fn("load_data", ScriptCtx::load_data);
}
