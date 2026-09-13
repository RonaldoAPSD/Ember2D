// editor/input/mod.rs — Input orchestration for EditorState.

use super::commands::Command;
use super::panel::PanelId;
use super::ui::ToolKind;
use super::{EditorMode, EditorState};
use ember2d::engine::UpdateContext;

mod canvas;
mod context_menu;
mod graph;
mod modal;
mod palette_editor;
mod panels;
mod script_editor;
mod shortcuts;
mod text;

impl EditorState {
    pub(super) fn handle_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { input, mouse, .. } = ctx;

        // ── Ignore drag state ─────────────────────────────────────────────────
        if self.ignore_drag && !mouse.left_held() {
            self.ignore_drag = false;
        }

        // Tick save message.
        if self.save_message.is_some() {
            self.save_message_timer += 1;
            if self.save_message_timer > 90 {
                self.save_message = None;
                self.save_message_timer = 0;
            }
        }

        // R14 (7A-2, docs/ember2d-master-plan.md): the DOCKED script panel
        // having focus swallows keyboard input the same way fullscreen
        // `EditorMode::Script` does — but unlike fullscreen, other panels
        // are still visible and clickable, so a click OUTSIDE the script
        // panel's own bounds must still fall through to
        // `handle_panel_input`, which is what reassigns `focused_panel` to
        // whatever was actually clicked. Checked before the `mode` match
        // below since it's an orthogonal FOCUS concern, not part of `mode`
        // itself — see `EditorMode::Script`'s own doc comment.
        if self.focused_panel == Some(PanelId::ScriptEditor) && !matches!(self.mode, EditorMode::Script)
        {
            let p = self.panels.get(PanelId::ScriptEditor);
            // 7D-3 checkpoint 7 (master plan §5.4): logical -> points —
            // `Panel::rect` is points-space now.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let click_outside =
                mouse.left_just_pressed() && !(mouse.in_bounds && p.contains(px, py));
            if !click_outside {
                self.handle_script_mode_input(input, mouse);
                return;
            }
        }

        // 7C-4 (master plan §5.3): one match on `mode` replaces the old
        // if-chain (modal → context menu → color picker → palette editor →
        // graph → script → spawn placement → palette search → text input),
        // whose correctness used to depend on checking those in exactly
        // this order. Every "hard exclusive" arm handles its own input and
        // returns; `Paint`/`Inspect`/`Select`/`Paste` fall through to the
        // shared panels/canvas/shortcuts dispatch below, since none of them
        // stop the rest of the editor (other panels, global shortcuts)
        // from working normally alongside them. `mem::take` moves `mode`
        // out by value rather than matching a borrow of it, since several
        // arms below need to call other `&mut self` methods (including,
        // for `Modal`, `*self = ns` on a successful level switch) that
        // can't run while `self.mode` is still borrowed.
        match std::mem::take(&mut self.mode) {
            EditorMode::Modal(modal) => {
                self.handle_modal_input(modal, input, mouse);
                return;
            }
            EditorMode::ContextMenu(menu) => {
                self.handle_context_menu_input(menu, input, mouse);
                return;
            }
            EditorMode::ColorPicker { is_fg } => {
                self.handle_color_picker_input(is_fg, input, mouse);
                return;
            }
            EditorMode::PaletteEditor => {
                self.handle_palette_editor_input(input, mouse);
                return;
            }
            EditorMode::Graph { gx, gy } => {
                self.update_graph_mode(gx, gy, input, mouse);
                return;
            }
            EditorMode::Script => {
                // Found live by the user (2026-09-12): every other
                // hard-exclusive arm's own handler restores `self.mode`
                // as its first action (see `handle_color_picker_input`/
                // `handle_palette_editor_input`/`handle_place_spawn_input`/
                // `handle_palette_search_input`'s own first lines) since
                // `std::mem::take` above already emptied it to
                // `EditorMode::default()`. This arm never did — a unit
                // variant has no payload to reconstruct the handler side,
                // so it's restored here instead, before the call.
                // Without it, `handle_script_mode_input`'s own
                // `fullscreen = matches!(self.mode, EditorMode::Script)`
                // always read `false` (mode was already `Paint` by the
                // time it ran), and `self.mode` stayed `Paint` for
                // `draw()` to read — the fullscreen editor rendered for
                // the one frame `load_script` set it, then silently
                // reverted on every frame after, invisible at 60fps.
                self.mode = EditorMode::Script;
                self.handle_script_mode_input(input, mouse);
                return;
            }
            EditorMode::PlaceSpawn(named) => {
                self.handle_place_spawn_input(named, input, mouse);
                return;
            }
            EditorMode::PaletteSearch => {
                self.handle_palette_search_input(input);
                return;
            }
            EditorMode::Prompt(purpose) => {
                self.handle_text_input(purpose, input);
                return;
            }
            mode @ (EditorMode::Paint(_)
            | EditorMode::Inspect
            | EditorMode::Select { .. }
            | EditorMode::Paste) => {
                self.mode = mode;
            }
        }

