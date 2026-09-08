// editor/input/context_menu.rs — Interaction for the right-click context menu.

use super::super::ui::{ContextMenuAction, WidgetId};
use super::super::EditorState;
use ember2d::input::Key;

impl EditorState {
    /// 7C-1 (master plan §5.3): reads `UiFrame::hit` (populated by
    /// `draw_context_menu` via `draw_row`, one `ContextMenuRow(i)` per
    /// item) instead of recomputing the menu's boundary-clamped `mx`/`my`
    /// independently here (E5).
    ///
    /// One deliberate behavior change from the removed math: a click
    /// landing inside the menu's outer rect but not on any item row (the
    /// border, or the one-cell padding `mh = items.len() + 2` used to
    /// leave below the last item) used to still confirm whatever item was
    /// last hovered; now, like every other panel migrated in this step, a
    /// click has to land on the actual registered row. That old behavior
    /// wasn't reachable through the menu's own drawn border (no dead space
    /// exists between rows in `draw_context_menu`), so this is a
    /// same-in-practice tightening, not a visible regression.
    pub(super) fn handle_context_menu_input(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        let Some(ref mut menu) = self.context_menu else { return };

        let hit = self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y);

        if let Some(WidgetId::ContextMenuRow(idx)) = hit {
            menu.selected = idx;
        }

        if mouse.left_just_pressed() {
            if let Some(WidgetId::ContextMenuRow(idx)) = hit {
                if idx < menu.items.len() {
                    let action = menu.items[idx].1.clone();
                    self.execute_context_action(action);
                }
            }
            self.context_menu = None;
        }

        if input.just_pressed(Key::Escape) || mouse.right_just_pressed() {
            self.context_menu = None;
        }
    }

    fn execute_context_action(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::NewLevel => {
                self.start_text_input(crate::editor::TextInputPurpose::NewLevelName);
            }
            ContextMenuAction::NewScript => {
                self.start_text_input(crate::editor::TextInputPurpose::NewScriptName);
            }
            ContextMenuAction::NewFolder => {
                // TODO: folder creation
            }
            ContextMenuAction::DeleteFile(path) => {
                let _ = std::fs::remove_file(path);
                self.refresh_project_files();
            }
            ContextMenuAction::CloseTab(id) => {
                self.panels.hide(id);
            }
            ContextMenuAction::CloseOthers(id) => {
                let p = self.panels.get(id);
                let side = p.dock;
                let docked = self.panels.get_docked_panels(side);
                for other in docked {
                    if other != id {
                        self.panels.hide(other);
                    }
                }
            }
            ContextMenuAction::FloatPanel(id) => {
                let p = self.panels.get_mut(id);
                p.dock = crate::editor::ui::DockSide::None;
                // Cell (10, 10) in pixels (Phase 7 Part 1c) — was `p.x = 10; p.y = 10;`.
                p.rect.x = 10.0 * ember2d::renderer::CELL_W as f32;
                p.rect.y = 10.0 * ember2d::renderer::CELL_H as f32;
            }
            ContextMenuAction::FocusCamera(sel) => {
                let pos = match sel {
                    crate::editor::ui::HierarchySelection::Player => Some(self.grid.spawn_point),
                    crate::editor::ui::HierarchySelection::Spawn(i) => {
                        self.grid.extra_spawns.get(i).map(|(_, x, y)| (*x, *y))
                    }
                };
                if let Some((gx, gy)) = pos {
                    let (cw, ch) = (
                        self.layout.canvas_w as f32 / self.zoom,
                        self.layout.canvas_h as f32 / self.zoom,
                    );
                    self.target_scroll.0 = gx - cw / 2.0;
                    self.target_scroll.1 = gy - ch / 2.0;
                    self.clamp_scroll();
                }
            }
            ContextMenuAction::DuplicateEntity(sel) => {
                if let crate::editor::ui::HierarchySelection::Spawn(i) = sel {
                    if let Some(spawn) = self.grid.extra_spawns.get(i).cloned() {
                        self.grid.extra_spawns.push((
                            format!("{} Copy", spawn.0),
                            spawn.1 + 1.0,
                            spawn.2 + 1.0,
                        ));
                        self.unsaved = true;
                    }
                }
            }
            ContextMenuAction::DeleteEntity(sel) => {
                match sel {
                    crate::editor::ui::HierarchySelection::Player => {} // Cannot delete player spawn
                    crate::editor::ui::HierarchySelection::Spawn(i) => {
                        if i < self.grid.extra_spawns.len() {
                            self.grid.extra_spawns.remove(i);
                            self.hierarchy_sel = None;
                            self.unsaved = true;
                        }
                    }
                }
            }
        }
    }
}
