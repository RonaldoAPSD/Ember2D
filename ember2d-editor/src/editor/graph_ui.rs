// editor/graph_ui.rs — Drawing and hit-testing for the node graph editor.
//
// Was `editor/node_graph/ui.rs` until Step 5a (docs/ember2d-phase5-plan.md)
// split the node graph in two: the data model + Rhai codegen moved to the
// crate-root `graph` module (see that module's header comment for why), and
// this half — which needs the renderer — stayed in `editor`.

use super::ui::{draw_row, UiFrame, WidgetId};
use ember2d::renderer::{color::Color, DrawSurface, Font};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::graph::*;

pub const NODE_MIN_W: usize = 20;

/// A node's title text drives its own box width — `Font`-routed (Phase 7
/// Part 2c, docs/ember2d-phase7-plan.md) so drawing (`draw_node`) and
/// hit-testing (`node_at`/`port_at`) always agree on `w`, same as before
/// this conversion: they were never at risk of independently drifting
/// (defect E5's class) since both call this one function rather than each
/// recomputing the width themselves — this just changes what the shared
/// computation is built out of.
pub fn node_size(font: &mut dyn Font, kind: &NodeKind) -> (usize, usize) {
    let ports = ports_for(kind);
    let (ins, outs) = split_ports(&ports);
    let rows = 1 + ins.len().max(outs.len());
    let rows = rows.max(2);
    let title_len = super::ui::cells(font, &kind.title()) + 4;
    let w = title_len.max(NODE_MIN_W);
    (w, rows + 1)
}

pub fn port_screen_pos(
    font: &mut dyn Font,
    node: &Node,
    port: &PortSpec,
    port_dir_idx: usize,
    view_ox: i32,
    view_oy: i32,
) -> Option<(i32, i32)> {
    let (w, _h) = node_size(font, &node.kind);
    let sx = node.x + view_ox;
    let sy = node.y + view_oy;
    let row = sy + 1 + port_dir_idx as i32;
    let col = match port.dir {
        PortDir::In => sx,
        PortDir::Out => sx + w as i32 - 1,
    };
    Some((col, row))
}

