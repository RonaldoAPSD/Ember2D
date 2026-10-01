// editor/input/panels/hierarchy_and_palette.rs — Hierarchy panel selection
// clicks, and Palette panel clicks (search bar, New/Edit buttons, rows).
// Split out of the single `input/panels.rs` in Phase 7 Part 1f
// (docs/ember2d-phase7-plan.md) — see `mod.rs`'s header comment for the
// split's overall shape and the `bool` "did this section consume the
// input" convention every extracted section follows.

use super::super::super::commands::Command;
use super::super::super::panel::PanelId;
use super::super::super::ui::ChromeMetrics;
use super::super::super::ui::HierarchySelection;
use super::super::super::ui::WidgetId;
use super::super::super::EditorMode;
use super::super::super::EditorState;

impl EditorState {
    /// 7C-1 (master plan §5.3): reads `UiFrame::hit` (populated by
    /// `draw_hierarchy` via `draw_row`) instead of recomputing `hier_row`
    /// from the panel's content origin independently here (E5) — the old
    /// arithmetic was never actually drifted from the draw side, just a
    /// second computation of the same relationship, same as
    /// `handle_palette_click`'s own note about the row math it replaced.
    pub(super) fn handle_hierarchy_click(&mut self, mouse: &ember2d::mouse::MouseState) -> bool {
        if self.panels.visible(PanelId::Hierarchy) && mouse.left_just_pressed() && mouse.in_bounds {
            // 7D-3 checkpoint 7 (master plan §5.4): logical -> points, the
            // input choke point every chrome hit-test in this file goes
            // through.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            if let Some(WidgetId::HierarchyRow(sel)) = self.ui_frame.hit(px, py) {
                match sel {
                    HierarchySelection::Player => {
                        self.hierarchy_sel = Some(sel);
                        self.center_on(
                            self.grid.spawn_point.0 as i32,
                            self.grid.spawn_point.1 as i32,
                        );
                    }
                    HierarchySelection::Spawn(idx) => {
                        if idx >= self.grid.extra_spawns.len() {
                            return false;
                        }
                        let (_, sx, sy) = self.grid.extra_spawns[idx];
                        self.hierarchy_sel = Some(sel);
                        self.center_on(sx as i32, sy as i32);
                    }
                }
                self.ignore_drag = true;
                return true;
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
            // 7D-3 checkpoint 7: logical -> points, same input choke point
            // as `handle_hierarchy_click` above.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            let p = self.panels.get(PanelId::Palette);
            // R72 (§3 in the master plan): was `p.content_h()` (a
            // CELL-ROUNDED row count) minus a literal `2` — an
            // approximation of `draw_palette_panel`'s own `max_rows`/
            // `visible_rows` (`ui/panels/dock.rs`) that only agreed with it
            // by coincidence at the old fixed 16px `CELL_H`, and drifted
            // once bars/rows followed the theme's real `row_h` instead —
            // the wheel could stop short of the palette's last few rows.
            // Recomputed here with the EXACT same formula the draw side
            // uses, from the same pixel `content_rect`, so scrolling can
            // never disagree with what's actually drawn.
            let metrics = ChromeMetrics::from_theme(&self.theme);
            let content = p.content_rect(&metrics);
            let max_rows = ((content.h / metrics.row_h).floor() as usize).max(1);
            let visible_rows = max_rows.saturating_sub(3);

            if p.contains(px, py) {
                use crate::editor::palette::PaletteRow;
                let layout = self.palette.build_layout();

                // Mouse wheel scroll — unaffected by this migration, not a
                // "click" with a drawn hitbox to drift from.
                if mouse.wheel_y != 0.0 {
                    let delta = -(mouse.wheel_y as i32);
                    let max_scroll = layout.len().saturating_sub(visible_rows);
                    self.palette_scroll =
                        (self.palette_scroll as i32 + delta).clamp(0, max_scroll as i32) as usize;
                }

                if mouse.left_just_pressed() {
                    self.ignore_drag = true;

                    match self.ui_frame.hit(px, py) {
                        Some(WidgetId::PaletteSearchBar) => {
                            self.mode = EditorMode::PaletteSearch;
                            return true;
                        }
                        Some(WidgetId::PaletteNewBtn) => {
                            // 7C-6 (master plan §5.3, D18): a standalone
                            // action, not part of a modal editor session —
                            // its own direct before/after snapshot.
                            let before = self.palette.clone();
                            self.palette.tiles.push(crate::editor::palette::TileDefinition {
                                name: "New Item".into(),
                                glyph: '?',
                                fg: ember2d::renderer::color::Color::White,
                                bg: ember2d::renderer::color::Color::Reset,
                                solid: false,
                                trigger: false,
                                tag: String::new(),
                                sprite: None,
                                clip: None,
                            });
                            self.undo.push(Command::UpdatePalette {
                                before,
                                after: self.palette.clone(),
                            });
                            self.palette.selected = self.palette.tiles.len() - 1;
                            self.palette_scroll = layout.len().saturating_sub(visible_rows);
                            self.save_message = Some("Added new palette item.".to_string());
                            self.save_message_timer = 0;
                            self.unsaved = true;
                            return true;
                        }
                        Some(WidgetId::PaletteEditBtn) => {
                            self.mode = EditorMode::PaletteEditor;
                            self.palette_editing_idx = self.palette.selected;
                            // 7C-6 (master plan §5.3, D18): the ONE place
                            // a fresh palette-edit session starts — snapshot
                            // here, not in `handle_palette_editor_input`
                            // (which re-enters `PaletteEditor` every frame
                            // the session stays open, including through a
                            // `ColorPicker` excursion that always returns
                            // to it). See `palette_edit_before`'s own doc
                            // comment for where this gets pushed.
                            self.palette_edit_before = Some(self.palette.clone());
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
            }
        }
        false
    }
}
