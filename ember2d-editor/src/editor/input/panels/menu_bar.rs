// editor/input/panels/menu_bar.rs — top menu bar label click, and clicking
// (or dismissing) an open dropdown. Split out of the single
// `input/panels.rs` in Phase 7 Part 1f (docs/ember2d-phase7-plan.md) — see
// `mod.rs`'s header comment for the split's overall shape and the `bool`
// "did this section consume the input" convention every extracted section
// follows.

use super::super::super::ui::WidgetId;
use super::super::super::ui::{self, MenuKind, ToolbarAction};
use super::super::super::EditorMode;
use super::super::super::EditorState;
use super::super::super::TextInputPurpose;
use super::super::super::{Modal, ModalPurpose};
use ember2d::input::Key;

impl EditorState {
    /// Clicking a top menu-bar label opens or closes its dropdown; clicking
    /// anywhere else on the bar's own strip still consumes the frame
    /// (matching the original's unconditional `return;` inside its row
    /// check). `WidgetId::MenuBar`/`MenuLabel` (7D-3, docs/ember2d-master-plan.md
    /// §5.4) replace the old `mouse.cell_y == TOOLBAR_ROW` raw-cell gate —
    /// the toolbar's real row height comes from the active theme now, not
    /// a fixed cell, so there's no cell-row constant left to compare
    /// against; `UiFrame::hit`'s own reverse search already tells us
    /// whether the click landed on the bar at all.
    ///
    /// Phase 7 Part 1d (docs/ember2d-phase7-plan.md): `UiFrame::hit`
    /// replaces the removed `menu_label_at` — see `ui/menu.rs`'s own
    /// note on the padding-cell fix this includes.
    pub(super) fn handle_menu_bar_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if !mouse.left_just_pressed() {
            return false;
        }
        // 7D-3 checkpoint 7: logical -> points, the same input choke point
        // `handle_panel_chrome_click` uses.
        let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
        match self.ui_frame.hit(px, py) {
            Some(WidgetId::MenuLabel(kind)) => {
                self.active_menu = if self.active_menu == Some(kind) { None } else { Some(kind) };
                self.ignore_drag = true;
                true
            }
            Some(WidgetId::MenuBar) => {
                self.active_menu = None;
                self.ignore_drag = true;
                true
            }
            _ => false,
        }
    }

    /// While a dropdown is open: clicking an item runs its action (or
    /// closes the menu on a miss), Escape dismisses it. `true` in either
    /// case, matching the original's unconditional `return;` at the end of
    /// each of those two branches.
    pub(super) fn handle_menu_dropdown_click(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) -> bool {
        let Some(menu) = self.active_menu else { return false };

        if mouse.left_just_pressed() {
            // `UiFrame::hit` replaces the removed `menu_item_at` —
            // re-resolve the actual action from the same `menu_entries`
            // list `draw_menu_dropdown` drew from, by index. 7D-3
            // checkpoint 7: logical -> points, same input choke point as
            // `handle_menu_bar_click`.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let action = match self.ui_frame.hit(px, py) {
                Some(WidgetId::MenuItem(hit_menu, idx)) if hit_menu == menu => {
                    // `MenuKind::Theme`'s entries are runtime-known — same
                    // special case `draw_menu_dropdown` (`ui/menu.rs`)
                    // makes when drawing this same list, so the two stay
                    // in sync (menu.rs's own doc comment on this).
                    let entries = if menu == MenuKind::Theme {
                        ui::theme_menu_entries(&self.available_themes)
                    } else {
                        ui::menu_entries(menu)
                    };
                    match entries.into_iter().nth(idx) {
                        Some(ui::MenuEntry::Item { action, .. }) => Some(action),
                        Some(ui::MenuEntry::DynamicItem { action, .. }) => Some(action),
                        None | Some(ui::MenuEntry::Sep) => None,
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
                        self.prompt_buffer = self.grid.name.clone();
                        self.mode = EditorMode::Prompt(TextInputPurpose::LevelName);
                        return true;
                    }
                    // Step 9.5-5: the names collider layers and masks resolve
                    // against, prefilled with the level's current list.
                    ToolbarAction::CollisionLayers => {
                        self.prompt_buffer = self.grid.collision_layers.join(", ");
                        self.mode = EditorMode::Prompt(TextInputPurpose::CollisionLayers);
                        return true;
                    }
                    ToolbarAction::ResizeLevel => {
                        self.prompt_buffer.clear();
                        self.mode = EditorMode::Prompt(TextInputPurpose::ResizeLevel);
                        return true;
                    }
                    ToolbarAction::SetSpawn => {
                        self.mode = EditorMode::PlaceSpawn(None);
                        self.save_message =
                            Some("Click on grid to place spawn. Esc to cancel.".to_string());
                        self.save_message_timer = 0;
                        return true;
                    }
                    ToolbarAction::AddNamedSpawn => {
                        self.prompt_buffer.clear();
                        self.mode = EditorMode::Prompt(TextInputPurpose::NamedSpawn);
                        return true;
                    }
                    ToolbarAction::NewLevel => {
                        // 7C-6 (master plan §5.3): always confirms first
                        // now (CLAUDE.md's "Development Rules": "new
                        // level... confirms first," unconditionally,
                        // unlike level-switch's own `unsaved`-gated
                        // check) — `ModalPurpose::ConfirmNewLevel`'s own
                        // "Yes" handler (`input/modal.rs`) is exactly
                        // what used to run directly here.
                        self.mode = EditorMode::Modal(Modal {
                            title: "New Level?".to_string(),
                            message: "Create a new level? The current one will be saved first."
                                .to_string(),
                            purpose: ModalPurpose::ConfirmNewLevel,
                        });
                        return true;
                    }
                    ToolbarAction::SetTheme(name) => {
                        // 7D-4 (master plan §5.4): reloads theme/
                        // theme_chrome_tex/font in place — see
                        // `EditorState::switch_theme`'s own doc comment
                        // (theme_loader.rs).
                        self.switch_theme(&name);
                        return true;
                    }
                    ToolbarAction::SetUiScale(choice) => {
                        // 7D-3 checkpoint 7 (master plan §5.4): persists
                        // immediately (`set_ui_scale`, `theme_loader.rs`) —
                        // takes effect on the very next `draw()`, which
                        // reads `effective_ui_scale()` and rebuilds fonts
                        // if the resolved scale actually changed.
                        self.set_ui_scale(choice);
                        return true;
                    }
                    _ => {
                        self.dispatch_toolbar_action(action);
                        return true;
                    }
                }
            }
            return true;
        }
        if input.just_pressed(Key::Escape) {
            self.active_menu = None;
            return true;
        }
        false
    }
}
