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
use super::super::super::ui::{self, ChromeMetrics, HierarchySelection};
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

            // R64 (§3 in the master plan): was `mouse.cell_y` minus a
            // CELL-ROUNDED `p.content_y()` — a 16px-row index against rows
            // actually drawn (and hit-registered, `draw_row_px`/
            // `WidgetId::FileBrowserRow`/`HierarchyRow` in `ui/panels/dock.rs`)
            // at the theme's real `row_h` (20px in `ember-clean`), so a
            // right-click could target the wrong file or entity entirely.
            // Reads the SAME `UiFrame` hit the left-click handlers
            // (`handle_file_browser_click`/`handle_hierarchy_click`) already
            // trust, rather than recomputing row math a second, independent
            // way (E5).
            if let Some(WidgetId::FileBrowserRow(row_idx)) =
                self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y)
            {
                let mut items = self.file_browser_new_items();
                if row_idx < self.file_browser_files.len() {
                    if let Some(action) = self.file_browser_delete_action(row_idx) {
                        items.push(("Delete", action));
                    }
                }
                self.mode = EditorMode::ContextMenu(ui::ContextMenu { x: col, y: row, selected: 0, items });
                return true;
            }

            // R64: the same exact-hit fix above only covers a right-click on
            // an actual row. A right-click in the File Browser's own empty
            // tail space (below the last file, still inside the panel) keeps
            // the old "New …" only menu — gated on the panel's real pixel
            // content rect (`content_rect`, not the cell-rounded
            // `content_y()` this step replaced) rather than a specific row.
            if let Some(PanelId::FileBrowser) = self.panels.panel_at(mouse.pixel_x, mouse.pixel_y) {
                let metrics = ChromeMetrics::from_theme(&self.theme);
                let content = self.panels.get(PanelId::FileBrowser).content_rect(&metrics);
                if mouse.pixel_y > content.y + metrics.row_h {
                    self.mode = EditorMode::ContextMenu(ui::ContextMenu {
                        x: col,
                        y: row,
                        selected: 0,
                        items: self.file_browser_new_items(),
                    });
                    return true;
                }
            }

            // R64: was `mouse.cell_y` minus a CELL-ROUNDED `p.content_y()`
            // against rows actually drawn (and hit-registered,
            // `WidgetId::HierarchyRow` in `ui/panels/dock.rs`) at the
            // theme's real `row_h` — same exact-hit fix as the file browser
            // above, reading the same `UiFrame` hit `handle_hierarchy_click`
            // already trusts.
            if let Some(WidgetId::HierarchyRow(sel)) = self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                let mut items = vec![("Focus Camera", ui::ContextMenuAction::FocusCamera(sel))];
                if let HierarchySelection::Spawn(_) = sel {
                    items.push(("Duplicate", ui::ContextMenuAction::DuplicateEntity(sel)));
                    items.push(("Delete", ui::ContextMenuAction::DeleteEntity(sel)));
                }
                self.mode = EditorMode::ContextMenu(ui::ContextMenu { x: col, y: row, selected: 0, items });
                return true;
            }
        }
        false
    }

    /// The three actions always offered when right-clicking the File
    /// Browser panel — shared by the exact-row hit and the empty-tail-space
    /// fallback above so neither can drift from the other's item list.
    fn file_browser_new_items(&self) -> Vec<(&'static str, ui::ContextMenuAction)> {
        vec![
            ("New Level", ui::ContextMenuAction::NewLevel),
            ("New Script", ui::ContextMenuAction::NewScript),
            ("New Folder", ui::ContextMenuAction::NewFolder),
        ]
    }

    /// `Some(DeleteFile(path))` for a real, non-directory file row; `None`
    /// for `[UP]`/directory rows, which never offered Delete.
    fn file_browser_delete_action(&self, row_idx: usize) -> Option<ui::ContextMenuAction> {
        let raw = &self.file_browser_files[row_idx];
        if raw.contains("[UP]") || raw.starts_with("/ ") {
            return None;
        }
        // 7C-6 (master plan §5.3): found live testing this step's own
        // confirm-modal fix — `clean` was ALWAYS a bare filename
        // (untrimmed, at that), never combined with `current_folder`/
        // `project_folder` the way every other file action in this
        // codebase already does (`file_and_script.rs`'s own
        // `relative_path`/`path` construction). `std::fs::remove_file`
        // resolves a relative path against the process's CWD, not the
        // open project, so Delete silently did nothing (or, worse, deleted
        // an unrelated same-named file in the CWD) for any project opened
        // from somewhere other than the process's own working directory —
        // which is the common case. Also excludes directory rows (checked
        // above) — deleting a whole folder was never a supported action
        // here in the first place (`std::fs::remove_file` errors, silently,
        // on a directory).
        let clean = (if raw.len() > 3 { &raw[3..] } else { raw.as_str() }).trim();
        let relative = if self.current_folder == "." {
            clean.to_string()
        } else {
            format!("{}/{}", self.current_folder, clean)
        };
        let path = match &self.project_folder {
            Some(folder) => format!("{}/{}", folder, relative),
            None => relative,
        };
        Some(ui::ContextMenuAction::DeleteFile(path))
    }
}
