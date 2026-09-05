// editor/node_graph/codegen.rs — Rhai code generation from node graph.

use std::collections::HashMap;
use super::*;

pub fn generate_graph(graph: &NodeGraph) -> String {
    if graph.nodes.is_empty() { return String::new(); }

    let mut on_start   = String::new();
    let mut on_update  = String::new();
    let mut on_collide = String::new();

    let mut tmp_counter = 0usize;
    let mut spawn_vars: HashMap<NodeId, String> = HashMap::new();

    for node in &graph.nodes {
        if !node.kind.is_event() { continue; }
        let ports = ports_for(&node.kind);
        let (_, outs) = split_ports(&ports);
        let exec_body = if let Some((pidx, _)) = outs.iter().find(|(_, p)| p.kind == PortKind::Exec) {
            gen_exec_chain(graph, node.id, *pidx, 1, &mut tmp_counter, &mut spawn_vars)
        } else {
            String::new()
        };

        match &node.kind {
            NodeKind::OnStart => { on_start.push_str(&exec_body); }
            NodeKind::OnUpdate => { on_update.push_str(&exec_body); }
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
    spawn_vars: &mut HashMap<NodeId, String>,
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
    spawn_vars: &mut HashMap<NodeId, String>,
) -> String {
    let ind = indent(depth);
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    let data_ins: Vec<_> = ins.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();

    macro_rules! resolve {
        ($idx:expr) => {{
            let abs_port = data_ins.get($idx).map(|(i, _)| *i).unwrap_or(0);
            resolve_data(graph, node.id, abs_port, spawn_vars)
        }};
    }

    let exec_outs: Vec<usize> = outs.iter()
        .filter(|(_, p)| p.kind == PortKind::Exec)
        .map(|(i, _)| *i)
        .collect();

    let mut out = String::new();

    match &node.kind {
        NodeKind::SetVelocity => {
            out += &format!("{}ctx.set_velocity(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetPosition => {
            out += &format!("{}ctx.set_position(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::Despawn => { out += &format!("{}ctx.despawn(id);\n", ind); }
        NodeKind::Spawn => {
            let var = format!("__tmp{}", tmp);
            *tmp += 1;
            spawn_vars.insert(node.id, var.clone());
            out += &format!("{}let {} = ctx.spawn({}, {}, {}, {});\n",
                ind, var, resolve!(0), resolve!(2), resolve!(3), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::LoadLevel { path } => { out += &format!("{}ctx.load_level(\"{}\");\n", ind, path); }
        NodeKind::PlaySound { path } => {
            out += &format!("{}ctx.play_sound(\"{}\");\n", ind, path);
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::Log => {
            out += &format!("{}ctx.log({});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetGlyph => {
            out += &format!("{}ctx.set_glyph(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::DrawHUD => {
            out += &format!("{}ctx.draw_hud({}, {}, {}, \"White\", \"Reset\");\n",
                ind, resolve!(0), resolve!(1), resolve!(2));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::Branch => {
            let cond = resolve!(0);
            let true_body  = exec_outs.get(0).map(|&p| gen_exec_chain(graph, node.id, p, depth + 1, tmp, spawn_vars)).unwrap_or_default();
            let false_body = exec_outs.get(1).map(|&p| gen_exec_chain(graph, node.id, p, depth + 1, tmp, spawn_vars)).unwrap_or_default();
            out += &format!("{}if {} {{\n{}{}}} else {{\n{}{}}}\n", ind, cond, true_body, ind, false_body, ind);
        }
        NodeKind::Sequence { .. } => {
            for &p in &exec_outs { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetVar { name } => {
            out += &format!("{}let __var_{} = {};\n", ind, name, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetGlobal { name } => {
            out += &format!("{}ctx.set_global(\"{}\", {});\n", ind, name, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetPersistent { name } => {
            out += &format!("{}ctx.set_persistent(\"{}\", {});\n", ind, name, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetCamera => {
            out += &format!("{}ctx.set_camera({}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::ShakeCamera => {
            out += &format!("{}ctx.shake_camera({}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::StartTimer { name } => {
            out += &format!("{}ctx.start_timer(\"{}\", {});\n", ind, name, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetVisible => {
            out += &format!("{}ctx.set_visible(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetZOrder => {
            out += &format!("{}ctx.set_layer_order(id, {});\n", ind, resolve!(0));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::CancelTimer { name } => {
            out += &format!("{}ctx.cancel_timer(\"{}\");\n", ind, name);
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::PlayMusic { path } => {
            out += &format!("{}ctx.play_music(\"{}\");\n", ind, path);
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::StopMusic => {
            out += &format!("{}ctx.stop_music();\n", ind);
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetColor => {
            out += &format!("{}ctx.set_tint(id, {}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::DrawBox => {
            out += &format!("{}ctx.draw_box({}, {}, {}, {}, {}, {});\n", ind, resolve!(0), resolve!(1), resolve!(2), resolve!(3), resolve!(4), resolve!(5));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::FillRect => {
            out += &format!("{}ctx.fill_rect({}, {}, {}, {}, {}, {}, {});\n", ind, resolve!(0), resolve!(1), resolve!(2), resolve!(3), resolve!(4), resolve!(5), resolve!(6));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::ClearHUD => {
            out += &format!("{}ctx.clear_hud();\n", ind);
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        NodeKind::SetColliderLayer => {
            out += &format!("{}ctx.set_collider_layer({}, {});\n", ind, resolve!(0), resolve!(1));
            if let Some(&p) = exec_outs.first() { out += &gen_exec_chain(graph, node.id, p, depth, tmp, spawn_vars); }
        }
        _ => {}
    }
    out
}

fn resolve_data(
    graph: &NodeGraph,
    to_node: NodeId,
    to_port: usize,
    spawn_vars: &HashMap<NodeId, String>,
) -> String {
    if let Some(e) = graph.data_in_edge(to_node, to_port) {
        let src = match graph.get(e.from_node) { Some(n)=>n, None=>return "0.0".into() };
        return codegen_expr(graph, src, e.from_port, spawn_vars);
    }
    "0.0".into()
}

fn codegen_expr(
    graph: &NodeGraph,
    node: &Node,
    out_port: usize,
    spawn_vars: &HashMap<NodeId, String>,
) -> String {
    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    let data_ins: Vec<_> = ins.iter().filter(|(_, p)| p.kind == PortKind::Data).collect();

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
        NodeKind::CompareFloat { op } => format!("({} {} {})", resolve_in!(0), op.as_str(), resolve_in!(1)),
        NodeKind::MathOp { op } => format!("({} {} {})", resolve_in!(0), op.as_str(), resolve_in!(1)),
        NodeKind::GetVar { name } => format!("__var_{}", name),
        NodeKind::OnCollide { .. } => "other".into(),
        NodeKind::OnUpdate => "ctx.get_delta()".into(),
        NodeKind::Spawn => spawn_vars.get(&node.id).cloned().unwrap_or_else(|| "0".into()),

        NodeKind::GetGlobal { name } => format!("ctx.get_global(\"{}\")", name),
        NodeKind::GetPersistent { name } => format!("ctx.get_persistent(\"{}\")", name),
        NodeKind::GetMousePos => if out_idx == 0 { "ctx.get_mouse_world_x()".into() } else { "ctx.get_mouse_world_y()".into() },
        NodeKind::IsSolidAt => format!("ctx.is_solid_at({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::GetEntityAt => format!("ctx.get_entity_at({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::GetDistance => format!("ctx.get_distance({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::GetAngleTo => format!("ctx.get_angle_to({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::TimerDone { name } => format!("ctx.timer_done(\"{}\")", name),
        NodeKind::RandomInt => format!("ctx.random_int({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::RandomFloat => "ctx.random_float()".into(),
        NodeKind::RandomBool => format!("ctx.random_bool({})", resolve_in!(0)),
        NodeKind::RandomChoice => format!("ctx.random_choice({})", resolve_in!(0)),
        NodeKind::GetElapsed => "ctx.get_elapsed()".into(),
        NodeKind::EntityExists => format!("ctx.entity_exists({})", resolve_in!(0)),
        NodeKind::HasTag => format!("ctx.has_tag({}, {})", resolve_in!(0), resolve_in!(1)),
        NodeKind::GetColliderLayer => format!("ctx.get_collider_layer({})", resolve_in!(0)),
        NodeKind::FindEntitiesInRect => format!("ctx.find_entities_in_rect({}, {}, {}, {})", resolve_in!(0), resolve_in!(1), resolve_in!(2), resolve_in!(3)),
        NodeKind::CountByTag => format!("ctx.count_by_tag({})", resolve_in!(0)),
        NodeKind::FindByTag => format!("ctx.find_by_tag({})", resolve_in!(0)),
        NodeKind::FindAllByTag => format!("ctx.find_all_by_tag({})", resolve_in!(0)),
        NodeKind::MouseLeftPressed => "ctx.mouse_left_pressed()".into(),
        NodeKind::MouseLeftHeld => "ctx.mouse_left_held()".into(),
        _ => "0.0".into(),
    }
}
