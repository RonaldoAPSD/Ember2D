// editor/input/mod.rs — Input orchestration for EditorState.

use crate::engine::UpdateContext;
use crate::editor::commands::Command;
use super::EditorState;

mod canvas;
mod graph;
mod panels;
mod shortcuts;
mod text;

impl EditorState {
    pub(super) fn handle_update(&mut self, ctx: UpdateContext) {
        let UpdateContext { input, mouse, .. } = ctx;

        // ── Ignore drag state ─────────────────────────────────────────────────
        if self.ignore_drag {
            if !mouse.left_held() {
                self.ignore_drag = false;
            }
        }

        // Graph editor mode swallows all input.
        if self.graph_mode.is_some() {
            self.update_graph_mode(input, mouse);
            return;
        }

        // Tick save message.
        if self.save_message.is_some() {
            self.save_message_timer += 1;
            if self.save_message_timer > 90 {
                self.save_message       = None;
                self.save_message_timer = 0;
            }
        }

        // ── Spawn placement modes ─────────────────────────────────────────────
        if self.placing_spawn {
            if input.just_pressed(crate::input::Key::Escape) || mouse.right_just_pressed() {
                self.placing_spawn = false;
                self.save_message = Some("Spawn placement cancelled.".to_string());
                self.save_message_timer = 0;
                return;
            }
            if mouse.left_just_pressed() {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    let before = self.grid.spawn_point;
                    let after  = (gx as f32, gy as f32);
                    self.undo.push(Command::MoveSpawn { before, after });
                    self.grid.spawn_point = after;
                    self.unsaved = true;
                    self.placing_spawn = false;
                    self.ignore_drag = true;
                    self.save_message = Some("Spawn placed.".to_string());
                    self.save_message_timer = 0;
                }
            }
            return;
        }
        if let Some(buf) = self.placing_named_spawn.clone() {
            if input.just_pressed(crate::input::Key::Escape) || mouse.right_just_pressed() {
                self.placing_named_spawn = None;
                self.save_message = Some("Spawn placement cancelled.".to_string());
                self.save_message_timer = 0;
                return;
            }
            if mouse.left_just_pressed() {
                if let Some((gx, gy)) = self.mouse_to_grid(mouse.cell_x, mouse.cell_y) {
                    let before = self.grid.extra_spawns.clone();
                    let mut after = before.clone();
                    after.push((buf, gx as f32, gy as f32));
                    self.undo.push(Command::UpdateExtraSpawns { before, after: after.clone() });
                    self.grid.extra_spawns = after;
                    self.unsaved = true;
                    self.placing_named_spawn = None;
                    self.ignore_drag = true;
                    self.save_message = Some("Named spawn placed.".to_string());
                    self.save_message_timer = 0;
                }
            }
            return;
        }

        // ── Text input ────────────────────────────────────────────────────────
        if self.text_input.is_some() {
            self.handle_text_input(input);
            return;
        }

        // ── Specialized interaction modes (panels, canvas, shortcuts) ──────────
        
        self.handle_panel_input(input, mouse);
        self.handle_canvas_input(input, mouse);
        self.handle_shortcuts(input, mouse);
    }
}
