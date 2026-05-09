// editor/input/graph.rs — Node graph editor input handling.

use crate::input::Key;
use super::super::EditorState;
use super::super::{key_to_char, TEXT_INPUT_KEYS, apply_param_edit, param_default_for};
use super::super::node_graph::{self, PortDir};

impl EditorState {
    pub(super) fn update_graph_mode(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState) {
        use node_graph::{palette_make, palette_entries, node_at, port_at};

        let (gx, gy) = match self.graph_mode { Some(p) => p, None => return };

        let click = mouse.left_just_pressed();
        let rclick = mouse.right_just_pressed();
        let released = mouse.left_just_released();
        let col = mouse.cell_x as i32;
        let row = mouse.cell_y as i32;

        // ── Inline param editing (text input) ─────────────────────────────────
        if let Some((nid, ref mut buf)) = self.graph_editing_param {
            for &key in TEXT_INPUT_KEYS {
                if input.just_pressed(key) {
                    let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
                    if let Some(ch) = key_to_char(key, shift) { buf.push(ch); }
                }
            }
            if input.just_pressed(Key::Backspace) { buf.pop(); }
            if input.just_pressed(Key::Enter) {
                let buf_clone = buf.clone();
                let nid_copy = nid;
                self.graph_editing_param = None;
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    if let Some(graph) = &mut new_tile.graph {
                        if let Some(node) = graph.get_mut(nid_copy) {
                            apply_param_edit(&mut node.kind, &buf_clone);
                        }
                    }
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
            if input.just_pressed(Key::Escape) { self.graph_editing_param = None; }
            return;
        }

        // ── Palette open ──────────────────────────────────────────────────────
        if let Some((px, py)) = self.graph_palette_open {
            let entries = palette_entries();
            let visible_h = (self.layout.screen_h.saturating_sub(py + 1)).min(18);
            let selectable: Vec<usize> = entries.iter().enumerate()
                .filter(|(_, e)| !e.0.is_empty())
                .map(|(i, _)| i)
                .collect();

            if input.just_pressed(Key::Escape) || rclick { self.graph_palette_open = None; return; }

            // Scroll wheel for palette
            if mouse.wheel_y != 0.0 {
                let delta = -(mouse.wheel_y as i32);
                self.graph_palette_scroll = (self.graph_palette_scroll as i32 + delta)
                    .clamp(0, (entries.len() as i32 - visible_h as i32).max(0)) as usize;
            }

            let mut clicked_entry = false;
            let mut clicked_outside = false;
            if mouse.in_bounds {
                if mouse.cell_x >= px && mouse.cell_x < px + 22 && mouse.cell_y > py {
                    let idx = self.graph_palette_scroll + (mouse.cell_y - py - 1);
                    if idx < entries.len() && !entries[idx].0.is_empty() {
                        self.graph_palette_cursor = idx;
                        if click {
                            clicked_entry = true;
                        }
                    }
                } else if click {
                    clicked_outside = true;
                }
            }
            if clicked_outside {
                self.graph_palette_open = None;
                return;
            }

            if input.just_pressed(Key::Up) {
                let cur = self.graph_palette_cursor;
                if let Some(pos) = selectable.iter().position(|&i| i == cur) {
                    if pos > 0 { self.graph_palette_cursor = selectable[pos - 1]; }
                }
            }
            if input.just_pressed(Key::Down) {
                let cur = self.graph_palette_cursor;
                if let Some(pos) = selectable.iter().position(|&i| i == cur) {
                    if pos + 1 < selectable.len() { self.graph_palette_cursor = selectable[pos + 1]; }
                } else if !selectable.is_empty() {
                    self.graph_palette_cursor = selectable[0];
                }
            }

            // Ensure cursor is in view
            if self.graph_palette_cursor < self.graph_palette_scroll {
                self.graph_palette_scroll = self.graph_palette_cursor;
            } else if self.graph_palette_cursor >= self.graph_palette_scroll + visible_h {
                self.graph_palette_scroll = self.graph_palette_cursor - visible_h + 1;
            }

            if input.just_pressed(Key::Enter) || clicked_entry {
                let key = entries.get(self.graph_palette_cursor).map(|e| e.0).unwrap_or("");
                if let Some(kind) = palette_make(key) {
                    let graph_x = (px as i32) - self.graph_view_ox;
                    let graph_y = (py as i32) - self.graph_view_oy;
                    if let Some(tile) = self.grid.get(gx, gy).cloned() {
                        let mut new_tile = tile.clone();
                        if let Some(graph) = &mut new_tile.graph {
                            graph.add_node(kind, graph_x, graph_y);
                        }
                        self.grid.place(gx, gy, new_tile);
                        self.unsaved = true;
                    }
                }
                self.graph_palette_open = None;
            }
            return;
        }

        // ── Escape / global keys ──────────────────────────────────────────────
        if input.just_pressed(Key::Escape) {
            if self.graph_connecting.is_some() { self.graph_connecting = None; return; }
            self.graph_mode = None;
            return;
        }

        // Pan with arrow keys
        let pan = 2i32;
        if input.just_pressed(Key::Left)  { self.graph_view_ox += pan; }
        if input.just_pressed(Key::Right) { self.graph_view_ox -= pan; }
        if input.just_pressed(Key::Up)    { self.graph_view_oy += pan; }
        if input.just_pressed(Key::Down)  { self.graph_view_oy -= pan; }

        // Auto-layout
        if input.just_pressed(Key::F) {
            if let Some(tile) = self.grid.get(gx, gy).cloned() {
                let mut new_tile = tile.clone();
                if let Some(graph) = &mut new_tile.graph { graph.auto_layout(); }
                self.grid.place(gx, gy, new_tile);
                self.unsaved = true;
            }
        }

        // Delete selected node
        if input.just_pressed(Key::Delete) || input.just_pressed(Key::Backspace) {
            if let Some(sel) = self.graph_selected_node {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    if let Some(graph) = &mut new_tile.graph { graph.remove_node(sel); }
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                    self.graph_selected_node = None;
                }
            }
        }

        // Scroll wheel pan
        if mouse.wheel_y != 0.0 { self.graph_view_oy += mouse.wheel_y as i32; }

        // ── Mouse drag in progress ────────────────────────────────────────────
        if mouse.left_held() {
            if let Some((nid, ox, oy)) = self.graph_dragging_node {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    if let Some(graph) = &mut new_tile.graph {
                        if let Some(node) = graph.get_mut(nid) {
                            node.x = col - self.graph_view_ox - ox;
                            node.y = row - self.graph_view_oy - oy;
                        }
                    }
                    self.grid.place(gx, gy, new_tile);
                }
            }
        }
        if released {
            if self.graph_dragging_node.is_some() {
                self.graph_dragging_node = None;
                self.unsaved = true;
            }
        }

