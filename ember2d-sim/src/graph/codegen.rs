// graph/codegen.rs — Rhai code generation from node graph.
//
// 7.5-12 (R34, docs/ember2d-master-plan.md §5.6) hardened this file: every
// user-entered string spliced into the generated source is now escaped
// (`escape_rhai_string`) or, for the two fields that become bare Rhai
// identifiers rather than string literals, sanitized to one
// (`sanitize_ident`) — before this, a tag/path/name containing `"` or `\`
// could break out of its Rhai string literal and inject arbitrary script
// code. `gen_exec_chain`/`resolve_data` also carry cycle guards now
// (`GenCtx::exec_visited`/`data_visited`) as defense in depth against a
// cycle already sitting in a saved level file — `NodeGraph::add_edge`
// (graph/mod.rs) is the primary defense, refusing to let the editor draw
// one in the first place. Unconnected data ports default per the target
// port's own `DataType` (`default_for_port`) instead of an unconditional
// `0.0`, and a `SetVar`/`GetVar`'s `__var_*` local is now declared once at
// the top of whichever lifecycle function references it
// (`GenCtx::declared_vars`) rather than wherever `SetVar` happens to sit —
// a `let` written inside a `Branch` arm is Rhai-block-scoped to that arm,
// so a variable set only inside a branch used to be unreadable once the
// `if`/`else` closed.

use super::*;
use std::collections::{BTreeSet, HashMap};

/// `generate_graph`'s result: the generated Rhai source, which of the three
/// lifecycle functions it actually gave real content (used by the
/// graph/script concatenation collision check below), and any warnings
/// generation itself had to work around (a sanitized identifier, a cycle
/// that got truncated) — both `ember2d-sim/src/simulation/spawn.rs` and
/// `ember2d-editor/src/editor/impl_state/graph_sidecars.rs` forward these
/// into their own `LogEntry` sink (7C-7's console-log mechanism) rather
/// than dropping them silently.
pub struct GraphSource {
    pub source: String,
    pub defined_fns: Vec<&'static str>,
    pub warnings: Vec<String>,
}

impl GraphSource {
    fn empty() -> Self {
        GraphSource { source: String::new(), defined_fns: Vec::new(), warnings: Vec::new() }
    }
}

/// Threaded through the whole recursive codegen pass — one instance per
/// `generate_graph` call, reused across every event node's own exec chain.
/// Bundled into a struct once the flat parameter list (temp-var counter,
/// spawn-result variable names, two distinct cycle-guard sets, the hoisted-
/// variable collector, and the diagnostic sink) grew past what's readable
/// as individual `&mut` arguments.
struct GenCtx<'a> {
    tmp: &'a mut usize,
    // Lookup-only by `NodeId` (inserted once when a `Spawn` node is
    // generated, read back later at `codegen_expr` for that same node id,
    // never iterated) — pre-existing, not part of 7.5-12's own scope
    // (R91/R92 already track annotating this crate's remaining lookup-only
    // `Hash*` sites); left as found rather than opportunistically migrated.
    spawn_vars: &'a mut HashMap<NodeId, String>,
    // Nodes already reached by the current exec-chain traversal. Monotonic
    // for the whole graph, never removed: `NodeGraph::add_edge` refuses any
    // new edge that would close a cycle, and every node's single exec-in
    // port can have at most one predecessor at a time, so the "reachable
    // via exec" graph is a forest with no legitimate merge points — a node
    // can only be seen twice here via an actual cycle (e.g. one hand-edited
    // into a saved level file, bypassing `add_edge` entirely).
    exec_visited: &'a mut BTreeSet<NodeId>,
    // Nodes on the CURRENT data-expression recursion path only. Unlike
    // `exec_visited`, data ports legitimately fan in — two different
    // statements can each read the same upstream node's output (e.g. two
    // sibling arguments both wired to the same `GetPosition`) — so this one
    // is pushed right before descending into a producer and popped again
    // right after (`resolve_data`): a real "currently on the call stack"
    // set, not an ever-visited one.
    data_visited: &'a mut BTreeSet<NodeId>,
    // Every `__var_*` identifier the CURRENT lifecycle function (on_start,
    // on_update, or on_collide — `generate_graph` gives each its own fresh
    // set) references, from either a `SetVar` write or a `GetVar` read.
    // Hoisted to one `let` block at the top of that function's body once
    // the whole body is generated, so a variable set inside a `Branch` arm
    // is still readable once the `if`/`else` closes.
    declared_vars: &'a mut BTreeSet<String>,
    warnings: &'a mut Vec<String>,
}

