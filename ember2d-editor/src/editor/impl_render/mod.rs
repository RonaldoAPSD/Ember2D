// editor/impl_render/mod.rs — Rendering logic for EditorState. Full-screen
// mode rendering (graph/fullscreen-script) split into `modes.rs` (7D-3
// checkpoint 5, docs/ember2d-master-plan.md §5.4) purely to keep this file
// under CLAUDE.md's 750-line hard limit — no behavioral change from the
// split.

use ember2d::engine::RenderContext;
use ember2d::renderer::color::Color;
use ember2d::renderer::{DrawSurface, UiPainter, UiSpace};

use super::panel::{draw_panel_chrome, DockSide, PanelId};
use super::ui::{self, HierarchySelection, MenuState, ToolKind};
use super::EditorMode;
use super::EditorState;
use super::TextInputPurpose;

mod modes;

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

        // 7D-3 (docs/ember2d-master-plan.md §5.4): captured before ANY mode
        // dispatch (including the early-return script/graph paths just
        // below) — every mode needs its own `UiSpace` snapshot ready for
        // the FOLLOWING frame's chrome hit-testing (`ui_space`'s own doc
        // comment, same one-frame-lag contract `ui_frame` already keeps),
        // not just Paint mode. Chrome doesn't actually draw through
        // `UiSpace`/`UiPainter` yet (later checkpoints of this same step)
        // — this only keeps the value itself correct and current so those
        // checkpoints have nothing left to wire up here.
        let display = renderer.display_scale();
        let ui_scale = self.effective_ui_scale(display);
        self.rebuild_fonts_if_scale_changed(ui_scale);
        self.ui_space = UiSpace::from_surface(renderer, ui_scale);

        // R69 (§3 in the master plan): `apply_layout` now runs BEFORE the
        // script/graph mode early-returns below, not after — those modes
        // used to leave `PanelManager`'s own screen size stale (a resize
        // while in fullscreen script/graph mode didn't take effect until
        // the user returned to Paint mode and back), since this call used
        // to sit after both `return`s. `metrics` (7D-3, §5.4) is a
        // `ChromeMetrics` built fresh from the CURRENT theme every frame —
        // see that type's own doc comment for why nothing here keeps a
        // stale copy across a theme switch.
        let metrics = ui::ChromeMetrics::from_theme(&self.theme);
        // 7D-3 checkpoint 7: panels are positioned in POINTS (since
        // checkpoint 3) — `apply_layout`'s own screen size must be the
        // POINTS-space screen (`self.ui_space.screen_pt()`), not
        // `renderer.pixel_width()/height()`'s raw LOGICAL size. The two
        // only ever agreed by coincidence while `ui_scale` stayed pinned
        // to `render_scale` (every earlier checkpoint of this step); once
        // a real preference can differ from the display's own scale, this
        // is the one place that difference would silently misplace every
        // panel if it read the wrong screen size.
        let (screen_pt_w, screen_pt_h) = self.ui_space.screen_pt();
        self.panels.apply_layout(screen_pt_w, screen_pt_h, &metrics);

        // Script editor mode
        if matches!(self.mode, EditorMode::Script) {
            self.render_script_mode(renderer, &metrics);
            return;
        }

        // Graph editor mode renders its own full screen.
        if let EditorMode::Graph { gx, gy } = &self.mode {
            let (gx, gy) = (*gx, *gy);
            self.render_graph_mode(renderer, mouse, gx, gy);
            return;
        }

        // `viewport` is the Viewport panel's CONTENT rect (inside its
        // border/title bar), in POINTS — what `Layout.canvas_x`/`y`/`w`/`h`
        // used to mean, now read from the one place that actually knows it
        // (7C-3, master plan §5.3, E4). Every `ui::draw_*` function below
        // and `mouse_to_grid` (`impl_state/viewport.rs`) read this SAME
        // value — see the Viewport scissor rect below for why that single
        // source of truth is what fixes R66-A. Converted to LOGICAL pixels
        // (`viewport_logical`) for the canvas draw calls specifically —
        // the canvas/viewport stays on the engine's own fixed-cell,
        // `render_scale`-only grid forever (7C-9 decision gate, §7.1),
        // never `ui_scale`.
        let viewport = self.panels.viewport().content_rect(&metrics);
        let vl = self.ui_space.rect_to_logical(viewport.into());
        let viewport_logical = ui::UiRect::new(vl.x, vl.y, vl.w, vl.h);
        // 7D-3 checkpoint 7: modals (`draw_text_input`/`draw_confirm_modal`/
        // `draw_palette_editor_modal`/`draw_color_picker_modal`) center
        // themselves against the POINTS-space screen size, same reasoning
        // as `apply_layout`'s own `screen_pt_w`/`screen_pt_h` above — they
        // size themselves in points, so they must center in points.
        let (screen_w, screen_h) = (screen_pt_w, screen_pt_h);

        renderer.draw_rect_filled(
            0,
            0,
            renderer.width(),
            renderer.height(),
            ' ',
            Color::Reset,
            Color::Reset,
        );

        // 7D-3 checkpoint 7 (master plan §5.4): every chrome draw call from
        // here on goes through one `UiPainter`, built from this frame's own
        // `self.ui_space` — the choke point that actually applies
        // `ui_scale` (`painter.surface()` is the escape hatch back to the
        // raw `renderer` for the viewport/canvas calls, which stay outside
        // `UiPainter` entirely per the 7C-9 decision gate).
        let mut painter = UiPainter::new(renderer, self.ui_space);

        ui::draw_menu_toolbar(
            &mut painter,
            self.font.as_mut(),
            &self.theme,
            &metrics,
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
            draw_panel_chrome(
                &mut painter,
                panel,
                &mut self.ui_frame,
                &self.theme,
                &self.theme_chrome_tex,
                self.font.as_mut(),
                &metrics,
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
                    // `metrics.bar_h` (7D-3, docs/ember2d-master-plan.md
                    // §5.4 — was a hardcoded `CELL_H`) — this strip sits
                    // exactly where `draw_panel_chrome`'s own title-bar row
                    // does (`panel/chrome.rs`), through the same
                    // `ChromeMetrics` value, so the two can't drift.
                    let strip = ember2d_sim::math::Rect::new(
                        panel.rect.x,
                        panel.rect.y,
                        panel.rect.w,
                        metrics.bar_h,
                    );
                    ui::draw_dock_tabs(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        strip,
                        &tab_info,
                        active,
                        &mut self.ui_frame,
                    );
                }
            }

            match pid {
                PanelId::Viewport => {
                    // ── Set Hardware Scissor ─────────────────────────────────────
                    // R66-A (§3 in the master plan): was built from the
                    // CELL-ROUNDED `pcx`/`pcy`/`pcw`/`pch` bridge (`* 8`/
                    // `* 16`) — an independent re-derivation of the
                    // viewport rect that could disagree with `viewport`
                    // (used by every canvas draw call below) by up to a
                    // cell after a sub-cell panel resize, since drag/resize
                    // never snapped to whole cells. Built from `viewport`
                    // directly now (through `painter.clip`, which converts
                    // the same points rect to logical the same way
                    // `draw_panel_chrome` did) — the exact same value the
                    // canvas itself draws from, so scissor and content can
                    // never clip against a different rect than what's drawn.
                    painter.clip(Some(viewport.into()));
                    let renderer = painter.surface();

                    // Render Viewport content within its panel area — the
                    // canvas/viewport stays on the engine's own fixed-cell
                    // grid forever (7C-9 decision gate, §7.1), so this uses
                    // `viewport_logical`, not the points-space `viewport`
                    // above, and the raw `renderer` escape hatch, not
                    // `painter`.
                    ui::draw_void(renderer, &self.grid, self.scroll, self.zoom, viewport_logical);
                    ui::draw_level_boundary(
                        renderer,
                        &self.grid,
                        self.scroll,
                        self.zoom,
                        viewport_logical,
                    );
                    if self.show_grid {
                        ui::draw_grid_overlay(
                            renderer,
                            &self.grid,
                            self.scroll,
                            self.zoom,
                            viewport_logical,
                        );
                    }
                    ui::draw_grid(
                        renderer,
                        &self.grid,
                        self.active_layer,
                        self.scroll,
                        self.zoom,
                        viewport_logical,
                    );
                    ui::draw_spawn_marker(
                        renderer,
                        self.grid.spawn_point,
                        self.scroll,
                        self.zoom,
                        viewport_logical,
                    );
                    ui::draw_extra_spawns(
                        renderer,
                        &self.grid.extra_spawns,
                        self.scroll,
                        self.zoom,
                        viewport_logical,
                    );

                    // ── Mode overlays ─────────────────────────────────────────────
                    let sel_anchor = if let EditorMode::Select { start, .. } = &self.mode {
                        *start
                    } else {
                        None
                    };
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
                                viewport_logical,
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
                                viewport_logical,
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
                            viewport_logical,
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
                            viewport_logical,
                        );
                    } else {
                        ui::draw_cursor_highlight(
                            renderer,
                            mouse,
                            &self.palette,
                            matches!(self.mode, EditorMode::Inspect),
                            self.scroll,
                            self.zoom,
                            viewport_logical,
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
                            viewport_logical,
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
                                viewport_logical,
                            );
                        }
                    }

                    // Reset Scissor
                    painter.clip(None);
                }
                PanelId::Hierarchy => {
                    ui::draw_hierarchy(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        &self.grid,
                        self.hierarchy_sel,
                        panel.content_rect(&metrics).into(),
                        &mut self.ui_frame,
                    );
                }
                PanelId::Palette => {
                    ui::draw_palette_panel(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        &self.palette,
                        mode_label,
                        self.palette_scroll,
                        panel.content_rect(&metrics).into(),
                        &mut self.ui_frame,
                    );
                }
                PanelId::Inspector => {
                    ui::draw_inspector(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        insp_tile,
                        insp_pos,
                        insp_mode_tag,
                        panel.content_rect(&metrics).into(),
                        &mut self.ui_frame,
                    );
                }
                PanelId::Console => {
                    ui::draw_console(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        &self.console_log,
                        panel.content_rect(&metrics).into(),
                    );
                }
                PanelId::Stats => {
                    ui::draw_stats_panel(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        &self.grid,
                        &self.palette,
                        panel.content_rect(&metrics).into(),
                    );
                }
                PanelId::ScriptEditor => {
                    let script_error =
                        self.script_error().map(|(line, msg)| (line, msg.to_string()));
                    let script_selection = self.script_selection();
                    ui::draw_script_editor(
                        &mut painter,
                        self.code_font.as_mut(),
                        &self.theme,
                        self.script_path.as_deref(),
                        &self.script_buffer,
                        self.script_cursor,
                        self.script_scroll,
                        self.script_hscroll,
                        self.script_unsaved,
                        panel.content_rect(&metrics).into(),
                        script_error.as_ref().map(|(line, msg)| (*line, msg.as_str())),
                        script_selection,
                        self.script_find_active.then_some(self.script_find_query.as_str()),
                    );
                }
                PanelId::FileBrowser => {
                    ui::draw_file_browser_panel(
                        &mut painter,
                        self.font.as_mut(),
                        &self.theme,
                        &self.file_browser_files,
                        self.file_browser_cursor,
                        self.file_browser_scroll,
                        &self.current_folder,
                        panel.content_rect(&metrics).into(),
                        &mut self.ui_frame,
                    );
                }
            }
        }

        // ── Modal Overlays ───────────────────────────────────────────────────
        if matches!(self.mode, EditorMode::PaletteEditor | EditorMode::ColorPicker { .. }) {
            if let Some(pal) = self.palette.tiles.get(self.palette_editing_idx) {
                ui::draw_palette_editor_modal(
                    &mut painter,
                    self.font.as_mut(),
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
                &mut painter,
                self.font.as_mut(),
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
                current_theme: self.theme.name.clone(),
                current_ui_scale: self.prefs.ui_scale,
            };

            ui::draw_menu_dropdown(
                &mut painter,
                self.font.as_mut(),
                &self.theme,
                &metrics,
                menu,
                &self.available_themes,
                mouse.pixel_x,
                mouse.pixel_y,
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
            &mut painter,
            self.font.as_mut(),
            &self.theme,
            &metrics,
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
            &mut painter,
            self.font.as_mut(),
            &self.theme,
            &metrics,
            mouse,
            &self.palette,
            self.show_grid,
            &self.save_path,
            tile_under,
            &mode_hint,
            self.scroll,
            self.active_layer,
            self.erase_size,
            // R88 (§3 in the master plan): `vl`/`viewport_logical` — the
            // LOGICAL-pixel conversion of `viewport` via
            // `self.ui_space.rect_to_logical` (computed just above,
            // "Converted to LOGICAL pixels" comment) — not the raw
            // POINTS-space `viewport.x/y` this used to pass. `mouse.pixel_x
            // /y` (what `draw_status_bar` subtracts this from, chrome.rs)
            // is logical, per the 7C-9 decision gate the comment there
            // cites; passing points mixed two different units whenever
            // `ui_scale != render_scale`, silently scaling the readout by
            // their ratio — invisible at the common `ui_scale ==
            // render_scale` case (points and logical coincide there), which
            // is why this survived undetected. `mouse_to_grid`
            // (`impl_state/viewport.rs`) already did this conversion
            // correctly; this call site just wasn't matching it, despite
            // its own old comment claiming it did.
            (vl.x, vl.y),
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
                &mut painter,
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
                &mut painter,
                self.font.as_mut(),
                &self.theme,
                vp.content_rect(&metrics).into(),
            );
        }

        if let EditorMode::Modal(m) = &self.mode {
            ui::draw_confirm_modal(
                &mut painter,
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
            ui::draw_context_menu(
                &mut painter,
                self.font.as_mut(),
                &self.theme,
                &self.theme_chrome_tex,
                cm,
                &mut self.ui_frame,
            );
        }
    }
}
