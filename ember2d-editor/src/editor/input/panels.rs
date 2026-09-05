// editor/input/panels.rs — UI panels and menu interaction for level editor.

use ember2d::input::Key;
use super::super::EditorState;
use super::super::panel::PanelId;
use super::super::{TextInput, TextInputPurpose};
use super::super::ui::{self, ToolbarAction, HierarchySelection,
                       INSP_GLYPH_OFF, INSP_TAG_OFF,
                       INSP_SOLID_OFF, INSP_TRIG_OFF, INSP_CAM_OFF,
                       INSP_SCRIPT_OFF, INSP_EXIT_OFF, INSP_GRAPH_BTN,
                       INSP_LAYER_OFF, INSP_MASK_OFF};
use super::super::commands::Command;
use ember2d_sim::graph::NodeGraph;

impl EditorState {
    pub(super) fn handle_panel_input(&mut self, input: &ember2d::input::InputManager, mouse: &ember2d::mouse::MouseState) {
        // F-key panel toggles (work in any mode).
        if input.just_pressed(Key::F1) { self.panels.toggle(PanelId::Console); }
        if input.just_pressed(Key::F2) { self.panels.toggle(PanelId::Inspector); }
        if input.just_pressed(Key::F3) { self.console_log.clear(); }

        // ── Panel drag / resize / close / tabs ───────────────────────────────────────
        if mouse.left_just_pressed() && mouse.in_bounds {
            let col = mouse.cell_x;
            let row = mouse.cell_y;

            // Tabs (switch active panel in dock)
            let sw = self.layout.screen_w;
            let sh = self.layout.screen_h;
            if let Some(tid) = self.panels.tab_at(col, row, sw, sh) {
                self.panels.set_active(tid);
                self.ignore_drag = true;
                return;
            }
            
            // Focus tracking
            if let Some(pid) = self.panels.panel_at(col, row) {
                self.focused_panel = Some(pid);
                self.panels.bring_to_front(pid);
            } else if !self.panels.is_point_on_panel(col, row) {
                self.focused_panel = None;
            }

            if let Some(pid) = self.panels.close_btn_at(col, row) {
                if pid != PanelId::Viewport { self.panels.hide(pid); }
                self.ignore_drag = true;
                return;
            }
            if let Some(pid) = self.panels.resize_handle_at(col, row) {
                if pid != PanelId::Viewport { self.panels.start_resize(pid, col as i32, row as i32); }
                self.ignore_drag = true;
                return;
            }
            if let Some(pid) = self.panels.title_bar_at(col, row) {
                if pid != PanelId::Viewport { self.panels.start_drag(pid, col as i32, row as i32); }
                self.ignore_drag = true;
                return;
            }
        }

        // ── Right-click context menus ───────────────────────────────────────
        if mouse.right_just_pressed() && mouse.in_bounds {
            let col = mouse.cell_x;
            let row = mouse.cell_y;
            let sw = self.layout.screen_w;
            let sh = self.layout.screen_h;

            // 1. Tab Context Menu
            if let Some(tid) = self.panels.tab_at(col, row, sw, sh) {
                self.context_menu = Some(ui::ContextMenu {
                    x: col, y: row,
                    selected: 0,
                    items: vec![
                        ("Close Tab", ui::ContextMenuAction::CloseTab(tid)),
                        ("Close Others", ui::ContextMenuAction::CloseOthers(tid)),
                        ("Float Panel", ui::ContextMenuAction::FloatPanel(tid)),
                    ],
                });
                return;
            }

            // 2. Panel-specific context menus
            if let Some(pid) = self.panels.panel_at(col, row) {
                match pid {
                    PanelId::FileBrowser => {
                        let p = self.panels.get(pid);
                        if row > p.content_y() {
                            let row_idx = self.file_browser_scroll + (row - (p.content_y() + 1));
                            let mut items = vec![
                                ("New Level", ui::ContextMenuAction::NewLevel),
                                ("New Script", ui::ContextMenuAction::NewScript),
                                ("New Folder", ui::ContextMenuAction::NewFolder),
                            ];
                            if row_idx < self.file_browser_files.len() {
                                let raw = &self.file_browser_files[row_idx];
                                if !raw.contains("[UP]") {
                                    let clean = if raw.len() > 3 { &raw[3..] } else { raw };
                                    items.push(("Delete", ui::ContextMenuAction::DeleteFile(clean.to_string())));
                                }
                            }
                            self.context_menu = Some(ui::ContextMenu { x: col, y: row, selected: 0, items });
                            return;
                        }
                    }
                    PanelId::Hierarchy => {
                        let p = self.panels.get(pid);
                        let cy = p.content_y();
                        if row >= cy + 1 {
                            let hier_row = row - cy;
                            let sel = if hier_row == 1 { Some(HierarchySelection::Player) }
                                     else { Some(HierarchySelection::Spawn(hier_row - 2)) };
                            
                            if let Some(s) = sel {
                                let mut items = vec![("Focus Camera", ui::ContextMenuAction::FocusCamera(s))];
                                if let HierarchySelection::Spawn(_) = s {
                                    items.push(("Duplicate", ui::ContextMenuAction::DuplicateEntity(s)));
                                    items.push(("Delete", ui::ContextMenuAction::DeleteEntity(s)));
                                }
                                self.context_menu = Some(ui::ContextMenu { x: col, y: row, selected: 0, items });
                                return;
                            }
                        }
                    }
                    _ => {}
                }
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
            self.ignore_drag = true;
            return;
        }

        // ── Open menu: dropdown click or dismiss ──────────────────────────────
        if let Some(menu) = self.active_menu {
            if mouse.left_just_pressed() {
                let action = ui::menu_item_at(menu, mouse.cell_x, mouse.cell_y, &self.layout);
                self.active_menu = None;
                self.ignore_drag = true;
                if let Some(action) = action {
                    match action {
                        ToolbarAction::CloseProject => {
                            self.pending_transition = Some(ember2d::engine::Transition::ToStart);
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
                            self.placing_spawn = true;
                            self.save_message = Some("Click on grid to place spawn. Esc to cancel.".to_string());
                            self.save_message_timer = 0;
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
                        _ => { self.dispatch_toolbar_action(action); return; }
                    }
                }
                return;
            }
            if input.just_pressed(Key::Escape) { self.active_menu = None; return; }
        }

        // ── File Browser Panel ───────────────────────────────────────────────
        if self.panels.visible(PanelId::FileBrowser) && mouse.in_bounds {
            let p = self.panels.get(PanelId::FileBrowser);
            if p.contains(mouse.cell_x, mouse.cell_y) {
                let cy = p.content_y();
                let ch = p.content_h();
                
                // Mouse wheel scroll
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = self.file_browser_files.len().saturating_sub(ch.saturating_sub(1));
                    self.file_browser_scroll = (self.file_browser_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() && mouse.cell_y > cy {
                    self.ignore_drag = true;
                    let row_idx = self.file_browser_scroll + (mouse.cell_y - (cy + 1));
                    if row_idx < self.file_browser_files.len() {
                        self.file_browser_cursor = row_idx;
                        let raw_name = self.file_browser_files[row_idx].clone();
                        
                        // 1. Navigation (UP)
                        if raw_name.contains("[UP]") {
                            if let Some(parent) = std::path::Path::new(&self.current_folder).parent() {
                                self.current_folder = parent.to_string_lossy().to_string();
                                if self.current_folder.is_empty() { self.current_folder = ".".to_string(); }
                            } else {
                                self.current_folder = ".".to_string();
                            }
                            self.file_browser_cursor = 0;
                            self.file_browser_scroll = 0;
                            self.refresh_project_files();
                            return;
                        }

                        // 2. Directory
                        if raw_name.starts_with("/ ") {
                            let dir_name = raw_name[2..].trim();
                            self.current_folder = if self.current_folder == "." {
                                dir_name.to_string()
                            } else {
                                format!("{}/{}", self.current_folder, dir_name)
                            };
                            self.file_browser_cursor = 0;
                            self.file_browser_scroll = 0;
                            self.refresh_project_files();
                            return;
                        }

                        // 3. Files
                        let clean_name = if raw_name.len() > 3 { &raw_name[3..].trim() } else { "" };
                        if clean_name.is_empty() { return; }

                        let relative_path = if self.current_folder == "." {
                            clean_name.to_string()
                        } else {
                            format!("{}/{}", self.current_folder, clean_name)
                        };

                        if clean_name.ends_with(".rhai") {
                            self.load_script(&relative_path);
                        } else if clean_name.ends_with(".level") {
                            if let Some(ref folder) = self.project_folder {
                                let path = format!("{}/{}", folder, relative_path);
                                self.modal = Some(crate::editor::Modal {
                                    title:   "Switch Level?".to_string(),
                                    message: format!("Load {}?", clean_name),
                                    purpose: crate::editor::ModalPurpose::ConfirmSwitchLevel { path },
                                });
                            }
                        }
                        return;
                    }
                }
            }
        }

        // ── Script Editor Panel ──────────────────────────────────────────────
        if self.panels.visible(PanelId::ScriptEditor) && mouse.in_bounds {
            let p = self.panels.get(PanelId::ScriptEditor);
            if p.contains(mouse.cell_x, mouse.cell_y) {
                let cy = p.content_y();
                let ch = p.content_h();

                // Mouse wheel scroll
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = self.script_buffer.len().saturating_sub(ch.saturating_sub(1));
                    self.script_scroll = (self.script_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() && mouse.cell_y > cy {
                    self.ignore_drag = true;
                    let row_idx = self.script_scroll + (mouse.cell_y - (cy + 1));
                    if row_idx < self.script_buffer.len() {
                        self.script_cursor.1 = row_idx;
                        let gutter_w = 4;
                        let col = mouse.cell_x as i32 - p.content_x() as i32 - gutter_w;
                        self.script_cursor.0 = (col.max(0) as usize).min(self.script_buffer[row_idx].len());
                    }
                    return;
                }
            }
        }

        // ── Hierarchy click ───────────────────────────────────────────────────
        if self.panels.visible(PanelId::Hierarchy) && mouse.left_just_pressed() && mouse.in_bounds {
            let p = self.panels.get(PanelId::Hierarchy);
            let cy = p.content_y();
            if p.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y >= cy {
                let hier_row = mouse.cell_y - cy;
                if hier_row == 1 {
                    self.hierarchy_sel = Some(HierarchySelection::Player);
                    self.center_on(self.grid.spawn_point.0 as i32, self.grid.spawn_point.1 as i32);
                    self.ignore_drag = true;
                    return;
                } else if hier_row >= 2 {
                    let idx = hier_row - 2;
                    if idx < self.grid.extra_spawns.len() {
                        let (_, sx, sy) = self.grid.extra_spawns[idx];
                        self.hierarchy_sel = Some(HierarchySelection::Spawn(idx));
                        self.center_on(sx as i32, sy as i32);
                        self.ignore_drag = true;
                        return;
                    }
                }
            }
        }

        // ── Palette panel click (select tile by clicking) ─────────────────────
        if self.panels.visible(PanelId::Palette) && mouse.in_bounds {
            let p = self.panels.get(PanelId::Palette);
            let cy = p.content_y();
            let ch = p.content_h();
            let pcx = p.content_x();
            let pcw = p.content_w();

            if p.contains(mouse.cell_x, mouse.cell_y) {
                use crate::editor::palette::PaletteRow;
                let layout = self.palette.build_layout();

                // Mouse wheel scroll
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = layout.len().saturating_sub(ch.saturating_sub(2));
                    self.palette_scroll = (self.palette_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() {
                    self.ignore_drag = true;
                    self.palette_search_focused = false; // Default clear

                    if mouse.cell_y == cy + 1 {
                        // Search bar click
                        self.palette_search_focused = true;
                        return;
                    } else if mouse.cell_y == cy + ch - 1 {
                        // Bottom row buttons
                        let col = mouse.cell_x;
                        if col >= pcx && col < pcx + 10 {
                            // [+ New] button
                            self.palette.tiles.push(crate::editor::palette::TileDefinition {
                                name: "New Item".into(),
                                glyph: '?',
                                fg: ember2d::renderer::color::Color::White,
                                bg: ember2d::renderer::color::Color::Reset,
                                solid: false,
                                trigger: false,
                                tag: String::new(),
                            });
                            self.palette.selected = self.palette.tiles.len() - 1;
                            self.palette_scroll = layout.len().saturating_sub(ch.saturating_sub(2));
                            self.save_message = Some("Added new palette item.".to_string());
                            self.save_message_timer = 0;
                            self.unsaved = true;
                            return;
                        } else if col >= pcx + pcw - 10 {
                            // [ Edit ] button
                            self.palette_editor_open = true;
                            self.palette_editing_idx = self.palette.selected;
                            return;
                        }
                    } else if mouse.cell_y > cy + 1 && mouse.cell_y < cy + ch - 1 {
                        let row_idx = self.palette_scroll + (mouse.cell_y - (cy + 2));
                        if row_idx < layout.len() {
                            match &layout[row_idx] {
                                PaletteRow::Header(name) => {
                                    if self.palette.collapsed.contains(name) {
                                        self.palette.collapsed.remove(name);
                                    } else {
                                        self.palette.collapsed.insert(name.clone());
                                    }
                                }
                                PaletteRow::Item(idx) => {
                                    self.palette.select(*idx);
                                }
                            }
                            return;
                        }
                    }
                }
            } else if mouse.left_just_pressed() {
                self.palette_search_focused = false;
            }
        }


        // ── Inspector panel clicks ─────────────────────────────────────────────
        let insp_tile_pos = match self.hierarchy_sel {
            Some(HierarchySelection::Player) => None,
            Some(HierarchySelection::Spawn(i)) => self.grid.extra_spawns.get(i)
                .map(|(_, x, y)| (*x as i32, *y as i32)),
            None => if self.select_mode { self.selected_pos } else { self.inspected_pos },
        };
        if self.panels.visible(PanelId::Inspector) && mouse.left_just_pressed() && mouse.in_bounds {
            let p = self.panels.get(PanelId::Inspector);
            let cy = p.content_y();
            if p.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y >= cy {
                self.ignore_drag = true;
                let glyph_row  = cy + INSP_GLYPH_OFF;
                let tag_row    = cy + INSP_TAG_OFF;
                let solid_row  = cy + INSP_SOLID_OFF;
                let trig_row   = cy + INSP_TRIG_OFF;
                let cam_row    = cy + INSP_CAM_OFF;
                let script_row = cy + INSP_SCRIPT_OFF;
                let exit_row   = cy + INSP_EXIT_OFF;
                let layer_row  = cy + INSP_LAYER_OFF;
                let mask_row   = cy + INSP_MASK_OFF;
                
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
                        r if r == layer_row => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.collider_layer.clone(),
                                purpose: TextInputPurpose::PlayerColliderLayer,
                            });
                        }
                        r if r == mask_row => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.collider_mask.join(","),
                                purpose: TextInputPurpose::PlayerColliderMask,
                            });
                        }
                        r if r == solid_row => {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.solid = !after.solid;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                        r if r == trig_row => {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.trigger = !after.trigger;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                        r if r == cam_row => {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.camera_follow = !after.camera_follow;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                        _ => {}
                    }
                } else if let Some((gx, gy)) = insp_tile_pos {
                    if self.grid.get(gx, gy, self.active_layer).is_some() {
                        if mouse.cell_y == cy + INSP_GRAPH_BTN {
                            if self.grid.get(gx, gy, self.active_layer).map_or(false, |t| t.graph.is_none()) {
                                if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.graph = Some(NodeGraph::default());
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
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
                                    let g = self.grid.get(gx, gy, self.active_layer).map(|t| t.glyph.to_string()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: g, purpose: TextInputPurpose::TileGlyph { gx, gy } });
                                }
                                r if r == tag_row => {
                                    let tag = self.grid.get(gx, gy, self.active_layer).map(|t| t.tag.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: tag, purpose: TextInputPurpose::TileTag { gx, gy } });
                                }
                                r if r == script_row => {
                                    let script = self.grid.get(gx, gy, self.active_layer).and_then(|t| t.script.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: script, purpose: TextInputPurpose::ScriptPath { gx, gy } });
                                }
                                r if r == exit_row => {
                                    let path = self.grid.get(gx, gy, self.active_layer).and_then(|t| t.next_level.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: path, purpose: TextInputPurpose::TileNextLevel { gx, gy } });
                                }
                                r if r == layer_row => {
                                    let layer = self.grid.get(gx, gy, self.active_layer).map(|t| t.collider_layer.clone()).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: layer, purpose: TextInputPurpose::TileColliderLayer { gx, gy } });
                                }
                                r if r == mask_row => {
                                    let mask = self.grid.get(gx, gy, self.active_layer).map(|t| t.collider_mask.join(",")).unwrap_or_default();
                                    self.text_input = Some(TextInput { buffer: mask, purpose: TextInputPurpose::TileColliderMask { gx, gy } });
                                }
                                r if r == solid_row => {
                                    if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.solid = !new_tile.solid;
                                        self.undo.push(Command::Batch { cells: vec![(gx, gy, self.active_layer, Some(tile), Some(new_tile.clone()))] });
                                        self.grid.place(gx, gy, self.active_layer, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                r if r == trig_row => {
                                    if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.trigger = !new_tile.trigger;
                                        self.undo.push(Command::Batch { cells: vec![(gx, gy, self.active_layer, Some(tile), Some(new_tile.clone()))] });
                                        self.grid.place(gx, gy, self.active_layer, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                r if r == cam_row => {
                                    if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.camera_follow = !new_tile.camera_follow;
                                        self.undo.push(Command::Batch { cells: vec![(gx, gy, self.active_layer, Some(tile), Some(new_tile.clone()))] });
                                        self.grid.place(gx, gy, self.active_layer, new_tile);
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
    }
}