pub fn generate_graph(graph: &NodeGraph) -> GraphSource {
    if graph.nodes.is_empty() {
        return GraphSource::empty();
    }

    let mut on_start = String::new();
    let mut on_update = String::new();
    let mut on_collide = String::new();
    let mut start_vars: BTreeSet<String> = BTreeSet::new();
    let mut update_vars: BTreeSet<String> = BTreeSet::new();
    let mut collide_vars: BTreeSet<String> = BTreeSet::new();

    let mut tmp_counter = 0usize;
    let mut spawn_vars: HashMap<NodeId, String> = HashMap::new();
    let mut exec_visited: BTreeSet<NodeId> = BTreeSet::new();
    let mut data_visited: BTreeSet<NodeId> = BTreeSet::new();
    let mut warnings: Vec<String> = Vec::new();

    for node in &graph.nodes {
        if !node.kind.is_event() {
            continue;
        }
        let ports = ports_for(&node.kind);
        let (_, outs) = split_ports(&ports);
        let bucket_vars = match &node.kind {
            NodeKind::OnStart => &mut start_vars,
            NodeKind::OnCollide { .. } => &mut collide_vars,
            _ => &mut update_vars, // OnUpdate, OnKeyHeld, OnKeyPress
        };
        let exec_body = if let Some((pidx, _)) = outs.iter().find(|(_, p)| p.kind == PortKind::Exec)
        {
            let mut ctx = GenCtx {
                tmp: &mut tmp_counter,
                spawn_vars: &mut spawn_vars,
                exec_visited: &mut exec_visited,
                data_visited: &mut data_visited,
                declared_vars: bucket_vars,
                warnings: &mut warnings,
            };
            gen_exec_chain(graph, node.id, *pidx, 1, &mut ctx)
        } else {
            String::new()
        };

        match &node.kind {
            NodeKind::OnStart => {
                on_start.push_str(&exec_body);
            }
            NodeKind::OnUpdate => {
                on_update.push_str(&exec_body);
            }
            NodeKind::OnKeyHeld { key } => {
                on_update.push_str(&format!(
                    "    if ctx.is_held({}) {{\n{}    }}\n",
                    escape_rhai_string(key),
                    exec_body
                ));
            }
            NodeKind::OnKeyPress { key } => {
                on_update.push_str(&format!(
                    "    if ctx.just_pressed({}) {{\n{}    }}\n",
                    escape_rhai_string(key),
                    exec_body
                ));
            }
            NodeKind::OnCollide { tag_filter } => {
                if tag_filter.is_empty() {
                    on_collide.push_str(&exec_body);
                } else {
                    on_collide.push_str(&format!(
                        "    if ctx.get_tag(other) == {} {{\n{}    }}\n",
                        escape_rhai_string(tag_filter),
                        exec_body
                    ));
                }
            }
            _ => {}
        }
    }

    let mut defined_fns = Vec::new();
    if !on_start.trim().is_empty() {
        defined_fns.push("on_start");
    }
    if !on_update.trim().is_empty() {
        defined_fns.push("on_update");
    }
    if !on_collide.trim().is_empty() {
        defined_fns.push("on_collide");
    }

    let source = format!(
        "fn on_start(id, ctx) {{\n{}}}\nfn on_update(id, ctx) {{\n{}}}\nfn on_collide(id, other, ctx) {{\n{}}}\n",
        hoist(on_start, &start_vars),
        hoist(on_update, &update_vars),
        hoist(on_collide, &collide_vars),
    );

    GraphSource { source, defined_fns, warnings }
}

