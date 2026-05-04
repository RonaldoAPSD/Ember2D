// editor/impl_input.rs — Input / update logic for EditorState.

use crate::engine::UpdateContext;
use crate::input::Key;

use super::EditorState;
use super::{TextInput, TextInputPurpose, TEXT_INPUT_KEYS, key_to_char, DEFAULT_LEVEL_W, DEFAULT_LEVEL_H};
use super::{apply_param_edit, param_default_for};
use super::commands::{Command, UndoStack};
use super::node_graph;
use super::panel::PanelId;
use super::ui::{self, HierarchySelection, ToolKind, ToolbarAction,
                INSP_GLYPH_OFF, INSP_TAG_OFF, INSP_SOLID_OFF, INSP_TRIG_OFF, INSP_CAM_OFF,
                INSP_SCRIPT_OFF, INSP_EXIT_OFF, INSP_GRAPH_BTN};

// ── Graph editor input ────────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn update_graph_mode(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState) {
        use node_graph::{palette_make, palette_entries, node_at, port_at, PortDir};

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
            let selectable: Vec<usize> = entries.iter().enumerate()
                .filter(|(_, e)| !e.0.is_empty())
                .map(|(i, _)| i)
                .collect();

            if input.just_pressed(Key::Escape) || rclick { self.graph_palette_open = None; return; }

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
            if input.just_pressed(Key::Enter) || click {
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
                                    param_default_for(&tile.graph.as_ref().unwrap()
                                        .get(nid).unwrap().kind),
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

// ── Main update ───────────────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn handle_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { input, mouse, .. } = ctx;

        // Graph editor mode swallows all input.
        if self.graph_mode.is_some() {
            self.update_graph_mode(input, mouse);
            return;
        }

        // Tick save message.
        if self.save_message.is_some() {
            self.save_message_timer += 1;
            if self.save_message_timer > 90 {
                self.save_message       = None;
                self.save_message_timer = 0;
            }
        }

        // F-key panel toggles (work in any mode).
        if input.just_pressed(Key::F1) { self.panels.toggle(PanelId::Console); }
        if input.just_pressed(Key::F2) { self.panels.toggle(PanelId::Inspector); }
        if input.just_pressed(Key::F3) { self.console_log.clear(); }

        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let alt   = input.is_held(Key::LeftAlt)   || input.is_held(Key::RightAlt);

        // ── Text input ────────────────────────────────────────────────────────
        if let Some(ref mut ti) = self.text_input {
            for &key in TEXT_INPUT_KEYS {
                if input.just_pressed(key) {
                    if let Some(ch) = key_to_char(key, shift) { ti.buffer.push(ch); }
                }
            }
            if input.just_pressed(Key::Backspace) { ti.buffer.pop(); }

            if input.just_pressed(Key::Enter) {
                let ti = self.text_input.take().unwrap();
                match ti.purpose {
                    TextInputPurpose::LevelName => {
                        if !ti.buffer.is_empty() { self.grid.name = ti.buffer; self.unsaved = true; }
                    }
                    TextInputPurpose::SaveAs => {
                        if !ti.buffer.is_empty() { self.save_path = ti.buffer; self.save(); }
                    }
                    TextInputPurpose::ScriptPath { gx, gy } => {
                        if let Some(tile) = self.grid.get(gx, gy).cloned() {
                            let mut new_tile = tile.clone();
                            new_tile.script = if ti.buffer.is_empty() { None } else { Some(ti.buffer) };
                            self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                            self.grid.place(gx, gy, new_tile);
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::TileNextLevel { gx, gy } => {
                        if let Some(tile) = self.grid.get(gx, gy).cloned() {
                            let mut new_tile = tile.clone();
                            new_tile.next_level = if ti.buffer.is_empty() { None } else { Some(ti.buffer) };
                            self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                            self.grid.place(gx, gy, new_tile);
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::TileTag { gx, gy } => {
                        if let Some(tile) = self.grid.get(gx, gy).cloned() {
                            let mut new_tile = tile.clone();
                            new_tile.tag = ti.buffer;
                            self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                            self.grid.place(gx, gy, new_tile);
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::NamedSpawn => {
                        if !ti.buffer.is_empty() {
                            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                                self.grid.extra_spawns.push((ti.buffer, gx as f32, gy as f32));
                                self.unsaved = true;
                            }
                        }
                    }
                    TextInputPurpose::ResizeLevel => {
                        let s = ti.buffer.replace('x', " ").replace('X', " ").replace(',', " ");
                        let parts: Vec<&str> = s.split_whitespace().collect();
                        let parsed = if parts.len() == 2 {
                            parts[0].parse::<usize>().ok().zip(parts[1].parse::<usize>().ok())
                        } else {
                            None
                        };
                        match parsed {
                            Some((w, h)) if w >= 4 && h >= 3 => {
                                self.grid.resize(w, h);
                                self.clamp_scroll();
                                self.unsaved = true;
                                self.save_message = Some(format!("Resized to {}×{}", w, h));
                                self.save_message_timer = 0;
                            }
                            Some((w, h)) => {
                                self.save_message = Some(format!("Too small: {}×{} (min 4×3)", w, h));
                                self.save_message_timer = 0;
                            }
                            None => {
                                self.save_message = Some("Invalid format — enter WxH e.g. 40x20".to_string());
                                self.save_message_timer = 0;
                            }
                        }
                    }
                    TextInputPurpose::PlayerTag => {
                        self.grid.player.tag = ti.buffer;
                        self.unsaved = true;
                    }
                    TextInputPurpose::PlayerScript => {
                        self.grid.player.script = if ti.buffer.is_empty() { None } else { Some(ti.buffer) };
                        self.unsaved = true;
                    }
                    TextInputPurpose::TileGlyph { gx, gy } => {
                        if let Some(ch) = ti.buffer.chars().next() {
                            if let Some(tile) = self.grid.get(gx, gy).cloned() {
                                let mut new_tile = tile.clone();
                                new_tile.glyph = ch;
                                self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                                self.grid.place(gx, gy, new_tile);
                                self.unsaved = true;
                            }
                        }
                    }
                    TextInputPurpose::PlayerGlyph => {
                        if let Some(ch) = ti.buffer.chars().next() {
                            self.grid.player.glyph = ch;
                            self.unsaved = true;
                        }
                    }
                    TextInputPurpose::NewLevelName => {
                        let name = ti.buffer.trim().to_string();
                        if !name.is_empty() {
                            let file_name = format!("{}.level", name);
                            let path = if let Some(ref folder) = self.project_folder {
                                format!("{}/{}", folder, file_name)
                            } else {
                                file_name
                            };
                            self.save();
                            let mut new_grid = crate::editor::grid::LevelGrid::new(
                                DEFAULT_LEVEL_W, DEFAULT_LEVEL_H,
                            );
                            new_grid.name = name.clone();
                            match new_grid.to_level_data().save(&path) {
                                Ok(()) => {
                                    self.grid      = new_grid;
                                    self.undo      = UndoStack::new();
                                    self.save_path = path.clone();
                                    self.unsaved   = false;
                                    self.save_message = Some(format!("New level: {}", path));
                                    self.save_message_timer = 0;
                                }
                                Err(e) => {
                                    self.save_message = Some(format!("Error: {}", e));
                                    self.save_message_timer = 0;
                                }
                            }
                        }
                    }
                }
                return;
            }

            if input.just_pressed(Key::Escape) { self.text_input = None; }
            return;
        }

        // ── Ctrl+Z / Ctrl+Y ──────────────────────────────────────────────────
        let ctrl = input.is_held(Key::LeftCtrl) || input.is_held(Key::RightCtrl);
        if ctrl {
            if input.just_pressed(Key::Z) {
                if let Some(cmd) = self.undo.pop_undo() { self.reverse_command(&cmd); self.unsaved = true; }
                return;
            }
            if input.just_pressed(Key::Y) {
                if let Some(cmd) = self.undo.pop_redo() { self.apply_command(&cmd); self.unsaved = true; }
                return;
            }
        }

        // ── Panel drag / resize / close ───────────────────────────────────────
        if mouse.left_just_pressed() && mouse.in_bounds {
            let col = mouse.cell_x;
            let row = mouse.cell_y;
            if let Some(pid) = self.panels.close_btn_at(col, row) {
                self.panels.hide(pid);
                return;
            }
            if let Some(pid) = self.panels.resize_handle_at(col, row) {
                self.panels.start_resize(pid, col as i32, row as i32);
                return;
            }
            if let Some(pid) = self.panels.title_bar_at(col, row) {
                self.panels.start_drag(pid, col as i32, row as i32);
                return;
            }
        }
        if mouse.left_held() {
            let sw = self.layout.screen_w;
            let sh = self.layout.screen_h;
            if self.panels.is_dragging() {
                self.panels.update_drag(mouse.cell_x as i32, mouse.cell_y as i32, sw, sh);
            } else if self.panels.is_resizing() {
                self.panels.update_resize(mouse.cell_x as i32, mouse.cell_y as i32);
            }
        }
        if mouse.left_just_released() {
            let sw = self.layout.screen_w;
            let sh = self.layout.screen_h;
            self.panels.end_drag(sw, sh);
            self.panels.end_resize();
        }

        // ── Menu bar click (row 1) ────────────────────────────────────────────
        if mouse.left_just_pressed() && mouse.cell_y == self.layout.toolbar_row {
            if let Some(kind) = ui::menu_label_at(mouse.cell_x) {
                self.active_menu = if self.active_menu == Some(kind) { None } else { Some(kind) };
            } else {
                self.active_menu = None;
            }
            return;
        }

        // ── Open menu: dropdown click or dismiss ──────────────────────────────
        if let Some(menu) = self.active_menu {
            if mouse.left_just_pressed() {
                let action = ui::menu_item_at(menu, mouse.cell_x, mouse.cell_y, &self.layout);
                self.active_menu = None;
                if let Some(action) = action {
                    match action {
                        ToolbarAction::CloseProject => {
                            self.pending_transition = Some(crate::engine::Transition::ToStart);
                            return;
                        }
                        ToolbarAction::RenameLevel => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.name.clone(),
                                purpose: TextInputPurpose::LevelName,
                            });
                            return;
                        }
                        ToolbarAction::ResizeLevel => {
                            self.text_input = Some(TextInput {
                                buffer:  String::new(),
                                purpose: TextInputPurpose::ResizeLevel,
                            });
                            return;
                        }
                        ToolbarAction::SetSpawn => {
                            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                                self.grid.spawn_point = (gx as f32, gy as f32);
                                self.unsaved = true;
                            }
                            return;
                        }
                        ToolbarAction::AddNamedSpawn => {
                            self.text_input = Some(TextInput {
                                buffer: String::new(),
                                purpose: TextInputPurpose::NamedSpawn,
                            });
                            return;
                        }
                        ToolbarAction::NewLevel => {
                            self.text_input = Some(TextInput {
                                buffer:  String::new(),
                                purpose: TextInputPurpose::NewLevelName,
                            });
                            return;
                        }
                        ToolbarAction::FindReplace => {
                            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                                self.find_replace_at(gx, gy);
                            }
                            return;
                        }
                        _ => { self.dispatch_toolbar_action(action); return; }
                    }
                }
                return;
            }
            if input.just_pressed(Key::Escape) { self.active_menu = None; return; }
        }

        // ── Hierarchy click ───────────────────────────────────────────────────
        if self.panels.visible(PanelId::Hierarchy) && mouse.left_just_pressed() && mouse.in_bounds {
            let panel = self.panels.get(PanelId::Hierarchy);
            let cy = panel.content_y();
            if panel.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y >= cy {
                let hier_row = mouse.cell_y - cy;
                if hier_row == 1 {
                    self.hierarchy_sel = Some(HierarchySelection::Player);
                    self.center_on(self.grid.spawn_point.0 as i32, self.grid.spawn_point.1 as i32);
                    return;
                } else if hier_row >= 2 {
                    let idx = hier_row - 2;
                    if idx < self.grid.extra_spawns.len() {
                        let (_, sx, sy) = self.grid.extra_spawns[idx];
                        self.hierarchy_sel = Some(HierarchySelection::Spawn(idx));
                        self.center_on(sx as i32, sy as i32);
                        return;
                    }
                }
            }
        }

        // ── Middle-mouse drag pan ─────────────────────────────────────────────
        if mouse.middle_just_pressed() {
            self.pan_anchor = Some((mouse.cell_x, mouse.cell_y, self.scroll.0, self.scroll.1));
        }
        if mouse.middle_held() {
            if let Some((ax, ay, sx, sy)) = self.pan_anchor {
                let dx = mouse.cell_x as i32 - ax as i32;
                let dy = mouse.cell_y as i32 - ay as i32;
                self.scroll.0 = (sx - dx).max(0);
                self.scroll.1 = (sy - dy).max(0);
                self.clamp_scroll();
            }
        }
        if mouse.middle_just_released() { self.pan_anchor = None; }

        // ── File browser ──────────────────────────────────────────────────────
        if self.browsing {
            if input.just_pressed(Key::Escape) { self.browsing = false; return; }
            if input.just_pressed(Key::Down) && self.file_cursor + 1 < self.file_list.len() {
                self.file_cursor += 1;
            }
            if input.just_pressed(Key::Up) && self.file_cursor > 0 {
                self.file_cursor -= 1;
            }
            if input.just_pressed(Key::Enter) && !self.file_list.is_empty() {
                let path    = self.file_list[self.file_cursor].clone();
                let pf      = self.project_folder.clone();
                let pn      = self.project_name.clone();
                match EditorState::load(&path) {
                    Ok(mut new_editor) => {
                        new_editor.project_folder = pf;
                        new_editor.project_name   = pn;
                        *self = new_editor;
                    }
                    Err(e) => {
                        self.save_message       = Some(format!("Load error: {}", e));
                        self.save_message_timer = 0;
                        self.browsing           = false;
                    }
                }
            }
            return;
        }

        // ── Paste mode ────────────────────────────────────────────────────────
        if self.pasting {
            if input.just_pressed(Key::Escape) {
                self.pasting = false;
                self.active_tool = ToolKind::Paint;
                return;
            }
            if input.just_pressed(Key::H) { self.paste_flip_x = !self.paste_flip_x; }
            if input.just_pressed(Key::LeftBracket)  { self.paste_rotate = (self.paste_rotate + 3) % 4; }
            if input.just_pressed(Key::RightBracket) { self.paste_rotate = (self.paste_rotate + 1) % 4; }
            if input.just_pressed(Key::J) { self.paste_flip_y = !self.paste_flip_y; }
            if mouse.left_just_pressed() {
                if let Some(cursor) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.stamp_paste(cursor);
                }
                self.pasting     = false;
                self.active_tool = ToolKind::Paint;
            }
            return;
        }

        // ── Copy/Cut select mode ──────────────────────────────────────────────
        if self.selecting || self.cutting {
            if input.just_pressed(Key::Escape) {
                self.selecting   = false;
                self.cutting     = false;
                self.sel_anchor  = None;
                self.active_tool = ToolKind::Paint;
                return;
            }
            if mouse.left_just_pressed() {
                if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.sel_anchor = Some(pos);
                }
            }
            if mouse.left_just_released() {
                if let (Some(anchor), Some(current)) = (
                    self.sel_anchor, self.mouse_to_grid(mouse.cell_x, mouse.cell_y),
                ) {
                    if self.cutting { self.cut_selection(anchor, current); }
                    else            { self.copy_selection(anchor, current); }
                }
                self.selecting   = false;
                self.cutting     = false;
                self.sel_anchor  = None;
                self.active_tool = ToolKind::Paint;
            }
            return;
        }

        // ── Palette panel click (select tile by clicking) ─────────────────────
        if self.panels.visible(PanelId::Palette) && mouse.left_just_pressed() && mouse.in_bounds {
            let panel = self.panels.get(PanelId::Palette);
            let cy = panel.content_y();
            if panel.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y > cy {
                let idx = mouse.cell_y.saturating_sub(cy + 1);
                self.palette.select(idx);
                return;
            }
        }

        // ── Track inspected tile (last canvas cell the mouse was over) ────────
        let click = mouse.left_just_pressed();
        let l = &self.layout;
        let on_canvas = mouse.in_bounds
            && mouse.cell_x >= l.canvas_x
            && mouse.cell_x < l.canvas_x + l.canvas_w
            && mouse.cell_y >= l.canvas_y
            && mouse.cell_y < l.canvas_y + l.canvas_h;

        if on_canvas {
            self.inspected_pos = self.mouse_to_grid(mouse.cell_x, mouse.cell_y);
            if click || mouse.right_just_pressed() {
                self.hierarchy_sel = None;
            }
        }

        if self.select_mode && click && on_canvas {
            self.selected_pos = self.inspected_pos;
            return;
        }

        // ── Inspector panel clicks ─────────────────────────────────────────────
        let insp_tile_pos = match self.hierarchy_sel {
            Some(HierarchySelection::Player) => None,
            Some(HierarchySelection::Spawn(i)) => self.grid.extra_spawns.get(i)
                .map(|(_, x, y)| (*x as i32, *y as i32)),
            None => if self.select_mode { self.selected_pos } else { self.inspected_pos },
        };
        if self.panels.visible(PanelId::Inspector) && click && mouse.in_bounds {
            let (insp_x, insp_cy, insp_w) = {
                let p = self.panels.get(PanelId::Inspector);
                (p.x.max(0) as usize, p.content_y(), p.w)
            };
            let insp_panel = self.panels.get(PanelId::Inspector);
            if insp_panel.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y >= insp_cy {
                let cy = insp_cy;
                let glyph_row  = cy + INSP_GLYPH_OFF;
                let tag_row    = cy + INSP_TAG_OFF;
                let solid_row  = cy + INSP_SOLID_OFF;
                let trig_row   = cy + INSP_TRIG_OFF;
                let cam_row    = cy + INSP_CAM_OFF;
                let script_row = cy + INSP_SCRIPT_OFF;
                let exit_row   = cy + INSP_EXIT_OFF;
                let _ = (insp_x, insp_w);
                if self.hierarchy_sel == Some(HierarchySelection::Player) {
                    match mouse.cell_y {
                        r if r == glyph_row => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.glyph.to_string(),
                                purpose: TextInputPurpose::PlayerGlyph,
                            });
                        }
                        r if r == tag_row => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.tag.clone(),
                                purpose: TextInputPurpose::PlayerTag,
                            });
                        }
                        r if r == script_row => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.script.clone().unwrap_or_default(),
                                purpose: TextInputPurpose::PlayerScript,
                            });
                        }
                        r if r == solid_row => {
                            self.grid.player.solid = !self.grid.player.solid;
                            self.unsaved = true;
                        }
                        r if r == trig_row => {
                            self.grid.player.trigger = !self.grid.player.trigger;
                            self.unsaved = true;
                        }
                        r if r == cam_row => {
                            self.grid.player.camera_follow = !self.grid.player.camera_follow;
                            self.unsaved = true;
                        }
                        _ => {}
                    }
                } else if let Some((gx, gy)) = insp_tile_pos {
                    if self.grid.get(gx, gy).is_some() {
                        if mouse.cell_y == cy + INSP_GRAPH_BTN {
                            if self.grid.get(gx, gy).map_or(false, |t| t.graph.is_none()) {
                                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.graph = Some(node_graph::NodeGraph::default());
                                    self.grid.place(gx, gy, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            self.graph_mode = Some((gx, gy));
                            self.graph_view_ox = 4;
                            self.graph_view_oy = 3;
                            self.graph_selected_node = None;
                            self.graph_connecting    = None;
                            self.graph_palette_open  = None;
                        } else {
                            match mouse.cell_y {
                                r if r == glyph_row => {
                                    let g = self.grid.get(gx, gy).map(|t| t.glyph.to_string()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: g, purpose: TextInputPurpose::TileGlyph { gx, gy } });
                                }
                                r if r == tag_row => {
                                    let tag = self.grid.get(gx, gy).map(|t| t.tag.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: tag, purpose: TextInputPurpose::TileTag { gx, gy } });
                                }
                                r if r == script_row => {
                                    let script = self.grid.get(gx, gy).and_then(|t| t.script.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: script, purpose: TextInputPurpose::ScriptPath { gx, gy } });
                                }
                                r if r == exit_row => {
                                    let path = self.grid.get(gx, gy).and_then(|t| t.next_level.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: path, purpose: TextInputPurpose::TileNextLevel { gx, gy } });
                                }
                                r if r == solid_row => {
                                    if let Some(tile) = self.grid.get(gx, gy).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.solid = !new_tile.solid;
                                        self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                                        self.grid.place(gx, gy, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                r if r == trig_row => {
                                    if let Some(tile) = self.grid.get(gx, gy).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.trigger = !new_tile.trigger;
                                        self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                                        self.grid.place(gx, gy, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                r if r == cam_row => {
                                    if let Some(tile) = self.grid.get(gx, gy).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.camera_follow = !new_tile.camera_follow;
                                        self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                                        self.grid.place(gx, gy, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // ── Palette keys 1–9 ─────────────────────────────────────────────────
        let number_keys = [
            (Key::Key1,1),(Key::Key2,2),(Key::Key3,3),(Key::Key4,4),(Key::Key5,5),
            (Key::Key6,6),(Key::Key7,7),(Key::Key8,8),(Key::Key9,9),
        ];
        for (key, num) in &number_keys {
            if input.just_pressed(*key) { self.palette.select_by_key(*num); }
        }

        // ── Keyboard shortcuts ────────────────────────────────────────────────

        if input.just_pressed(Key::Escape) {
            if self.show_help {
                self.show_help = false;
                return;
            }
            if self.rect_anchor.is_some() {
                self.rect_anchor = None;
                self.active_tool = ToolKind::Paint;
            } else if self.line_anchor.is_some() {
                self.line_anchor = None;
                self.active_tool = ToolKind::Paint;
            }
            return;
        }

        // F5 / Shift+F5 — Preview / Preview from cursor.
        if input.just_pressed(Key::F5) {
            let mut data = self.grid.to_level_data();
            data.path = self.save_path.clone();
            if shift {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    data.spawn_point = (gx as f32, gy as f32);
                }
            }
            self.pending_transition = Some(crate::engine::Transition::ToPlay(data));
            return;
        }

        // S — save / Shift+S — save-as.
        if input.just_pressed(Key::S) {
            if shift {
                self.text_input = Some(TextInput {
                    buffer:  self.save_path.clone(),
                    purpose: TextInputPurpose::SaveAs,
                });
                return;
            }
            self.save();
        }

        if input.just_pressed(Key::Tab) { self.show_grid = !self.show_grid; }
        if input.just_pressed(Key::B)   { self.panels.toggle(PanelId::Palette); }
        if input.just_pressed(Key::G)   { self.show_physics = !self.show_physics; }

        // Home — center view on player spawn point.
        if input.just_pressed(Key::Home) {
            let (sx, sy) = self.grid.spawn_point;
            self.center_on(sx as i32, sy as i32);
        }

        // ? (Shift+Slash) — toggle help screen. / — find & replace.
        if input.just_pressed(Key::Slash) {
            if shift {
                self.show_help = !self.show_help;
                return;
            }
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.find_replace_at(gx, gy);
            }
            return;
        }

        // Delete — erase tile under cursor.
        if input.just_pressed(Key::Delete) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.erase_brush(gx, gy);
            }
            return;
        }

        // Q — toggle select mode (click to inspect instead of paint).
        if input.just_pressed(Key::Q) {
            self.select_mode = !self.select_mode;
            if !self.select_mode {
                self.selected_pos = None;
                self.active_tool  = ToolKind::Paint;
            } else {
                self.active_tool = ToolKind::Select;
            }
        }

        if input.just_pressed(Key::Backquote)  { self.panels.toggle(PanelId::Stats); }

        // U — undo, R — redo.
        if input.just_pressed(Key::U) {
            if let Some(cmd) = self.undo.pop_undo() { self.reverse_command(&cmd); self.unsaved = true; }
        }
        if input.just_pressed(Key::R) {
            if let Some(cmd) = self.undo.pop_redo() { self.apply_command(&cmd); self.unsaved = true; }
        }

        // E — cycle eraser size.
        if input.just_pressed(Key::E) {
            self.erase_size = match self.erase_size { 1 => 3, 3 => 5, _ => 1 };
        }

        // P / Shift+P — spawn points.
        if input.just_pressed(Key::P) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if shift {
                    self.text_input = Some(TextInput {
                        buffer:  String::new(),
                        purpose: TextInputPurpose::NamedSpawn,
                    });
                } else {
                    self.grid.spawn_point = (gx as f32, gy as f32);
                    self.unsaved = true;
                }
            }
            return;
        }

        // N — rename.
        if input.just_pressed(Key::N) {
            self.text_input = Some(TextInput { buffer: self.grid.name.clone(), purpose: TextInputPurpose::LevelName });
            return;
        }

        // T — script attachment.
        if input.just_pressed(Key::T) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy) {
                    let existing = tile.script.clone().unwrap_or_default();
                    self.text_input = Some(TextInput { buffer: existing, purpose: TextInputPurpose::ScriptPath { gx, gy } });
                    return;
                }
            }
        }

        // D — set exit destination (next_level path) on tile under cursor.
        if input.just_pressed(Key::D) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy) {
                    let existing = tile.next_level.clone().unwrap_or_default();
                    self.text_input = Some(TextInput { buffer: existing, purpose: TextInputPurpose::TileNextLevel { gx, gy } });
                    return;
                }
            }
        }

        // I — edit tile tag.
        if input.just_pressed(Key::I) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy) {
                    self.text_input = Some(TextInput { buffer: tile.tag.clone(), purpose: TextInputPurpose::TileTag { gx, gy } });
                    return;
                }
            }
        }

        // ; — toggle solid, ' — toggle trigger on tile under cursor.
        if input.just_pressed(Key::Semicolon) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    new_tile.solid = !new_tile.solid;
                    self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
        }
        if input.just_pressed(Key::Apostrophe) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    let mut new_tile = tile.clone();
                    new_tile.trigger = !new_tile.trigger;
                    self.undo.push(Command::Batch { cells: vec![(gx, gy, Some(tile), Some(new_tile.clone()))] });
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
        }

        // F — flood fill.
        if input.just_pressed(Key::F) {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.flood_fill(gx, gy);
            }
        }

        // L — line tool (anchor on first press, stamp on second).
        if input.just_pressed(Key::L) {
            if self.line_anchor.is_none() {
                if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.line_anchor = Some(pos);
                    self.active_tool = ToolKind::Line;
                }
            } else {
                if let Some(end) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.stamp_line(self.line_anchor.unwrap(), end);
                }
                self.line_anchor = None;
                self.active_tool = ToolKind::Paint;
            }
            return;
        }

        // C — copy-select, X — cut-select.
        if input.just_pressed(Key::C) {
            self.selecting = true; self.sel_anchor = None;
            self.active_tool = ToolKind::Copy;
            return;
        }
        if input.just_pressed(Key::X) {
            self.cutting = true; self.sel_anchor = None;
            self.active_tool = ToolKind::Cut;
            return;
        }

        // V — paste.
        if input.just_pressed(Key::V) && !self.clipboard.is_empty() {
            self.pasting = true;
            self.active_tool = ToolKind::Paste;
            return;
        }

        // O — open file browser.
        if input.just_pressed(Key::O) {
            self.refresh_file_list();
            self.browsing = true;
            return;
        }

        // Z — resize level.
        if input.just_pressed(Key::Z) {
            self.text_input = Some(TextInput {
                buffer:  String::new(),
                purpose: TextInputPurpose::ResizeLevel,
            });
            return;
        }

        // Arrow keys — scroll canvas (smooth: fire on frame 1, then every 2 frames after a 12-frame delay).
        let any_arrow = input.is_held(Key::Left) || input.is_held(Key::Right)
            || input.is_held(Key::Up) || input.is_held(Key::Down);
        if any_arrow { self.scroll_repeat = self.scroll_repeat.saturating_add(1); }
        else { self.scroll_repeat = 0; }
        let do_scroll = self.scroll_repeat == 1
            || (self.scroll_repeat > 12 && self.scroll_repeat % 2 == 0);
        if do_scroll {
            let scroll_speed = if shift { 5 } else { 1 };
            if input.is_held(Key::Left)  { self.scroll.0 -= scroll_speed; }
            if input.is_held(Key::Right) { self.scroll.0 += scroll_speed; }
            if input.is_held(Key::Up)    { self.scroll.1 -= scroll_speed; }
            if input.is_held(Key::Down)  { self.scroll.1 += scroll_speed; }
            self.clamp_scroll();
        }

        // Mouse wheel — scroll canvas (vertical = row scroll, horizontal = col scroll).
        if mouse.wheel_y != 0.0 {
            let dy = if mouse.wheel_y > 0.0 { 3i32 } else { -3i32 };
            self.scroll.1 += dy;
            self.clamp_scroll();
        }
        if mouse.wheel_x != 0.0 {
            let dx = if mouse.wheel_x > 0.0 { 3i32 } else { -3i32 };
            self.scroll.0 += dx;
            self.clamp_scroll();
        }

        // In select mode, no painting or erasing — only selection.
        if self.select_mode { return; }

        // ── Toolbar sticky tools (no modifier needed) ─────────────────────────
        if !shift && !alt {
            match self.active_tool {
                ToolKind::Rect => {
                    if mouse.left_just_pressed() {
                        if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.rect_anchor = Some(pos);
                        }
                    }
                    if mouse.left_just_released() {
                        if let (Some(anchor), Some(current)) = (
                            self.rect_anchor.take(),
                            self.mouse_to_grid(mouse.cell_x, mouse.cell_y),
                        ) {
                            self.stamp_rect(anchor, current);
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.erase_brush(gx, gy);
                        }
                    } else if mouse.right_held() && self.erase_size == 1 {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            if let Some(removed) = self.grid.erase(gx, gy) {
                                self.undo.push(Command::EraseTile { before: removed });
                                self.unsaved = true;
                            }
                        }
                    }
                    return;
                }
                ToolKind::Line => {
                    if mouse.left_just_pressed() {
                        if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            if self.line_anchor.is_none() {
                                self.line_anchor = Some(pos);
                            } else {
                                self.stamp_line(self.line_anchor.unwrap(), pos);
                                self.line_anchor = None;
                                self.active_tool = ToolKind::Paint;
                            }
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.erase_brush(gx, gy);
                        }
                    }
                    return;
                }
                ToolKind::Fill => {
                    if mouse.left_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.flood_fill(gx, gy);
                        }
                    }
                    if mouse.right_just_pressed() {
                        if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                            self.erase_brush(gx, gy);
                        }
                    }
                    return;
                }
                _ => {}
            }
        }

        // ── Mouse: rectangle tool (Shift+drag) ────────────────────────────────
        if shift {
            if mouse.left_just_pressed() {
                if let Some(pos) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    self.rect_anchor = Some(pos);
                }
            }
            if mouse.left_just_released() {
                if let (Some(anchor), Some(current)) = (
                    self.rect_anchor.take(),
                    self.mouse_to_grid(mouse.cell_x, mouse.cell_y),
                ) {
                    self.stamp_rect(anchor, current);
                }
            }
            return;
        } else {
            self.rect_anchor = None;
        }

        // ── Mouse: alt+drag = scatter paint ──────────────────────────────────
        if alt && mouse.left_held() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if (gx * 1234 + gy * 5678 + self.undo.len() as i32) % 2 == 0 {
                    let new_tile = self.palette.current().to_tile_record(gx, gy);
                    let existing = self.grid.get(gx, gy).cloned();
                    self.undo.push(Command::PlaceTile { before: existing, after: new_tile.clone() });
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
            return;
        }

        // ── Normal left-click paint ───────────────────────────────────────────
        if mouse.left_held() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                let new_tile = self.palette.current().to_tile_record(gx, gy);
                let existing = self.grid.get(gx, gy).cloned();
                let same = existing.as_ref().map(|t| {
                    t.glyph == new_tile.glyph && t.solid == new_tile.solid
                        && t.trigger == new_tile.trigger && t.tag == new_tile.tag
                }).unwrap_or(false);
                if !same {
                    self.undo.push(Command::PlaceTile { before: existing, after: new_tile.clone() });
                    self.grid.place(gx, gy, new_tile);
                    self.unsaved = true;
                }
            }
        }

        // ── Right-click erase (brush size) ───────────────────────────────────
        if mouse.right_just_pressed() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                self.erase_brush(gx, gy);
            }
        } else if mouse.right_held() && self.erase_size == 1 {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                if let Some(removed) = self.grid.erase(gx, gy) {
                    self.undo.push(Command::EraseTile { before: removed });
                    self.unsaved = true;
                }
            }
        }
    }
}
