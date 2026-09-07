// editor/input/panels/file_and_script.rs — File Browser and Script Editor
// panel clicks (scroll, navigate, open a file/script). Split out of the
// single `input/panels.rs` in Phase 7 Part 1f (docs/ember2d-phase7-plan.md)
// — see `mod.rs`'s header comment for the split's overall shape and the
// `bool` "did this section consume the input" convention every extracted
// section follows.

use super::super::super::EditorState;
use super::super::super::panel::PanelId;

impl EditorState {
    pub(super) fn handle_file_browser_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if self.panels.visible(PanelId::FileBrowser) && mouse.in_bounds {
            let p = self.panels.get(PanelId::FileBrowser);
            if p.contains(mouse.pixel_x, mouse.pixel_y) {
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
                        let clean_name = if raw_name.len() > 3 { &raw_name[3..].trim() } else { "" };
                        if clean_name.is_empty() { return true; }

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
                        return true;
                    }
                }
            }
        }
        false
    }

    pub(super) fn handle_script_editor_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if self.panels.visible(PanelId::ScriptEditor) && mouse.in_bounds {
            let p = self.panels.get(PanelId::ScriptEditor);
            if p.contains(mouse.pixel_x, mouse.pixel_y) {
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
                        // R11 (7A-2, docs/ember2d-master-plan.md): clamp
                        // against the CHARACTER count, not the byte length
                        // — see script_editor.rs's `char_byte_offset` doc
                        // comment for why a byte-length bound produces an
                        // out-of-range char index on any multi-byte line.
                        self.script_cursor.0 = (col.max(0) as usize).min(self.script_buffer[row_idx].chars().count());
                    }
                    return true;
                }
            }
        }
        false
    }
}