/// Prepends one `let __var_name = 0.0;` per name in `vars` (sorted —
/// `BTreeSet`'s own iteration order — so the generated source is
/// deterministic run to run) to `body`. A no-op when `vars` is empty, which
/// covers every event that never touches a graph variable.
fn hoist(body: String, vars: &BTreeSet<String>) -> String {
    if vars.is_empty() {
        return body;
    }
    let mut out = String::new();
    for name in vars {
        out.push_str(&format!("    let __var_{} = 0.0;\n", name));
    }
    out.push_str(&body);
    out
}

/// Cheap textual check for whether `src` defines a Rhai function named
/// `name` — good enough for the concatenation collision check below, not a
/// substitute for actually parsing the file (this only needs to catch the
/// common case of a level author accidentally naming a script function the
/// graph already owns, R34).
fn file_defines_fn(src: &str, name: &str) -> bool {
    let needle = format!("fn {}", name);
    src.match_indices(&needle).any(|(i, _)| src[i + needle.len()..].trim_start().starts_with('('))
}

/// The R34 concatenation-collision check: does `file_src` (a tile's own
/// hand-written script) redefine any lifecycle function `graph_source`
/// already generated real content for? Both `ember2d-sim`'s runtime combine
/// (`simulation/spawn.rs::do_on_start`) and `ember2d-editor`'s save-time
/// export (`impl_state/graph_sidecars.rs::migrate_graph_sidecars`) call
/// this with the same two sources before concatenating them, so the check
/// can't drift between the two mirrored call sites. Before this existed,
/// concatenating the two let Rhai's own last-definition-wins semantics
/// silently discard whichever version came first — always the graph's,
/// since the file's text is appended after it.
pub fn concatenation_collisions(graph_source: &GraphSource, file_src: &str) -> Vec<&'static str> {
    graph_source
        .defined_fns
        .iter()
        .copied()
        .filter(|name| file_defines_fn(file_src, name))
        .collect()
}

