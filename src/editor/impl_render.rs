// editor/impl_render.rs — Rendering logic for EditorState.

use crate::engine::RenderContext;
use crate::renderer::color::Color;

use super::EditorState;
use super::node_graph;
use super::panel::{PanelId, DockSide, draw_panel_chrome};
use super::TextInputPurpose;
use super::ui::{self, HierarchySelection, Layout, MenuState};

// ── Graph editor rendering ────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn render_graph_mode(&mut self, renderer: &mut crate::renderer::Renderer,
                          mouse: &crate::mouse::MouseState, gx: i32, gy: i32) {
        let sw = renderer.width;
        let sh = renderer.height;

        // Resolve graph reference
        let graph = match self.grid.get(gx, gy, self.active_layer).and_then(|t| t.graph.as_ref()) {
            Some(g) => g.clone(),
            None => {
                renderer.draw_str(0, 0, "No graph", Color::Red, Color::Black);
                return;
            }
        };

        node_graph::draw_graph(
            renderer, &graph,
            self.graph_selected_node,
            self.graph_connecting,
            mouse.cell_x, mouse.cell_y,
            self.graph_view_ox, self.graph_view_oy,
            sw, sh,
        );

        // Title bar (row 0)
        let tag = self.grid.get(gx, gy, self.active_layer).map(|t| t.tag.clone()).unwrap_or_default();
        let title = format!(
            " GRAPH — {} ({},{})   Esc=back  F=layout  RClick=add  Del=remove",
            if tag.is_empty() { "(tile)" } else { &tag }, gx, gy
        );
        let title: String = format!("{:<width$}", title, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, 0, &title, Color::White, Color::DarkBlue);

        // Status bar (row 1 — help / connection hint)
        let status = if self.graph_editing_param.is_some() {
            let buf = self.graph_editing_param.as_ref().map(|(_, b)| b.as_str()).unwrap_or("");
            format!(" Editing param: {}█", buf)
        } else if self.graph_connecting.is_some() {
            " Drawing wire — click an input port to connect, Esc to cancel".into()
        } else {
            " LClick=select/drag  RClick=add node  Click port>wire  F=auto-layout".into()
        };
        let status: String = format!("{:<width$}", status, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, 1, &status, Color::Black, Color::DarkGrey);

        // Palette overlay
        if let Some((px, py)) = self.graph_palette_open {
            node_graph::draw_palette(
                renderer,
                self.graph_palette_scroll,
                self.graph_palette_cursor,
                px, py, sw, sh,
            );
        }

        // Inline param edit overlay (show buffer in status bar — already done above)
    }
}

impl EditorState {
    pub(super) fn render_script_mode(&mut self, renderer: &mut crate::renderer::Renderer) {
        let sw = renderer.width;
        let sh = renderer.height;

        // Title bar
        let title = match &self.script_path {
            Some(p) => format!(" SCRIPT EDITOR — {}{}   Esc=back  Ctrl+S=save", p, if self.script_unsaved { "*" } else { "" }),
            None    => " SCRIPT EDITOR — (no file) ".to_string(),
        };
        let title: String = format!("{:<width$}", title, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, 0, &title, Color::Black, Color::Cyan);

        // Editor area
        ui::draw_script_editor(renderer, self.script_path.as_deref(), &self.script_buffer,
                               self.script_cursor, self.script_scroll, self.script_unsaved,
                               0, 1, sw, sh - 2);

        // Status bar
        let status = format!(" Line: {:<4} Col: {:<4} ", self.script_cursor.1 + 1, self.script_cursor.0 + 1);
        let status: String = format!("{:<width$}", status, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, sh - 1, &status, Color::White, Color::DarkBlue);
    }
}