        // ── Specialized interaction modes (panels, canvas, shortcuts) ──────────

        self.handle_panel_input(input, mouse);
        self.handle_canvas_input(input, mouse);
        self.handle_shortcuts(input, mouse);

        // ── Smooth camera lerp ────────────────────────────────────────────────
        let lerp_factor = 0.2; // Snappiness
        self.scroll.0 += (self.target_scroll.0 - self.scroll.0) * lerp_factor;
        self.scroll.1 += (self.target_scroll.1 - self.scroll.1) * lerp_factor;

        // Snap if close enough to prevent infinite micro-movement
        if (self.scroll.0 - self.target_scroll.0).abs() < 0.001 {
            self.scroll.0 = self.target_scroll.0;
        }
        if (self.scroll.1 - self.target_scroll.1).abs() < 0.001 {
            self.scroll.1 = self.target_scroll.1;
        }
    }

    fn handle_place_spawn_input(
        &mut self,
        named: Option<String>,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        self.mode = EditorMode::PlaceSpawn(named.clone());

        if input.just_pressed(ember2d::input::Key::Escape) || mouse.right_just_pressed() {
            self.save_message = Some("Spawn placement cancelled.".to_string());
            self.save_message_timer = 0;
            self.mode = EditorMode::Paint(ToolKind::Paint);
            return;
        }
        if mouse.left_just_pressed() {
            if let Some((gx, gy)) = self.mouse_to_grid(mouse.pixel_x, mouse.pixel_y) {
                match named {
                    None => {
                        let before = self.grid.spawn_point;
                        let after = (gx as f32, gy as f32);
                        self.undo.push(Command::MoveSpawn { before, after });
                        self.grid.spawn_point = after;
                        self.save_message = Some("Spawn placed.".to_string());
                    }
                    Some(name) => {
                        let before = self.grid.extra_spawns.clone();
                        let mut after = before.clone();
                        after.push((name, gx as f32, gy as f32));
                        self.undo.push(Command::UpdateExtraSpawns { before, after: after.clone() });
                        self.grid.extra_spawns = after;
                        self.save_message = Some("Named spawn placed.".to_string());
                    }
                }
                self.unsaved = true;
                self.ignore_drag = true;
                self.save_message_timer = 0;
                self.mode = EditorMode::Paint(ToolKind::Paint);
            }
        }
    }

    fn handle_palette_search_input(&mut self, input: &mut ember2d::input::InputManager) {
        self.mode = EditorMode::PaletteSearch;
        use ember2d::input::Key;
        // R12 (7A-2, docs/ember2d-master-plan.md): see script_editor.rs's
        // own R12 comment for why take_text() replaces key_to_char here.
        input.begin_text_capture();
        for ch in input.take_text().chars() {
            self.palette.search.push(ch);
        }
        if input.just_pressed(Key::Backspace) {
            self.palette.search.pop();
        }
        if input.just_pressed(Key::Enter) || input.just_pressed(Key::Escape) {
            self.mode = EditorMode::Paint(ToolKind::Paint);
        }
        if input.just_pressed(Key::Escape) {
            self.palette.search.clear();
        }
    }
}
