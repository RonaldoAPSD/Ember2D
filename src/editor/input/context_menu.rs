// editor/input/context_menu.rs — Interaction for the right-click context menu.

use crate::input::Key;
use super::super::EditorState;
use super::super::ui::ContextMenuAction;

impl EditorState {
    pub(super) fn handle_context_menu_input(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState) {
        let Some(ref mut menu) = self.context_menu else { return };
        
        let mw = 20usize;
        let mh = menu.items.len() + 2;
        let mut mx = menu.x;
        let mut my = menu.y;

        // Same boundary logic as draw
        if mx + mw > self.layout.screen_w { mx = self.layout.screen_w.saturating_sub(mw); }
        if my + mh > self.layout.screen_h { my = self.layout.screen_h.saturating_sub(mh); }

        let in_bounds = mouse.cell_x >= mx && mouse.cell_x < mx + mw
                     && mouse.cell_y >= my && mouse.cell_y < my + mh;

        // Hover selection
        if in_bounds && mouse.cell_y > my && mouse.cell_y < my + mh - 1 {
            menu.selected = mouse.cell_y - my - 1;
        }

        if mouse.left_just_pressed() {
            if in_bounds && menu.selected < menu.items.len() {
                let action = menu.items[menu.selected].1.clone();
                self.execute_context_action(action);
                self.context_menu = None;
            } else {
                self.context_menu = None;
            }
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
                    if other != id { self.panels.hide(other); }
                }
            }
            ContextMenuAction::FloatPanel(id) => {
                let p = self.panels.get_mut(id);
                p.dock = crate::editor::ui::DockSide::None;
                p.x = 10; p.y = 10;
            }
            ContextMenuAction::FocusCamera(sel) => {
                let pos = match sel {
                    crate::editor::ui::HierarchySelection::Player => Some(self.grid.spawn_point),
                    crate::editor::ui::HierarchySelection::Spawn(i) => self.grid.extra_spawns.get(i).map(|(_, x, y)| (*x, *y)),
                };
                if let Some((gx, gy)) = pos {
                    let (cw, ch) = (self.layout.canvas_w as f32 / self.zoom, self.layout.canvas_h as f32 / self.zoom);
                    self.scroll.0 = (gx - cw / 2.0) as i32;
                    self.scroll.1 = (gy - ch / 2.0) as i32;
                    self.clamp_scroll();
                }
            }
            ContextMenuAction::DuplicateEntity(sel) => {
                if let crate::editor::ui::HierarchySelection::Spawn(i) = sel {
                    if let Some(spawn) = self.grid.extra_spawns.get(i).cloned() {
                        self.grid.extra_spawns.push((format!("{} Copy", spawn.0), spawn.1 + 1.0, spawn.2 + 1.0));
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
