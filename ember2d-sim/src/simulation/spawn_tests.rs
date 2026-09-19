// simulation/spawn_tests.rs — regression coverage for 7.5-12's (R34,
// docs/ember2d-master-plan.md §5.6) graph/script concatenation collision
// check in `do_on_start`, exercised through the real `Simulation::on_start`
// entry point rather than just the extracted `concatenation_collisions`
// predicate `graph/codegen_tests.rs` already covers at the unit level.
// Split into its own sibling file rather than appended to `spawn.rs` for
// the same 750-line-limit reason every other `*_tests.rs` sibling in this
// crate is split out.

use super::*;
use crate::color::Color;
use crate::graph::{NodeGraph, NodeKind};
use crate::level::{LevelData, TileRecord};
use crate::level_source::LevelSource;
use std::collections::BTreeMap as Map;

/// An in-memory `LevelSource` test double — real disk access isn't needed
/// (or allowed, per `ember2d-sim`'s own determinism rules) to exercise
/// `do_on_start`'s file-reading branch.
struct FakeSource {
    files: Map<String, String>,
}

impl LevelSource for FakeSource {
    fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }
    fn read_to_string(&self, path: &str) -> Result<String, String> {
        self.files.get(path).cloned().ok_or_else(|| format!("not found: {path}"))
    }
    fn load_level(&self, path: &str) -> Result<LevelData, String> {
        Err(format!("FakeSource does not support load_level: {path}"))
    }
}

fn level_with_graph_and_script(graph: NodeGraph, script_src: &str) -> (LevelData, FakeSource) {
    let mut level = LevelData::empty(10, 10);
    let mut tile = TileRecord::new(2, 2, 0, 'n', Color::White, Color::Reset, false, true, "npc");
    tile.graph = Some(graph);
    tile.script = Some("npc.rhai".to_string());
    level.tiles.push(tile);

    let mut files = Map::new();
    files.insert("npc.rhai".to_string(), script_src.to_string());
    (level, FakeSource { files })
}

fn graph_with_real_on_start() -> NodeGraph {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let log = graph.add_node(NodeKind::Log, 100, 0);
    assert!(graph.add_edge(on_start, 0, log, 0));
    graph
}

#[test]
fn a_graph_and_script_defining_the_same_lifecycle_fn_fails_to_compile_with_a_logged_error() {
    let (level, source) = level_with_graph_and_script(
        graph_with_real_on_start(),
        "fn on_start(id, ctx) {\n    ctx.log(\"from file\");\n}\n",
    );
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(source));

    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let logs = sim.on_start(&mut world, 10, 10, &mut persistent);

    assert!(
        logs.iter().any(|l| l.text.contains("both define") && l.text.contains("on_start")),
        "expected a logged collision error, got: {:?}",
        logs.iter().map(|l| &l.text).collect::<Vec<_>>()
    );
    // The tile must not carry a broken/ambiguous script — no Script
    // component was attached for the colliding tile's entity (id 1: the
    // player spawns after the tile loop, so the tile itself is entity 1).
    assert!(
        world.scripts.get(&1).is_none(),
        "a colliding tile must not attach a script rather than run an ambiguous one"
    );
}

#[test]
fn a_graph_and_script_defining_different_lifecycle_fns_combine_normally() {
    let (level, source) = level_with_graph_and_script(
        graph_with_real_on_start(),
        "fn on_collide(id, other, ctx) {\n    ctx.log(\"hit\");\n}\n",
    );
    let mut sim = Simulation::new(level);
    sim.set_level_source(Box::new(source));

    let mut world = World::new();
    let mut persistent = BTreeMap::new();
    let logs = sim.on_start(&mut world, 10, 10, &mut persistent);

    assert!(
        !logs.iter().any(|l| l.text.contains("both define")),
        "no real collision here, must not be reported as one: {:?}",
        logs.iter().map(|l| &l.text).collect::<Vec<_>>()
    );
    assert!(world.scripts.get(&1).is_some(), "the combined script must still attach normally");
}
