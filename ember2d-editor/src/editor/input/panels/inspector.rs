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

use super::super::super::commands::Command;
use super::super::super::panel::PanelId;
use super::super::super::ui::HierarchySelection;
use super::super::super::ui::{InspectorField, WidgetId};
use super::super::super::EditorMode;
use super::super::super::EditorState;
use super::super::super::TextInputPurpose;
use super::super::super::{InspTarget, ui};
use ember2d_sim::graph::NodeGraph;

impl EditorState {
    pub(super) fn handle_inspector_click(&mut self, mouse: &ember2d::mouse::MouseState) {
        let insp_tile_pos = match self.hierarchy_sel {
            Some(HierarchySelection::Player) => None,
            Some(HierarchySelection::Spawn(i)) => {
                self.grid.extra_spawns.get(i).map(|(_, x, y)| (*x as i32, *y as i32))
            }
            None => {
                if matches!(self.mode, EditorMode::Inspect) {
                    self.selected_pos
                } else {
                    self.inspected_pos
                }
            }
        };
        // Step 9-6: the wheel scrolls the Inspector's rows, clamped to the
        // subject's own row count (`ui::inspector_rows`, what draws them).
        if self.panels.visible(PanelId::Inspector) && mouse.wheel_y != 0.0 && mouse.in_bounds {
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            if self.panels.get(PanelId::Inspector).contains(px, py) {
                let rows = match (self.hierarchy_sel, insp_tile_pos) {
                    (Some(HierarchySelection::Player), _) => {
                        ui::inspector_rows(&ui::InspSubject::Player(&self.grid.player)).len()
                    }
                    (_, Some((gx, gy))) => self
                        .grid
                        .get(gx, gy, self.active_layer)
                        .map(|t| ui::inspector_rows(&ui::InspSubject::Tile(t)).len())
                        .unwrap_or(0),
                    _ => 0,
                };
                let metrics = ui::ChromeMetrics::from_theme(&self.theme);
                let content = self.panels.get(PanelId::Inspector).content_rect(&metrics);
                let max = ui::max_inspector_scroll(rows, content.h, self.theme.metrics.row_h);
                let step = if mouse.wheel_y > 0.0 { -2i32 } else { 2 };
                self.inspector_scroll =
                    (self.inspector_scroll as i32 + step).clamp(0, max as i32) as usize;
            }
        }
        if self.panels.visible(PanelId::Inspector) && mouse.left_just_pressed() && mouse.in_bounds {
            // 7D-3 checkpoint 7 (master plan §5.4): logical -> points, the
            // input choke point every chrome hit-test in this file goes
            // through.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let p = self.panels.get(PanelId::Inspector);
            if p.contains(px, py) {
                let hit = self.ui_frame.hit(px, py);
                self.ignore_drag = true;

                if self.hierarchy_sel == Some(HierarchySelection::Player) {
                    match hit {
                        Some(WidgetId::InspectorRow(InspectorField::Glyph)) => {
                            self.prompt_buffer = self.grid.player.glyph.to_string();
                            self.mode = EditorMode::Prompt(TextInputPurpose::PlayerGlyph);
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Tag)) => {
                            self.prompt_buffer = self.grid.player.tag.clone();
                            self.mode = EditorMode::Prompt(TextInputPurpose::PlayerTag);
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Script)) => {
                            self.prompt_buffer =
                                self.grid.player.script.clone().unwrap_or_default();
                            self.mode = EditorMode::Prompt(TextInputPurpose::PlayerScript);
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Layer)) => {
                            self.prompt_buffer = self.grid.player.collider_layer.clone();
                            self.mode = EditorMode::Prompt(TextInputPurpose::PlayerColliderLayer);
                        }
                        Some(WidgetId::InspectorRow(InspectorField::Mask)) => {
                            self.prompt_buffer = self.grid.player.collider_mask.join(",");
                            self.mode = EditorMode::Prompt(TextInputPurpose::PlayerColliderMask);
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
                        // Step 9-6: colours, collider size.
                        Some(WidgetId::InspectorRow(f)) => {
                            self.inspector_click_new_field(InspTarget::Player, f);
                        }
                        // Any non-Inspector hit: no-op, same as before.
                        _ => {}
                    }
                } else if let Some((gx, gy)) = insp_tile_pos {
                    if self.grid.get(gx, gy, self.active_layer).is_some() {
                        match hit {
                            Some(WidgetId::InspectorRow(InspectorField::GraphBtn)) => {
                                if self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .is_some_and(|t| t.graph.is_none())
                                {
                                    if let Some(tile) =
                                        self.grid.get(gx, gy, self.active_layer).cloned()
                                    {
                                        let mut new_tile = tile.clone();
                                        new_tile.graph = Some(NodeGraph::default());
                                        self.grid.place(gx, gy, self.active_layer, new_tile);
                                        self.unsaved = true;
                                    }
                                }
                                self.mode = EditorMode::Graph { gx, gy };
                                self.graph_view_ox = 4;
                                self.graph_view_oy = 3;
                                self.graph_selected_node = None;
                                self.graph_connecting = None;
                                self.graph_palette_open = None;
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Glyph)) => {
                                let g = self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .map(|t| t.glyph.to_string())
                                    .unwrap_or_default();
                                self.prompt_buffer = g;
                                self.mode =
                                    EditorMode::Prompt(TextInputPurpose::TileGlyph { gx, gy });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Tag)) => {
                                let tag = self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .map(|t| t.tag.clone())
                                    .unwrap_or_default();
                                self.prompt_buffer = tag;
                                self.mode =
                                    EditorMode::Prompt(TextInputPurpose::TileTag { gx, gy });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Script)) => {
                                let script = self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .and_then(|t| t.script.clone())
                                    .unwrap_or_default();
                                self.prompt_buffer = script;
                                self.mode =
                                    EditorMode::Prompt(TextInputPurpose::ScriptPath { gx, gy });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Exit)) => {
                                let path = self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .and_then(|t| t.next_level.clone())
                                    .unwrap_or_default();
                                self.prompt_buffer = path;
                                self.mode =
                                    EditorMode::Prompt(TextInputPurpose::TileNextLevel { gx, gy });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Layer)) => {
                                let layer = self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .map(|t| t.collider_layer.clone())
                                    .unwrap_or_default();
                                self.prompt_buffer = layer;
                                self.mode =
                                    EditorMode::Prompt(TextInputPurpose::TileColliderLayer {
                                        gx,
                                        gy,
                                    });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Mask)) => {
                                let mask = self
                                    .grid
                                    .get(gx, gy, self.active_layer)
                                    .map(|t| t.collider_mask.join(","))
                                    .unwrap_or_default();
                                self.prompt_buffer = mask;
                                self.mode =
                                    EditorMode::Prompt(TextInputPurpose::TileColliderMask {
                                        gx,
                                        gy,
                                    });
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Solid)) => {
                                if let Some(tile) =
                                    self.grid.get(gx, gy, self.active_layer).cloned()
                                {
                                    let mut new_tile = tile.clone();
                                    new_tile.solid = !new_tile.solid;
                                    self.undo.push(Command::Batch {
                                        cells: vec![(
                                            gx,
                                            gy,
                                            self.active_layer,
                                            Some(tile),
                                            Some(new_tile.clone()),
                                        )],
                                    });
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            Some(WidgetId::InspectorRow(InspectorField::Trigger)) => {
                                if let Some(tile) =
                                    self.grid.get(gx, gy, self.active_layer).cloned()
                                {
                                    let mut new_tile = tile.clone();
                                    new_tile.trigger = !new_tile.trigger;
                                    self.undo.push(Command::Batch {
                                        cells: vec![(
                                            gx,
                                            gy,
                                            self.active_layer,
                                            Some(tile),
                                            Some(new_tile.clone()),
                                        )],
                                    });
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            Some(WidgetId::InspectorRow(InspectorField::CameraFollow)) => {
                                if let Some(tile) =
                                    self.grid.get(gx, gy, self.active_layer).cloned()
                                {
                                    let mut new_tile = tile.clone();
                                    new_tile.camera_follow = !new_tile.camera_follow;
                                    self.undo.push(Command::Batch {
                                        cells: vec![(
                                            gx,
                                            gy,
                                            self.active_layer,
                                            Some(tile),
                                            Some(new_tile.clone()),
                                        )],
                                    });
                                    self.grid.place(gx, gy, self.active_layer, new_tile);
                                    self.unsaved = true;
                                }
                            }
                            // Step 9-6: colours, sprite, clip, the Actor section.
                            Some(WidgetId::InspectorRow(f)) => {
                                self.inspector_click_new_field(InspTarget::Tile { gx, gy }, f);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
