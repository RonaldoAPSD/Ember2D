// editor/impl_state/graph_sidecars.rs — save-time node-graph-to-Rhai
// export, split out of `impl_state/mod.rs` (7D-3, docs/ember2d-master-plan.md
// §5.4) once that file had no room left under CLAUDE.md's 750-line hard
// limit for the R70 fix (`switch_to_level`'s theme/font/prefs carry-over).
// `migrate_graph_sidecars` is unchanged from its own copy there — purely a
// location move. (Not folded into the existing `export.rs` in this same
// directory — that file is the unrelated "Export Standalone Game" feature;
// sharing a name with this one would be confusing, not economical.)

use super::super::EditorState;
use ember2d::play::resolve_exit_path;
use ember2d_sim::graph as node_graph;
use ember2d_sim::level::LevelData;
use std::path::Path;

impl EditorState {
    /// Level format v2 (Step 3d): for each tile carrying a live node-graph
    /// (editor-authoring state — see `TileRecord::graph`'s doc comment),
    /// generate its Rhai source, combine it with whatever `tile.script`
    /// already pointed to (matching `play/spawn.rs::do_on_start`'s runtime
    /// combine, just moved to save time), and write the result to a sidecar
    /// `.rhai` file next to the level file. `tile.script` is repointed at the
    /// sidecar and `tile.graph` is dropped from the serialized record.
    ///
    /// `data` is `self.grid.to_level_data()`'s own fresh clone, so mutating
    /// it here never touches `self.grid` — the live editor keeps every
    /// graph fully editable after a save.
    pub(super) fn migrate_graph_sidecars(&self, data: &mut LevelData) {
        let level_path = Path::new(&self.save_path);
        let dir = level_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let stem = level_path.file_stem().and_then(|s| s.to_str()).unwrap_or("level");

        for tile in &mut data.tiles {
            let Some(graph) = tile.graph.take() else { continue };

            let mut source = node_graph::generate_graph(&graph);
            if let Some(ref path) = tile.script {
                // Step 7.5-9 (docs/ember2d-master-plan.md §5.6): `resolve_exit_path`
                // now takes its own `exists` check as an injected closure
                // rather than calling `Path::exists` internally (that was
                // real filesystem access inside `ember2d-sim`, R17) — this
                // editor call site is allowed real fs access already
                // (CLAUDE.md), so it just passes one directly.
                let full = resolve_exit_path(path, &self.save_path, &|p| Path::new(p).exists());
                if let Ok(existing) = std::fs::read_to_string(&full) {
                    source.push('\n');
                    source.push_str(&existing);
                }
            }

            // Keyed by (x, y, layer) — the same tuple `LevelGrid` keys tiles
            // by — so every graph-bearing tile in the level gets a distinct,
            // stable sidecar name across repeated saves.
            let filename = format!("{}_graph_{}_{}_{}.rhai", stem, tile.x, tile.y, tile.layer);
            let sidecar_path = dir.join(&filename);
            if let Err(e) = std::fs::write(&sidecar_path, source) {
                eprintln!("Failed to write graph sidecar '{}': {}", sidecar_path.display(), e);
                continue;
            }
            tile.script = Some(filename);
        }
    }
}