/// Escapes `s` into a double-quoted Rhai string literal — every character
/// that would otherwise end the literal early or inject a control character
/// is backslash-escaped. Used for every user-entered value spliced in as a
/// Rhai *string literal* (a tag, a path, a global/persistent/timer name, a
/// `StringLit`'s own value); `sanitize_ident` below is the sibling for the
/// two fields (`SetVar`/`GetVar`'s `name`) that become a bare Rhai
/// *identifier* instead, where quoting would be wrong.
fn escape_rhai_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Turns `name` into a safe Rhai identifier suffix (`[A-Za-z_][A-Za-z0-9_]*`
/// — the plan's own validation pattern), replacing every other character
/// with `_` and prefixing with `_` if the result would otherwise be empty
/// or start with a digit. `SetVar`/`GetVar`'s `name` becomes a bare
/// `__var_{name}` Rhai variable, not a quoted argument, so this can't just
/// reuse `escape_rhai_string` — an unescaped name here would let it inject
/// arbitrary code the same way an unescaped string literal would. Records a
/// warning when sanitizing actually changed the name — the plan's own text
/// asks for "a UI error"; there's no UI in this step's chosen scope (sim-
/// side hardening only), so the sanitized-and-warned fallback is the
/// pragmatic substitute, surfaced through `GraphSource::warnings`.
fn sanitize_ident(name: &str, warnings: &mut Vec<String>) -> String {
    let mut out = String::with_capacity(name.len());
    for (i, c) in name.chars().enumerate() {
        let ok = if i == 0 {
            c.is_ascii_alphabetic() || c == '_'
        } else {
            c.is_ascii_alphanumeric() || c == '_'
        };
        out.push(if ok { c } else { '_' });
    }
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if out != name {
        warnings.push(format!(
            "variable name {:?} isn't a valid identifier — using {:?} instead",
            name, out
        ));
    }
    out
}

fn indent(n: usize) -> String {
    "    ".repeat(n)
}

fn gen_exec_chain(
    graph: &NodeGraph,
    from_node: NodeId,
    from_port: usize,
    depth: usize,
    ctx: &mut GenCtx,
) -> String {
    let edge = match graph.exec_out_edge(from_node, from_port) {
        Some(e) => e.clone(),
        None => return String::new(),
    };
    if !ctx.exec_visited.insert(edge.to_node) {
        ctx.warnings.push(format!(
            "execution cycle detected at node {} — stopping this branch here instead of hanging",
            edge.to_node
        ));
        return String::new();
    }
    let node = match graph.get(edge.to_node) {
        Some(n) => n.clone(),
        None => return String::new(),
    };
    gen_node_stmt(graph, &node, depth, ctx)
}

fn gen_node_stmt(graph: &NodeGraph, node: &Node, depth: usize, ctx: &mut GenCtx) -> String {
    let ind = indent(depth);
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    let data_ins: Vec<_> = ins.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();

    macro_rules! resolve {
        ($idx:expr) => {{
            let abs_port = data_ins.get($idx).map(|(i, _)| *i).unwrap_or(0);
            resolve_data(graph, node.id, abs_port, ctx)
        }};
    }

    let exec_outs: Vec<usize> =
        outs.iter().filter(|(_, p)| p.kind == PortKind::Exec).map(|(i, _)| *i).collect();

    let mut out = String::new();

    match &node.kind {
        NodeKind::SetVelocity => {
            out += &format!("{}ctx.set_velocity(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetPosition => {
            out += &format!("{}ctx.set_position(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::Despawn => {
            out += &format!("{}ctx.despawn(id);\n", ind);
        }
        NodeKind::Spawn => {
            let var = format!("__tmp{}", ctx.tmp);
            *ctx.tmp += 1;
            ctx.spawn_vars.insert(node.id, var.clone());
            out += &format!(
                "{}let {} = ctx.spawn_entity({}, {}, {}, {});\n",
                ind,
                var,
                resolve!(0),
                resolve!(2),
                resolve!(3),
                resolve!(1)
            );
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::LoadLevel { path } => {
            out += &format!("{}ctx.load_level({});\n", ind, escape_rhai_string(path));
        }
        NodeKind::PlaySound { path } => {
            out += &format!("{}ctx.play_sound({});\n", ind, escape_rhai_string(path));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::Log => {
            out += &format!("{}ctx.log({});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetGlyph => {
            out += &format!("{}ctx.set_glyph(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::DrawHUD => {
            out += &format!(
                "{}ctx.draw_hud({}, {}, {}, \"White\", \"Reset\");\n",
                ind,
                resolve!(0),
                resolve!(1),
                resolve!(2)
            );
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::Branch => {
            let cond = resolve!(0);
            let true_body = exec_outs
                .first()
                .map(|&p| gen_exec_chain(graph, node.id, p, depth + 1, ctx))
                .unwrap_or_default();
            let false_body = exec_outs
                .get(1)
                .map(|&p| gen_exec_chain(graph, node.id, p, depth + 1, ctx))
                .unwrap_or_default();
            out += &format!(
                "{}if {} {{\n{}{}}} else {{\n{}{}}}\n",
                ind, cond, true_body, ind, false_body, ind
            );
        }
        NodeKind::Sequence { .. } => {
            for &p in &exec_outs {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetVar { name } => {
            let ident = sanitize_ident(name, ctx.warnings);
            ctx.declared_vars.insert(ident.clone());
            out += &format!("{}__var_{} = {};\n", ind, ident, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetGlobal { name } => {
            out +=
                &format!("{}ctx.set_global({}, {});\n", ind, escape_rhai_string(name), resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetPersistent { name } => {
            out += &format!(
                "{}ctx.set_persistent({}, {});\n",
                ind,
                escape_rhai_string(name),
                resolve!(0)
            );
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetCamera => {
            out += &format!("{}ctx.set_camera({}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::ShakeCamera => {
            out += &format!("{}ctx.shake_camera({}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::StartTimer { name } => {
            out += &format!(
                "{}ctx.start_timer({}, {});\n",
                ind,
                escape_rhai_string(name),
                resolve!(0)
            );
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetVisible => {
            out += &format!("{}ctx.set_visible(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetZOrder => {
            out += &format!("{}ctx.set_layer_order(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::CancelTimer { name } => {
            out += &format!("{}ctx.cancel_timer({});\n", ind, escape_rhai_string(name));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::PlayMusic { path } => {
            out += &format!("{}ctx.play_music({});\n", ind, escape_rhai_string(path));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::StopMusic => {
            out += &format!("{}ctx.stop_music();\n", ind);
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetColor => {
            out += &format!("{}ctx.set_tint(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::DrawBox => {
            out += &format!(
                "{}ctx.draw_box({}, {}, {}, {}, {}, {});\n",
                ind,
                resolve!(0),
                resolve!(1),
                resolve!(2),
                resolve!(3),
                resolve!(4),
                resolve!(5)
            );
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::FillRect => {
            out += &format!(
                "{}ctx.fill_rect({}, {}, {}, {}, {}, {}, {});\n",
                ind,
                resolve!(0),
                resolve!(1),
                resolve!(2),
                resolve!(3),
                resolve!(4),
                resolve!(5),
                resolve!(6)
            );
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::ClearHUD => {
            out += &format!("{}ctx.clear_hud();\n", ind);
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        NodeKind::SetColliderLayer => {
            out += &format!("{}ctx.set_collider_layer({}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, ctx);
            }
        }
        _ => {}
    }
    out
}

/// Looks up a `data_in` port's own `DataType` (via `ports_for` on whatever
/// node owns it) and returns that type's default literal — used both when a
/// data-in port has no incoming edge at all and when it has one but the
/// producer node no longer exists (a dangling edge, e.g. after a save-file
/// hand-edit). Falls back to `Float` if `node_id`/`port` can't be resolved
/// at all, matching the unconditional `0.0` this replaces for every port
/// that genuinely is float-typed.
fn default_for_port(graph: &NodeGraph, node_id: NodeId, port: usize) -> String {
    graph
        .get(node_id)
        .map(|n| ports_for(&n.kind))
        .and_then(|ports| ports.get(port).map(|p| p.data_type.default_literal().to_string()))
        .unwrap_or_else(|| DataType::Float.default_literal().to_string())
}

fn resolve_data(graph: &NodeGraph, to_node: NodeId, to_port: usize, ctx: &mut GenCtx) -> String {
    let Some(edge) = graph.data_in_edge(to_node, to_port).cloned() else {
        return default_for_port(graph, to_node, to_port);
    };
    if !ctx.data_visited.insert(edge.from_node) {
        ctx.warnings.push(format!(
            "data cycle detected through node {} — using a default value there instead of hanging",
            edge.from_node
        ));
        return default_for_port(graph, to_node, to_port);
    }
    let result = match graph.get(edge.from_node) {
        Some(src) => codegen_expr(graph, src, edge.from_port, ctx),
        None => default_for_port(graph, to_node, to_port),
    };
    ctx.data_visited.remove(&edge.from_node);
    result
}

fn codegen_expr(graph: &NodeGraph, node: &Node, out_port: usize, ctx: &mut GenCtx) -> String {
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    let data_ins: Vec<_> = ins.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();

    let out_idx = outs
        .iter()
        .filter(|(_, p)| p.kind == PortKind::Data)
        .position(|(i, _)| *i == out_port)
        .unwrap_or(0);

    macro_rules! resolve_in {
        ($idx:expr) => {{
            let abs = data_ins.get($idx).map(|(i, _)| *i).unwrap_or(0);
            resolve_data(graph, node.id, abs, ctx)
        }};
    }

    match &node.kind {
        NodeKind::FloatLit { value } => format!("{:.6}", value),
        NodeKind::StringLit { value } => escape_rhai_string(value),
        NodeKind::GetPosition => {
            if out_idx == 0 {
                "ctx.get_x(id)".into()
            } else {
                "ctx.get_y(id)".into()
            }
        }
        NodeKind::GetVelocity => {
            if out_idx == 0 {
                "ctx.get_vel_x(id)".into()
            } else {
                "ctx.get_vel_y(id)".into()
            }
        }
        NodeKind::GetTag => "ctx.get_tag(id)".into(),
        NodeKind::GetDelta => "ctx.get_delta()".into(),
        NodeKind::CompareFloat { op } => {
            format!("({} {} {})", resolve_in!(0), op.as_str(), resolve_in!(1))
        }
        NodeKind::MathOp { op } => {
            format!("({} {} {})", resolve_in!(0), op.as_str(), resolve_in!(1))
        }
        NodeKind::GetVar { name } => {
            let ident = sanitize_ident(name, ctx.warnings);
            ctx.declared_vars.insert(ident.clone());
            format!("__var_{}", ident)
        }
        NodeKind::OnCollide { .. } => "other".into(),
        NodeKind::OnUpdate => "ctx.get_delta()".into(),
        NodeKind::Spawn => ctx
            .spawn_vars
            .get(&node.id)
            .cloned()
            .unwrap_or_else(|| DataType::Entity.default_literal().to_string()),

        NodeKind::GetGlobal { name } => format!("ctx.get_global({})", escape_rhai_string(name)),
        NodeKind::GetPersistent { name } => {
            format!("ctx.get_persistent({})", escape_rhai_string(name))
        }
        NodeKind::GetMousePos => {
            if out_idx == 0 {
                "ctx.get_mouse_world_x()".into()
            } else {
                "ctx.get_mouse_world_y()".into()
            }
        }
        NodeKind::IsSolidAt => format!("ctx.is_solid_at({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::GetEntityAt => {
            format!("ctx.get_entity_at({}, {})", resolve_in!(0), resolve_in!(1))
        }
        NodeKind::GetDistance => {
            format!("ctx.get_distance({}, {})", resolve_in!(0), resolve_in!(1))
        }
        NodeKind::GetAngleTo => format!("ctx.get_angle_to({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::TimerDone { name } => format!("ctx.timer_done({})", escape_rhai_string(name)),
        NodeKind::RandomInt => format!("ctx.random_int({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::RandomFloat => "ctx.random_float()".into(),
        NodeKind::RandomBool => format!("ctx.random_bool({})", resolve_in!(0)),
        NodeKind::RandomChoice => format!("ctx.random_choice({})", resolve_in!(0)),
        NodeKind::GetElapsed => "ctx.get_elapsed()".into(),
        NodeKind::EntityExists => format!("ctx.entity_exists({})", resolve_in!(0)),
        NodeKind::HasTag => format!("ctx.has_tag({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::GetColliderLayer => format!("ctx.get_collider_layer({})", resolve_in!(0)),
        NodeKind::FindEntitiesInRect => format!(
            "ctx.find_entities_in_rect({}, {}, {}, {})",
            resolve_in!(0),
            resolve_in!(1),
            resolve_in!(2),
            resolve_in!(3)
        ),
        NodeKind::CountByTag => format!("ctx.count_by_tag({})", resolve_in!(0)),
        NodeKind::FindByTag => format!("ctx.find_by_tag({})", resolve_in!(0)),
        NodeKind::FindAllByTag => format!("ctx.find_all_by_tag({})", resolve_in!(0)),
        NodeKind::MouseLeftPressed => "ctx.mouse_left_pressed()".into(),
        NodeKind::MouseLeftHeld => "ctx.mouse_left_held()".into(),
        _ => "0.0".into(),
    }
}

#[cfg(test)]
#[path = "codegen_tests.rs"]
mod codegen_tests;
