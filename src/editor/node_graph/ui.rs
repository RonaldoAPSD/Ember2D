// editor/node_graph/ui.rs — Drawing and hit-testing for the node graph.

use crate::renderer::{color::Color, Renderer};
use super::*;

pub const NODE_MIN_W: usize = 20;

pub fn node_size(kind: &NodeKind) -> (usize, usize) {
    let ports = ports_for(kind);
    let (ins, outs) = split_ports(&ports);
    let rows = 1 + ins.len().max(outs.len());
    let rows = rows.max(2);
    let title_len = kind.title().len() + 4;
    let w = title_len.max(NODE_MIN_W);
    (w, rows + 1)
}

pub fn port_screen_pos(node: &Node, port: &PortSpec, port_dir_idx: usize, view_ox: i32, view_oy: i32) -> Option<(i32, i32)> {
    let (w, _h) = node_size(&node.kind);
    let sx = node.x + view_ox;
    let sy = node.y + view_oy;
    let row = (sy + 1 + port_dir_idx as i32) as i32;
    let col = match port.dir { PortDir::In  => sx, PortDir::Out => sx + w as i32 - 1 };
    Some((col, row))
}

pub fn draw_node(renderer: &mut Renderer, node: &Node, selected: bool, view_ox: i32, view_oy: i32, screen_w: usize, screen_h: usize) {
    let (w, h) = node_size(&node.kind);
    let sx = node.x + view_ox;
    let sy = node.y + view_oy;
    if sx + w as i32 <= 0 || sy + h as i32 <= 0 || sx >= screen_w as i32 || sy >= screen_h as i32 { return; }
    let sx = sx.max(0) as usize;
    let sy = sy.max(0) as usize;

    let (title_fg, title_bg) = if selected { (Color::Black, Color::Cyan) } else { (Color::White, Color::DarkBlue) };
    let title = node.kind.title();
    let title_str: String = format!(" {:<width$}", title, width = w.saturating_sub(2)).chars().take(w).collect();
    renderer.draw_str(sx, sy, &title_str, title_fg, title_bg);

    let body_rows = h.saturating_sub(2);
    for r in 0..body_rows {
        let fill: String = std::iter::repeat(' ').take(w).collect();
        renderer.draw_str(sx, sy + 1 + r, &fill, Color::White, Color::DarkGrey);
    }
    let bot_line: String = std::iter::once('+').chain(std::iter::repeat('-').take(w.saturating_sub(2))).chain(std::iter::once('+')).collect();
    if sy + h - 1 < screen_h { renderer.draw_str(sx, sy + h - 1, &bot_line, Color::DarkGrey, Color::DarkGrey); }

    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    for (dir_idx, (_, spec)) in ins.iter().enumerate() {
        let row = sy + 1 + dir_idx;
        if row >= screen_h { break; }
        let (glyph, glyph_fg) = if spec.kind == PortKind::Exec { ('>', Color::White) } else { ('*', Color::Yellow) };
        renderer.draw_char(sx, row, glyph, glyph_fg, Color::DarkGrey);
        let lbl: String = format!(" {}", spec.label).chars().take(w.saturating_sub(1)).collect();
        renderer.draw_str(sx + 1, row, &lbl, Color::White, Color::DarkGrey);
    }
    for (dir_idx, (_, spec)) in outs.iter().enumerate() {
        let row = sy + 1 + dir_idx;
        if row >= screen_h { break; }
        let (glyph, glyph_fg) = if spec.kind == PortKind::Exec { ('>', Color::White) } else { ('o', Color::Cyan) };
        let lbl = spec.label;
        let lbl_len = lbl.len();
        let glyph_col = sx + w - 1;
        let lbl_col = glyph_col.saturating_sub(lbl_len + 1);
        if glyph_col < screen_w { renderer.draw_char(glyph_col, row, glyph, glyph_fg, Color::DarkGrey); }
        if lbl_col < screen_w && lbl_col > sx { renderer.draw_str(lbl_col, row, lbl, Color::DarkGrey, Color::DarkGrey); }
    }
}

