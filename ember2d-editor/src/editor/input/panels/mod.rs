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

use super::super::panel::PanelId;
use super::super::ui::WidgetId;
use super::super::EditorState;
use ember2d::input::Key;

mod context_menu_trigger;
mod file_and_script;
mod hierarchy_and_palette;
mod inspector;
mod menu_bar;

impl EditorState {
    pub(super) fn handle_panel_input(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        // F-key panel toggles (work in any mode).
        if input.just_pressed(Key::F1) {
            self.panels.toggle(PanelId::Console);
        }
        if input.just_pressed(Key::F2) {
            self.panels.toggle(PanelId::Inspector);
        }
        if input.just_pressed(Key::F3) {
            self.console_log.clear();
        }

        // Step 8-4: a press held on a File Browser asset row owns the mouse
        // until it's released (and dropped) — see `update_asset_drag`.
        if self.update_asset_drag(input, mouse) {
            return;
        }
        if self.handle_panel_chrome_click(mouse) {
            return;
        }
        if self.handle_panel_context_menu_trigger(mouse) {
            return;
        }
        self.update_panel_drag_and_resize(mouse);
        if self.handle_menu_bar_click(mouse) {
            return;
        }
        if self.handle_menu_dropdown_click(input, mouse) {
            return;
        }
        if self.handle_file_browser_click(mouse) {
            return;
        }
        if self.handle_script_editor_click(mouse) {
            return;
        }
        if self.handle_hierarchy_click(mouse) {
            return;
        }
        if self.handle_palette_click(mouse) {
            return;
        }
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
            // 7D-3 checkpoint 7 (master plan §5.4): `UiFrame`/`Panel::rect`
            // are POINTS-space; `mouse.pixel_x/y` are LOGICAL — the INPUT
            // choke point (`UiSpace::logical_to_pt`, its own doc comment)
            // converts once here for every chrome hit-test below. Only
            // needed once `ui_scale` can actually differ from
            // `render_scale` (this step's own final checkpoint) — the two
            // agreed numerically at every earlier checkpoint.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
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
                    if pid != PanelId::Viewport {
                        self.panels.hide(pid);
                    }
                    self.ignore_drag = true;
                    return true;
                }
                Some(WidgetId::ResizeHandle(pid)) => {
                    if pid != PanelId::Viewport {
                        self.panels.start_resize(pid, px, py);
                    }
                    self.ignore_drag = true;
                    return true;
                }
                Some(WidgetId::TitleBar(pid)) => {
                    // 7C-3 (master plan §5.3, E4): the Viewport's title bar
                    // is no longer excluded here — it starts a drag like
                    // any other panel's. `apply_layout` (`panel/mod.rs`)
                    // still unconditionally recomputes the Viewport's dock
                    // and rect as "whatever's left over" every frame
                    // regardless of what dragging it produces, so it always
                    // snaps back to filling that space on the very next
                    // frame — "docks/undocks like any panel" describes the
                    // input mechanics here, not the layout result, which
                    // deliberately still stays master-fill.
                    let metrics = super::super::ui::ChromeMetrics::from_theme(&self.theme);
                    self.panels.start_drag(pid, px, py, &metrics);
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
        // Points-space screen bounds (7D-3, docs/ember2d-master-plan.md
        // §5.4 — was `screen_size_px`) — read straight from `PanelManager`
        // (7C-3, master plan §5.3, E4), which already has them; `metrics`
        // is rebuilt fresh from the current theme, matching every other
        // `PanelManager` call site this step touched.
        let metrics = super::super::ui::ChromeMetrics::from_theme(&self.theme);
        if mouse.left_held() {
            // 7D-3 checkpoint 7: `mouse.pixel_x/y` (logical) -> points, same
            // input choke point `handle_panel_chrome_click` uses.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let (sw_pt, sh_pt) = self.panels.screen_size_pt();
            if self.panels.is_dragging() {
                self.panels.update_drag(px, py, sw_pt, sh_pt, &metrics);
            } else if self.panels.is_resizing() {
                self.panels.update_resize(px, py, &metrics);
            }
        }
        if mouse.left_just_released() {
            let (sw_pt, sh_pt) = self.panels.screen_size_pt();
            self.panels.end_drag(sw_pt, sh_pt, &metrics);
            self.panels.end_resize();
        }
    }
}
