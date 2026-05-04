// editor/node_graph.rs — Blueprint-style node graph for visual scripting.
//
// Nodes are ASCII boxes. Edges are Manhattan-routed wires drawn with `-|+`.
// The graph compiles to a Rhai source string via generate_graph().

use serde::{Deserialize, Serialize};
use crate::renderer::{color::Color, Renderer};

// ── IDs ───────────────────────────────────────────────────────────────────────

pub type NodeId = u32;

// ── Enum helpers ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp { Gt, Lt, Eq, Ne, Ge, Le }

impl CmpOp {
    pub fn as_str(self) -> &'static str {
        match self { CmpOp::Gt => ">", CmpOp::Lt => "<", CmpOp::Eq => "==",
                     CmpOp::Ne => "!=", CmpOp::Ge => ">=", CmpOp::Le => "<=" }
    }
    pub fn label(self) -> &'static str {
        match self { CmpOp::Gt => "GT", CmpOp::Lt => "LT", CmpOp::Eq => "EQ",
                     CmpOp::Ne => "NE", CmpOp::Ge => "GE", CmpOp::Le => "LE" }
    }
    pub fn next(self) -> Self {
        match self { CmpOp::Gt=>CmpOp::Lt, CmpOp::Lt=>CmpOp::Eq, CmpOp::Eq=>CmpOp::Ne,
                     CmpOp::Ne=>CmpOp::Ge, CmpOp::Ge=>CmpOp::Le, CmpOp::Le=>CmpOp::Gt }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathOp { Add, Sub, Mul, Div }

impl MathOp {
    pub fn as_str(self) -> &'static str {
        match self { MathOp::Add=>"+", MathOp::Sub=>"-", MathOp::Mul=>"*", MathOp::Div=>"/" }
    }
    pub fn label(self) -> &'static str {
        match self { MathOp::Add=>"Add", MathOp::Sub=>"Sub", MathOp::Mul=>"Mul", MathOp::Div=>"Div" }
    }
    pub fn next(self) -> Self {
        match self { MathOp::Add=>MathOp::Sub, MathOp::Sub=>MathOp::Mul,
                     MathOp::Mul=>MathOp::Div, MathOp::Div=>MathOp::Add }
    }
}

// ── NodeKind ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    // ── Events (exec-out only) ────────────────────────────────────────────────
    OnStart,
    OnUpdate,
    OnKeyHeld   { key: String },
    OnKeyPress  { key: String },
    OnCollide   { tag_filter: String },

    // ── Flow control ──────────────────────────────────────────────────────────
    Branch,
    Sequence    { outputs: usize },

    // ── Action nodes (exec-in + exec-out, except terminals) ───────────────────
    SetVelocity,
    SetPosition,
    Despawn,
    Spawn,
    LoadLevel   { path: String },
    PlaySound   { path: String },
    Log,
    SetGlyph,
    DrawHUD,

    // ── Pure functions (data-out only) ────────────────────────────────────────
    GetPosition,
    GetVelocity,
    GetTag,
    GetDelta,
    FloatLit    { value: f64 },
    StringLit   { value: String },
    CompareFloat { op: CmpOp },
    MathOp      { op: MathOp },

    // ── Variables ─────────────────────────────────────────────────────────────
    GetVar { name: String },
    SetVar { name: String },
}

impl NodeKind {
    pub fn title(&self) -> String {
        match self {
            NodeKind::OnStart           => "On Start".into(),
            NodeKind::OnUpdate          => "On Update".into(),
            NodeKind::OnKeyHeld { key } => format!("Key Held [{}]", key),
            NodeKind::OnKeyPress{ key } => format!("Key Press [{}]", key),
            NodeKind::OnCollide { tag_filter } => format!("On Collide [{}]", tag_filter),
            NodeKind::Branch            => "Branch".into(),
            NodeKind::Sequence { .. }   => "Sequence".into(),
            NodeKind::SetVelocity       => "Set Velocity".into(),
            NodeKind::SetPosition       => "Set Position".into(),
            NodeKind::Despawn           => "Despawn".into(),
            NodeKind::Spawn             => "Spawn".into(),
            NodeKind::LoadLevel  { path }   => format!("Load Level [{}]", path),
            NodeKind::PlaySound  { path }   => format!("Play Sound [{}]", path),
            NodeKind::Log               => "Log".into(),
            NodeKind::SetGlyph          => "Set Glyph".into(),
            NodeKind::DrawHUD           => "Draw HUD".into(),
            NodeKind::GetPosition       => "Get Position".into(),
            NodeKind::GetVelocity       => "Get Velocity".into(),
            NodeKind::GetTag            => "Get Tag".into(),
            NodeKind::GetDelta          => "Get Delta".into(),
            NodeKind::FloatLit { value }=> format!("Float {:.2}", value),
            NodeKind::StringLit{ value }=> format!("Str \"{}\"", value),
            NodeKind::CompareFloat { op }=> format!("Compare {}", op.label()),
            NodeKind::MathOp   { op }   => op.label().into(),
            NodeKind::GetVar   { name } => format!("Get {}", name),
            NodeKind::SetVar   { name } => format!("Set {}", name),
        }
    }

