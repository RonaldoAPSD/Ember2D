// editor/input/panels/mod.rs — UI panels and menu interaction for level editor.
//
// Split into `mod.rs` + five sibling files in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) purely to keep every file under CLAUDE.md's
// 600-line hard limit — this file was 606 lines as a single flat file (was
// 575 lines before Phase 7's own Part 1c/1d/1e work touched it), all one
// `handle_panel_input` function. No behavioral change from the split: each
// extracted section becomes its own `pub(super)` method, returning `bool`
// wherever the original code used a bare `return;` to short-circuit the
// rest of `handle_panel_input` for that frame — `true` means "this section
// consumed the input, stop"; `false` means "fall through to the next
// section," exactly matching what reaching the end of that `if` block
// without hitting `return` already meant before the split.
//
//   (this file)         — panel drag/resize/close/tabs, and the
//                          non-exclusive per-frame drag/resize update
//   context_menu_trigger — right-click opens a context menu (distinct from
//                          `input/context_menu.rs`, which drives an
//                          ALREADY-OPEN one)
//   menu_bar             — top menu bar label click + open dropdown click
//   file_and_script      — File Browser and Script Editor panel clicks
//   hierarchy_and_palette — Hierarchy and Palette panel clicks
//   inspector             — Inspector panel field clicks (the last section;
//                          nothing follows it, so it needs no bool return)

use ember2d::input::Key;
use super::super::EditorState;
use super::super::panel::PanelId;
use super::super::ui::WidgetId;

mod context_menu_trigger;
mod menu_bar;
mod file_and_script;
mod hierarchy_and_palette;
mod inspector;

impl EditorState {
    pub(super) fn handle_panel_input(&mut self, input: &ember2d::input::InputManager, mouse: &ember2d::mouse::MouseState) {
        // F-key panel toggles (work in any mode).
        if input.just_pressed(Key::F1) { self.panels.toggle(PanelId::Console); }
        if input.just_pressed(Key::F2) { self.panels.toggle(PanelId::Inspector); }
        if input.just_pressed(Key::F3) { self.console_log.clear(); }

        if self.handle_panel_chrome_click(mouse) { return; }
        if self.handle_panel_context_menu_trigger(mouse) { return; }
        self.update_panel_drag_and_resize(mouse);
        if self.handle_menu_bar_click(mouse) { return; }
        if self.handle_menu_dropdown_click(input, mouse) { return; }
        if self.handle_file_browser_click(mouse) { return; }
        if self.handle_script_editor_click(mouse) { return; }
        if self.handle_hierarchy_click(mouse) { return; }
        if self.handle_palette_click(mouse) { return; }
        self.handle_inspector_click(mouse);
    }

    /// Panel drag / resize / close / tabs. `true` if the click landed on
    /// one of these widgets and no further section should run this frame.
    ///
    /// Phase 7 Part 1d (docs/ember2d-phase7-plan.md): one `UiFrame::hit`
    /// query replaces the old four separate `tab_at`/`close_btn_at`/
    /// `resize_handle_at`/`title_bar_at` calls — see `ui/frame.rs`'s
    /// header comment for why a single hit list, populated by the exact
    /// code that drew each widget, structurally can't drift from what
    /// was drawn (defect E5). `panel_at`/`is_point_on_panel` (focus
    /// tracking) are the one exception, kept as direct pixel queries —
    /// see `Panel::contains`'s own doc comment for why.
    fn handle_panel_chrome_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if mouse.left_just_pressed() && mouse.in_bounds {
            let (px, py) = (mouse.pixel_x, mouse.pixel_y);
            let hit = self.ui_frame.hit(px, py);

            // Tabs (switch active panel in dock) — matches the old
            // `tab_at`'s early return, which skipped focus tracking
            // entirely for a tab click.
            if let Some(WidgetId::Tab(tid)) = hit {
                self.panels.set_active(tid);
                self.ignore_drag = true;
                return true;
            }

            // Focus tracking
            if let Some(pid) = self.panels.panel_at(px, py) {
                self.focused_panel = Some(pid);
                self.panels.bring_to_front(pid);
            } else if !self.panels.is_point_on_panel(px, py) {
                self.focused_panel = None;
            }

            match hit {
                Some(WidgetId::CloseBtn(pid)) => {
                    if pid != PanelId::Viewport { self.panels.hide(pid); }
                    self.ignore_drag = true;
                    return true;
                }
                Some(WidgetId::ResizeHandle(pid)) => {
                    if pid != PanelId::Viewport { self.panels.start_resize(pid, px, py); }
                    self.ignore_drag = true;
                    return true;
                }
                Some(WidgetId::TitleBar(pid)) => {
                    if pid != PanelId::Viewport { self.panels.start_drag(pid, px, py); }
                    self.ignore_drag = true;
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    /// Advances any in-progress drag/resize. Not exclusive with any other
    /// section — always runs, matching the original's two independent
    /// `if mouse.left_held()`/`if mouse.left_just_released()` blocks that
    /// never themselves `return`.
    fn update_panel_drag_and_resize(&mut self, mouse: &ember2d::mouse::MouseState) {
        if mouse.left_held() {
            // Pixel-space screen bounds (Phase 7 Part 1c) — `self.layout`
            // only has the cell-count screen size, so convert here rather
            // than growing `Layout` with a redundant pixel copy this phase
            // doesn't otherwise need.
            let sw_px = self.layout.screen_w as f32 * ember2d::renderer::CELL_W as f32;
            let sh_px = self.layout.screen_h as f32 * ember2d::renderer::CELL_H as f32;
            if self.panels.is_dragging() {
                self.panels.update_drag(mouse.pixel_x, mouse.pixel_y, sw_px, sh_px);
            } else if self.panels.is_resizing() {
                self.panels.update_resize(mouse.pixel_x, mouse.pixel_y);
            }
        }
        if mouse.left_just_released() {
            let sw_px = self.layout.screen_w as f32 * ember2d::renderer::CELL_W as f32;
            let sh_px = self.layout.screen_h as f32 * ember2d::renderer::CELL_H as f32;
            self.panels.end_drag(sw_px, sh_px);
            self.panels.end_resize();
        }
    }
}
