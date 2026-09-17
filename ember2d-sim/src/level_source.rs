// level_source.rs — LevelSource: the one seam through which a running
// Simulation can ever ask about a file, Step 7.5-9 (docs/ember2d-master-
// plan.md §5.6, R17/R41 fix).
//
// CLAUDE.md's Determinism section forbids filesystem access inside
// `ember2d-sim` — but a level transition (`ctx.load_level`, stepping onto
// an exit tile) and a node-graph tile's script-source combine
// (`simulation/spawn.rs::do_on_start`) both genuinely need to read a file
// mid-step. Before this step, `simulation.rs`/`simulation/spawn.rs` called
// `std::fs::read_to_string`/`Path::exists`/`LevelData::load` directly —
// real violations of the rule, tracked as R17 (and R41's `eprintln!`
// cousin in `world.rs`), allow-listed in `scripts/check.ps1`/`check.sh`
// specifically so this step could fix them for real instead of the
// allowlist becoming permanent.
//
// `Simulation` holds a `Box<dyn LevelSource>` and calls through it for
// everything file-shaped; `ember2d-sim` itself never calls `std::fs`
// again anywhere reachable from a running step. The default
// (`NullLevelSource`) does zero I/O and reports "not found" for
// everything — deliberately not a working implementation, so the
// structural guarantee ("this crate cannot touch a filesystem") holds
// even for a `Simulation` nobody configured. `ember2d`'s `FsLevelSource`
// (ember2d/src/level_source.rs) is the real, disk-backed implementation
// every actual game wires up; a test that needs real level transitions
// (`ember2d/tests/common/mod.rs`'s `TurnHarness`) does the same. A test
// that doesn't touch level transitions or graph-script sidecars (most of
// them) never needs to know this trait exists at all.

use crate::level::LevelData;

pub trait LevelSource {
    /// Whether `path` names a real file, from whatever "real" means to
    /// this implementation (the OS filesystem for `FsLevelSource`, an
    /// in-memory map for a test double). What `resolve_exit_path` uses to
    /// decide "does this string already resolve on its own, or should it
    /// be joined against the current level's own directory" — see that
    /// function's own doc comment (simulation.rs).
    fn exists(&self, path: &str) -> bool;
    /// The raw text at `path` — what a node-graph tile's script-source
    /// combine (`do_on_start`, simulation/spawn.rs) needs: the graph's own
    /// generated Rhai source plus whatever `tile.script` already pointed
    /// to, concatenated.
    fn read_to_string(&self, path: &str) -> Result<String, String>;
    /// Loads and parses a whole level file — what a level transition
    /// (`late_step`'s exit-tile handling, `ctx.load_level` via
    /// `apply_script_result`) needs.
    fn load_level(&self, path: &str) -> Result<LevelData, String>;
}

/// `Simulation`'s default — see this module's own header comment for why
/// "does nothing, reports not found" is the correct default rather than a
/// working one.
pub struct NullLevelSource;

impl LevelSource for NullLevelSource {
    fn exists(&self, _path: &str) -> bool {
        false
    }
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        Err(format!("no LevelSource configured (wanted to read '{path}')"))
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        Err(format!("no LevelSource configured (wanted to load '{path}')"))
    }
}