    pub fn is_event(&self) -> bool {
        matches!(self, NodeKind::OnStart | NodeKind::OnUpdate
            | NodeKind::OnKeyHeld {..} | NodeKind::OnKeyPress {..} | NodeKind::OnCollide {..})
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, NodeKind::Despawn | NodeKind::LoadLevel {..})
    }
}

// ── Port spec ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDir  { In, Out }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortKind { Exec, Data }

#[derive(Debug, Clone)]
pub struct PortSpec {
    pub label: &'static str,
    pub dir:   PortDir,
    pub kind:  PortKind,
}

impl PortSpec {
    const fn exec_in(label: &'static str)  -> Self { PortSpec { label, dir: PortDir::In,  kind: PortKind::Exec } }
    const fn exec_out(label: &'static str) -> Self { PortSpec { label, dir: PortDir::Out, kind: PortKind::Exec } }
    const fn data_in(label: &'static str)  -> Self { PortSpec { label, dir: PortDir::In,  kind: PortKind::Data } }
    const fn data_out(label: &'static str) -> Self { PortSpec { label, dir: PortDir::Out, kind: PortKind::Data } }
}

/// Returns the ordered list of ports for a node. Port indices in Edge structs
/// are indices into the In or Out sublists (split by PortDir).
pub fn ports_for(kind: &NodeKind) -> Vec<PortSpec> {
    use PortSpec as P;
    match kind {
        // Events
        NodeKind::OnStart  => vec![P::exec_out("Out")],
        NodeKind::OnUpdate => vec![P::exec_out("Out"), P::data_out("Delta")],
        NodeKind::OnKeyHeld {..} | NodeKind::OnKeyPress {..}
                           => vec![P::exec_out("Out")],
        NodeKind::OnCollide {..}
                           => vec![P::exec_out("Out"), P::data_out("Other")],
        // Flow
        NodeKind::Branch   => vec![
            P::exec_in("In"), P::data_in("Cond"),
            P::exec_out("True"), P::exec_out("False"),
        ],
        NodeKind::Sequence { outputs } => {
            let mut v = vec![P::exec_in("In")];
            for i in 0..*outputs { v.push(P::exec_out(seq_label(i))); }
            v
        }
        // Actions
        NodeKind::SetVelocity => vec![
            P::exec_in("In"), P::data_in("VX"), P::data_in("VY"), P::exec_out("Out"),
        ],
        NodeKind::SetPosition => vec![
            P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::exec_out("Out"),
        ],
        NodeKind::Despawn  => vec![P::exec_in("In")],
        NodeKind::Spawn    => vec![
            P::exec_in("In"),
            P::data_in("Glyph"), P::data_in("Tag"), P::data_in("X"), P::data_in("Y"),
            P::exec_out("Out"), P::data_out("Entity"),
        ],
        NodeKind::LoadLevel {..} => vec![P::exec_in("In")],
        NodeKind::PlaySound {..} => vec![P::exec_in("In"), P::exec_out("Out")],
        NodeKind::Log      => vec![P::exec_in("In"), P::data_in("Msg"), P::exec_out("Out")],
        NodeKind::SetGlyph => vec![P::exec_in("In"), P::data_in("Glyph"), P::exec_out("Out")],
        NodeKind::DrawHUD  => vec![
            P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::data_in("Text"),
            P::exec_out("Out"),
        ],
        // Pure
        NodeKind::GetPosition => vec![P::data_out("X"), P::data_out("Y")],
        NodeKind::GetVelocity => vec![P::data_out("VX"), P::data_out("VY")],
        NodeKind::GetTag      => vec![P::data_out("Tag")],
        NodeKind::GetDelta    => vec![P::data_out("Delta")],
        NodeKind::FloatLit {..}    => vec![P::data_out("Value")],
        NodeKind::StringLit {..}   => vec![P::data_out("Value")],
        NodeKind::CompareFloat {..} => vec![P::data_in("A"), P::data_in("B"), P::data_out("Bool")],
        NodeKind::MathOp {..}      => vec![P::data_in("A"), P::data_in("B"), P::data_out("Result")],
        // Variables
        NodeKind::GetVar {..} => vec![P::data_out("Value")],
        NodeKind::SetVar {..} => vec![P::exec_in("In"), P::data_in("Value"), P::exec_out("Out")],
    }
}

fn seq_label(i: usize) -> &'static str {
    match i { 0=>"0", 1=>"1", 2=>"2", 3=>"3", _=>"N" }
}

// ── Node & Edge ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id:   NodeId,
    pub kind: NodeKind,
    pub x:    i32,
    pub y:    i32,
}

/// Edge connects output port `from_port` (index into Out ports) of `from_node`
/// to input port `to_port` (index into In ports) of `to_node`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge {
    pub from_node: NodeId,
    pub from_port: usize,
    pub to_node:   NodeId,
    pub to_port:   usize,
}

// ── NodeGraph ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeGraph {
    pub nodes:   Vec<Node>,
    pub edges:   Vec<Edge>,
    next_id:     NodeId,
}