        // ── Right click → open palette ────────────────────────────────────────
        if rclick {
            if mouse.cell_y > 1 {
                self.graph_palette_open = Some((mouse.cell_x, mouse.cell_y));
                self.graph_palette_cursor = 1; // first selectable (OnStart)
            }
            return;
        }

        // ── Left click ────────────────────────────────────────────────────────
        if click && mouse.cell_y > 1 {
            let graph_snap = self.grid.get(gx, gy).and_then(|t| t.graph.as_ref()).is_some();
            if !graph_snap { return; }

            // Check port first
            let hit_port = self.grid.get(gx, gy).and_then(|t| t.graph.as_ref())
                .and_then(|g| port_at(g, col, row, self.graph_view_ox, self.graph_view_oy));

            if let Some((nid, dir_idx, dir, kind)) = hit_port {
                if dir == PortDir::Out {
                    self.graph_connecting = Some((nid, dir_idx));
                } else if dir == PortDir::In {
                    if let Some((from_id, from_di)) = self.graph_connecting.take() {
                        if let Some(tile) = self.grid.get(gx, gy).cloned() {
                            let mut new_tile = tile.clone();
                            if let Some(graph) = &mut new_tile.graph {
                                graph.add_edge(from_id, from_di, nid, dir_idx);
                            }
                            self.grid.place(gx, gy, new_tile);
                            self.unsaved = true;
                        }
                    }
                }
                let _ = kind;
                self.graph_selected_node = Some(nid);
                return;
            }

            // Check node
            let hit_node = self.grid.get(gx, gy).and_then(|t| t.graph.as_ref())
                .and_then(|g| node_at(g, col, row, self.graph_view_ox, self.graph_view_oy));

            if let Some(nid) = hit_node {
                self.graph_selected_node = Some(nid);
                self.graph_connecting = None;
                if let Some(tile) = self.grid.get(gx, gy) {
                    if let Some(graph) = &tile.graph {
                        if let Some(node) = graph.get(nid) {
                            let nx = node.x + self.graph_view_ox;
                            let ny = node.y + self.graph_view_oy;
                            let row_in_node = row - ny;
                            if row_in_node == 0 {
                                self.graph_dragging_node = Some((nid, col - nx, row - ny));
                            } else {
                                self.graph_editing_param = Some((
                                    nid,
                                    param_default_for(&node.kind),
                                ));
                            }
                        }
                    }
                }
            } else {
                self.graph_selected_node = None;
                self.graph_connecting    = None;
            }
        }
    }
}
