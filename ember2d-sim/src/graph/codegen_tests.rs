// graph/codegen_tests.rs — regression tests for 7.5-12 (R34,
// docs/ember2d-master-plan.md §5.6). Split out of `codegen.rs` itself
// (`#[cfg(test)] mod codegen_tests;` at that file's end) purely to keep
// `codegen.rs` under CLAUDE.md's 750-line hard limit, same pattern as every
// other `*_tests.rs` sibling in this crate.

use super::*;

fn compiles(src: &str) -> bool {
    rhai::Engine::new().compile(src).is_ok()
}

// ── String/identifier escaping ──────────────────────────────────────────

#[test]
fn a_tag_filter_containing_a_quote_and_backslash_cannot_break_out_of_its_string_literal() {
    let mut graph = NodeGraph::default();
    let evil = "player\", ctx.despawn(id); if (\"x\" == \"x";
    let on_collide = graph.add_node(NodeKind::OnCollide { tag_filter: evil.to_string() }, 0, 0);
    let despawn = graph.add_node(NodeKind::Despawn, 100, 0);
    assert!(graph.add_edge(on_collide, 0, despawn, 0));

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    // The escaped literal must appear verbatim once, not be split across
    // two separate Rhai statements by an unescaped quote.
    assert_eq!(result.source.matches("ctx.get_tag(other) ==").count(), 1);
}

#[test]
fn a_path_containing_a_backslash_round_trips_through_escaping() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let load = graph.add_node(
        NodeKind::LoadLevel { path: "levels\\weird \"name\".level".to_string() },
        100,
        0,
    );
    assert!(graph.add_edge(on_start, 0, load, 0));

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
}

#[test]
fn a_variable_name_with_invalid_identifier_characters_is_sanitized_and_warned_about() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let lit = graph.add_node(NodeKind::FloatLit { value: 1.0 }, 0, 50);
    let set = graph.add_node(NodeKind::SetVar { name: "1; ctx.despawn(id".to_string() }, 100, 0);
    assert!(graph.add_edge(on_start, 0, set, 0));
    assert!(graph.add_edge(lit, 0, set, 1));

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    assert!(
        !result.warnings.is_empty(),
        "sanitizing an invalid identifier must be recorded as a warning"
    );
}

// ── Cycle guards ─────────────────────────────────────────────────────────

#[test]
fn add_edge_refuses_an_edge_that_would_close_an_exec_cycle() {
    let mut graph = NodeGraph::default();
    let seq_a = graph.add_node(NodeKind::Sequence { outputs: 1 }, 0, 0);
    let seq_b = graph.add_node(NodeKind::Sequence { outputs: 1 }, 100, 0);
    assert!(graph.add_edge(seq_a, 1, seq_b, 0), "the first edge is a normal DAG edge");
    assert!(
        !graph.add_edge(seq_b, 1, seq_a, 0),
        "the second edge would close A -> B -> A and must be refused"
    );
    assert_eq!(graph.edges.len(), 1, "the rejected edge must not have been added");
}

#[test]
fn add_edge_refuses_a_node_wired_directly_to_its_own_input() {
    let mut graph = NodeGraph::default();
    let math = graph.add_node(NodeKind::MathOp { op: MathOp::Add }, 0, 0);
    assert!(!graph.add_edge(math, 2, math, 0), "a node feeding its own input must be refused");
    assert!(graph.edges.is_empty());
}

#[test]
fn a_hand_constructed_exec_cycle_terminates_generation_instead_of_hanging() {
    // Bypasses `add_edge`'s own guard by pushing the cycle directly, the
    // same way a hand-edited (or pre-7.5-12) saved level file would.
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let seq_a = graph.add_node(NodeKind::Sequence { outputs: 1 }, 100, 0);
    let seq_b = graph.add_node(NodeKind::Sequence { outputs: 1 }, 200, 0);
    graph.edges.push(Edge { from_node: on_start, from_port: 0, to_node: seq_a, to_port: 0 });
    graph.edges.push(Edge { from_node: seq_a, from_port: 1, to_node: seq_b, to_port: 0 });
    graph.edges.push(Edge { from_node: seq_b, from_port: 1, to_node: seq_a, to_port: 0 });

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    assert!(!result.warnings.is_empty(), "the cycle must be reported as a warning");
}

#[test]
fn a_hand_constructed_data_cycle_terminates_generation_instead_of_hanging() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let math_a = graph.add_node(NodeKind::MathOp { op: MathOp::Add }, 0, 50);
    let math_b = graph.add_node(NodeKind::MathOp { op: MathOp::Add }, 100, 50);
    let log = graph.add_node(NodeKind::Log, 200, 0);
    graph.edges.push(Edge { from_node: on_start, from_port: 0, to_node: log, to_port: 0 });
    // math_a's A input <- math_b's output, math_b's A input <- math_a's
    // output: a two-node data cycle, feeding Log's Msg port.
    graph.edges.push(Edge { from_node: math_b, from_port: 2, to_node: math_a, to_port: 0 });
    graph.edges.push(Edge { from_node: math_a, from_port: 2, to_node: math_b, to_port: 0 });
    graph.edges.push(Edge { from_node: math_a, from_port: 2, to_node: log, to_port: 1 });

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    assert!(!result.warnings.is_empty(), "the cycle must be reported as a warning");
}