impl NodeGraph {
    pub fn add_node(&mut self, kind: NodeKind, x: i32, y: i32) -> NodeId {
        let id = self.next_id;
        self.next_id += 1;
        self.nodes.push(Node { id, kind, x, y });
        id
    }

    pub fn remove_node(&mut self, id: NodeId) {
        self.nodes.retain(|n| n.id != id);
        self.edges.retain(|e| e.from_node != id && e.to_node != id);
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    pub fn add_edge(&mut self, from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize) {
        // Remove any existing edge into the same input port (one driver per port)
        self.edges.retain(|e| !(e.to_node == to_node && e.to_port == to_port));
        self.edges.push(Edge { from_node, from_port, to_node, to_port });
    }

    pub fn remove_edges_for(&mut self, id: NodeId) {
        self.edges.retain(|e| e.from_node != id && e.to_node != id);
    }

    pub fn exec_out_edge(&self, from_node: NodeId, from_port: usize) -> Option<&Edge> {
        self.edges.iter().find(|e| e.from_node == from_node && e.from_port == from_port)
    }

    pub fn data_in_edge(&self, to_node: NodeId, to_port: usize) -> Option<&Edge> {
        self.edges.iter().find(|e| e.to_node == to_node && e.to_port == to_port)
    }

    /// Lay out nodes left-to-right along exec chains.
    pub fn auto_layout(&mut self) {
        // Build exec adjacency for a simple BFS layout.
        let roots: Vec<NodeId> = self.nodes.iter()
            .filter(|n| n.kind.is_event())
            .map(|n| n.id)
            .collect();

        let mut col: std::collections::HashMap<NodeId, i32> = std::collections::HashMap::new();
        let mut row_in_col: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();

        let mut queue = std::collections::VecDeque::new();
        for (r, &id) in roots.iter().enumerate() {
            col.insert(id, 0);
            queue.push_back((id, 0i32, r as i32));
        }

        while let Some((nid, c, _r)) = queue.pop_front() {
            let ports = self.nodes.iter().find(|n| n.id == nid)
                .map(|n| ports_for(&n.kind)).unwrap_or_default();
            let out_exec: Vec<usize> = ports.iter().enumerate()
                .filter(|(_, p)| p.dir == PortDir::Out && p.kind == PortKind::Exec)
                .map(|(i, _)| i)
                .collect();
            for (slot, &port_i) in out_exec.iter().enumerate() {
                if let Some(e) = self.exec_out_edge(nid, port_i) {
                    let child = e.to_node;
                    col.entry(child).or_insert(c + 1);
                    queue.push_back((child, c + 1, slot as i32));
                }
            }
        }

        // Pure (data-only) nodes go at col -1
        for node in &self.nodes {
            if !col.contains_key(&node.id) && !node.kind.is_event() {
                col.insert(node.id, -1);
            }
        }

        // Assign positions
        let col_w = 26i32;
        let row_h = 10i32;
        let node_ids: Vec<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        for id in node_ids {
            let c = *col.get(&id).unwrap_or(&0);
            let r = row_in_col.entry(c).or_insert(0);
            let x = c * col_w + 2;
            let y = *r * row_h + 2;
            if let Some(n) = self.get_mut(id) {
                n.x = x;
                n.y = y;
            }
            *r += 1;
        }
    }
}

// ── Port index helpers ────────────────────────────────────────────────────────

/// Split ports into (inputs, outputs) by PortDir.
pub fn split_ports(ports: &[PortSpec]) -> (Vec<(usize, &PortSpec)>, Vec<(usize, &PortSpec)>) {
    let ins:  Vec<_> = ports.iter().enumerate().filter(|(_, p)| p.dir == PortDir::In ).collect();
    let outs: Vec<_> = ports.iter().enumerate().filter(|(_, p)| p.dir == PortDir::Out).collect();
    (ins, outs)
}

// ── Code generation ───────────────────────────────────────────────────────────

pub fn generate_graph(graph: &NodeGraph) -> String {
    if graph.nodes.is_empty() { return String::new(); }

    let mut on_start   = String::new();
    let mut on_update  = String::new();
    let mut on_collide = String::new();

    // Collect spawn tmp-var counter
    let mut tmp_counter = 0usize;
    let mut spawn_vars: std::collections::HashMap<NodeId, String> = std::collections::HashMap::new();

    for node in &graph.nodes {
        if !node.kind.is_event() { continue; }
        let ports = ports_for(&node.kind);
        let (_, outs) = split_ports(&ports);
        // exec-out is always first out port for events
        let exec_body = if let Some((pidx, _)) = outs.iter().find(|(_, p)| p.kind == PortKind::Exec) {
            gen_exec_chain(graph, node.id, *pidx, 1, &mut tmp_counter, &mut spawn_vars)
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
                on_update.push_str(&format!("    if ctx.is_held(\"{}\") {{\n{}    }}\n", key, exec_body));
            }
            NodeKind::OnKeyPress { key } => {
                on_update.push_str(&format!("    if ctx.just_pressed(\"{}\") {{\n{}    }}\n", key, exec_body));
            }
            NodeKind::OnCollide { tag_filter } => {
                if tag_filter.is_empty() {
                    on_collide.push_str(&exec_body);
                } else {
                    on_collide.push_str(&format!(
                        "    if ctx.get_tag(other) == \"{}\" {{\n{}    }}\n", tag_filter, exec_body));
                }
            }
            _ => {}
        }
    }

