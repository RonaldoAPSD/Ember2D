// editor/input/panels/hierarchy_and_palette.rs — Hierarchy panel selection
// clicks, and Palette panel clicks (search bar, New/Edit buttons, rows).
// Split out of the single `input/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) — see `mod.rs`'s header comment for the
// split's overall shape and the `bool` "did this section consume the
// input" convention every extracted section follows.

use super::super::super::EditorState;
use super::super::super::panel::PanelId;
use super::super::super::ui::HierarchySelection;
use super::super::super::ui::WidgetId;

impl EditorState {
    pub(super) fn handle_hierarchy_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if self.panels.visible(PanelId::Hierarchy) && mouse.left_just_pressed() && mouse.in_bounds {
            let p = self.panels.get(PanelId::Hierarchy);
            let cy = p.content_y();
            if p.contains(mouse.pixel_x, mouse.pixel_y) && mouse.cell_y >= cy {
                let hier_row = mouse.cell_y - cy;
                if hier_row == 1 {
                    self.hierarchy_sel = Some(HierarchySelection::Player);
                    self.center_on(self.grid.spawn_point.0 as i32, self.grid.spawn_point.1 as i32);
                    self.ignore_drag = true;
                    return true;
                } else if hier_row >= 2 {
                    let idx = hier_row - 2;
                    if idx < self.grid.extra_spawns.len() {
                        let (_, sx, sy) = self.grid.extra_spawns[idx];
                        self.hierarchy_sel = Some(HierarchySelection::Spawn(idx));
                        self.center_on(sx as i32, sy as i32);
                        self.ignore_drag = true;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Phase 7 Part 1d (docs/ember2d-phase7-plan.md): row/button hit
    /// rects come from `UiFrame` now, pushed by `draw_palette_panel` at
    /// the exact spot each is drawn — see that function's own note for
    /// two real fixes this includes (the New/Edit buttons' hitboxes were
    /// each one cell narrower than their drawn text). The old
    /// independent `row_idx = scroll + (cell_y - (cy+2))` arithmetic
    /// (mathematically the same relationship as the draw loop's own
    /// `row = list_start + (i - scroll)`, just solved for `i` instead of
    /// `row` — not wildly drifted, but still a second computation with
    /// nothing tying it to the first) is gone.
    pub(super) fn handle_palette_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if self.panels.visible(PanelId::Palette) && mouse.in_bounds {
            let p = self.panels.get(PanelId::Palette);
            let ch = p.content_h();

            if p.contains(mouse.pixel_x, mouse.pixel_y) {
                use crate::editor::palette::PaletteRow;
                let layout = self.palette.build_layout();

                // Mouse wheel scroll — unaffected by this migration, not a
                // "click" with a drawn hitbox to drift from.
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = layout.len().saturating_sub(ch.saturating_sub(2));
                    self.palette_scroll = (self.palette_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() {
                    self.ignore_drag = true;
                    self.palette_search_focused = false; // Default clear

                    match self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                        Some(WidgetId::PaletteSearchBar) => {
                            self.palette_search_focused = true;
                            return true;
                        }
                        Some(WidgetId::PaletteNewBtn) => {
                            self.palette.tiles.push(crate::editor::palette::TileDefinition {
                                name: "New Item".into(),
                                glyph: '?',
                                fg: ember2d::renderer::color::Color::White,
                                bg: ember2d::renderer::color::Color::Reset,
                                solid: false,
                                trigger: false,
                                tag: String::new(),
                            });
                            self.palette.selected = self.palette.tiles.len() - 1;
                            self.palette_scroll = layout.len().saturating_sub(ch.saturating_sub(2));
                            self.save_message = Some("Added new palette item.".to_string());
                            self.save_message_timer = 0;
                            self.unsaved = true;
                            return true;
                        }
                        Some(WidgetId::PaletteEditBtn) => {
                            self.palette_editor_open = true;
                            self.palette_editing_idx = self.palette.selected;
                            return true;
                        }
                        Some(WidgetId::PaletteRow(idx)) => {
                            if let Some(row) = layout.get(idx) {
                                match row {
                                    PaletteRow::Header(name) => {
                                        if self.palette.collapsed.contains(name) {
                                            self.palette.collapsed.remove(name);
                                        } else {
                                            self.palette.collapsed.insert(name.clone());
                                        }
                                    }
                                    PaletteRow::Item(tile_idx) => {
                                        self.palette.select(*tile_idx);
                                    }
                                }
                            }
                            return true;
                        }
                        _ => {}
                    }
                }
            } else if mouse.left_just_pressed() {
                self.palette_search_focused = false;
            }
        }
        false
    }
}
