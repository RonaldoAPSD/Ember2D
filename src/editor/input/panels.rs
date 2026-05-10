// editor/input/panels.rs — UI panels and menu interaction for level editor.

use crate::input::Key;
use crate::renderer::color::Color;
use super::super::EditorState;
use super::super::panel::PanelId;
use super::super::{TextInput, TextInputPurpose};
use super::super::ui::{self, ToolbarAction, HierarchySelection,
                       INSP_NAME_OFF, INSP_GLYPH_OFF, INSP_TAG_OFF, INSP_FG_OFF, INSP_BG_OFF,
                       INSP_SOLID_OFF, INSP_TRIG_OFF, INSP_CAM_OFF,
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
        if self.panels.visible(PanelId::Palette) && mouse.in_bounds {
            let panel = self.panels.get(PanelId::Palette);
            let py = panel.y.max(0) as usize;
            let ph = panel.h;
            let cy = panel.content_y();

            if panel.contains(mouse.cell_x, mouse.cell_y) {
                // Mouse wheel scroll
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = self.palette.tiles.len().saturating_sub(ph.saturating_sub(2));
                    self.palette_scroll = (self.palette_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() {
                    self.ignore_drag = true;
                    if mouse.cell_y == py + ph - 1 {
                        // [+ New Item] button
                        self.palette.tiles.push(crate::editor::palette::TileDefinition {
                            name: "New Item".into(),
                            glyph: '?',
                            fg: crate::renderer::color::Color::White,
                            bg: crate::renderer::color::Color::Reset,
                            solid: false,
                            trigger: false,
                            tag: String::new(),
                        });
                        self.palette.selected = self.palette.tiles.len() - 1;
                        self.save_message = Some("Added new palette item.".to_string());
                        self.save_message_timer = 0;
                        self.unsaved = true;
                        return;
                    } else if mouse.cell_y > cy {
                        let idx = self.palette_scroll + mouse.cell_y.saturating_sub(cy + 1);
                        if idx < self.palette.tiles.len() {
                            self.palette.select(idx);
                            return;
                        }
                    }
                }
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
                let ix = insp_panel.x.max(0) as usize;
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
                } else if insp_tile_pos.is_none() {
                    // ── Palette Edit Mode ────────────────────────────────────
                    let sel = self.palette.selected;
                    let name_row = cy + INSP_NAME_OFF;
                    let glyph_row = cy + INSP_GLYPH_OFF;
                    let fg_row   = cy + INSP_FG_OFF;
                    let bg_row   = cy + INSP_BG_OFF;
                    let solid_row = cy + INSP_SOLID_OFF;
                    let trig_row = cy + INSP_TRIG_OFF;
                    let tag_inp_row = cy + 13; // Tag input row
                    
                    // If picker is open, it eats all clicks in the inspector
                    if let Some(is_fg) = self.palette_color_picker {
                        let picker_y = if is_fg { fg_row + 1 } else { bg_row + 1 };
                        if mouse.cell_y >= picker_y && mouse.cell_y < picker_y + 2
                           && mouse.cell_x >= ix + 1 && mouse.cell_x < ix + 17 {
                            let col_idx = (mouse.cell_x - (ix + 1)) / 2;
                            let row_idx = mouse.cell_y - picker_y;
                            let color_idx = row_idx * 8 + col_idx;
                            let colors = [
                                Color::Black, Color::White, Color::Red, Color::Green, Color::Yellow, Color::Blue, Color::Cyan, Color::Magenta,
                                Color::DarkGrey, Color::Grey, Color::DarkRed, Color::DarkGreen, Color::DarkBlue, Color::DarkYellow, Color::DarkCyan, Color::DarkMagenta,
                            ];
                            if color_idx < colors.len() {
                                if is_fg { self.palette.tiles[sel].fg = colors[color_idx]; }
                                else     { self.palette.tiles[sel].bg = colors[color_idx]; }
                                self.unsaved = true;
                            }
                        }
                        self.palette_color_picker = None;
                        return;
                    }

                    match mouse.cell_y {
                        r if r == name_row => {
                            self.text_input = Some(TextInput { buffer: self.palette.tiles[sel].name.clone(), purpose: TextInputPurpose::PaletteName });
                        }
                        r if r == glyph_row => {
                            self.text_input = Some(TextInput { buffer: self.palette.tiles[sel].glyph.to_string(), purpose: TextInputPurpose::TileGlyph { gx: -1, gy: -1 } });
                        }
                        r if r == fg_row => {
                            self.palette_color_picker = Some(true);
                        }
                        r if r == bg_row => {
                            self.palette_color_picker = Some(false);
                        }
                        r if r == solid_row => {
                            self.palette.tiles[sel].solid = !self.palette.tiles[sel].solid;
                            self.unsaved = true;
                        }
                        r if r == trig_row => {
                            self.palette.tiles[sel].trigger = !self.palette.tiles[sel].trigger;
                            self.unsaved = true;
                        }
                        r if r == tag_inp_row => {
                            self.text_input = Some(TextInput { buffer: self.palette.tiles[sel].tag.clone(), purpose: TextInputPurpose::TileTag { gx: -1, gy: -1 } });
                        }
                        _ => {}
                    }
                } else if let Some((gx, gy)) = insp_tile_pos {
                    if self.grid.get(gx, gy, self.active_layer).is_some() {
                        if mouse.cell_y == cy + INSP_GRAPH_BTN {
                            if self.grid.get(gx, gy, self.active_layer).map_or(false, |t| t.graph.is_none()) {
                                if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.graph = Some(node_graph::NodeGraph::default());
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