// ── Main render ───────────────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn handle_render(&mut self, ctx: RenderContext) {
        let RenderContext { renderer, mouse, .. } = ctx;

        // Script editor mode
        if self.script_mode {
            self.render_script_mode(renderer);
            return;
        }

        // Graph editor mode renders its own full screen.
        if let Some((gx, gy)) = self.graph_mode {
            self.render_graph_mode(renderer, mouse, gx, gy);
            return;
        }

        // Reposition docked panels, then derive canvas bounds for the layout.
        self.panels.apply_layout(renderer.width, renderer.height);
        let (cx, cy, cw, ch) = self.panels.canvas_bounds(renderer.width, renderer.height);
        self.layout = Layout::new(renderer.width, renderer.height).with_canvas(cx, cy, cw, ch);
        let layout = self.layout.clone();

        renderer.draw_rect_filled(0, 0, renderer.width, renderer.height, ' ', Color::Reset, Color::Reset);

        ui::draw_menu_toolbar(renderer, self.active_menu, self.active_tool, &layout);


        // ── Mode resolution ──────────────────────────────────────────────────
        let grid_cursor = self.mouse_to_grid(mouse.cell_x, mouse.cell_y);

        let mode_label = if self.pasting          { Some("PASTE") }
            else if self.cutting                  { Some("CUT") }
            else if self.selecting                { Some("COPY") }
            else if self.rect_anchor.is_some()    { Some("RECT") }
            else if self.line_anchor.is_some()    { Some("LINE") }
            else                                  { None };

        // ── Inspector tile / position resolution ──────────────────────────────
        let player_tile: Option<crate::level::TileRecord> =
            if matches!(self.hierarchy_sel, Some(HierarchySelection::Player)) {
                Some(self.make_player_tile_record())
            } else {
                None
            };
        let (insp_tile, insp_pos, insp_mode_tag): (Option<&crate::level::TileRecord>, Option<(i32,i32)>, &str) =
            match self.hierarchy_sel {
                Some(HierarchySelection::Player) => {
                    let pos = Some((self.grid.spawn_point.0 as i32, self.grid.spawn_point.1 as i32));
                    (player_tile.as_ref(), pos, "PLAYER")
                }
                Some(HierarchySelection::Spawn(i)) => {
                    let pos = self.grid.extra_spawns.get(i)
                        .map(|(_, x, y)| (*x as i32, *y as i32));
                    (None, pos, "SPAWN")
                }
                None => {
                    let pos = if self.select_mode { self.selected_pos } else { self.inspected_pos };
                    let tile = pos.and_then(|(gx, gy)| self.grid.get(gx, gy, self.active_layer));
                    let tag  = if self.select_mode { "[SEL]" } else { "[EDT]" };
                    (tile, pos, tag)
                }
            };

        // ── All panels (back-to-front by z-order) ────────────────────────────
        for pid in self.panels.in_draw_order() {
            let panel = self.panels.get(pid);
            let pcy = panel.content_y();
            let pcx = panel.content_x();
            let pch = panel.content_h();
            let pcw = panel.content_w();
            draw_panel_chrome(renderer, panel);

            // Draw tabs if docked
            if panel.dock != DockSide::None {
                let docked = self.panels.get_docked_panels(panel.dock);
                if docked.len() > 1 {
                    let mut tab_info = Vec::new();
                    for id in docked {
                        tab_info.push((id, self.panels.get(id).title));
                    }
                    let active = match panel.dock {
                        DockSide::Left   => self.panels.active_left,
                        DockSide::Right  => self.panels.active_right,
                        DockSide::Bottom => self.panels.active_bottom,
                        DockSide::None   => None,
                    };
                    ui::draw_dock_tabs(renderer, panel.x as usize, panel.y as usize, panel.w, &tab_info, active);
                }
            }

            match pid {
                PanelId::Viewport => {
                    // Render Viewport content within its panel area
                    ui::draw_void(renderer, &self.grid, self.scroll, self.zoom, &layout);
                    ui::draw_level_boundary(renderer, &self.grid, self.scroll, self.zoom, &layout);
                    if self.show_grid { ui::draw_grid_overlay(renderer, &self.grid, self.scroll, self.zoom, &layout); }
                    ui::draw_grid(renderer, &self.grid, self.active_layer, self.scroll, self.zoom, &layout);
                    ui::draw_spawn_marker(renderer, self.grid.spawn_point, self.scroll, self.zoom, &layout);
                    ui::draw_extra_spawns(renderer, &self.grid.extra_spawns, self.scroll, self.zoom, &layout);

                    // ── Mode overlays ─────────────────────────────────────────────
                    if self.pasting {
                        if let Some(cursor) = grid_cursor {
                            ui::draw_paste_preview(renderer, &self.clipboard, cursor,
                                self.paste_flip_x, self.paste_flip_y, self.paste_rotate, self.scroll, self.zoom, &layout);
                        }
                    } else if self.selecting || self.cutting {
                        if let (Some(anchor), Some(current)) = (self.sel_anchor, grid_cursor) {
                            ui::draw_selection_preview(renderer, anchor, current, self.scroll, self.zoom, &layout);
                        }
                    } else if let Some(anchor) = self.rect_anchor {
                        let current = grid_cursor.unwrap_or(anchor);
                        ui::draw_rect_preview(renderer, anchor, current, self.palette.current().glyph, self.scroll, self.zoom, &layout);
                    } else if let Some(anchor) = self.line_anchor {
                        let current = grid_cursor.unwrap_or(anchor);
                        ui::draw_line_preview(renderer, anchor, current, self.palette.current().glyph, self.scroll, self.zoom, &layout);
                    } else {
                        ui::draw_cursor_highlight(renderer, mouse, &self.palette, self.select_mode, self.zoom, &layout);
                    }

                    // Physics overlay — tints solid/trigger tiles.
                    if self.show_physics {
                        ui::draw_physics_overlay(renderer, &self.grid, self.active_layer, self.scroll, self.zoom, &layout);
                    }

                    // Erase brush preview — only when right button held and brush > 1 cell.
                    if mouse.right_held() && self.erase_size > 1 {
                        if let Some(cursor) = grid_cursor {
                            ui::draw_erase_preview(renderer, cursor, self.erase_size, self.scroll, self.zoom, &layout);
                        }
                    }
                }
                PanelId::Hierarchy => {
                    ui::draw_hierarchy(renderer, &self.grid, self.hierarchy_sel, pcx, pcy, pcw, pch);
                }
                PanelId::Palette   => {
                    ui::draw_palette_panel(renderer, &self.palette, mode_label, self.palette_scroll, pcx, pcy, pcw, pch);
                }
                PanelId::Inspector => {
                    ui::draw_inspector(renderer, insp_tile, insp_pos, insp_mode_tag,
                                       pcx, pcy, pcw, pch);
                }
                PanelId::Console   => {
                    ui::draw_console(renderer, &self.console_log, pcx, pcy, pcw, pch);
                }
                PanelId::Stats     => {
                    ui::draw_stats_panel(renderer, &self.grid, &self.palette, pcx, pcy, pcw, pch);
                }
                PanelId::ScriptEditor => {
                    ui::draw_script_editor(renderer, self.script_path.as_deref(), &self.script_buffer,
                                           self.script_cursor, self.script_scroll, self.script_unsaved,
                                           pcx, pcy, pcw, pch);
                }
                PanelId::FileBrowser => {
                    ui::draw_file_browser_panel(renderer, &self.file_browser_files, self.file_browser_cursor,
                                                self.file_browser_scroll, &self.current_folder, pcx, pcy, pcw, pch);
                }
            }
        }

        // ── Modal Overlays ───────────────────────────────────────────────────
        if self.palette_editor_open {
            if let Some(pal) = self.palette.tiles.get(self.palette_editing_idx) {
                ui::draw_palette_editor_modal(renderer, pal, self.palette_editor_focus.as_ref(), &self.layout);
            }
        }

        if let Some(is_fg) = self.color_picker_open {
            ui::draw_color_picker_modal(renderer, self.color_picker_hsv, is_fg, &self.layout);
        }

        // ── Menu dropdown (drawn over panels and canvas) ──────────────────────
        if let Some(menu) = self.active_menu {
            let menu_state = MenuState {
                can_undo:       self.undo.can_undo(),
                can_redo:       self.undo.can_redo(),
                clipboard_full: !self.clipboard.is_empty(),
                show_palette:   self.panels.visible(PanelId::Palette),
                show_grid:      self.show_grid,
                show_hierarchy: self.panels.visible(PanelId::Hierarchy),
                show_inspector: self.panels.visible(PanelId::Inspector),
                show_console:   self.panels.visible(PanelId::Console),
                show_stats:     self.panels.visible(PanelId::Stats),
                show_script_editor: self.panels.visible(PanelId::ScriptEditor),
                show_file_browser:  self.panels.visible(PanelId::FileBrowser),
                show_physics:   self.show_physics,
                active_tool:    self.active_tool,
                active_layer:   self.active_layer,
            };

            ui::draw_menu_dropdown(renderer, menu, mouse.cell_x, mouse.cell_y, &menu_state, &layout);
        }

        // ── Title bar ─────────────────────────────────────────────────────────
        let full_name = match &self.project_name {
            Some(pn) => format!("{} / {}", pn, self.grid.name),
            None     => self.grid.name.clone(),
        };
        let title_name = self.save_message.as_deref().unwrap_or(&full_name);
        ui::draw_title_bar(
            renderer, title_name, self.unsaved,
            self.undo.len(), self.undo.redo_len(),
            self.scroll, (self.grid.width, self.grid.height),
        );

        // ── Status / text input ───────────────────────────────────────────────
        let tile_under = grid_cursor.and_then(|(gx, gy)| self.grid.get(gx, gy, self.active_layer));

        let mode_hint = if self.select_mode {
            "SELECT mode: click canvas to inspect tile  Q=exit select".to_string()
        } else if self.pasting {
            "PASTE H=flipX J=flipY []=rotate  click=stamp  Esc=cancel".to_string()
        } else if self.cutting {
            "CUT: drag to select, Esc=cancel".to_string()
        } else if self.selecting {
            "COPY: drag to select, Esc=cancel".to_string()
        } else if self.rect_anchor.is_some() {
            format!("RECT: {} — release to fill", self.palette.current().name)
        } else if let Some(anchor) = self.line_anchor {
            format!("LINE from ({},{}) — press L to stamp", anchor.0, anchor.1)
        } else {
            String::new()
        };

        ui::draw_status_bar(
            renderer, mouse, &self.palette, self.show_grid,
            &self.save_path, tile_under, &mode_hint,
            self.scroll, self.active_layer, self.erase_size, &layout,
        );

        if let Some(ref ti) = self.text_input {
            let resize_hint = format!("New size WxH (current {}x{})", self.grid.width, self.grid.height);
            let prompt = match &ti.purpose {
                TextInputPurpose::LevelName         => "Level name",
                TextInputPurpose::SaveAs            => "Save as",
                TextInputPurpose::ScriptPath { .. }    => "Script path",
                TextInputPurpose::TileNextLevel { .. } => "Exit level path",
                TextInputPurpose::TileTag { .. }       => "Tile tag",
                TextInputPurpose::TileGlyph { .. }  => "Glyph char",
                TextInputPurpose::NamedSpawn        => "Spawn name",
                TextInputPurpose::ResizeLevel       => resize_hint.as_str(),
                TextInputPurpose::PlayerTag         => "Player tag",
                TextInputPurpose::PlayerScript      => "Player script",
                TextInputPurpose::PlayerGlyph       => "Player glyph",
                TextInputPurpose::NewLevelName      => "New level name",
                TextInputPurpose::PaletteName       => "Palette item name",
                TextInputPurpose::TileColliderLayer { .. } => "Collider layer",
                TextInputPurpose::TileColliderMask  { .. } => "Mask (comma-separated, empty=all)",
                TextInputPurpose::PlayerColliderLayer      => "Player layer",
                TextInputPurpose::PlayerColliderMask       => "Player mask (comma-separated)",
                TextInputPurpose::NewScriptName            => "New script name (e.g. ai.rhai)",
                TextInputPurpose::PaletteFgCustom          => "Custom FG Hex (e.g. #FF8C00)",
                TextInputPurpose::PaletteBgCustom          => "Custom BG Hex (e.g. #222222)",
            };
            ui::draw_text_input(renderer, prompt, &ti.buffer, &layout);
        }

        // Help screen overlay.
        if self.show_help {
            ui::draw_help_overlay(renderer, &layout);
        }

        if let Some(ref m) = self.modal {
            ui::draw_confirm_modal(renderer, &m.title, &m.message, &layout);
        }

        if let Some(ref cm) = self.context_menu {
            ui::draw_context_menu(renderer, cm);
        }
    }
}