#[test]
fn a_legitimate_data_diamond_is_not_mistaken_for_a_cycle() {
    // MathOp reads GetPosition.X for BOTH its A and B inputs — the same
    // upstream node feeding two sibling ports is a normal DAG fan-in, not a
    // cycle, and must not be defaulted away.
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let pos = graph.add_node(NodeKind::GetPosition, 0, 50);
    let math = graph.add_node(NodeKind::MathOp { op: MathOp::Add }, 100, 50);
    let log = graph.add_node(NodeKind::Log, 200, 0);
    assert!(graph.add_edge(on_start, 0, log, 0));
    assert!(graph.add_edge(pos, 0, math, 0));
    assert!(graph.add_edge(pos, 0, math, 1));
    assert!(graph.add_edge(math, 2, log, 1));

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    assert!(result.warnings.is_empty(), "a legitimate fan-in must not be reported as a cycle");
    assert_eq!(
        result.source.matches("ctx.get_x(id)").count(),
        2,
        "both MathOp inputs must still resolve to the shared upstream expression"
    );
}

// ── Typed unconnected-port defaults ─────────────────────────────────────

#[test]
fn an_unconnected_string_typed_port_does_not_default_to_a_numeric_literal() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let log = graph.add_node(NodeKind::Log, 100, 0);
    assert!(graph.add_edge(on_start, 0, log, 0)); // Msg left unconnected

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    assert!(
        result.source.contains("ctx.log(\"\")"),
        "an unconnected Msg port must default to an empty string, not 0.0:\n{}",
        result.source
    );
}

#[test]
fn an_unconnected_branch_condition_defaults_to_a_real_bool() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let branch = graph.add_node(NodeKind::Branch, 100, 0);
    let despawn = graph.add_node(NodeKind::Despawn, 200, 0);
    assert!(graph.add_edge(on_start, 0, branch, 0));
    assert!(graph.add_edge(branch, 2, despawn, 0)); // True -> Despawn, Cond left unconnected

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
}

// ── Variable hoisting ────────────────────────────────────────────────────

#[test]
fn a_variable_set_inside_a_branch_arm_is_still_readable_after_the_branch_closes() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let branch = graph.add_node(NodeKind::Branch, 100, 0);
    // A real bool-producing node — `Branch.Cond` is `DataType::Bool` typed
    // now, and Rhai's own `if` requires an actual `bool`, not a float.
    let cond = graph.add_node(NodeKind::CompareFloat { op: CmpOp::Gt }, 100, -50);
    let cond_a = graph.add_node(NodeKind::FloatLit { value: 1.0 }, 50, -70);
    let cond_b = graph.add_node(NodeKind::FloatLit { value: 0.0 }, 150, -70);
    let set_val = graph.add_node(NodeKind::FloatLit { value: 5.0 }, 200, 0);
    let set = graph.add_node(NodeKind::SetVar { name: "x".to_string() }, 300, 0);
    let log = graph.add_node(NodeKind::Log, 400, 0);
    let get = graph.add_node(NodeKind::GetVar { name: "x".to_string() }, 300, 100);

    assert!(graph.add_edge(on_start, 0, branch, 0));
    assert!(graph.add_edge(cond_a, 0, cond, 0));
    assert!(graph.add_edge(cond_b, 0, cond, 1));
    assert!(graph.add_edge(cond, 0, branch, 1));
    assert!(graph.add_edge(branch, 2, set, 0)); // True arm sets x
    assert!(graph.add_edge(set_val, 0, set, 1));
    assert!(graph.add_edge(branch, 3, log, 0)); // False arm reads x (never set on this path)
    assert!(graph.add_edge(get, 0, log, 1));

    let result = generate_graph(&graph);
    assert!(compiles(&result.source), "generated source must still compile:\n{}", result.source);
    assert!(
        result.source.contains("let __var_x = 0.0;"),
        "x must be hoisted to one `let` at the top of on_start:\n{}",
        result.source
    );
    // The hoisted declaration must come before either branch arm's own use.
    let decl_pos = result.source.find("let __var_x").unwrap();
    let branch_pos = result.source.find("if ").unwrap();
    assert!(decl_pos < branch_pos, "the hoisted let must precede the branch:\n{}", result.source);
}

// ── Graph/script concatenation collision (R34) ──────────────────────────

#[test]
fn concatenation_collisions_is_empty_when_the_file_script_only_defines_other_functions() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let log = graph.add_node(NodeKind::Log, 100, 0);
    assert!(graph.add_edge(on_start, 0, log, 0));
    let result = generate_graph(&graph);

    let file_src = "fn on_collide(id, other, ctx) {\n    ctx.log(\"hit\");\n}\n";
    assert_eq!(concatenation_collisions(&result, file_src), Vec::<&str>::new());
}

