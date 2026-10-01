// editor/input/context_menu.rs — Interaction for the right-click context menu.

use super::super::commands::Command;
use super::super::ui::{ContextMenu, ContextMenuAction, ToolKind, WidgetId};
use super::super::{EditorMode, EditorState};
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
    ///
    /// 7C-4 (master plan §5.3): `menu` comes in owned (moved out of `mode`
    /// by `handle_update`'s `mem::take`) instead of being read from the
    /// deleted `context_menu: Option<ContextMenu>` field.
    pub(super) fn handle_context_menu_input(
        &mut self,
        mut menu: ContextMenu,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        // 7D-3 checkpoint 7 (master plan §5.4): logical -> points, the
        // input choke point (`UiSpace::logical_to_pt`) every chrome
        // hit-test now goes through.
        let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
        let hit = self.ui_frame.hit(px, py);

        if let Some(WidgetId::ContextMenuRow(idx)) = hit {
            menu.selected = idx;
        }

        if mouse.left_just_pressed() {
            // Default outcome — `execute_context_action` may override this
            // (`NewLevel`/`NewScript` transition to `Prompt` via
            // `start_text_input`), so it's set before the call, not after.
            self.mode = EditorMode::Paint(ToolKind::Paint);
            if let Some(WidgetId::ContextMenuRow(idx)) = hit {
                if idx < menu.items.len() {
                    let action = menu.items[idx].1.clone();
                    self.execute_context_action(action);
                }
            }
            return;
        }

        if input.just_pressed(Key::Escape) || mouse.right_just_pressed() {
            self.mode = EditorMode::Paint(ToolKind::Paint);
            return;
        }

        // Nothing happened this frame — stay open.
        self.mode = EditorMode::ContextMenu(menu);
    }

    fn execute_context_action(&mut self, action: ContextMenuAction) {
        match action {
            ContextMenuAction::NewLevel => {
                // 7C-6 (master plan §5.3): same confirm-first treatment as
                // the menu bar's own "New Level" (`menu_bar.rs`) — this is
                // the identical action from a different entry point (the
                // File Browser's right-click menu), so it needs the same
                // guard.
                self.mode = EditorMode::Modal(crate::editor::Modal {
                    title: "New Level?".to_string(),
                    message: "Create a new level? The current one will be saved first.".to_string(),
                    purpose: crate::editor::ModalPurpose::ConfirmNewLevel,
                });
            }
            ContextMenuAction::NewScript => {
                self.start_text_input(crate::editor::TextInputPurpose::NewScriptName);
            }
            ContextMenuAction::NewFolder => {
                // TODO: folder creation
            }
            ContextMenuAction::DeleteFile(path) => {
                // 7C-6 (master plan §5.3): used to delete immediately,
                // with no confirmation at all — CLAUDE.md's own
                // "Development Rules" names "delete file" explicitly.
                self.mode = EditorMode::Modal(crate::editor::Modal {
                    title: "Delete File?".to_string(),
                    message: format!("Delete {}? This cannot be undone.", path),
                    purpose: crate::editor::ModalPurpose::ConfirmDeleteFile { path },
                });
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
                // A fixed top-left offset (7D-3 chrome audit,
                // docs/ember2d-master-plan.md §5.4, checkpoint 6) — was
                // `10.0 * CELL_W`/`10.0 * CELL_H` (Phase 7 Part 1c's own
                // `p.x = 10; p.y = 10;` multiplied out), the last chrome
                // panel-positioning site still keyed off the engine's cell
                // grid. Same numeric position (80, 160) at the old fixed
                // 8x16 cell size, now a bare point literal.
                p.rect.x = 80.0;
                p.rect.y = 160.0;
            }
            ContextMenuAction::FocusCamera(sel) => {
                let pos = match sel {
                    crate::editor::ui::HierarchySelection::Player => Some(self.grid.spawn_point),
                    crate::editor::ui::HierarchySelection::Spawn(i) => {
                        self.grid.extra_spawns.get(i).map(|(_, x, y)| (*x, *y))
                    }
                };
                if let Some((gx, gy)) = pos {
                    // Step 9-5: `center_on` (impl_state/viewport.rs) does
                    // exactly this in level cells; this used to repeat it
                    // with the viewport's glyph-cell size, which is wrong
                    // once a level cell isn't 8×16.
                    self.center_on(gx.round() as i32, gy.round() as i32);
                }
            }
            ContextMenuAction::DuplicateEntity(sel) => {
                // 7C-6 (master plan §5.3, D18): hierarchy Duplicate/Delete
                // had no undo support at all before this step — reuses
                // the existing `UpdateExtraSpawns` command (already used
                // by `MoveSpawn`'s sibling), a whole-list snapshot, since
                // both actions already work by rewriting the whole list.
                if let crate::editor::ui::HierarchySelection::Spawn(i) = sel {
                    if let Some(spawn) = self.grid.extra_spawns.get(i).cloned() {
                        let before = self.grid.extra_spawns.clone();
                        self.grid.extra_spawns.push((
                            format!("{} Copy", spawn.0),
                            spawn.1 + 1.0,
                            spawn.2 + 1.0,
                        ));
                        self.undo.push(Command::UpdateExtraSpawns {
                            before,
                            after: self.grid.extra_spawns.clone(),
                        });
                        self.unsaved = true;
                    }
                }
            }
            ContextMenuAction::DeleteEntity(sel) => {
                match sel {
                    crate::editor::ui::HierarchySelection::Player => {} // Cannot delete player spawn
                    crate::editor::ui::HierarchySelection::Spawn(i) => {
                        if i < self.grid.extra_spawns.len() {
                            let before = self.grid.extra_spawns.clone();
                            self.grid.extra_spawns.remove(i);
                            self.undo.push(Command::UpdateExtraSpawns {
                                before,
                                after: self.grid.extra_spawns.clone(),
                            });
                            self.hierarchy_sel = None;
                            self.unsaved = true;
                        }
                    }
                }
            }
        }
    }
}
