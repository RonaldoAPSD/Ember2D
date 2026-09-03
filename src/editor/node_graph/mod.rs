// editor/node_graph/mod.rs — Node graph module re-exports and core logic.

use serde::{Deserialize, Serialize};

mod codegen;
mod ui;

pub use codegen::*;
pub use ui::*;

pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CmpOp { Gt, Lt, Eq, Ne, Ge, Le }

impl CmpOp {
    pub fn as_str(self) -> &'static str { match self { CmpOp::Gt => ">", CmpOp::Lt => "<", CmpOp::Eq => "==", CmpOp::Ne => "!=", CmpOp::Ge => ">=", CmpOp::Le => "<=" } }
    pub fn label(self) -> &'static str { match self { CmpOp::Gt => "GT", CmpOp::Lt => "LT", CmpOp::Eq => "EQ", CmpOp::Ne => "NE", CmpOp::Ge => "GE", CmpOp::Le => "LE" } }
    pub fn next(self) -> Self { match self { CmpOp::Gt=>CmpOp::Lt, CmpOp::Lt=>CmpOp::Eq, CmpOp::Eq=>CmpOp::Ne, CmpOp::Ne=>CmpOp::Ge, CmpOp::Ge=>CmpOp::Le, CmpOp::Le=>CmpOp::Gt } }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathOp { Add, Sub, Mul, Div }

impl MathOp {
    pub fn as_str(self) -> &'static str { match self { MathOp::Add=>"+", MathOp::Sub=>"-", MathOp::Mul=>"*", MathOp::Div=>"/" } }
    pub fn label(self) -> &'static str { match self { MathOp::Add=>"Add", MathOp::Sub=>"Sub", MathOp::Mul=>"Mul", MathOp::Div=>"Div" } }
    pub fn next(self) -> Self { match self { MathOp::Add=>MathOp::Sub, MathOp::Sub=>MathOp::Mul, MathOp::Mul=>MathOp::Div, MathOp::Div=>MathOp::Add } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeKind {
    OnStart, OnUpdate, OnKeyHeld { key: String }, OnKeyPress { key: String }, OnCollide { tag_filter: String },
    Branch, Sequence { outputs: usize },
    SetVelocity, SetPosition, Despawn, Spawn, LoadLevel { path: String }, PlaySound { path: String }, Log, SetGlyph, DrawHUD,
    GetPosition, GetVelocity, GetTag, GetDelta, FloatLit { value: f64 }, StringLit { value: String }, CompareFloat { op: CmpOp }, MathOp { op: MathOp },
    GetVar { name: String }, SetVar { name: String },

    // V0.4 Additions
    GetGlobal { name: String }, SetGlobal { name: String },
    GetPersistent { name: String }, SetPersistent { name: String },
    GetMousePos, IsSolidAt, GetEntityAt, GetDistance, GetAngleTo,
    SetCamera, ShakeCamera,
    StartTimer { name: String }, TimerDone { name: String }, CancelTimer { name: String },
    SetVisible, SetZOrder,

    // Final Parity Additions
    PlayMusic { path: String }, StopMusic,
    SetColor, DrawBox, FillRect, ClearHUD,
    RandomInt, RandomFloat, RandomBool, RandomChoice,
    GetElapsed, EntityExists, HasTag, GetColliderLayer, SetColliderLayer,
    FindEntitiesInRect, CountByTag, FindByTag, FindAllByTag,
    MouseLeftPressed, MouseLeftHeld,
}

impl NodeKind {
    pub fn title(&self) -> String {
        match self {
            NodeKind::OnStart => "On Start".into(), NodeKind::OnUpdate => "On Update".into(), NodeKind::OnKeyHeld { key } => format!("Key Held [{}]", key), NodeKind::OnKeyPress{ key } => format!("Key Press [{}]", key), NodeKind::OnCollide { tag_filter } => format!("On Collide [{}]", tag_filter),
            NodeKind::Branch => "Branch".into(), NodeKind::Sequence { .. } => "Sequence".into(), NodeKind::SetVelocity => "Set Velocity".into(), NodeKind::SetPosition => "Set Position".into(), NodeKind::Despawn => "Despawn".into(), NodeKind::Spawn => "Spawn".into(), NodeKind::LoadLevel { path } => format!("Load Level [{}]", path), NodeKind::PlaySound { path } => format!("Play Sound [{}]", path), NodeKind::Log => "Log".into(), NodeKind::SetGlyph => "Set Glyph".into(), NodeKind::DrawHUD => "Draw HUD".into(),
            NodeKind::GetPosition => "Get Position".into(), NodeKind::GetVelocity => "Get Velocity".into(), NodeKind::GetTag => "Get Tag".into(), NodeKind::GetDelta => "Get Delta".into(), NodeKind::FloatLit { value }=> format!("Float {:.2}", value), NodeKind::StringLit{ value }=> format!("Str \"{}\"", value), NodeKind::CompareFloat { op }=> format!("Compare {}", op.label()), NodeKind::MathOp { op } => op.label().into(), NodeKind::GetVar { name } => format!("Get {}", name), NodeKind::SetVar { name } => format!("Set {}", name),

            NodeKind::GetGlobal { name } => format!("Get Global [{}]", name), NodeKind::SetGlobal { name } => format!("Set Global [{}]", name),
            NodeKind::GetPersistent { name } => format!("Get Persist [{}]", name), NodeKind::SetPersistent { name } => format!("Set Persist [{}]", name),
            NodeKind::GetMousePos => "Mouse Pos".into(), NodeKind::IsSolidAt => "Is Solid At".into(), NodeKind::GetEntityAt => "Get Entity At".into(), NodeKind::GetDistance => "Get Distance".into(), NodeKind::GetAngleTo => "Get Angle To".into(),
            NodeKind::SetCamera => "Set Camera".into(), NodeKind::ShakeCamera => "Shake Camera".into(),
            NodeKind::StartTimer { name } => format!("Start Timer [{}]", name), NodeKind::TimerDone { name } => format!("Timer Done [{}]", name), NodeKind::CancelTimer { name } => format!("Cancel Timer [{}]", name),
            NodeKind::SetVisible => "Set Visible".into(), NodeKind::SetZOrder => "Set Z-Order".into(),

            NodeKind::PlayMusic { path } => format!("Play Music [{}]", path), NodeKind::StopMusic => "Stop Music".into(),
            NodeKind::SetColor => "Set Color".into(), NodeKind::DrawBox => "Draw Box".into(), NodeKind::FillRect => "Fill Rect".into(), NodeKind::ClearHUD => "Clear HUD".into(),
            NodeKind::RandomInt => "Random Int".into(), NodeKind::RandomFloat => "Random Float".into(), NodeKind::RandomBool => "Random Bool".into(), NodeKind::RandomChoice => "Random Choice".into(),
            NodeKind::GetElapsed => "Get Elapsed".into(), NodeKind::EntityExists => "Entity Exists".into(), NodeKind::HasTag => "Has Tag".into(), NodeKind::GetColliderLayer => "Get Layer".into(), NodeKind::SetColliderLayer => "Set Layer".into(),
            NodeKind::FindEntitiesInRect => "Entities In Rect".into(), NodeKind::CountByTag => "Count Tagged".into(), NodeKind::FindByTag => "Find Tagged".into(), NodeKind::FindAllByTag => "Find All Tagged".into(),
            NodeKind::MouseLeftPressed => "Mouse L-Press".into(), NodeKind::MouseLeftHeld => "Mouse L-Held".into(),
        }
    }
    pub fn is_event(&self) -> bool { matches!(self, NodeKind::OnStart | NodeKind::OnUpdate | NodeKind::OnKeyHeld {..} | NodeKind::OnKeyPress {..} | NodeKind::OnCollide {..}) }
    pub fn is_terminal(&self) -> bool { matches!(self, NodeKind::Despawn | NodeKind::LoadLevel {..}) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDir  { In, Out }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortKind { Exec, Data }

#[derive(Debug, Clone)]
pub struct PortSpec { pub label: &'static str, pub dir: PortDir, pub kind: PortKind }

impl PortSpec {
    const fn exec_in(label: &'static str)  -> Self { PortSpec { label, dir: PortDir::In,  kind: PortKind::Exec } }
    const fn exec_out(label: &'static str) -> Self { PortSpec { label, dir: PortDir::Out, kind: PortKind::Exec } }
    const fn data_in(label: &'static str)  -> Self { PortSpec { label, dir: PortDir::In,  kind: PortKind::Data } }
    const fn data_out(label: &'static str) -> Self { PortSpec { label, dir: PortDir::Out, kind: PortKind::Data } }
}

pub fn ports_for(kind: &NodeKind) -> Vec<PortSpec> {
    use PortSpec as P;
    match kind {
        NodeKind::OnStart => vec![P::exec_out("Out")], NodeKind::OnUpdate => vec![P::exec_out("Out"), P::data_out("Delta")], NodeKind::OnKeyHeld {..} | NodeKind::OnKeyPress {..} => vec![P::exec_out("Out")], NodeKind::OnCollide {..} => vec![P::exec_out("Out"), P::data_out("Other")],
        NodeKind::Branch => vec![P::exec_in("In"), P::data_in("Cond"), P::exec_out("True"), P::exec_out("False")],
        NodeKind::Sequence { outputs } => { let mut v = vec![P::exec_in("In")]; for i in 0..*outputs { v.push(P::exec_out(match i { 0=>"0", 1=>"1", 2=>"2", 3=>"3", _=>"N" })); } v }
        NodeKind::SetVelocity => vec![P::exec_in("In"), P::data_in("VX"), P::data_in("VY"), P::exec_out("Out")],
        NodeKind::SetPosition => vec![P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::exec_out("Out")],
        NodeKind::Despawn => vec![P::exec_in("In")], NodeKind::Spawn => vec![P::exec_in("In"), P::data_in("Glyph"), P::data_in("Tag"), P::data_in("X"), P::data_in("Y"), P::exec_out("Out"), P::data_out("Entity")],
        NodeKind::LoadLevel {..} => vec![P::exec_in("In")], NodeKind::PlaySound {..} => vec![P::exec_in("In"), P::exec_out("Out")], NodeKind::Log => vec![P::exec_in("In"), P::data_in("Msg"), P::exec_out("Out")], NodeKind::SetGlyph => vec![P::exec_in("In"), P::data_in("Glyph"), P::exec_out("Out")], NodeKind::DrawHUD => vec![P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::data_in("Text"), P::exec_out("Out")],
        NodeKind::GetPosition => vec![P::data_out("X"), P::data_out("Y")], NodeKind::GetVelocity => vec![P::data_out("VX"), P::data_out("VY")], NodeKind::GetTag => vec![P::data_out("Tag")], NodeKind::GetDelta => vec![P::data_out("Delta")], NodeKind::FloatLit {..} => vec![P::data_out("Value")], NodeKind::StringLit {..} => vec![P::data_out("Value")], NodeKind::CompareFloat {..} => vec![P::data_in("A"), P::data_in("B"), P::data_out("Bool")], NodeKind::MathOp {..} => vec![P::data_in("A"), P::data_in("B"), P::data_out("Result")],
        NodeKind::GetVar {..} => vec![P::data_out("Value")], NodeKind::SetVar {..} => vec![P::exec_in("In"), P::data_in("Value"), P::exec_out("Out")],

        NodeKind::GetGlobal {..} => vec![P::data_out("Value")], NodeKind::SetGlobal {..} => vec![P::exec_in("In"), P::data_in("Value"), P::exec_out("Out")],
        NodeKind::GetPersistent {..} => vec![P::data_out("Value")], NodeKind::SetPersistent {..} => vec![P::exec_in("In"), P::data_in("Value"), P::exec_out("Out")],
        NodeKind::GetMousePos => vec![P::data_out("X"), P::data_out("Y")], NodeKind::IsSolidAt => vec![P::data_in("X"), P::data_in("Y"), P::data_out("Bool")], NodeKind::GetEntityAt => vec![P::data_in("X"), P::data_in("Y"), P::data_out("Entity")], NodeKind::GetDistance => vec![P::data_in("A"), P::data_in("B"), P::data_out("Dist")], NodeKind::GetAngleTo => vec![P::data_in("A"), P::data_in("B"), P::data_out("Rad")],
        NodeKind::SetCamera => vec![P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::exec_out("Out")], NodeKind::ShakeCamera => vec![P::exec_in("In"), P::data_in("Pwr"), P::data_in("Dur"), P::exec_out("Out")],
        NodeKind::StartTimer {..} => vec![P::exec_in("In"), P::data_in("Sec"), P::exec_out("Out")], NodeKind::TimerDone {..} => vec![P::data_out("Bool")],
        NodeKind::SetVisible => vec![P::exec_in("In"), P::data_in("Bool"), P::exec_out("Out")], NodeKind::SetZOrder => vec![P::exec_in("In"), P::data_in("Z"), P::exec_out("Out")],

        NodeKind::CancelTimer {..} => vec![P::exec_in("In"), P::exec_out("Out")],
        NodeKind::PlayMusic {..} => vec![P::exec_in("In"), P::exec_out("Out")],
        NodeKind::StopMusic => vec![P::exec_in("In"), P::exec_out("Out")],
        NodeKind::SetColor => vec![P::exec_in("In"), P::data_in("FG"), P::data_in("BG"), P::exec_out("Out")],
        NodeKind::DrawBox => vec![P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::data_in("W"), P::data_in("H"), P::data_in("FG"), P::data_in("BG"), P::exec_out("Out")],
        NodeKind::FillRect => vec![P::exec_in("In"), P::data_in("X"), P::data_in("Y"), P::data_in("W"), P::data_in("H"), P::data_in("Char"), P::data_in("FG"), P::data_in("BG"), P::exec_out("Out")],
        NodeKind::ClearHUD => vec![P::exec_in("In"), P::exec_out("Out")],
        NodeKind::RandomInt => vec![P::data_in("Min"), P::data_in("Max"), P::data_out("Val")],
        NodeKind::RandomFloat => vec![P::data_out("Val")],
        NodeKind::RandomBool => vec![P::data_in("Prob"), P::data_out("Val")],
        NodeKind::RandomChoice => vec![P::data_in("Arr"), P::data_out("Val")],
        NodeKind::GetElapsed => vec![P::data_out("Val")],
        NodeKind::EntityExists => vec![P::data_in("ID"), P::data_out("Bool")],
        NodeKind::HasTag => vec![P::data_in("ID"), P::data_in("Tag"), P::data_out("Bool")],
        NodeKind::GetColliderLayer => vec![P::data_in("ID"), P::data_out("Lyr")],
        NodeKind::SetColliderLayer => vec![P::exec_in("In"), P::data_in("ID"), P::data_in("Lyr"), P::exec_out("Out")],
        NodeKind::FindEntitiesInRect => vec![P::data_in("X"), P::data_in("Y"), P::data_in("W"), P::data_in("H"), P::data_out("Arr")],
        NodeKind::CountByTag => vec![P::data_in("Tag"), P::data_out("Val")],
        NodeKind::FindByTag => vec![P::data_in("Tag"), P::data_out("ID")],
        NodeKind::FindAllByTag => vec![P::data_in("Tag"), P::data_out("Arr")],
        NodeKind::MouseLeftPressed => vec![P::data_out("Bool")],
        NodeKind::MouseLeftHeld => vec![P::data_out("Bool")],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node { pub id: NodeId, pub kind: NodeKind, pub x: i32, pub y: i32 }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Edge { pub from_node: NodeId, pub from_port: usize, pub to_node: NodeId, pub to_port: usize }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeGraph { pub nodes: Vec<Node>, pub edges: Vec<Edge>, next_id: NodeId }

impl NodeGraph {
    pub fn add_node(&mut self, kind: NodeKind, x: i32, y: i32) -> NodeId { let id = self.next_id; self.next_id += 1; self.nodes.push(Node { id, kind, x, y }); id }
    pub fn remove_node(&mut self, id: NodeId) { self.nodes.retain(|n| n.id != id); self.edges.retain(|e| e.from_node != id && e.to_node != id); }
    pub fn get(&self, id: NodeId) -> Option<&Node> { self.nodes.iter().find(|n| n.id == id) }
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> { self.nodes.iter_mut().find(|n| n.id == id) }
    pub fn add_edge(&mut self, from_node: NodeId, from_port: usize, to_node: NodeId, to_port: usize) { self.edges.retain(|e| !(e.to_node == to_node && e.to_port == to_port)); self.edges.push(Edge { from_node, from_port, to_node, to_port }); }
    pub fn remove_edges_for(&mut self, id: NodeId) { self.edges.retain(|e| e.from_node != id && e.to_node != id); }
    pub fn exec_out_edge(&self, from_node: NodeId, from_port: usize) -> Option<&Edge> { self.edges.iter().find(|e| e.from_node == from_node && e.from_port == from_port) }
    pub fn data_in_edge(&self, to_node: NodeId, to_port: usize) -> Option<&Edge> { self.edges.iter().find(|e| e.to_node == to_node && e.to_port == to_port) }

    pub fn auto_layout(&mut self) {
        let roots: Vec<NodeId> = self.nodes.iter().filter(|n| n.kind.is_event()).map(|n| n.id).collect();
        let mut col: std::collections::HashMap<NodeId, i32> = std::collections::HashMap::new();
        let mut row_in_col: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
        let mut queue = std::collections::VecDeque::new();
        for (r, &id) in roots.iter().enumerate() { col.insert(id, 0); queue.push_back((id, 0i32, r as i32)); }
        while let Some((nid, c, _r)) = queue.pop_front() {
            let ports = self.nodes.iter().find(|n| n.id == nid).map(|n| ports_for(&n.kind)).unwrap_or_default();
            let out_exec: Vec<usize> = ports.iter().enumerate().filter(|(_, p)| p.dir == PortDir::Out && p.kind == PortKind::Exec).map(|(i, _)| i).collect();
            for (slot, &port_i) in out_exec.iter().enumerate() { if let Some(e) = self.exec_out_edge(nid, port_i) { let child = e.to_node; col.entry(child).or_insert(c + 1); queue.push_back((child, c + 1, slot as i32)); } }
        }
        for node in &self.nodes { if !col.contains_key(&node.id) && !node.kind.is_event() { col.insert(node.id, -1); } }
        let col_w = 26i32; let row_h = 10i32;
        let node_ids: Vec<NodeId> = self.nodes.iter().map(|n| n.id).collect();
        for id in node_ids { let c = *col.get(&id).unwrap_or(&0); let r = row_in_col.entry(c).or_insert(0); let x = c * col_w + 2; let y = *r * row_h + 2; if let Some(n) = self.get_mut(id) { n.x = x; n.y = y; } *r += 1; }
    }
}

pub fn split_ports(ports: &[PortSpec]) -> (Vec<(usize, &PortSpec)>, Vec<(usize, &PortSpec)>) {
    let ins:  Vec<_> = ports.iter().enumerate().filter(|(_, p)| p.dir == PortDir::In ).collect();
    let outs: Vec<_> = ports.iter().enumerate().filter(|(_, p)| p.dir == PortDir::Out).collect();
    (ins, outs)
}