#[test]
fn concatenation_collisions_flags_a_lifecycle_function_the_graph_already_defines() {
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let log = graph.add_node(NodeKind::Log, 100, 0);
    assert!(graph.add_edge(on_start, 0, log, 0));
    let result = generate_graph(&graph);
    assert_eq!(result.defined_fns, vec!["on_start"]);

    let file_src = "fn on_start(id, ctx) {\n    ctx.log(\"from file\");\n}\n";
    assert_eq!(concatenation_collisions(&result, file_src), vec!["on_start"]);
}

#[test]
fn concatenation_collisions_ignores_a_lifecycle_function_the_graph_never_used() {
    // The graph only wires up OnStart — on_update/on_collide are empty
    // stubs, so a file script defining `on_update` isn't a real collision.
    let mut graph = NodeGraph::default();
    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let log = graph.add_node(NodeKind::Log, 100, 0);
    assert!(graph.add_edge(on_start, 0, log, 0));
    let result = generate_graph(&graph);

    let file_src = "fn on_update(id, ctx) {\n    ctx.log(\"tick\");\n}\n";
    assert_eq!(concatenation_collisions(&result, file_src), Vec::<&str>::new());
}

// ── Property-style: every generatable graph shape compiles ──────────────

/// Builds a small pseudo-random graph over most of the supported node
/// kinds and asserts the generated Rhai always compiles — the step's own
/// "Tests" line (docs/ember2d-master-plan.md §5.6, 7.5-12). `rand` (already
/// a workspace dependency, not a new one) drives this deterministically
/// from a fixed seed per iteration rather than reaching for `proptest`/
/// `quickcheck` — neither is in the dependency tree, and §6.3 asks new ones
/// be justified; a handful of fixed seeds already exercises the escaping,
/// cycle-guard, typed-default, and hoisting paths together.
fn random_graph(seed: u64) -> NodeGraph {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    fn weird_str(rng: &mut impl Rng) -> String {
        let pool = ["plain", "with\"quote", "with\\slash", "with\nnewline", "", "1starts_digit"];
        pool[rng.gen_range(0..pool.len())].to_string()
    }
    fn value_node(rng: &mut impl Rng) -> NodeKind {
        match rng.gen_range(0..5) {
            0 => NodeKind::FloatLit { value: rng.gen_range(-100.0..100.0) },
            1 => NodeKind::StringLit { value: weird_str(rng) },
            2 => NodeKind::GetPosition,
            3 => NodeKind::GetVar { name: weird_str(rng) },
            _ => NodeKind::RandomFloat,
        }
    }
    fn action_node(rng: &mut impl Rng) -> NodeKind {
        match rng.gen_range(0..8) {
            0 => NodeKind::Log,
            1 => NodeKind::SetGlyph,
            2 => NodeKind::SetVelocity,
            3 => NodeKind::SetVar { name: weird_str(rng) },
            4 => NodeKind::PlaySound { path: weird_str(rng) },
            5 => NodeKind::SetVisible,
            6 => NodeKind::Despawn,
            _ => NodeKind::ShakeCamera,
        }
    }

    let mut rng = StdRng::seed_from_u64(seed);
    let mut graph = NodeGraph::default();

    let on_start = graph.add_node(NodeKind::OnStart, 0, 0);
    let on_update = graph.add_node(NodeKind::OnUpdate, 0, 100);
    let on_collide =
        graph.add_node(NodeKind::OnCollide { tag_filter: weird_str(&mut rng) }, 0, 200);

    // Every event's own single exec_out port is index 0 (`ports_for`).
    for (root, y) in [(on_start, 0i32), (on_update, 100), (on_collide, 200)] {
        let mut prev = root;
        let mut prev_exec_out = 0usize;
        for i in 0..3i32 {
            let action = action_node(&mut rng);
            let is_terminal = action.is_terminal();
            let node = graph.add_node(action, 100 + i * 40, y);

            let ports = ports_for(&graph.get(node).unwrap().kind);
            let (ins, outs) = split_ports(&ports);
            let data_in_ports: Vec<usize> =
                ins.iter().filter(|(_, p)| p.kind == PortKind::Data).map(|(idx, _)| *idx).collect();
            let exec_in_port =
                ins.iter().find(|(_, p)| p.kind == PortKind::Exec).map(|(idx, _)| *idx);
            let exec_out_port =
                outs.iter().find(|(_, p)| p.kind == PortKind::Exec).map(|(idx, _)| *idx);

            if let Some(ei) = exec_in_port {
                graph.add_edge(prev, prev_exec_out, node, ei);
            }
            for &di in data_in_ports.iter().take(2) {
                let val = graph.add_node(value_node(&mut rng), 100 + i * 20, y - 20);
                graph.add_edge(val, 0, node, di);
            }

            match exec_out_port {
                Some(eo) if !is_terminal => {
                    prev = node;
                    prev_exec_out = eo;
                }
                _ => break,
            }
        }
    }
    graph
}

#[test]
fn random_graphs_over_the_supported_node_kinds_always_produce_source_that_compiles() {
    for seed in 0..40u64 {
        let graph = random_graph(seed);
        let result = generate_graph(&graph);
        assert!(
            compiles(&result.source),
            "seed {} produced source that failed to compile:\n{}",
            seed,
            result.source
        );
    }
}