pub fn draw_wire(renderer: &mut Renderer, ox: i32, oy: i32, ix: i32, iy: i32, edge_idx: usize, color: Color, screen_w: usize, screen_h: usize) {
    let ch_col = ox + 2 + (edge_idx as i32 % 4);
    let x0 = ox + 1; let x1 = ch_col;
    if oy >= 0 && (oy as usize) < screen_h {
        for x in x0.min(x1)..=x0.max(x1) { if x >= 0 && (x as usize) < screen_w { renderer.draw_char(x as usize, oy as usize, '-', color, Color::Black); } }
    }
    let y0 = oy.min(iy); let y1 = oy.max(iy);
    for y in y0..=y1 { if y >= 0 && (y as usize) < screen_h && ch_col >= 0 && (ch_col as usize) < screen_w {
        let ch = if y == y0 || y == y1 { '+' } else { '|' };
        renderer.draw_char(ch_col as usize, y as usize, ch, color, Color::Black);
    } }
    let x0 = ch_col; let x1 = ix - 1;
    if iy >= 0 && (iy as usize) < screen_h {
        for x in x0.min(x1)..=x0.max(x1) { if x >= 0 && (x as usize) < screen_w { renderer.draw_char(x as usize, iy as usize, '-', color, Color::Black); } }
    }
}

pub fn draw_graph(renderer: &mut Renderer, graph: &NodeGraph, selected_node: Option<NodeId>, connecting: Option<(NodeId, usize)>, mouse_col: usize, mouse_row: usize, view_ox: i32, view_oy: i32, screen_w: usize, screen_h: usize) {
    for y in 0..screen_h { let row: String = std::iter::repeat(' ').take(screen_w).collect(); renderer.draw_str(0, y, &row, Color::DarkGrey, Color::Black); }
    let mut port_positions: Vec<(NodeId, Vec<(i32,i32)>, Vec<(i32,i32)>)> = Vec::new();
    for node in &graph.nodes {
        let ports = ports_for(&node.kind);
        let (ins, outs) = split_ports(&ports);
        let in_pos: Vec<(i32,i32)> = ins.iter().enumerate().map(|(di, (_, p))| port_screen_pos(node, p, di, view_ox, view_oy).unwrap_or((-1,-1))).collect();
        let out_pos: Vec<(i32,i32)> = outs.iter().enumerate().map(|(di, (_, p))| port_screen_pos(node, p, di, view_ox, view_oy).unwrap_or((-1,-1))).collect();
        port_positions.push((node.id, in_pos, out_pos));
    }
    for (ei, edge) in graph.edges.iter().enumerate() {
        let src = port_positions.iter().find(|(id, _, _)| *id == edge.from_node);
        let dst = port_positions.iter().find(|(id, _, _)| *id == edge.to_node);
        if let (Some((_, _, out_pos)), Some((_, in_pos, _))) = (src, dst) {
            if let (Some(&(ox,oy)), Some(&(ix,iy))) = (out_pos.get(edge.from_port), in_pos.get(edge.to_port)) {
                let sel = selected_node.map_or(false, |s| s == edge.from_node || s == edge.to_node);
                let color = if sel { Color::Cyan } else { Color::DarkGrey };
                draw_wire(renderer, ox, oy, ix, iy, ei, color, screen_w, screen_h);
            }
        }
    }
    if let Some((from_id, from_port_di)) = connecting {
        if let Some((_, _, out_pos)) = port_positions.iter().find(|(id, _, _)| *id == from_id) {
            if let Some(&(ox, oy)) = out_pos.get(from_port_di) { draw_wire(renderer, ox, oy, mouse_col as i32, mouse_row as i32, 0, Color::Yellow, screen_w, screen_h); }
        }
    }
    for node in &graph.nodes { let sel = selected_node == Some(node.id); draw_node(renderer, node, sel, view_ox, view_oy, screen_w, screen_h); }
}

pub fn draw_palette(renderer: &mut Renderer, scroll: usize, cursor: usize, px: usize, py: usize, _screen_w: usize, screen_h: usize) {
    let entries = palette_entries();
    let visible_h = (screen_h.saturating_sub(py + 1)).min(18);
    let w = 22usize;
    let hdr = format!("{:─<width$}", "─ Add Node ", width = w);
    renderer.draw_str(px, py, &hdr, Color::White, Color::DarkBlue);
    for (i, entry) in entries.iter().skip(scroll).take(visible_h).enumerate() {
        let row = py + 1 + i;
        if row >= screen_h { break; }
        let real_idx = scroll + i;
        let (fg, bg) = if real_idx == cursor { (Color::Black, Color::Cyan) } else if entry.0.is_empty() { (Color::DarkGrey, Color::Black) } else { (Color::White, Color::Black) };
        let label = if entry.0.is_empty() { format!(" {:─<width$}", entry.1, width = w.saturating_sub(2)) } else { format!("  {:<width$}", entry.1, width = w.saturating_sub(2)) };
        let line: String = label.chars().take(w).collect();
        renderer.draw_str(px, row, &line, fg, bg);
    }
    let end_row = py + 1 + visible_h;
    if end_row < screen_h && scroll + visible_h < entries.len() { renderer.draw_str(px, end_row, "  ▼ more", Color::DarkGrey, Color::Black); }
}

