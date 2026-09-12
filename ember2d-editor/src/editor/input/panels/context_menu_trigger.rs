// editor/input/panels/context_menu_trigger.rs — right-click opens a
// context menu. Split out of the single `input/panels.rs` in Phase 7 Part
// 1f (docs/ember2d-phase7-plan.md) — see `mod.rs`'s header comment for the
// split's overall shape and the `bool` "did this section consume the
// input" convention every extracted section follows.
//
// Distinct from `input/context_menu.rs`, which drives an ALREADY-OPEN
// context menu's own input (hovering/selecting/confirming an item) — this
// file only decides whether a right-click should open one in the first
// place, and with which items.

use super::super::super::panel::PanelId;
use super::super::super::ui::WidgetId;
use super::super::super::ui::{self, HierarchySelection};
use super::super::super::EditorMode;
use super::super::super::EditorState;

impl EditorState {
    /// `true` if the right-click opened a context menu and no further
    /// section should run this frame.
    pub(super) fn handle_panel_context_menu_trigger(
        &mut self,
        mouse: &ember2d::mouse::MouseState,
    ) -> bool {
        if mouse.right_just_pressed() && mouse.in_bounds {
            let col = mouse.cell_x;
            let row = mouse.cell_y;

            // 1. Tab Context Menu — pixel-space UiFrame hit (Phase 7 Part 1d).
            if let Some(WidgetId::Tab(tid)) = self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                self.mode = EditorMode::ContextMenu(ui::ContextMenu {
                    x: col,
                    y: row,
                    selected: 0,
                    items: vec![
                        ("Close Tab", ui::ContextMenuAction::CloseTab(tid)),
                        ("Close Others", ui::ContextMenuAction::CloseOthers(tid)),
                        ("Float Panel", ui::ContextMenuAction::FloatPanel(tid)),
                    ],
                });
                return true;
            }

            // 2. Panel-specific context menus
            if let Some(pid) = self.panels.panel_at(mouse.pixel_x, mouse.pixel_y) {
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
                                if !raw.contains("[UP]") && !raw.starts_with("/ ") {
                                    // 7C-6 (master plan §5.3): found live
                                    // testing this step's own confirm-modal
                                    // fix — `clean` was ALWAYS a bare
                                    // filename (untrimmed, at that), never
                                    // combined with `current_folder`/
                                    // `project_folder` the way every other
                                    // file action in this codebase already
                                    // does (`file_and_script.rs`'s own
                                    // `relative_path`/`path` construction).
                                    // `std::fs::remove_file` resolves a
                                    // relative path against the process's
                                    // CWD, not the open project, so Delete
                                    // silently did nothing (or, worse,
                                    // deleted an unrelated same-named file
                                    // in the CWD) for any project opened
                                    // from somewhere other than the
                                    // process's own working directory —
                                    // which is the common case. Also
                                    // excludes directory rows now (`/ `
                                    // prefix) — deleting a whole folder was
                                    // never a supported action here in the
                                    // first place (`std::fs::remove_file`
                                    // errors, silently, on a directory).
                                    let clean = (if raw.len() > 3 { &raw[3..] } else { raw }).trim();
                                    let relative = if self.current_folder == "." {
                                        clean.to_string()
                                    } else {
                                        format!("{}/{}", self.current_folder, clean)
                                    };
                                    let path = match &self.project_folder {
                                        Some(folder) => format!("{}/{}", folder, relative),
                                        None => relative,
                                    };
                                    items.push(("Delete", ui::ContextMenuAction::DeleteFile(path)));
                                }
                            }
                            self.mode = EditorMode::ContextMenu(ui::ContextMenu {
                                x: col,
                                y: row,
                                selected: 0,
                                items,
                            });
                            return true;
                        }
                    }
                    PanelId::Hierarchy => {
                        let p = self.panels.get(pid);
                        let cy = p.content_y();
                        if row > cy {
                            let hier_row = row - cy;
                            let sel = if hier_row == 1 {
                                Some(HierarchySelection::Player)
                            } else {
                                Some(HierarchySelection::Spawn(hier_row - 2))
                            };

                            if let Some(s) = sel {
                                let mut items =
                                    vec![("Focus Camera", ui::ContextMenuAction::FocusCamera(s))];
                                if let HierarchySelection::Spawn(_) = s {
                                    items.push((
                                        "Duplicate",
                                        ui::ContextMenuAction::DuplicateEntity(s),
                                    ));
                                    items.push(("Delete", ui::ContextMenuAction::DeleteEntity(s)));
                                }
                                self.mode = EditorMode::ContextMenu(ui::ContextMenu {
                                    x: col,
                                    y: row,
                                    selected: 0,
                                    items,
                                });
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        false
    }
}
