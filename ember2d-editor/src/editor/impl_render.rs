// editor/impl_render.rs — Rendering logic for EditorState.

use ember2d::engine::RenderContext;
use ember2d::renderer::color::Color;
use ember2d::renderer::DrawSurface;

use super::graph_ui;
use super::panel::{draw_panel_chrome, DockSide, PanelId};
use super::ui::{self, HierarchySelection, MenuState, ToolKind};
use super::EditorMode;
use super::EditorState;
use super::TextInputPurpose;

// ── Graph editor rendering ────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn render_graph_mode(
        &mut self,
        renderer: &mut dyn DrawSurface,
        mouse: &ember2d::mouse::MouseState,
        gx: i32,
        gy: i32,
    ) {
        let sw = renderer.width();
        let sh = renderer.height();

        // Resolve graph reference
        let graph = match self.grid.get(gx, gy, self.active_layer).and_then(|t| t.graph.as_ref()) {
            Some(g) => g.clone(),
            None => {
                renderer.draw_str(0, 0, "No graph", Color::Red, Color::Black);
                return;
            }
        };

        graph_ui::draw_graph(
            renderer,
            self.font.as_mut(),
            &graph,
            self.graph_selected_node,
            self.graph_connecting,
            mouse.cell_x,
            mouse.cell_y,
            self.graph_view_ox,
            self.graph_view_oy,
            sw,
            sh,
        );

        // Title bar (row 0)
        let tag =
            self.grid.get(gx, gy, self.active_layer).map(|t| t.tag.clone()).unwrap_or_default();
        let title = format!(
            " GRAPH — {} ({},{})   Esc=back  F=layout  RClick=add  Del=remove",
            if tag.is_empty() { "(tile)" } else { &tag },
            gx,
            gy
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
            graph_ui::draw_palette(
                renderer,
                self.graph_palette_scroll,
                self.graph_palette_cursor,
                px,
                py,
                sw,
                sh,
                &mut self.ui_frame,
            );
        }

        // Inline param edit overlay (show buffer in status bar — already done above)
    }
}

impl EditorState {
    pub(super) fn render_script_mode(&mut self, renderer: &mut dyn DrawSurface) {
        let sw = renderer.width();
        let sh = renderer.height();

        // Title bar
        let title = match &self.script_path {
            Some(p) => format!(
                " SCRIPT EDITOR — {}{}   Esc=back  Ctrl+S=save",
                p,
                if self.script_unsaved { "*" } else { "" }
            ),
            None => " SCRIPT EDITOR — (no file) ".to_string(),
        };
        let title: String = format!("{:<width$}", title, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, 0, &title, Color::Black, Color::Cyan);

        // Editor area
        ui::draw_script_editor(
            renderer,
            &self.theme,
            self.script_path.as_deref(),
            &self.script_buffer,
            self.script_cursor,
            self.script_scroll,
            self.script_hscroll,
            self.script_unsaved,
            0,
            1,
            sw,
            sh - 2,
            self.script_error(),
            self.script_selection(),
            self.script_find_active.then_some(self.script_find_query.as_str()),
        );

        // Status bar
        let status =
            format!(" Line: {:<4} Col: {:<4} ", self.script_cursor.1 + 1, self.script_cursor.0 + 1);
        let status: String = format!("{:<width$}", status, width = sw).chars().take(sw).collect();
        renderer.draw_str(0, sh - 1, &status, Color::White, Color::DarkBlue);
    }
}

// ── Main render ───────────────────────────────────────────────────────────────

impl EditorState {
    pub(super) fn handle_render(&mut self, ctx: RenderContext) {
        let RenderContext { renderer, mouse, .. } = ctx;
        self.draw(renderer, mouse);
    }

