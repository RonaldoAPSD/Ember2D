// editor/input/panels.rs — UI panels and menu interaction for level editor.

use crate::input::Key;
use super::super::EditorState;
use super::super::panel::PanelId;
use super::super::{TextInput, TextInputPurpose};
use super::super::ui::{self, ToolbarAction, HierarchySelection,
                       INSP_GLYPH_OFF, INSP_TAG_OFF, INSP_SOLID_OFF, INSP_TRIG_OFF, INSP_CAM_OFF,
                       INSP_SCRIPT_OFF, INSP_EXIT_OFF, INSP_GRAPH_BTN};
use super::super::commands::Command;
use super::super::node_graph;

impl EditorState {
    pub(super) fn handle_panel_input(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState) {
        // F-key panel toggles (work in any mode).
        if input.just_pressed(Key::F1) { self.panels.toggle(PanelId::Console); }
        if input.just_pressed(Key::F2) { self.panels.toggle(PanelId::Inspector); }
        if input.just_pressed(Key::F3) { self.console_log.clear(); }

        // ── Panel drag / resize / close ───────────────────────────────────────
        if mouse.left_just_pressed() && mouse.in_bounds {
            let col = mouse.cell_x;
            let row = mouse.cell_y;
            if let Some(pid) = self.panels.close_btn_at(col, row) {
                self.panels.hide(pid);
                self.ignore_drag = true;
                return;
            }
            if let Some(pid) = self.panels.resize_handle_at(col, row) {
                self.panels.start_resize(pid, col as i32, row as i32);
                self.ignore_drag = true;
                return;
            }
            if let Some(pid) = self.panels.title_bar_at(col, row) {
                self.panels.start_drag(pid, col as i32, row as i32);
                self.ignore_drag = true;
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

        // ── Hierarchy click ───────────────────────────────────────────────────
        if self.panels.visible(PanelId::Hierarchy) && mouse.left_just_pressed() && mouse.in_bounds {
            let panel = self.panels.get(PanelId::Hierarchy);
            let cy = panel.content_y();
            if panel.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y >= cy {
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
        if self.panels.visible(PanelId::Palette) && mouse.left_just_pressed() && mouse.in_bounds {
            let panel = self.panels.get(PanelId::Palette);
            let cy = panel.content_y();
            if panel.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y > cy {
                let idx = mouse.cell_y.saturating_sub(cy + 1);
                self.palette.select(idx);
                self.ignore_drag = true;
                return;
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
            let insp_cy = {
                let p = self.panels.get(PanelId::Inspector);
                p.content_y()
            };
            let insp_panel = self.panels.get(PanelId::Inspector);
            if insp_panel.contains(mouse.cell_x, mouse.cell_y) && mouse.cell_y >= insp_cy {
                self.ignore_drag = true;
                let cy = insp_cy;
                let glyph_row  = cy + INSP_GLYPH_OFF;
                let tag_row    = cy + INSP_TAG_OFF;
                let solid_row  = cy + INSP_SOLID_OFF;
                let trig_row   = cy + INSP_TRIG_OFF;
                let cam_row    = cy + INSP_CAM_OFF;
                let script_row = cy + INSP_SCRIPT_OFF;
                let exit_row   = cy + INSP_EXIT_OFF;
                
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
    }
}
