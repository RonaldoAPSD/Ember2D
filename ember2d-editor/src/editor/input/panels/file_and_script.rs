// editor/input/panels/file_and_script.rs — File Browser and Script Editor
// panel clicks (scroll, navigate, open a file/script). Split out of the
// single `input/panels.rs` in Phase 7 Part 1f (docs/ember2d-phase7-plan.md)
// — see `mod.rs`'s header comment for the split's overall shape and the
// `bool` "did this section consume the input" convention every extracted
// section follows.

use super::super::super::panel::PanelId;
use super::super::super::ui::ChromeMetrics;
use super::super::super::ui::ScriptLayout;
use super::super::super::ui::WidgetId;
use super::super::super::EditorState;

impl EditorState {
    /// 7C-1 (master plan §5.3): reads `UiFrame::hit` (populated by
    /// `draw_file_browser_panel`) instead of recomputing `row_idx` from
    /// the panel's content origin independently here (E5).
    pub(super) fn handle_file_browser_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if self.panels.visible(PanelId::FileBrowser) && mouse.in_bounds {
            // 7D-3 checkpoint 7 (master plan §5.4): logical -> points, the
            // input choke point every chrome hit-test in this file goes
            // through — `Panel::rect`/`UiFrame` are points-space.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let p = self.panels.get(PanelId::FileBrowser);
            if p.contains(px, py) {
                // R72 (§3 in the master plan): was `p.content_h()` (a
                // CELL-ROUNDED row count) minus a literal `1` — an
                // approximation of `draw_file_browser_panel`'s own
                // `max_visible` (`ui/panels/dock.rs`) that only agreed with
                // it by coincidence at the old fixed 16px `CELL_H`. Recomputed
                // here with that exact same formula, from the same pixel
                // `content_rect`, so the wheel can always reach the last file.
                let metrics = ChromeMetrics::from_theme(&self.theme);
                let content = p.content_rect(&metrics);
                let max_visible =
                    ((content.h - metrics.row_h) / metrics.row_h).floor().max(0.0) as usize;

                // Mouse wheel scroll
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = self.file_browser_files.len().saturating_sub(max_visible);
                    self.file_browser_scroll = (self.file_browser_scroll as i32 + delta)
                        .clamp(0, max_scroll as i32)
                        as usize;
                }

                if mouse.left_just_pressed() {
                    if let Some(WidgetId::FileBrowserRow(row_idx)) = self.ui_frame.hit(px, py) {
                        self.ignore_drag = true;
                        self.file_browser_cursor = row_idx;
                        let Some(raw_name) = self.file_browser_files.get(row_idx).cloned() else {
                            return true;
                        };

                        // Step 8-4: an asset row (image / tileset / clip)
                        // isn't opened by a press — the press selects it
                        // (the panel previews the selected row) and may
                        // start a drag onto the palette or a canvas tile
                        // (`impl_state/asset_drop.rs`).
                        if let Some(asset) =
                            crate::editor::assets::classify(&self.current_folder, &raw_name)
                        {
                            self.begin_asset_drag(asset, px, py);
                            return true;
                        }

                        // 1. Navigation (UP)
                        if raw_name.contains("[UP]") {
                            if let Some(parent) =
                                std::path::Path::new(&self.current_folder).parent()
                            {
                                self.current_folder = parent.to_string_lossy().to_string();
                                if self.current_folder.is_empty() {
                                    self.current_folder = ".".to_string();
                                }
                            } else {
                                self.current_folder = ".".to_string();
                            }
                            self.file_browser_cursor = 0;
                            self.file_browser_scroll = 0;
                            self.refresh_project_files();
                            return true;
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
                            return true;
                        }

                        // 3. Files
                        let clean_name = if raw_name.len() > 3 { raw_name[3..].trim() } else { "" };
                        if clean_name.is_empty() {
                            return true;
                        }

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
                                // 7C-6 (master plan §5.3): only confirm
                                // when there's actually something to lose
                                // — CLAUDE.md's own "Development Rules"
                                // names "switch level WITH UNSAVED EDITS"
                                // specifically, unlike "new level"/"delete
                                // file" above, which confirm
                                // unconditionally. Used to always show
                                // this modal, even with nothing unsaved.
                                //
                                // R60 (found live by the user, 2026-09-12):
                                // `self.unsaved` alone missed a real case —
                                // switching replaces the whole `EditorState`
                                // (`switch_to_level`'s `*self = ns`), which
                                // silently drops an open script buffer too.
                                // A grid-clean level with an edited-but-not-
                                // saved script must still confirm.
                                if self.unsaved || self.script_unsaved {
                                    self.mode =
                                        crate::editor::EditorMode::Modal(crate::editor::Modal {
                                            title: "Switch Level?".to_string(),
                                            message: format!("Load {}?", clean_name),
                                            purpose:
                                                crate::editor::ModalPurpose::ConfirmSwitchLevel {
                                                    path,
                                                },
                                        });
                                } else {
                                    self.switch_to_level(&path);
                                }
                            }
                        }
                        return true;
                    }
                }
            }
        }
        false
    }

    /// R67 (§3 in the master plan): reads the exact same `ScriptLayout`
    /// (`ui/script_layout.rs`) the docked-focused path
    /// (`input/script_editor.rs`'s `handle_script_mode_input`) and
    /// `draw_script_editor` both use — was its own independent formula
    /// with a fixed `gutter_w = 4` and no `hscroll` term at all, so a
    /// click on a horizontally-scrolled line landed in the wrong column
    /// here specifically (the FIRST click, before the panel has focus),
    /// and the wheel stepped by 1 row instead of the focused path's 2.
    pub(super) fn handle_script_editor_click(
        &mut self,
        mouse: &ember2d::mouse::MouseState,
    ) -> bool {
        if self.panels.visible(PanelId::ScriptEditor) && mouse.in_bounds {
            // 7D-3 checkpoint 7: logical -> points, same input choke point
            // as `handle_file_browser_click` above — `ScriptLayout` (built
            // from a points-space `content_rect`) needs a points-space
            // pixel position too.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let p = self.panels.get(PanelId::ScriptEditor);
            if p.contains(px, py) {
                let metrics = ChromeMetrics::from_theme(&self.theme);
                let content = p.content_rect(&metrics);
                let has_error = self.script_error().is_some();
                let layout = ScriptLayout::compute(
                    &self.theme,
                    self.code_font.as_mut(),
                    content.into(),
                    self.script_buffer.len(),
                    has_error,
                    false,
                );

                // Mouse wheel scroll — same 2-row step as the focused path.
                if mouse.wheel_y != 0.0 {
                    let delta = if mouse.wheel_y > 0.0 { -2i32 } else { 2i32 };
                    let max_scroll = self.script_buffer.len().saturating_sub(layout.visible_rows);
                    self.script_scroll =
                        (self.script_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() {
                    self.ignore_drag = true;
                    if let Some((col, row)) = layout.hit(
                        px,
                        py,
                        self.script_scroll,
                        self.script_hscroll,
                        &self.script_buffer,
                    ) {
                        self.script_cursor = (col, row);
                    }
                    return true;
                }
            }
        }
        false
    }
}