pub fn palette_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("", "Events"), ("OnStart", "On Start"), ("OnUpdate", "On Update"), ("OnKeyHeld", "Key Held"), ("OnKeyPress", "Key Press"), ("OnCollide", "On Collide"),
        ("", "Flow"), ("Branch", "Branch"), ("Sequence", "Sequence"),
        ("", "Actions"), ("SetVelocity", "Set Velocity"), ("SetPosition", "Set Position"), ("Despawn", "Despawn"), ("Spawn", "Spawn"), ("LoadLevel", "Load Level"), ("PlaySound", "Play Sound"), ("Log", "Log"), ("SetGlyph", "Set Glyph"), ("DrawHUD", "Draw HUD"),
        ("", "Values"), ("FloatLit", "Float Literal"), ("StringLit", "String Literal"), ("CompareFloat","Compare Float"), ("MathOp", "Math Op"), ("GetPosition", "Get Position"), ("GetVelocity", "Get Velocity"), ("GetTag", "Get Tag"), ("GetDelta", "Get Delta"),
        ("", "Variables"), ("GetVar", "Get Variable"), ("SetVar", "Set Variable"),
    ]
}

pub fn palette_make(key: &str) -> Option<NodeKind> {
    Some(match key {
        "OnStart" => NodeKind::OnStart, "OnUpdate" => NodeKind::OnUpdate, "OnKeyHeld" => NodeKind::OnKeyHeld { key: "space".into() }, "OnKeyPress" => NodeKind::OnKeyPress { key: "space".into() }, "OnCollide" => NodeKind::OnCollide { tag_filter: "player".into() },
        "Branch" => NodeKind::Branch, "Sequence" => NodeKind::Sequence { outputs: 3 },
        "SetVelocity" => NodeKind::SetVelocity, "SetPosition" => NodeKind::SetPosition, "Despawn" => NodeKind::Despawn, "Spawn" => NodeKind::Spawn, "LoadLevel" => NodeKind::LoadLevel { path: String::new() }, "PlaySound" => NodeKind::PlaySound { path: String::new() }, "Log" => NodeKind::Log, "SetGlyph" => NodeKind::SetGlyph, "DrawHUD" => NodeKind::DrawHUD,
        "FloatLit" => NodeKind::FloatLit { value: 0.0 }, "StringLit" => NodeKind::StringLit { value: String::new() }, "CompareFloat" => NodeKind::CompareFloat { op: CmpOp::Gt }, "MathOp" => NodeKind::MathOp { op: MathOp::Add },
        "GetPosition" => NodeKind::GetPosition, "GetVelocity" => NodeKind::GetVelocity, "GetTag" => NodeKind::GetTag, "GetDelta" => NodeKind::GetDelta,
        "GetVar" => NodeKind::GetVar { name: "x".into() }, "SetVar" => NodeKind::SetVar { name: "x".into() },
        _ => return None,
    })
}

pub fn node_at(graph: &NodeGraph, col: i32, row: i32, view_ox: i32, view_oy: i32) -> Option<NodeId> {
    for node in graph.nodes.iter().rev() {
        let (w, h) = node_size(&node.kind);
        let sx = node.x + view_ox; let sy = node.y + view_oy;
        if col >= sx && col < sx + w as i32 && row >= sy && row < sy + h as i32 { return Some(node.id); }
    }
    None
}

pub fn port_at(graph: &NodeGraph, col: i32, row: i32, view_ox: i32, view_oy: i32) -> Option<(NodeId, usize, PortDir, PortKind)> {
    for node in &graph.nodes {
        let (w, _h) = node_size(&node.kind);
        let sx = node.x + view_ox; let sy = node.y + view_oy;
        let ports = ports_for(&node.kind);
        let (ins, outs) = split_ports(&ports);
        if col == sx { for (di, (_, spec)) in ins.iter().enumerate() { let pr = sy + 1 + di as i32; if row == pr { return Some((node.id, di, PortDir::In, spec.kind)); } } }
        if col == sx + w as i32 - 1 { for (di, (_, spec)) in outs.iter().enumerate() { let pr = sy + 1 + di as i32; if row == pr { return Some((node.id, di, PortDir::Out, spec.kind)); } } }
    }
    None
}