/// Nodes stay flat theme-colored fills rather than 9-slice (same call as
/// `chrome.rs`'s title/status bars, 7D-2, master plan §5.4) — a graph can
/// hold many nodes redrawn every frame, and unlike the editor's fixed
/// chrome, a node's size varies per-instance (`node_size` above), so a
/// 9-sliced border would need its own per-node draw-call budget for a
/// shape this simple. Port glyph/label colors (White=Exec, Yellow=data-in,
/// Cyan=data-out) stay literal — they signal PORT KIND, not chrome, same
/// "semantic color" reasoning `dock.rs` applies to log levels and entity
/// kinds.
#[allow(clippy::too_many_arguments)]
pub fn draw_node(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    node: &Node,
    selected: bool,
    view_ox: i32,
    view_oy: i32,
    screen_w: usize,
    screen_h: usize,
) {
    let (w, h) = node_size(font, &node.kind);
    let sx = node.x + view_ox;
    let sy = node.y + view_oy;
    if sx + w as i32 <= 0 || sy + h as i32 <= 0 || sx >= screen_w as i32 || sy >= screen_h as i32 {
        return;
    }
    let sx = sx.max(0) as usize;
    let sy = sy.max(0) as usize;

    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let border = theme.role_color(PaletteRole::PanelBorder);
    let accent = theme.role_color(PaletteRole::Accent);
    let dim = theme.role_color(PaletteRole::TextDim);

    // No themed "text-on-accent" role exists (same gap `chrome.rs`'s
    // `draw_dock_tabs` comments on), so a selected node stays plain
    // black-on-accent.
    let (title_fg, title_bg) = if selected {
        (Color::Black, accent)
    } else {
        (theme.role_color(PaletteRole::TitleText), theme.role_color(PaletteRole::TitleBg))
    };
    let title = node.kind.title();
    let title_str: String =
        format!(" {:<width$}", title, width = w.saturating_sub(2)).chars().take(w).collect();
    renderer.draw_str(sx, sy, &title_str, title_fg, title_bg);

    let body_rows = h.saturating_sub(2);
    for r in 0..body_rows {
        let fill: String = std::iter::repeat_n(' ', w).collect();
        renderer.draw_str(sx, sy + 1 + r, &fill, Color::White, panel_bg);
    }
    let bot_line: String = std::iter::once('+')
        .chain(std::iter::repeat_n('-', w.saturating_sub(2)))
        .chain(std::iter::once('+'))
        .collect();
    if sy + h - 1 < screen_h {
        renderer.draw_str(sx, sy + h - 1, &bot_line, border, border);
    }

    let ports = ports_for(&node.kind);
    let (ins, outs) = split_ports(&ports);
    for (dir_idx, (_, spec)) in ins.iter().enumerate() {
        let row = sy + 1 + dir_idx;
        if row >= screen_h {
            break;
        }
        let (glyph, glyph_fg) =
            if spec.kind == PortKind::Exec { ('>', Color::White) } else { ('*', Color::Yellow) };
        renderer.draw_char(sx, row, glyph, glyph_fg, panel_bg);
        let lbl: String = format!(" {}", spec.label).chars().take(w.saturating_sub(1)).collect();
        renderer.draw_str(sx + 1, row, &lbl, Color::White, panel_bg);
    }
    for (dir_idx, (_, spec)) in outs.iter().enumerate() {
        let row = sy + 1 + dir_idx;
        if row >= screen_h {
            break;
        }
        let (glyph, glyph_fg) =
            if spec.kind == PortKind::Exec { ('>', Color::White) } else { ('o', Color::Cyan) };
        let lbl = spec.label;
        let lbl_len = super::ui::cells(font, lbl);
        let glyph_col = sx + w - 1;
        let lbl_col = glyph_col.saturating_sub(lbl_len + 1);
        if glyph_col < screen_w {
            renderer.draw_char(glyph_col, row, glyph, glyph_fg, panel_bg);
        }
        if lbl_col < screen_w && lbl_col > sx {
            renderer.draw_str(lbl_col, row, lbl, dim, panel_bg);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_wire(
    renderer: &mut dyn DrawSurface,
    ox: i32,
    oy: i32,
    ix: i32,
    iy: i32,
    edge_idx: usize,
    color: Color,
    bg: Color,
    screen_w: usize,
    screen_h: usize,
) {
    // Determine routing column (midpoint + small offset per wire to avoid overlapping)
    let mid_x = (ox + ix) / 2;
    let offset = (edge_idx as i32 % 5) - 2;
    let lo = (ox + 1).min(ix - 1);
    let hi = (ox + 1).max(ix - 1);
    let routing_x = (mid_x + offset).clamp(lo, hi);

    // 1. Horizontal from start to routing_x
    if oy >= 0 && (oy as usize) < screen_h {
        let x0 = ox + 1;
        let x1 = routing_x;
        for x in x0.min(x1)..=x0.max(x1) {
            if x >= 0 && (x as usize) < screen_w {
                renderer.draw_char(x as usize, oy as usize, '-', color, bg);
            }
        }
    }

    // 2. Vertical at routing_x
    if routing_x >= 0 && (routing_x as usize) < screen_w {
        let y0 = oy.min(iy);
        let y1 = oy.max(iy);
        for y in y0..=y1 {
            if y >= 0 && (y as usize) < screen_h {
                let ch = if y == oy || y == iy { '+' } else { '|' };
                renderer.draw_char(routing_x as usize, y as usize, ch, color, bg);
            }
        }
    }

    // 3. Horizontal from routing_x to end
    if iy >= 0 && (iy as usize) < screen_h {
        let x0 = routing_x;
        let x1 = ix - 1;
        for x in x0.min(x1)..=x0.max(x1) {
            if x >= 0 && (x as usize) < screen_w {
                renderer.draw_char(x as usize, iy as usize, '-', color, bg);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_graph(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    graph: &NodeGraph,
    selected_node: Option<NodeId>,
    connecting: Option<(NodeId, usize)>,
    mouse_col: usize,
    mouse_row: usize,
    view_ox: i32,
    view_oy: i32,
    screen_w: usize,
    screen_h: usize,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    for y in 0..screen_h {
        let row: String = std::iter::repeat_n(' ', screen_w).collect();
        renderer.draw_str(0, y, &row, dim, panel_bg);
    }
    let mut port_positions: Vec<(NodeId, Vec<(i32, i32)>, Vec<(i32, i32)>)> = Vec::new();
    for node in &graph.nodes {
        let ports = ports_for(&node.kind);
        let (ins, outs) = split_ports(&ports);
        let in_pos: Vec<(i32, i32)> = ins
            .iter()
            .enumerate()
            .map(|(di, (_, p))| {
                port_screen_pos(font, node, p, di, view_ox, view_oy).unwrap_or((-1, -1))
            })
            .collect();
        let out_pos: Vec<(i32, i32)> = outs
            .iter()
            .enumerate()
            .map(|(di, (_, p))| {
                port_screen_pos(font, node, p, di, view_ox, view_oy).unwrap_or((-1, -1))
            })
            .collect();
        port_positions.push((node.id, in_pos, out_pos));
    }
    for (ei, edge) in graph.edges.iter().enumerate() {
        let src = port_positions.iter().find(|(id, _, _)| *id == edge.from_node);
        let dst = port_positions.iter().find(|(id, _, _)| *id == edge.to_node);
        if let (Some((_, _, out_pos)), Some((_, in_pos, _))) = (src, dst) {
            if let (Some(&(ox, oy)), Some(&(ix, iy))) =
                (out_pos.get(edge.from_port), in_pos.get(edge.to_port))
            {
                let sel = selected_node.is_some_and(|s| s == edge.from_node || s == edge.to_node);
                let color = if sel { accent } else { dim };
                draw_wire(renderer, ox, oy, ix, iy, ei, color, panel_bg, screen_w, screen_h);
            }
        }
    }
    // The in-progress wire while dragging a new connection also draws in
    // `accent` (was a distinct literal Yellow before theming) — no third
    // themed role exists between "selected" and "idle" wire states, and
    // this one already stands out by following the mouse every frame.
    if let Some((from_id, from_port_di)) = connecting {
        if let Some((_, _, out_pos)) = port_positions.iter().find(|(id, _, _)| *id == from_id) {
            if let Some(&(ox, oy)) = out_pos.get(from_port_di) {
                // Snap to valid input port
                let mut target_x = mouse_col as i32;
                let mut target_y = mouse_row as i32;
                if let Some((nid, di, dir, _)) =
                    port_at(font, graph, target_x, target_y, view_ox, view_oy)
                {
                    if dir == PortDir::In {
                        if let Some((_, in_positions, _)) =
                            port_positions.iter().find(|(id, _, _)| *id == nid)
                        {
                            if let Some(&(sx, sy)) = in_positions.get(di) {
                                target_x = sx;
                                target_y = sy;
                            }
                        }
                    }
                }
                draw_wire(
                    renderer,
                    ox,
                    oy,
                    target_x,
                    target_y,
                    0,
                    accent,
                    panel_bg,
                    screen_w,
                    screen_h,
                );
            }
        }
    }
    for node in &graph.nodes {
        let sel = selected_node == Some(node.id);
        draw_node(renderer, font, theme, node, sel, view_ox, view_oy, screen_w, screen_h);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw_palette(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    scroll: usize,
    cursor: usize,
    px: usize,
    py: usize,
    _screen_w: usize,
    screen_h: usize,
    frame: &mut UiFrame,
) {
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    let title_fg = theme.role_color(PaletteRole::TitleText);
    let title_bg = theme.role_color(PaletteRole::TitleBg);

    let entries = palette_entries();
    let visible_h = (screen_h.saturating_sub(py + 1)).min(18);
    let w = 22usize;
    let hdr = format!("{:=<width$}", "= Add Node ", width = w);
    renderer.draw_str(px, py, &hdr, title_fg, title_bg);
    for (i, entry) in entries.iter().skip(scroll).take(visible_h).enumerate() {
        let row = py + 1 + i;
        if row >= screen_h {
            break;
        }
        let real_idx = scroll + i;
        // No themed "text-on-accent" role — same gap `chrome.rs`'s
        // `draw_dock_tabs` comments on.
        let (fg, bg) = if real_idx == cursor {
            (Color::Black, accent)
        } else if entry.0.is_empty() {
            (dim, panel_bg)
        } else {
            (text_fg, panel_bg)
        };
        let label = if entry.0.is_empty() {
            format!(" [{:-<width$}]", entry.1, width = w.saturating_sub(4))
        } else {
            format!("  {:<width$}", entry.1, width = w.saturating_sub(2))
        };
        let line: String = label.chars().take(w).collect();
        // 7C-1 (master plan §5.3): registers every row — including
        // non-selectable header rows — so `input/graph.rs` can tell "click
        // landed inside the palette but on a header" apart from "click
        // landed outside the palette entirely" the same way the removed
        // manual `mouse.cell_x >= px && ...` arithmetic did (E5).
        draw_row(renderer, frame, WidgetId::GraphPaletteRow(real_idx), px, row, w, &line, fg, bg);
    }
    let end_row = py + 1 + visible_h;
    if end_row < screen_h && scroll + visible_h < entries.len() {
        renderer.draw_str(px, end_row, "  V more", dim, panel_bg);
    }
}

pub fn palette_entries() -> Vec<(&'static str, &'static str)> {
    vec![
        ("", "Events"),
        ("OnStart", "On Start"),
        ("OnUpdate", "On Update"),
        ("OnKeyHeld", "Key Held"),
        ("OnKeyPress", "Key Press"),
        ("OnCollide", "On Collide"),
        ("", "Flow"),
        ("Branch", "Branch"),
        ("Sequence", "Sequence"),
        ("", "Actions"),
        ("SetVelocity", "Set Velocity"),
        ("SetPosition", "Set Position"),
        ("Despawn", "Despawn"),
        ("Spawn", "Spawn"),
        ("LoadLevel", "Load Level"),
        ("PlaySound", "Play Sound"),
        ("Log", "Log"),
        ("SetGlyph", "Set Glyph"),
        ("DrawHUD", "Draw HUD"),
        ("SetCamera", "Set Camera"),
        ("ShakeCamera", "Shake Camera"),
        ("StartTimer", "Start Timer"),
        ("CancelTimer", "Cancel Timer"),
        ("SetVisible", "Set Visible"),
        ("SetZOrder", "Set Z-Order"),
        ("PlayMusic", "Play Music"),
        ("StopMusic", "Stop Music"),
        ("SetColor", "Set Color"),
        ("DrawBox", "Draw Box"),
        ("FillRect", "Fill Rect"),
        ("ClearHUD", "Clear HUD"),
        ("SetColliderLayer", "Set Layer"),
        ("", "Values"),
        ("FloatLit", "Float Literal"),
        ("StringLit", "String Literal"),
        ("CompareFloat", "Compare Float"),
        ("MathOp", "Math Op"),
        ("GetPosition", "Get Position"),
        ("GetVelocity", "Get Velocity"),
        ("GetTag", "Get Tag"),
        ("GetDelta", "Get Delta"),
        ("GetMousePos", "Mouse Pos"),
        ("IsSolidAt", "Is Solid At"),
        ("GetEntityAt", "Get Entity At"),
        ("GetDistance", "Get Distance"),
        ("GetAngleTo", "Get Angle To"),
        ("TimerDone", "Timer Done"),
        ("RandomInt", "Random Int"),
        ("RandomFloat", "Random Float"),
        ("RandomBool", "Random Bool"),
        ("RandomChoice", "Random Choice"),
        ("GetElapsed", "Get Elapsed"),
        ("EntityExists", "Entity Exists"),
        ("HasTag", "Has Tag"),
        ("GetColliderLayer", "Get Layer"),
        ("FindEntitiesInRect", "In Rect"),
        ("CountByTag", "Count Tagged"),
        ("FindByTag", "Find Tagged"),
        ("FindAllByTag", "Find All Tag"),
        ("MouseLeftPressed", "Mouse L-Press"),
        ("MouseLeftHeld", "Mouse L-Held"),
        ("", "Variables"),
        ("GetVar", "Get Variable"),
        ("SetVar", "Set Variable"),
        ("GetGlobal", "Get Global"),
        ("SetGlobal", "Set Global"),
        ("GetPersistent", "Get Persist"),
        ("SetPersistent", "Set Persist"),
    ]
}

pub fn palette_make(key: &str) -> Option<NodeKind> {
    Some(match key {
        "OnStart" => NodeKind::OnStart,
        "OnUpdate" => NodeKind::OnUpdate,
        "OnKeyHeld" => NodeKind::OnKeyHeld { key: "space".into() },
        "OnKeyPress" => NodeKind::OnKeyPress { key: "space".into() },
        "OnCollide" => NodeKind::OnCollide { tag_filter: "player".into() },
        "Branch" => NodeKind::Branch,
        "Sequence" => NodeKind::Sequence { outputs: 3 },
        "SetVelocity" => NodeKind::SetVelocity,
        "SetPosition" => NodeKind::SetPosition,
        "Despawn" => NodeKind::Despawn,
        "Spawn" => NodeKind::Spawn,
        "LoadLevel" => NodeKind::LoadLevel { path: String::new() },
        "PlaySound" => NodeKind::PlaySound { path: String::new() },
        "Log" => NodeKind::Log,
        "SetGlyph" => NodeKind::SetGlyph,
        "DrawHUD" => NodeKind::DrawHUD,
        "FloatLit" => NodeKind::FloatLit { value: 0.0 },
        "StringLit" => NodeKind::StringLit { value: String::new() },
        "CompareFloat" => NodeKind::CompareFloat { op: CmpOp::Gt },
        "MathOp" => NodeKind::MathOp { op: MathOp::Add },
        "GetPosition" => NodeKind::GetPosition,
        "GetVelocity" => NodeKind::GetVelocity,
        "GetTag" => NodeKind::GetTag,
        "GetDelta" => NodeKind::GetDelta,
        "GetVar" => NodeKind::GetVar { name: "x".into() },
        "SetVar" => NodeKind::SetVar { name: "x".into() },

        "GetGlobal" => NodeKind::GetGlobal { name: "score".into() },
        "SetGlobal" => NodeKind::SetGlobal { name: "score".into() },
        "GetPersistent" => NodeKind::GetPersistent { name: "gold".into() },
        "SetPersistent" => NodeKind::SetPersistent { name: "gold".into() },
        "GetMousePos" => NodeKind::GetMousePos,
        "IsSolidAt" => NodeKind::IsSolidAt,
        "GetEntityAt" => NodeKind::GetEntityAt,
        "GetDistance" => NodeKind::GetDistance,
        "GetAngleTo" => NodeKind::GetAngleTo,
        "SetCamera" => NodeKind::SetCamera,
        "ShakeCamera" => NodeKind::ShakeCamera,
        "StartTimer" => NodeKind::StartTimer { name: "cd".into() },
        "TimerDone" => NodeKind::TimerDone { name: "cd".into() },
        "CancelTimer" => NodeKind::CancelTimer { name: "cd".into() },
        "SetVisible" => NodeKind::SetVisible,
        "SetZOrder" => NodeKind::SetZOrder,

        "PlayMusic" => NodeKind::PlayMusic { path: String::new() },
        "StopMusic" => NodeKind::StopMusic,
        "SetColor" => NodeKind::SetColor,
        "DrawBox" => NodeKind::DrawBox,
        "FillRect" => NodeKind::FillRect,
        "ClearHUD" => NodeKind::ClearHUD,
        "SetColliderLayer" => NodeKind::SetColliderLayer,
        "RandomInt" => NodeKind::RandomInt,
        "RandomFloat" => NodeKind::RandomFloat,
        "RandomBool" => NodeKind::RandomBool,
        "RandomChoice" => NodeKind::RandomChoice,
        "GetElapsed" => NodeKind::GetElapsed,
        "EntityExists" => NodeKind::EntityExists,
        "HasTag" => NodeKind::HasTag,
        "GetColliderLayer" => NodeKind::GetColliderLayer,
        "FindEntitiesInRect" => NodeKind::FindEntitiesInRect,
        "CountByTag" => NodeKind::CountByTag,
        "FindByTag" => NodeKind::FindByTag,
        "FindAllByTag" => NodeKind::FindAllByTag,
        "MouseLeftPressed" => NodeKind::MouseLeftPressed,
        "MouseLeftHeld" => NodeKind::MouseLeftHeld,
        _ => return None,
    })
}

pub fn node_at(
    font: &mut dyn Font,
    graph: &NodeGraph,
    col: i32,
    row: i32,
    view_ox: i32,
    view_oy: i32,
) -> Option<NodeId> {
    for node in graph.nodes.iter().rev() {
        let (w, h) = node_size(font, &node.kind);
        let sx = node.x + view_ox;
        let sy = node.y + view_oy;
        if col >= sx && col < sx + w as i32 && row >= sy && row < sy + h as i32 {
            return Some(node.id);
        }
    }
    None
}

pub fn port_at(
    font: &mut dyn Font,
    graph: &NodeGraph,
    col: i32,
    row: i32,
    view_ox: i32,
    view_oy: i32,
) -> Option<(NodeId, usize, PortDir, PortKind)> {
    for node in &graph.nodes {
        let (w, _h) = node_size(font, &node.kind);
        let sx = node.x + view_ox;
        let sy = node.y + view_oy;
        let ports = ports_for(&node.kind);
        let (ins, outs) = split_ports(&ports);
        if col == sx {
            for (di, (_, spec)) in ins.iter().enumerate() {
                let pr = sy + 1 + di as i32;
                if row == pr {
                    return Some((node.id, di, PortDir::In, spec.kind));
                }
            }
        }
        if col == sx + w as i32 - 1 {
            for (di, (_, spec)) in outs.iter().enumerate() {
                let pr = sy + 1 + di as i32;
                if row == pr {
                    return Some((node.id, di, PortDir::Out, spec.kind));
                }
            }
        }
    }
    None
}