    /// The real drawing body — was `handle_render`'s own, taking the full
    /// `RenderContext` directly. Split out (7C-5, master plan §5.3) so it
    /// can run against any `DrawSurface`, not just a concrete `Renderer`:
    /// `EditorHarness` (`ember2d-editor/tests/common/mod.rs`) calls this
    /// with a `NullRenderer` to populate `self.ui_frame` headlessly — the
    /// same `UiFrame::push` calls 7C-1 already made part of drawing itself
    /// run regardless of what actually consumes the draw calls. The real
    /// `GameState::render`/`handle_render` path above still always passes
    /// a concrete `Renderer`, auto-coerced to `&mut dyn DrawSurface` at
    /// this call site — no change to how the live app renders.
    pub fn draw(&mut self, renderer: &mut dyn DrawSurface, mouse: &ember2d::mouse::MouseState) {
        // Phase 7 Part 1d (docs/ember2d-phase7-plan.md): fresh every render
        // pass, unconditionally — see `ui_frame`'s own doc comment and
        // `ui/frame.rs`'s header comment for the one-frame lag this implies
        // and why it's harmless.
        self.ui_frame.clear();

        // Script editor mode
        if matches!(self.mode, EditorMode::Script) {
            self.render_script_mode(renderer);
            return;
        }

        // Graph editor mode renders its own full screen.
        if let EditorMode::Graph { gx, gy } = &self.mode {
            let (gx, gy) = (*gx, *gy);
            self.render_graph_mode(renderer, mouse, gx, gy);
            return;
        }

        // Reposition docked panels — every `ui::draw_*` function below and
        // `mouse_to_grid` (impl_state.rs) read the Viewport panel's own
        // rect directly now (7C-3, master plan §5.3, E4). `apply_layout`
        // works in pixels (Phase 7 Part 1c, docs/ember2d-phase7-plan.md) —
        // `pixel_width`/`pixel_height`, not the cell-count `width`/`height`
        // some calls below still use for whole-screen (not viewport)
        // sizing. `viewport` is the panel's CONTENT rect (inside its
        // border/title bar) — what `Layout.canvas_x`/`y`/`w`/`h` used to
        // mean, now read from the one place that actually knows it.
        self.panels.apply_layout(renderer.pixel_width(), renderer.pixel_height());
        let viewport = self.panels.viewport().content_rect();
        let (screen_w, screen_h) = (renderer.width(), renderer.height());

        renderer.draw_rect_filled(
            0,
            0,
            renderer.width(),
            renderer.height(),
            ' ',
            Color::Reset,
            Color::Reset,
        );

        ui::draw_menu_toolbar(
            renderer,
            self.font.as_mut(),
            &self.theme,
            self.active_menu,
            self.mode.toolbar_label(),
            &mut self.ui_frame,
        );

        // ── Mode resolution ──────────────────────────────────────────────────
        let grid_cursor = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y);

        let mode_label = match self.mode {
            EditorMode::Paste => Some("PASTE"),
            EditorMode::Select { cutting: true, .. } => Some("CUT"),
            EditorMode::Select { cutting: false, .. } => Some("COPY"),
            _ if self.rect_anchor.is_some() => Some("RECT"),
            _ if self.line_anchor.is_some() => Some("LINE"),
            _ => None,
        };

