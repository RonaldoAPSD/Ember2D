// editor/input/panels/menu_bar.rs — top menu bar label click, and clicking
// (or dismissing) an open dropdown. Split out of the single
// `input/panels.rs` in Phase 7 Part 1f (docs/ember2d-phase7-plan.md) — see
// `mod.rs`'s header comment for the split's overall shape and the `bool`
// "did this section consume the input" convention every extracted section
// follows.

use ember2d::input::Key;
use super::super::super::EditorState;
use super::super::super::{TextInput, TextInputPurpose};
use super::super::super::ui::{self, ToolbarAction};
use super::super::super::ui::WidgetId;

impl EditorState {
    /// Clicking a top menu-bar label (row `layout.toolbar_row`) opens or
    /// closes its dropdown. `true` if the click was on that row at all —
    /// even a click that hits no label still consumes the frame (matching
    /// the original's unconditional `return;` inside this row check).
    ///
    /// Phase 7 Part 1d (docs/ember2d-phase7-plan.md): `UiFrame::hit`
    /// replaces the removed `menu_label_at` — see `ui/menu.rs`'s own
    /// note on the padding-cell fix this includes.
    pub(super) fn handle_menu_bar_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if mouse.left_just_pressed() && mouse.cell_y == self.layout.toolbar_row {
            if let Some(WidgetId::MenuLabel(kind)) = self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                self.active_menu = if self.active_menu == Some(kind) { None } else { Some(kind) };
            } else {
                self.active_menu = None;
            }
            self.ignore_drag = true;
            return true;
        }
        false
    }

    /// While a dropdown is open: clicking an item runs its action (or
    /// closes the menu on a miss), Escape dismisses it. `true` in either
    /// case, matching the original's unconditional `return;` at the end of
    /// each of those two branches.
    pub(super) fn handle_menu_dropdown_click(&mut self, input: &ember2d::input::InputManager, mouse: &ember2d::mouse::MouseState) -> bool {
        let Some(menu) = self.active_menu else { return false };

        if mouse.left_just_pressed() {
            // `UiFrame::hit` replaces the removed `menu_item_at` —
            // re-resolve the actual action from the same `menu_entries`
            // list `draw_menu_dropdown` drew from, by index.
            let action = match self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                Some(WidgetId::MenuItem(hit_menu, idx)) if hit_menu == menu => {
                    match ui::menu_entries(menu).into_iter().nth(idx) {
                        Some(ui::MenuEntry::Item { action, .. }) => Some(action),
                        _ => None,
                    }
                }
                _ => None,
            };
            self.active_menu = None;
            self.ignore_drag = true;
            if let Some(action) = action {
                match action {
                    ToolbarAction::CloseProject => {
                        self.pending_transition = Some(ember2d::engine::Transition::ToStart);
                        return true;
                    }
                    ToolbarAction::RenameLevel => {
                        self.text_input = Some(TextInput {
                            buffer: self.grid.name.clone(),
                            purpose: TextInputPurpose::LevelName,
                        });
                        return true;
                    }
                    ToolbarAction::ResizeLevel => {
                        self.text_input = Some(TextInput {
                            buffer:  String::new(),
                            purpose: TextInputPurpose::ResizeLevel,
                        });
                        return true;
                    }
                    ToolbarAction::SetSpawn => {
                        self.placing_spawn = true;
                        self.save_message = Some("Click on grid to place spawn. Esc to cancel.".to_string());
                        self.save_message_timer = 0;
                        return true;
                    }
                    ToolbarAction::AddNamedSpawn => {
                        self.text_input = Some(TextInput {
                            buffer: String::new(),
                            purpose: TextInputPurpose::NamedSpawn,
                        });
                        return true;
                    }
                    ToolbarAction::NewLevel => {
                        self.text_input = Some(TextInput {
                            buffer:  String::new(),
                            purpose: TextInputPurpose::NewLevelName,
                        });
                        return true;
                    }
                    _ => { self.dispatch_toolbar_action(action); return true; }
                }
            }
            return true;
        }
        if input.just_pressed(Key::Escape) { self.active_menu = None; return true; }
        false
    }
}
