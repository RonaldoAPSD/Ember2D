// editor/input/panels/inspector.rs — Inspector panel field clicks. Split
// out of the single `input/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) — see `mod.rs`'s header comment for the
// split's overall shape. This is the LAST section `handle_panel_input`
// calls, so unlike every other extracted section it needs no `bool`
// "consumed" return — the original had no `return` anywhere in this
// section either, since there was nothing left after it to skip.
//
// Phase 7 Part 1d (docs/ember2d-phase7-plan.md): field hit rects come from
// `UiFrame` now, pushed by `draw_inspector` at the exact row each is drawn
// — see that function's own note on the structural fix this includes (a
// field's hitbox could previously exist on a row the panel wasn't even
// tall enough to have drawn). The old independent `X_row = cy + INSP_X_OFF`
// locals, and the separate `mouse.cell_y == cy + INSP_GRAPH_BTN` check that
// used to run before the general row match, are both gone — a graph-button
// hit is now just another `InspectorField` arm in the same match.

use super::super::super::EditorState;
use super::super::super::panel::PanelId;
use super::super::super::{TextInput, TextInputPurpose};
use super::super::super::ui::HierarchySelection;
use super::super::super::ui::{WidgetId, InspectorField};
use super::super::super::commands::Command;
use ember2d_sim::graph::NodeGraph;

impl EditorState {
    pub(super) fn handle_inspector_click(&mut self, mouse: &ember2d::mouse::MouseState) {
        let insp_tile_pos = match self.hierarchy_sel {
            Some(HierarchySelection::Player) => None,
            Some(HierarchySelection::Spawn(i)) => self.grid.extra_spawns.get(i)
                .map(|(_, x, y)| (*x as i32, *y as i32)),
            None => if self.select_mode { self.selected_pos } else { self.inspected_pos },
        };
        if self.panels.visible(PanelId::Inspector) && mouse.left_just_pressed() && mouse.in_bounds {
            let p = self.panels.get(PanelId::Inspector);
            if p.contains(mouse.pixel_x, mouse.pixel_y) {
                let hit = self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y);
                self.ignore_drag = true;

                if self.hierarchy_sel == Some(HierarchySelection::Player) {
                    match hit {
                        Some(WidgetId::InspectorRow(InspectorField::Glyph)) => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.glyph.to_string(),
                                purpose: TextInputPurpose::PlayerGlyph,
                            });
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Tag)) => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.tag.clone(),
                                purpose: TextInputPurpose::PlayerTag,
                            });
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Script)) => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.script.clone().unwrap_or_default(),
                                purpose: TextInputPurpose::PlayerScript,
                            });
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Layer)) => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.collider_layer.clone(),
                                purpose: TextInputPurpose::PlayerColliderLayer,
                            });
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Mask)) => {
                            self.text_input = Some(TextInput {
                                buffer: self.grid.player.collider_mask.join(","),
                                purpose: TextInputPurpose::PlayerColliderMask,
                            });
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Solid)) => {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.solid = !after.solid;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Trigger)) => {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.trigger = !after.trigger;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                        Some(WidgetId::InspectorRow(InspectorField::CameraFollow)) => {
                            let before = self.grid.player.clone();
                            let mut after = before.clone();
                            after.camera_follow = !after.camera_follow;
                            self.undo.push(Command::UpdatePlayer { before, after: after.clone() });
                            self.grid.player = after;
                            self.unsaved = true;
                        }
                        // `Exit`/`GraphBtn` (and any non-Inspector hit) have
                        // no Player equivalent — no-op, same as before.
                        _ => {}
                    }
                } else if let Some((gx, gy)) = insp_tile_pos {
                    if self.grid.get(gx, gy, self.active_layer).is_some() {
                        match hit {
                            Some(WidgetId::InspectorRow(InspectorField::GraphBtn)) => {
                                if self.grid.get(gx, gy, self.active_layer).map_or(false, |t| t.graph.is_none()) {
                                    if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                        let mut new_tile = tile.clone();
                                        new_tile.graph = Some(NodeGraph::default());
                                        self.grid.place(gx, gy, self.active_layer, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                self.graph_mode = Some((gx, gy));
                                self.graph_view_ox = 4;
                                self.graph_view_oy = 3;
                                self.graph_selected_node = None;
                                self.graph_connecting    = None;
                                self.graph_palette_open  = None;
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Glyph)) => {
                                let g = self.grid.get(gx, gy, self.active_layer).map(|t| t.glyph.to_string()).unwrap_or_default();
                                self.text_input = Some(TextInput { buffer: g, purpose: TextInputPurpose::TileGlyph { gx, gy } });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Tag)) => {
                                let tag = self.grid.get(gx, gy, self.active_layer).map(|t| t.tag.clone()).unwrap_or_default();
                                self.text_input = Some(TextInput { buffer: tag, purpose: TextInputPurpose::TileTag { gx, gy } });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Script)) => {
                                let script = self.grid.get(gx, gy, self.active_layer).and_then(|t| t.script.clone()).unwrap_or_default();
                                self.text_input = Some(TextInput { buffer: script, purpose: TextInputPurpose::ScriptPath { gx, gy } });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Exit)) => {
                                let path = self.grid.get(gx, gy, self.active_layer).and_then(|t| t.next_level.clone()).unwrap_or_default();
                                self.text_input = Some(TextInput { buffer: path, purpose: TextInputPurpose::TileNextLevel { gx, gy } });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Layer)) => {
                                let layer = self.grid.get(gx, gy, self.active_layer).map(|t| t.collider_layer.clone()).unwrap_or_default();
                                self.text_input = Some(TextInput { buffer: layer, purpose: TextInputPurpose::TileColliderLayer { gx, gy } });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Mask)) => {
                                let mask = self.grid.get(gx, gy, self.active_layer).map(|t| t.collider_mask.join(",")).unwrap_or_default();
                                self.text_input = Some(TextInput { buffer: mask, purpose: TextInputPurpose::TileColliderMask { gx, gy } });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Solid)) => {
                                if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.solid = !new_tile.solid;
                                    self.undo.push(Command::Batch { cells: vec![(gx, gy, self.active_layer, Some(tile), Some(new_tile.clone()))] });
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Trigger)) => {
                                if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.trigger = !new_tile.trigger;
                                    self.undo.push(Command::Batch { cells: vec![(gx, gy, self.active_layer, Some(tile), Some(new_tile.clone()))] });
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            Some(WidgetId::InspectorRow(InspectorField::CameraFollow)) => {
                                if let Some(tile) = self.grid.get(gx, gy, self.active_layer).cloned() {
                                    let mut new_tile = tile.clone();
                                    new_tile.camera_follow = !new_tile.camera_follow;
                                    self.undo.push(Command::Batch { cells: vec![(gx, gy, self.active_layer, Some(tile), Some(new_tile.clone()))] });
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