        // ── Inspector tile / position resolution ──────────────────────────────
        let player_tile: Option<ember2d_sim::level::TileRecord> =
            if matches!(self.hierarchy_sel, Some(HierarchySelection::Player)) {
                Some(self.make_player_tile_record())
            } else {
                None
            };
        let (insp_tile, insp_pos, insp_mode_tag): (
            Option<&ember2d_sim::level::TileRecord>,
            Option<(i32, i32)>,
            &str,
        ) = match self.hierarchy_sel {
            Some(HierarchySelection::Player) => {
                let pos = Some((self.grid.spawn_point.0 as i32, self.grid.spawn_point.1 as i32));
                (player_tile.as_ref(), pos, "PLAYER")
            }
            Some(HierarchySelection::Spawn(i)) => {
                let pos = self.grid.extra_spawns.get(i).map(|(_, x, y)| (*x as i32, *y as i32));
                (None, pos, "SPAWN")
            }
            None => {
                let inspecting = matches!(self.mode, EditorMode::Inspect);
                let pos = if inspecting { self.selected_pos } else { self.inspected_pos };
                let tile = pos.and_then(|(gx, gy)| self.grid.get(gx, gy, self.active_layer));
                let tag = if inspecting { "[SEL]" } else { "[EDT]" };
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
            draw_panel_chrome(
                renderer,
                panel,
                &mut self.ui_frame,
                &self.theme,
                &self.theme_chrome_tex,
                self.font.as_mut(),
            );

            // Draw tabs if docked
            if panel.dock != DockSide::None {
                let docked = self.panels.get_docked_panels(panel.dock);
                if docked.len() > 1 {
                    let mut tab_info = Vec::new();
                    for id in docked {
                        tab_info.push((id, self.panels.get(id).title));
                    }
                    let active = match panel.dock {
                        DockSide::Left => self.panels.active_left,
                        DockSide::Right => self.panels.active_right,
                        DockSide::Bottom => self.panels.active_bottom,
                        DockSide::None => None,
                    };
                    ui::draw_dock_tabs(
                        renderer,
                        self.font.as_mut(),
                        &self.theme,
                        panel.cell_x().max(0) as usize,
                        panel.cell_y().max(0) as usize,
                        panel.cell_w(),
                        &tab_info,
                        active,
                        &mut self.ui_frame,
                    );
                }
            }

            match pid {
                PanelId::Viewport => {
                    // ── Set Hardware Scissor ─────────────────────────────────────
                    // 7C-2 (master plan §5.3, E2): was a hardcoded `* 8`/
                    // `* 16` pair duplicating CELL_W/CELL_H, the same class
                    // of literal 7B-2 already replaced everywhere else this
                    // conversion happens (`UiRect::from_cells`,
                    // `backend.rs`'s own draw calls).
                    let (sc_x, sc_y, sc_w, sc_h) = (
                        (pcx * ember2d::renderer::CELL_W) as u32,
                        (pcy * ember2d::renderer::CELL_H) as u32,
                        (pcw * ember2d::renderer::CELL_W) as u32,
                        (pch * ember2d::renderer::CELL_H) as u32,
                    );
                    renderer.set_scissor(Some((sc_x, sc_y, sc_w, sc_h)));

                    // Render Viewport content within its panel area
                    ui::draw_void(renderer, &self.grid, self.scroll, self.zoom, viewport);
                    ui::draw_level_boundary(renderer, &self.grid, self.scroll, self.zoom, viewport);
                    if self.show_grid {
                        ui::draw_grid_overlay(
                            renderer,
                            &self.grid,
                            self.scroll,
                            self.zoom,
                            viewport,
                        );
                    }
                    ui::draw_grid(
                        renderer,
                        &self.grid,
                        self.active_layer,
                        self.scroll,
                        self.zoom,
                        viewport,
                    );
                    ui::draw_spawn_marker(
                        renderer,
                        self.grid.spawn_point,
                        self.scroll,
                        self.zoom,
                        viewport,
                    );
                    ui::draw_extra_spawns(
                        renderer,
                        &self.grid.extra_spawns,
                        self.scroll,
                        self.zoom,
                        viewport,
                    );

                    // ── Mode overlays ─────────────────────────────────────────────
                    let sel_anchor =
                        if let EditorMode::Select { start, .. } = &self.mode { *start } else { None };
                    if matches!(self.mode, EditorMode::Paste) {
                        if let Some(cursor) = grid_cursor {
                            ui::draw_paste_preview(
                                renderer,
                                &self.clipboard,
                                cursor,
                                self.paste_flip_x,
                                self.paste_flip_y,
                                self.paste_rotate,
                                self.scroll,
                                self.zoom,
                                viewport,
                            );
                        }
                    } else if matches!(self.mode, EditorMode::Select { .. }) {
                        if let (Some(anchor), Some(current)) = (sel_anchor, grid_cursor) {
                            ui::draw_selection_preview(
                                renderer,
                                anchor,
                                current,
                                self.scroll,
                                self.zoom,
                                viewport,
                            );
                        }
                    } else if let Some(anchor) = self.rect_anchor {
                        let current = grid_cursor.unwrap_or(anchor);
                        ui::draw_rect_preview(
                            renderer,
                            anchor,
                            current,
                            self.palette.current().glyph,
                            self.scroll,
                            self.zoom,
                            viewport,
                        );
                    } else if let Some(anchor) = self.line_anchor {
                        let current = grid_cursor.unwrap_or(anchor);
                        ui::draw_line_preview(
                            renderer,
                            anchor,
                            current,
                            self.palette.current().glyph,
                            self.scroll,
                            self.zoom,
                            viewport,
                        );
                    } else {
                        ui::draw_cursor_highlight(
                            renderer,
                            mouse,
                            &self.palette,
                            matches!(self.mode, EditorMode::Inspect),
                            self.scroll,
                            self.zoom,
                            viewport,
                        );
                    }

                    // Physics overlay — tints solid/trigger tiles.
                    if self.show_physics {
                        ui::draw_physics_overlay(
                            renderer,
                            &self.grid,
                            self.active_layer,
                            self.scroll,
                            self.zoom,
                            viewport,
                        );
                    }

                    // Erase brush preview — only when right button held and brush > 1 cell.
                    if mouse.right_held() && self.erase_size > 1 {
                        if let Some(cursor) = grid_cursor {
                            ui::draw_erase_preview(
                                renderer,
                                cursor,
                                self.erase_size,
                                self.scroll,
                                self.zoom,
                                viewport,
                            );
                        }
                    }

                    // Reset Scissor
                    renderer.set_scissor(None);
                }
                PanelId::Hierarchy => {
                    ui::draw_hierarchy(
                        renderer,
                        &self.theme,
                        &self.grid,
                        self.hierarchy_sel,
                        pcx,
                        pcy,
                        pcw,
                        pch,
                        &mut self.ui_frame,
                    );
                }
                PanelId::Palette => {
                    ui::draw_palette_panel(
                        renderer,
                        self.font.as_mut(),
                        &self.theme,
                        &self.palette,
                        mode_label,
                        self.palette_scroll,
                        pcx,
                        pcy,
                        pcw,
                        pch,
                        &mut self.ui_frame,
                    );
                }
                PanelId::Inspector => {
                    ui::draw_inspector(
                        renderer,
                        &self.theme,
                        insp_tile,
                        insp_pos,
                        insp_mode_tag,
                        pcx,
                        pcy,
                        pcw,
                        pch,
                        &mut self.ui_frame,
                    );
                }
                PanelId::Console => {
                    ui::draw_console(renderer, &self.theme, &self.console_log, pcx, pcy, pcw, pch);
                }
                PanelId::Stats => {
                    ui::draw_stats_panel(renderer, &self.theme, &self.grid, &self.palette, pcx, pcy, pcw, pch);
                }
                PanelId::ScriptEditor => {
                    ui::draw_script_editor(
                        renderer,
                        &self.theme,
                        self.script_path.as_deref(),
                        &self.script_buffer,
                        self.script_cursor,
                        self.script_scroll,
                        self.script_hscroll,
                        self.script_unsaved,
                        pcx,
                        pcy,
                        pcw,
                        pch,
                        self.script_error(),
                        self.script_selection(),
                        self.script_find_active.then_some(self.script_find_query.as_str()),
                    );
                }
                PanelId::FileBrowser => {
                    ui::draw_file_browser_panel(
                        renderer,
                        &self.theme,
                        &self.file_browser_files,
                        self.file_browser_cursor,
                        self.file_browser_scroll,
                        &self.current_folder,
                        pcx,
                        pcy,
                        pcw,
                        pch,
                        &mut self.ui_frame,
                    );
                }
            }
        }

        // ── Modal Overlays ───────────────────────────────────────────────────
        if matches!(self.mode, EditorMode::PaletteEditor | EditorMode::ColorPicker { .. }) {
            if let Some(pal) = self.palette.tiles.get(self.palette_editing_idx) {
                ui::draw_palette_editor_modal(
                    renderer,
                    &self.theme,
                    &self.theme_chrome_tex,
                    pal,
                    self.palette_editor_focus.as_ref(),
                    screen_w,
                    screen_h,
                    &mut self.ui_frame,
                );
            }
        }

        if let EditorMode::ColorPicker { is_fg } = &self.mode {
            let is_fg = *is_fg;
            ui::draw_color_picker_modal(
                renderer,
                &self.theme,
                &self.theme_chrome_tex,
                self.color_picker_hsv,
                is_fg,
                screen_w,
                screen_h,
                &mut self.ui_frame,
            );
        }

        // ── Menu dropdown (drawn over panels and canvas) ──────────────────────
        if let Some(menu) = self.active_menu {
            let menu_state = MenuState {
                can_undo: self.undo.can_undo(),
                can_redo: self.undo.can_redo(),
                clipboard_full: !self.clipboard.is_empty(),
                show_palette: self.panels.visible(PanelId::Palette),
                show_grid: self.show_grid,
                show_hierarchy: self.panels.visible(PanelId::Hierarchy),
                show_inspector: self.panels.visible(PanelId::Inspector),
                show_console: self.panels.visible(PanelId::Console),
                show_stats: self.panels.visible(PanelId::Stats),
                show_script_editor: self.panels.visible(PanelId::ScriptEditor),
                show_file_browser: self.panels.visible(PanelId::FileBrowser),
                show_physics: self.show_physics,
                active_tool: match &self.mode {
                    EditorMode::Paint(t) => *t,
                    _ => ToolKind::Paint,
                },
                inspecting: matches!(self.mode, EditorMode::Inspect),
                copying: matches!(self.mode, EditorMode::Select { cutting: false, .. }),
                cutting: matches!(self.mode, EditorMode::Select { cutting: true, .. }),
                pasting: matches!(self.mode, EditorMode::Paste),
                active_layer: self.active_layer,
            };

            ui::draw_menu_dropdown(
                renderer,
                &self.theme,
                menu,
                mouse.cell_x,
                mouse.cell_y,
                &menu_state,
                &mut self.ui_frame,
            );
        }

        // ── Title bar ─────────────────────────────────────────────────────────
        let full_name = match &self.project_name {
            Some(pn) => format!("{} / {}", pn, self.grid.name),
            None => self.grid.name.clone(),
        };
        let title_name = self.save_message.as_deref().unwrap_or(&full_name);
        ui::draw_title_bar(
            renderer,
            self.font.as_mut(),
            &self.theme,
            title_name,
            self.unsaved,
            self.undo.len(),
            self.undo.redo_len(),
            self.scroll,
            (self.grid.width, self.grid.height),
        );

        // ── Status / text input ───────────────────────────────────────────────
        let tile_under = grid_cursor.and_then(|(gx, gy)| self.grid.get(gx, gy, self.active_layer));

        let mode_hint = if matches!(self.mode, EditorMode::Inspect) {
            "SELECT mode: click canvas to inspect tile  Q=exit select".to_string()
        } else if matches!(self.mode, EditorMode::Paste) {
            "PASTE H=flipX J=flipY []=rotate  click=stamp  Esc=cancel".to_string()
        } else if matches!(self.mode, EditorMode::Select { cutting: true, .. }) {
            "CUT: drag to select, Esc=cancel".to_string()
        } else if matches!(self.mode, EditorMode::Select { cutting: false, .. }) {
            "COPY: drag to select, Esc=cancel".to_string()
        } else if self.rect_anchor.is_some() {
            format!("RECT: {} — release to fill", self.palette.current().name)
        } else if let Some(anchor) = self.line_anchor {
            format!("LINE from ({},{}) — press L to stamp", anchor.0, anchor.1)
        } else {
            String::new()
        };

        ui::draw_status_bar(
            renderer,
            &self.theme,
            mouse,
            &self.palette,
            self.show_grid,
            &self.save_path,
            tile_under,
            &mode_hint,
            self.scroll,
            self.active_layer,
            self.erase_size,
            self.panels.viewport().content_x(),
            self.panels.viewport().content_y(),
            self.zoom,
        );

        if let EditorMode::Prompt(purpose) = &self.mode {
            let resize_hint =
                format!("New size WxH (current {}x{})", self.grid.width, self.grid.height);
            let prompt = match purpose {
                TextInputPurpose::LevelName => "Level name",
                TextInputPurpose::SaveAs => "Save as",
                TextInputPurpose::ScriptPath { .. } => "Script path",
                TextInputPurpose::TileNextLevel { .. } => "Exit level path",
                TextInputPurpose::TileTag { .. } => "Tile tag",
                TextInputPurpose::TileGlyph { .. } => "Glyph char",
                TextInputPurpose::NamedSpawn => "Spawn name",
                TextInputPurpose::ResizeLevel => resize_hint.as_str(),
                TextInputPurpose::PlayerTag => "Player tag",
                TextInputPurpose::PlayerScript => "Player script",
                TextInputPurpose::PlayerGlyph => "Player glyph",
                TextInputPurpose::NewLevelName => "New level name",
                TextInputPurpose::PaletteName => "Palette item name",
                TextInputPurpose::TileColliderLayer { .. } => "Collider layer",
                TextInputPurpose::TileColliderMask { .. } => "Mask (comma-separated, empty=all)",
                TextInputPurpose::PlayerColliderLayer => "Player layer",
                TextInputPurpose::PlayerColliderMask => "Player mask (comma-separated)",
                TextInputPurpose::NewScriptName => "New script name (e.g. ai.rhai)",
                TextInputPurpose::PaletteFgCustom => "Custom FG Hex (e.g. #FF8C00)",
                TextInputPurpose::PaletteBgCustom => "Custom BG Hex (e.g. #222222)",
            };
            ui::draw_text_input(
                renderer,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                prompt,
                &self.prompt_buffer,
                screen_w,
                screen_h,
            );
        }

        // Help screen overlay.
        if self.show_help {
            let vp = self.panels.viewport();
            ui::draw_help_overlay(
                renderer,
                self.font.as_mut(),
                &self.theme,
                vp.content_x(),
                vp.content_y(),
                vp.content_w(),
                vp.content_h(),
            );
        }

        if let EditorMode::Modal(m) = &self.mode {
            ui::draw_confirm_modal(
                renderer,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                &m.title,
                &m.message,
                screen_w,
                screen_h,
                &mut self.ui_frame,
            );
        }

        if let EditorMode::ContextMenu(cm) = &self.mode {
            ui::draw_context_menu(renderer, &self.theme, &self.theme_chrome_tex, cm, &mut self.ui_frame);
        }
    }
}