    format!(
        "fn on_start(id, ctx) {{\n{}}}\nfn on_update(id, ctx) {{\n{}}}\nfn on_collide(id, other, ctx) {{\n{}}}\n",
        on_start, on_update, on_collide,
    )
}

fn indent(n: usize) -> String { "    ".repeat(n) }

fn gen_exec_chain(
    graph: &NodeGraph,
    from_node: NodeId,
    from_port: usize,
    depth: usize,
    tmp: &mut usize,
    spawn_vars: &mut std::collections::HashMap<NodeId, String>,
) -> String {
    let edge = match graph.exec_out_edge(from_node, from_port) {
        Some(e) => e.clone(),
        None    => return String::new(),
    };
    let node = match graph.get(edge.to_node) {
        Some(n) => n.clone(),
        None    => return String::new(),
    };
    gen_node_stmt(graph, &node, depth, tmp, spawn_vars)
}

fn gen_node_stmt(
    graph: &NodeGraph,
    node: &Node,
    depth: usize,
    tmp: &mut usize,
    spawn_vars: &mut std::collections::HashMap<NodeId, String>,
) -> String {
    let ind = indent(depth);
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);

    // Helper: resolve a data input at in-port index (among data-in ports)
    let data_ins: Vec<_> = ins.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();

    macro_rules! resolve {
        ($idx:expr) => {{
            let abs_port = data_ins.get($idx).map(|(i, _)| *i).unwrap_or(0);
            resolve_data(graph, node.id, abs_port, spawn_vars)
        }};
    }

    // Find exec-out port indices
    let exec_outs: Vec<usize> = outs.iter()
        .filter(|(_, p)| p.kind == PortKind::Exec)
        .map(|(i, _)| *i)
        .collect();

    let mut out = String::new();

    match &node.kind {
        NodeKind::SetVelocity => {
            out += &format!("{}ctx.set_velocity(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::SetPosition => {
            out += &format!("{}ctx.set_position(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::Despawn => {
            out += &format!("{}ctx.despawn(id);\n", ind);
        }
        NodeKind::Spawn => {
            let var = format!("__tmp{}", tmp);
            *tmp += 1;
            spawn_vars.insert(node.id, var.clone());
            out += &format!("{}let {} = ctx.spawn({}, {}, {}, {});\n",
                ind, var, resolve!(0), resolve!(2), resolve!(3), resolve!(1));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::LoadLevel { path } => {
            out += &format!("{}ctx.load_level(\"{}\");\n", ind, path);
        }
        NodeKind::PlaySound { path } => {
            out += &format!("{}ctx.play_sound(\"{}\");\n", ind, path);
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::Log => {
            out += &format!("{}ctx.log({});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::SetGlyph => {
            out += &format!("{}ctx.set_glyph(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::DrawHUD => {
            out += &format!("{}ctx.draw_hud({}, {}, {}, \"White\", \"Reset\");\n",
                ind, resolve!(0), resolve!(1), resolve!(2));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::Branch => {
            let cond = resolve!(0);
            let true_body  = exec_outs.get(0).map(|&p|
                gen_exec_chain(graph, node.id, p, depth + 1, tmp, spawn_vars))
                .unwrap_or_default();
            let false_body = exec_outs.get(1).map(|&p|
                gen_exec_chain(graph, node.id, p, depth + 1, tmp, spawn_vars))
                .unwrap_or_default();
            out += &format!("{}if {} {{\n{}{}}} else {{\n{}{}}}\n",
                ind, cond, true_body, ind, false_body, ind);
        }
        NodeKind::Sequence { .. } => {
            for &p in &exec_outs {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        NodeKind::SetVar { name } => {
            out += &format!("{}let __var_{} = {};\n", ind, name, resolve!(0));
            if let Some(&p) = exec_outs.first() {
                out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars);
            }
        }
        _ => {}
    }
    out
}

fn resolve_data(
    graph: &NodeGraph,
    to_node: NodeId,
    to_port: usize,
    spawn_vars: &std::collections::HashMap<NodeId, String>,
) -> String {
    // to_port is the absolute port index (not just among data-ins)
    // Find edge into this port
    if let Some(e) = graph.data_in_edge(to_node, to_port) {
        let src = match graph.get(e.from_node) { Some(n)=>n, None=>return "0.0".into() };
        return codegen_expr(graph, src, e.from_port, spawn_vars);
    }
    // No edge — return fallback based on what the node kind expects
    "0.0".into()
}

fn codegen_expr(
    graph: &NodeGraph,
    node: &Node,
    out_port: usize,
    spawn_vars: &std::collections::HashMap<NodeId, String>,
) -> String {
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    let data_ins: Vec<_> = ins.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();
    let _data_outs: Vec<_> = outs.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();

    // out_port is the absolute port index; find its position among data-outs
    let out_idx = outs.iter().filter(|(_, p)| p.kind == PortKind::Data)
        .position(|(i, _)| *i == out_port).unwrap_or(0);

    macro_rules! resolve_in {
        ($idx:expr) => {{
            let abs = data_ins.get($idx).map(|(i, _)| *i).unwrap_or(0);
            resolve_data(graph, node.id, abs, spawn_vars)
        }};
    }

    match &node.kind {
        NodeKind::FloatLit { value }  => format!("{:.6}", value),
        NodeKind::StringLit { value } => format!("\"{}\"", value),
        NodeKind::GetPosition => if out_idx == 0 { "ctx.get_x(id)".into() } else { "ctx.get_y(id)".into() },
        NodeKind::GetVelocity => if out_idx == 0 { "ctx.get_vel_x(id)".into() } else { "ctx.get_vel_y(id)".into() },
        NodeKind::GetTag      => "ctx.get_tag(id)".into(),
        NodeKind::GetDelta    => "ctx.get_delta()".into(),
        NodeKind::CompareFloat { op } => {
            format!("({} {} {})", resolve_in!(0), op.as_str(), resolve_in!(1))
        }
        NodeKind::MathOp { op } => {
            format!("({} {} {})", resolve_in!(0), op.as_str(), resolve_in!(1))
        }
        NodeKind::GetVar { name } => format!("__var_{}", name),
        NodeKind::OnCollide { .. } => {
            // data-out "Other" resolves to the `other` argument
            "other".into()
        }
        NodeKind::OnUpdate => "ctx.get_delta()".into(),
        NodeKind::Spawn => {
            spawn_vars.get(&node.id).cloned().unwrap_or_else(|| "0".into())
        }
        _ => "0.0".into(),
    }
}

// ── Drawing ───────────────────────────────────────────────────────────────────

pub const NODE_MIN_W: usize = 20;

/// Compute (width, height) for a node in cells (including border rows).
pub fn node_size(kind: &NodeKind) -> (usize, usize) {
    let ports = ports_for(kind);
    let (ins, outs) = split_ports(&ports);
    let rows = 1 + ins.len().max(outs.len()); // title + max(in,out) rows
    let rows = rows.max(2);
    let title_len = kind.title().len() + 4; // "-- Title --"
    let w = title_len.max(NODE_MIN_W);
    (w, rows + 1) // +1 for bottom border
}

/// Absolute screen position of a port's connection point.
/// Returns None if off-screen.
pub fn port_screen_pos(
    node: &Node, port: &PortSpec, port_dir_idx: usize,
    view_ox: i32, view_oy: i32,
) -> Option<(i32, i32)> {
    let (w, _h) = node_size(&node.kind);
    let sx = node.x + view_ox;
    let sy = node.y + view_oy;
    let row = (sy + 1 + port_dir_idx as i32) as i32;
    let col = match port.dir {
        PortDir::In  => sx,
        PortDir::Out => sx + w as i32 - 1,
    };
    Some((col, row))
}

/// Draw a single node. `sx/sy` are screen (col, row) of the top-left corner.
pub fn draw_node(
    renderer: &mut Renderer,
    node: &Node,
    selected: bool,
    view_ox: i32, view_oy: i32,
    screen_w: usize, screen_h: usize,
) {
    let (w, h) = node_size(&node.kind);
    let sx = node.x + view_ox;
    let sy = node.y + view_oy;
    if sx + w as i32 <= 0 || sy + h as i32 <= 0
        || sx >= screen_w as i32 || sy >= screen_h as i32 { return; }

    let sx = sx.max(0) as usize;
    let sy = sy.max(0) as usize;

    // Title bar
    let (title_fg, title_bg) = if selected {
        (Color::Black, Color::Cyan)
    } else {
        (Color::White, Color::DarkBlue)
    };
    let title = node.kind.title();
    let title_str: String = format!(" {:<width$}", title, width = w.saturating_sub(2))
        .chars().take(w).collect();
    renderer.draw_str(sx, sy, &title_str, title_fg, title_bg);

    // Body background
    let body_rows = h.saturating_sub(2);
    for r in 0..body_rows {
        let fill: String = std::iter::repeat(' ').take(w).collect();
        renderer.draw_str(sx, sy + 1 + r, &fill, Color::White, Color::DarkGrey);
    }
    // Bottom border
    let bot_line: String = std::iter::once('+')
        .chain(std::iter::repeat('-').take(w.saturating_sub(2)))
        .chain(std::iter::once('+'))
        .collect();
    if sy + h - 1 < screen_h {
        renderer.draw_str(sx, sy + h - 1, &bot_line, Color::DarkGrey, Color::DarkGrey);
    }

    // Draw ports
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);

    for (dir_idx, (abs_idx, spec)) in ins.iter().enumerate() {
        let row = sy + 1 + dir_idx;
        if row >= screen_h { break; }
        // Port glyph at left edge
        let (glyph, glyph_fg) = if spec.kind == PortKind::Exec {
            ('>', Color::White)
        } else {
            ('*', Color::Yellow)
        };
        renderer.draw_char(sx, row, glyph, glyph_fg, Color::DarkGrey);
        // Label
        let lbl: String = format!(" {}", spec.label).chars().take(w.saturating_sub(1)).collect();
        renderer.draw_str(sx + 1, row, &lbl, Color::White, Color::DarkGrey);
        let _ = abs_idx;
    }

    for (dir_idx, (abs_idx, spec)) in outs.iter().enumerate() {
        let row = sy + 1 + dir_idx;
        if row >= screen_h { break; }
        let (glyph, glyph_fg) = if spec.kind == PortKind::Exec {
            ('>', Color::White)
        } else {
            ('o', Color::Cyan)
        };
        // Right-align label + glyph
        let lbl = spec.label;
        let lbl_len = lbl.len();
        let glyph_col = sx + w - 1;
        let lbl_col = glyph_col.saturating_sub(lbl_len + 1);
        if glyph_col < screen_w {
            renderer.draw_char(glyph_col, row, glyph, glyph_fg, Color::DarkGrey);
        }
        if lbl_col < screen_w && lbl_col > sx {
            renderer.draw_str(lbl_col, row, lbl, Color::DarkGrey, Color::DarkGrey);
        }
        let _ = abs_idx;
    }
}

/// Draw a Manhattan-routed wire between two screen positions.
/// `edge_idx` staggers the vertical channel column.
pub fn draw_wire(
    renderer: &mut Renderer,
    ox: i32, oy: i32,   // output port screen position
    ix: i32, iy: i32,   // input port screen position
    edge_idx: usize,
    color: Color,
    screen_w: usize, screen_h: usize,
) {
    let ch_col = ox + 2 + (edge_idx as i32 % 4);

    // Segment 1: horizontal from output to channel col
    let x0 = ox + 1;
    let x1 = ch_col;
    if oy >= 0 && (oy as usize) < screen_h {
        for x in x0.min(x1)..=x0.max(x1) {
            if x >= 0 && (x as usize) < screen_w {
                renderer.draw_char(x as usize, oy as usize, '-', color, Color::Black);
            }
        }
    }

    // Segment 2: vertical
    let y0 = oy.min(iy);
    let y1 = oy.max(iy);
    for y in y0..=y1 {
        if y >= 0 && (y as usize) < screen_h && ch_col >= 0 && (ch_col as usize) < screen_w {
            let ch = if y == y0 || y == y1 { '+' } else { '|' };
            renderer.draw_char(ch_col as usize, y as usize, ch, color, Color::Black);
        }
    }

    // Segment 3: horizontal from channel col to input port
    let x0 = ch_col;
    let x1 = ix - 1;
    if iy >= 0 && (iy as usize) < screen_h {
        for x in x0.min(x1)..=x0.max(x1) {
            if x >= 0 && (x as usize) < screen_w {
                renderer.draw_char(x as usize, iy as usize, '-', color, Color::Black);
            }
        }
    }
}

/// Draw all nodes and edges in the graph.
pub fn draw_graph(
    renderer: &mut Renderer,
    graph: &NodeGraph,
    selected_node: Option<NodeId>,
    connecting: Option<(NodeId, usize)>,  // from_node + from_port_dir_idx
    mouse_col: usize, mouse_row: usize,
    view_ox: i32, view_oy: i32,
    screen_w: usize, screen_h: usize,
) {
    // Draw background
    for y in 0..screen_h {
        let row: String = std::iter::repeat(' ').take(screen_w).collect();
        renderer.draw_str(0, y, &row, Color::DarkGrey, Color::Black);
    }

    // Compute node screen rects for edge drawing
    let mut port_positions: Vec<(NodeId, Vec<(i32,i32)>, Vec<(i32,i32)>)> = Vec::new();
    for node in &graph.nodes {
        let ports = ports_for(&node.kind);
        let (ins, outs) = split_ports(&ports);
        let in_pos: Vec<(i32,i32)> = ins.iter().enumerate().map(|(di, (_, p))| {
            port_screen_pos(node, p, di, view_ox, view_oy).unwrap_or((-1,-1))
        }).collect();
        let out_pos: Vec<(i32,i32)> = outs.iter().enumerate().map(|(di, (_, p))| {
            port_screen_pos(node, p, di, view_ox, view_oy).unwrap_or((-1,-1))
        }).collect();
        port_positions.push((node.id, in_pos, out_pos));
    }

    // Draw edges
    for (ei, edge) in graph.edges.iter().enumerate() {
        let src = port_positions.iter().find(|(id, _, _)| *id == edge.from_node);
        let dst = port_positions.iter().find(|(id, _, _)| *id == edge.to_node);
        if let (Some((_, _, out_pos)), Some((_, in_pos, _))) = (src, dst) {
            if let (Some(&(ox,oy)), Some(&(ix,iy))) =
                (out_pos.get(edge.from_port), in_pos.get(edge.to_port))
            {
                let sel = selected_node.map_or(false, |s| s == edge.from_node || s == edge.to_node);
                let color = if sel { Color::Cyan } else { Color::DarkGrey };
                draw_wire(renderer, ox, oy, ix, iy, ei, color, screen_w, screen_h);
            }
        }
    }

    // Draw live wire if connecting
    if let Some((from_id, from_port_di)) = connecting {
        if let Some((_, _, out_pos)) = port_positions.iter().find(|(id, _, _)| *id == from_id) {
            if let Some(&(ox, oy)) = out_pos.get(from_port_di) {
                draw_wire(renderer, ox, oy, mouse_col as i32, mouse_row as i32,
                          0, Color::Yellow, screen_w, screen_h);
            }
        }
    }

    // Draw nodes (on top of wires)
    for node in &graph.nodes {
        let sel = selected_node == Some(node.id);
        draw_node(renderer, node, sel, view_ox, view_oy, screen_w, screen_h);
    }
}

/// Draw the node-add palette at (px, py) on screen.
pub fn draw_palette(
    renderer: &mut Renderer,
    scroll: usize, cursor: usize,
    px: usize, py: usize,
    _screen_w: usize, screen_h: usize,
) {
    let entries = palette_entries();
    let visible_h = (screen_h.saturating_sub(py + 1)).min(18);
    let w = 22usize;

    // Header
    let hdr = format!("{:─<width$}", "─ Add Node ", width = w);
    renderer.draw_str(px, py, &hdr, Color::White, Color::DarkBlue);

    for (i, entry) in entries.iter().skip(scroll).take(visible_h).enumerate() {
        let row = py + 1 + i;
        if row >= screen_h { break; }
        let real_idx = scroll + i;
        let (fg, bg) = if real_idx == cursor {
            (Color::Black, Color::Cyan)
        } else if entry.0.is_empty() {
            (Color::DarkGrey, Color::Black)  // section header
        } else {
            (Color::White, Color::Black)
        };
        let label = if entry.0.is_empty() {
            format!(" {:─<width$}", entry.1, width = w.saturating_sub(2))
        } else {
            format!("  {:<width$}", entry.1, width = w.saturating_sub(2))
        };
        let line: String = label.chars().take(w).collect();
        renderer.draw_str(px, row, &line, fg, bg);
    }
    // Footer if more items
    let end_row = py + 1 + visible_h;
    if end_row < screen_h && scroll + visible_h < entries.len() {
        renderer.draw_str(px, end_row, "  ▼ more", Color::DarkGrey, Color::Black);
    }
}

/// (key, display_label) pairs for the palette. Empty key = section header.
pub fn palette_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("", "Events"),
        ("OnStart",     "On Start"),
        ("OnUpdate",    "On Update"),
        ("OnKeyHeld",   "Key Held"),
        ("OnKeyPress",  "Key Press"),
        ("OnCollide",   "On Collide"),
        ("", "Flow"),
        ("Branch",      "Branch"),
        ("Sequence",    "Sequence"),
        ("", "Actions"),
        ("SetVelocity", "Set Velocity"),
        ("SetPosition", "Set Position"),
        ("Despawn",     "Despawn"),
        ("Spawn",       "Spawn"),
        ("LoadLevel",   "Load Level"),
        ("PlaySound",   "Play Sound"),
        ("Log",         "Log"),
        ("SetGlyph",    "Set Glyph"),
        ("DrawHUD",     "Draw HUD"),
        ("", "Values"),
        ("FloatLit",    "Float Literal"),
        ("StringLit",   "String Literal"),
        ("CompareFloat","Compare Float"),
        ("MathOp",      "Math Op"),
        ("GetPosition", "Get Position"),
        ("GetVelocity", "Get Velocity"),
        ("GetTag",      "Get Tag"),
        ("GetDelta",    "Get Delta"),
        ("", "Variables"),
        ("GetVar",      "Get Variable"),
        ("SetVar",      "Set Variable"),
    ]
}

/// Build a default NodeKind from a palette key.
pub fn palette_make(key: &str) -> Option<NodeKind> {
    Some(match key {
        "OnStart"      => NodeKind::OnStart,
        "OnUpdate"     => NodeKind::OnUpdate,
        "OnKeyHeld"    => NodeKind::OnKeyHeld   { key: "space".into() },
        "OnKeyPress"   => NodeKind::OnKeyPress  { key: "space".into() },
        "OnCollide"    => NodeKind::OnCollide   { tag_filter: "player".into() },
        "Branch"       => NodeKind::Branch,
        "Sequence"     => NodeKind::Sequence    { outputs: 3 },
        "SetVelocity"  => NodeKind::SetVelocity,
        "SetPosition"  => NodeKind::SetPosition,
        "Despawn"      => NodeKind::Despawn,
        "Spawn"        => NodeKind::Spawn,
        "LoadLevel"    => NodeKind::LoadLevel   { path: String::new() },
        "PlaySound"    => NodeKind::PlaySound   { path: String::new() },
        "Log"          => NodeKind::Log,
        "SetGlyph"     => NodeKind::SetGlyph,
        "DrawHUD"      => NodeKind::DrawHUD,
        "FloatLit"     => NodeKind::FloatLit    { value: 0.0 },
        "StringLit"    => NodeKind::StringLit   { value: String::new() },
        "CompareFloat" => NodeKind::CompareFloat { op: CmpOp::Gt },
        "MathOp"       => NodeKind::MathOp      { op: MathOp::Add },
        "GetPosition"  => NodeKind::GetPosition,
        "GetVelocity"  => NodeKind::GetVelocity,
        "GetTag"       => NodeKind::GetTag,
        "GetDelta"     => NodeKind::GetDelta,
        "GetVar"       => NodeKind::GetVar      { name: "x".into() },
        "SetVar"       => NodeKind::SetVar      { name: "x".into() },
        _              => return None,
    })
}

/// Hit-test: return NodeId at screen position (col, row).
pub fn node_at(graph: &NodeGraph, col: i32, row: i32, view_ox: i32, view_oy: i32) -> Option<NodeId> {
    // Iterate in reverse so topmost (last drawn) wins
    for node in graph.nodes.iter().rev() {
        let (w, h) = node_size(&node.kind);
        let sx = node.x + view_ox;
        let sy = node.y + view_oy;
        if col >= sx && col < sx + w as i32 && row >= sy && row < sy + h as i32 {
            return Some(node.id);
        }
    }
    None
}

/// Hit-test: return (NodeId, port_dir_idx, PortDir, PortKind) for a port glyph at (col, row).
pub fn port_at(
    graph: &NodeGraph, col: i32, row: i32, view_ox: i32, view_oy: i32,
) -> Option<(NodeId, usize, PortDir, PortKind)> {
    for node in &graph.nodes {
        let (w, _h) = node_size(&node.kind);
        let sx = node.x + view_ox;
        let sy = node.y + view_oy;
        let ports = ports_for(&node.kind);
        let (ins, outs) = split_ports(&ports);
        // Check input ports (left edge)
        if col == sx {
            for (di, (_, spec)) in ins.iter().enumerate() {
                let pr = sy + 1 + di as i32;
                if row == pr { return Some((node.id, di, PortDir::In, spec.kind)); }
            }
        }
        // Check output ports (right edge)
        if col == sx + w as i32 - 1 {
            for (di, (_, spec)) in outs.iter().enumerate() {
                let pr = sy + 1 + di as i32;
                if row == pr { return Some((node.id, di, PortDir::Out, spec.kind)); }
            }
        }
    }
    None
}
