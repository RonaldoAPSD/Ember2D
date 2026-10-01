// editor/ui/menu.rs — Menu system rendering and logic.

use super::frame::{UiFrame, WidgetId};
use super::metrics::ChromeMetrics;
use super::rect::UiRect;
use super::types::*;
use super::widgets::{draw_row_px, draw_text_row};
use crate::editor::prefs::UiScaleChoice;
use ember2d::renderer::{color::Color, Font, UiPainter};
use ember2d::theme::{PaletteRole, Theme};
use ember2d_sim::math::Rect;

pub enum MenuEntry {
    Item {
        label: &'static str,
        shortcut: &'static str,
        action: ToolbarAction,
    },
    /// A runtime-known label (7D-4, master plan §5.4) — `theme_menu_entries`
    /// is the one place that builds these, since a shipped theme's
    /// directory name isn't a compile-time `&'static str` the way every
    /// other menu's entries are.
    DynamicItem {
        label: String,
        action: ToolbarAction,
    },
    Sep,
}

/// The top menu bar's labels, in draw/layout order (7D-3,
/// docs/ember2d-master-plan.md §5.4: dropped each label's own fixed CELL
/// column — `draw_menu_toolbar` now lays them out left-to-right by
/// MEASURED width instead, so a theme's own font determines real spacing
/// rather than an assumed monospace cell grid).
fn menu_label_defs() -> &'static [(&'static str, MenuKind)] {
    &[
        ("File", MenuKind::File),
        ("Edit", MenuKind::Edit),
        ("Level", MenuKind::Level),
        ("View", MenuKind::View),
        ("Tools", MenuKind::Tools),
        ("Layers", MenuKind::Layers),
        ("Theme", MenuKind::Theme),
    ]
}

/// `MenuKind::Theme`'s entries — one per `EditorState::available_themes`
/// (7D-4, master plan §5.4), unlike every other menu's fixed
/// `menu_entries` list, followed by a separator and the UI Scale picker
/// (7D-3 checkpoint 7, master plan §5.4: Auto/1x/2x/3x/4x — lives here,
/// not View, since View is already 13 rows and a short window at 4x would
/// overflow it, R81). Both real call sites (`draw_menu_dropdown` below,
/// and `handle_menu_dropdown_click`'s re-resolve-by-index in
/// `input/panels/menu_bar.rs`) special-case `MenuKind::Theme` to call this
/// instead of `menu_entries`, so the two stay in sync the same way
/// `menu_entries` itself already had to for every other menu.
pub fn theme_menu_entries(available: &[String]) -> Vec<MenuEntry> {
    let mut entries: Vec<MenuEntry> = available
        .iter()
        .map(|name| MenuEntry::DynamicItem {
            label: name.clone(),
            action: ToolbarAction::SetTheme(name.clone()),
        })
        .collect();
    entries.push(MenuEntry::Sep);
    entries.extend(UiScaleChoice::ALL.iter().map(|&choice| MenuEntry::DynamicItem {
        label: choice.menu_label(),
        action: ToolbarAction::SetUiScale(choice),
    }));
    entries
}

pub fn menu_entries(kind: MenuKind) -> Vec<MenuEntry> {
    use MenuEntry::*;
    use ToolKind::*;
    use ToolbarAction::*;
    match kind {
        MenuKind::File => vec![
            Item { label: "New Level", shortcut: "    ", action: NewLevel },
            Item { label: "New Script", shortcut: "    ", action: NewScript },
            Item { label: "Save", shortcut: "S   ", action: Save },
            Item { label: "Save As...", shortcut: "S+S ", action: SaveAs },
            Sep,
            Item { label: "Import Tileset...", shortcut: "    ", action: ImportTileset },
            Item { label: "Animation Clips...", shortcut: "    ", action: OpenClipEditor },
            Item { label: "Export Game...", shortcut: "    ", action: Export },
            Sep,
            Item { label: "Play", shortcut: "F5  ", action: Play },
            Sep,
            Item { label: "Close Project", shortcut: "    ", action: CloseProject },
        ],
        MenuKind::Edit => vec![
            Item { label: "Undo", shortcut: "U/^Z", action: Undo },
            Item { label: "Redo", shortcut: "R/^Y", action: Redo },
            Sep,
            Item { label: "Copy Select", shortcut: "C   ", action: EnterCopy },
            Item { label: "Cut Select", shortcut: "X   ", action: EnterCut },
            Item { label: "Paste", shortcut: "V   ", action: EnterPaste },
        ],
        MenuKind::Level => vec![
            Item { label: "New Level", shortcut: "    ", action: NewLevel },
            Sep,
            Item { label: "Rename Level", shortcut: "N   ", action: RenameLevel },
            Item { label: "Resize Level", shortcut: "Z   ", action: ResizeLevel },
            Sep,
            Item { label: "Set Spawn", shortcut: "P   ", action: SetSpawn },
            Item { label: "Add Spawn...", shortcut: "S+P ", action: AddNamedSpawn },
        ],
        MenuKind::View => vec![
            Item { label: "Hierarchy", shortcut: "H   ", action: ToggleHierarchy },
            Item { label: "Palette", shortcut: "B   ", action: TogglePalette },
            Item { label: "Grid", shortcut: "Tab ", action: ToggleGrid },
            Item { label: "Physics", shortcut: "G   ", action: TogglePhysics },
            Item { label: "Stats", shortcut: "`   ", action: ToggleStats },
            Item { label: "Inspector", shortcut: "F2  ", action: ToggleInspector },
            Item { label: "Console", shortcut: "F1  ", action: ToggleConsole },
            Item { label: "Scripter", shortcut: "    ", action: ToggleScriptEditor },
            Item { label: "Files", shortcut: "    ", action: ToggleFileBrowser },
            Sep,
            Item { label: "Shortcuts", shortcut: "?   ", action: ToggleHelp },
            Sep,
            Item { label: "API Docs", shortcut: "    ", action: OpenDocs },
        ],
        MenuKind::Tools => vec![
            Item { label: "Paint", shortcut: "    ", action: SetTool(Paint) },
            Item { label: "Select", shortcut: "Q   ", action: EnterInspect },
            Item { label: "Rect", shortcut: "    ", action: SetTool(Rect) },
            Item { label: "Line", shortcut: "L   ", action: SetTool(Line) },
            Item { label: "Fill", shortcut: "F   ", action: SetTool(Fill) },
        ],
        MenuKind::Layers => vec![
            Item { label: "Background", shortcut: "1   ", action: SetLayer(0) },
            Item { label: "Main", shortcut: "2   ", action: SetLayer(1) },
            Item { label: "Foreground", shortcut: "3   ", action: SetLayer(2) },
        ],
        // Never actually reached — both real call sites special-case
        // `MenuKind::Theme` to call `theme_menu_entries` instead (see its
        // own doc comment). Kept as an explicit empty arm, not folded into
        // a wildcard, so adding a genuinely new fixed-list `MenuKind` later
        // can't silently fall through here unnoticed.
        MenuKind::Theme => vec![],
    }
}

// `menu_label_at`/`menu_item_at` (independent hit-test functions) removed
// in Phase 7 Part 1d (docs/ember2d-phase7-plan.md) — replaced by
// `UiFrame::hit` reading `WidgetId::MenuLabel`/`MenuItem` entries
// `draw_menu_toolbar`/`draw_menu_dropdown` now push at the exact rect they
// draw. See `ui/frame.rs`'s header comment for why (defect E5). One real
// fix falls out of this for menu labels specifically: the old
// `menu_label_at` only covered a label's own text width (`"File"`, 4
// cells), narrower than the button's actual drawn extent (`" File "`, 6
// cells, padding included) — the padding cells were visibly part of the
// button but not clickable. The pushed rect below matches the padded draw
// call exactly, so the whole visible button is now clickable.

fn menu_checkmark(action: &ToolbarAction, ms: &MenuState) -> char {
    match action {
        ToolbarAction::SetTool(t) => {
            if *t == ms.active_tool {
                '>'
            } else {
                ' '
            }
        }
        ToolbarAction::EnterInspect => {
            if ms.inspecting {
                '>'
            } else {
                ' '
            }
        }
        ToolbarAction::EnterCopy => {
            if ms.copying {
                '>'
            } else {
                ' '
            }
        }
        ToolbarAction::EnterCut => {
            if ms.cutting {
                '>'
            } else {
                ' '
            }
        }
        ToolbarAction::EnterPaste => {
            if ms.pasting {
                '>'
            } else {
                ' '
            }
        }
        ToolbarAction::TogglePalette => {
            if ms.show_palette {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleGrid => {
            if ms.show_grid {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::TogglePhysics => {
            if ms.show_physics {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleInspector => {
            if ms.show_inspector {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleConsole => {
            if ms.show_console {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleStats => {
            if ms.show_stats {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleHierarchy => {
            if ms.show_hierarchy {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleScriptEditor => {
            if ms.show_script_editor {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::ToggleFileBrowser => {
            if ms.show_file_browser {
                'x'
            } else {
                ' '
            }
        }
        ToolbarAction::SetLayer(l) if *l == ms.active_layer => 'x',
        ToolbarAction::SetTheme(name) if *name == ms.current_theme => 'x',
        ToolbarAction::SetUiScale(choice) if *choice == ms.current_ui_scale => 'x',
        _ => ' ',
    }
}

fn is_action_enabled(action: &ToolbarAction, ms: &MenuState) -> bool {
    match action {
        ToolbarAction::Undo => ms.can_undo,
        ToolbarAction::Redo => ms.can_redo,
        ToolbarAction::EnterPaste => ms.clipboard_full,
        _ => true,
    }
}

/// `mode_label` is the toolbar's own status text ("Paint ", "Select",
/// "Copy  ", ...) — 7C-4 (master plan §5.3): computed by the caller from
/// `EditorMode` rather than passed as a bare `ToolKind`, since `ToolKind`
/// shrank to just the four `Paint` sub-tools and no longer has a variant
/// for every mode this indicator shows.
///
/// `metrics.bar_h` (7D-3, docs/ember2d-master-plan.md §5.4 — was a
/// hardcoded `CELL_H`) sizes this row; each label's own x position is now
/// MEASURED (was a fixed `CELL_W`-multiple column per label in
/// `menu_label_defs`), left-to-right, so real proportional text lays out
/// correctly instead of assuming a monospace cell grid. `WidgetId::MenuBar`
/// is pushed FIRST, covering the whole strip, so a click on empty toolbar
/// space still resolves to something (`UiFrame::hit`'s reverse search picks
/// whichever of the per-label pushes below wins when the click lands on
/// one of them instead) — replaces the old `mouse.cell_y == TOOLBAR_ROW`
/// raw-cell gate (`input/panels/menu_bar.rs`).
pub fn draw_menu_toolbar(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    metrics: &ChromeMetrics,
    active_menu: Option<MenuKind>,
    mode_label: &str,
    frame: &mut UiFrame,
) {
    let row_h = metrics.bar_h;
    let text_px = theme.font_sizes.body;
    let row_y = row_h; // one bar below the title bar, which occupies row 0
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let accent = theme.role_color(PaletteRole::Accent);
    let (pixel_w, _) = painter.space().screen_pt();
    painter.fill(Rect::new(0.0, row_y, pixel_w, row_h), panel_bg);
    frame.push(WidgetId::MenuBar, UiRect::new(0.0, row_y, pixel_w, row_h));
    let mut draw_x = 0.0;
    for &(label, kind) in menu_label_defs() {
        let open = active_menu == Some(kind);
        // No themed "text-on-accent" role — see `chrome.rs`'s
        // `draw_dock_tabs` comment on this same gap.
        let (fg, bg) = if open { (Color::Black, accent) } else { (text_fg, panel_bg) };
        let padded = format!(" {} ", label);
        let label_w = painter.measure(font, &padded, text_px);
        let label_rect = Rect::new(draw_x, row_y, label_w, row_h);
        draw_row_px(
            painter,
            frame,
            font,
            WidgetId::MenuLabel(kind),
            label_rect,
            text_px,
            &padded,
            fg,
            bg,
        );
        draw_x += label_w;
    }
    let indicator = format!("[ {} ]", mode_label);
    let indicator_w = painter.measure(font, &indicator, text_px);
    let pad = painter.measure(font, " ", text_px);
    let indicator_x = (pixel_w - indicator_w - pad).max(0.0);
    draw_text_row(
        painter,
        font,
        &indicator,
        Rect::new(indicator_x, row_y, indicator_w, row_h),
        text_px,
        accent,
        panel_bg,
    );
}

/// `metrics.bar_h` sizes each row; the dropdown's own x position is read
/// back from `WidgetId::MenuLabel(menu)`'s rect — pushed earlier THIS SAME
/// frame by `draw_menu_toolbar`, which always runs first — rather than
/// re-measuring the label layout a second time (7D-3, docs/ember2d-master-plan.md
/// §5.4: the same anti-drift discipline `ui/frame.rs`'s header comment
/// documents for every other widget). The hover check compares the mouse
/// position against each row's own drawn rect, replacing the old raw
/// `mouse_col`/`mouse_row` cell-int comparison. `mouse_x`/`mouse_y` MUST
/// already be in POINTS (`UiSpace::logical_to_pt` of `MouseState::pixel_x`
/// /`pixel_y`, same as `WidgetId::MenuItem`'s own `UiFrame::hit` lookup at
/// click time, `input/panels/menu_bar.rs`) — this call site used to pass
/// raw LOGICAL pixels straight through, so the drawn "hovered" highlight
/// silently drifted from wherever the cursor really was at any `ui_scale
/// != render_scale` (found live by the user, a menu open with the cursor
/// down near "Close Project" highlighting "Export Game..." instead — the
/// same class of bug as R88, this dropdown's own sibling code path R88's
/// fix never touched).
#[allow(clippy::too_many_arguments)]
pub fn draw_menu_dropdown(
    painter: &mut UiPainter,
    font: &mut dyn Font,
    theme: &Theme,
    metrics: &ChromeMetrics,
    menu: MenuKind,
    available_themes: &[String],
    mouse_x: f32,
    mouse_y: f32,
    ms: &MenuState,
    frame: &mut UiFrame,
) -> Option<usize> {
    let row_h = metrics.bar_h;
    let text_px = theme.font_sizes.body;
    let col_x = frame.rect_of(WidgetId::MenuLabel(menu)).map(|r| r.x).unwrap_or(0.0);
    let start_row_y = 2.0 * row_h; // below the title bar AND the toolbar
                                   // `MenuKind::Theme`'s entries are runtime-known (`available_themes`),
                                   // not the fixed per-kind list every other menu draws from — see
                                   // `theme_menu_entries`'s own doc comment.
    let entries = if menu == MenuKind::Theme {
        theme_menu_entries(available_themes)
    } else {
        menu_entries(menu)
    };
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    // Widest entry sets the dropdown's own width (7D-3, docs/ember2d-master-plan.md
    // §5.4) — was a fixed cell-count (`MENU_W` = 22) regardless of content.
    // `'>'` stands in for the widest realistic checkmark glyph when
    // measuring (the real draw below picks the actual one per-row).
    let menu_w_px = entries
        .iter()
        .map(|e| match e {
            MenuEntry::Sep => 0.0,
            MenuEntry::Item { label, shortcut, .. } => {
                painter.measure(font, &format!(" {} {:<11} {} ", '>', label, shortcut), text_px)
            }
            MenuEntry::DynamicItem { label, .. } => {
                painter.measure(font, &format!(" {} {} ", '>', label), text_px)
            }
        })
        .fold(0.0_f32, f32::max);
    painter.fill(Rect::new(col_x, start_row_y, menu_w_px, entries.len() as f32 * row_h), panel_bg);
    let mut hovered_row = None;
    for (i, entry) in entries.iter().enumerate() {
        let row_y = start_row_y + i as f32 * row_h;
        let row_rect = Rect::new(col_x, row_y, menu_w_px, row_h);
        let hovered = row_rect.contains_point(mouse_x, mouse_y);
        match entry {
            MenuEntry::Sep => {
                let line: String = "-".repeat(20);
                draw_text_row(painter, font, &line, row_rect, text_px, dim, panel_bg);
                // No hit pushed, and no `hovered_row` set — a separator was
                // never clickable and never actually highlights.
            }
            MenuEntry::Item { label, shortcut, action } => {
                let enabled = is_action_enabled(action, ms);
                let check = menu_checkmark(action, ms);
                // No themed "text-on-accent" role — same gap as above.
                let (fg, bg) = if !enabled {
                    (dim, panel_bg)
                } else if hovered {
                    hovered_row = Some(i);
                    (Color::Black, accent)
                } else {
                    (text_fg, panel_bg)
                };
                let text = format!(" {} {:<11} {} ", check, label, shortcut);
                draw_row_px(
                    painter,
                    frame,
                    font,
                    WidgetId::MenuItem(menu, i),
                    row_rect,
                    text_px,
                    &text,
                    fg,
                    bg,
                );
            }
            MenuEntry::DynamicItem { label, action } => {
                let check = menu_checkmark(action, ms);
                // No themed "text-on-accent" role — same gap as above.
                let (fg, bg) = if hovered {
                    hovered_row = Some(i);
                    (Color::Black, accent)
                } else {
                    (text_fg, panel_bg)
                };
                let text = format!(" {} {} ", check, label);
                draw_row_px(
                    painter,
                    frame,
                    font,
                    WidgetId::MenuItem(menu, i),
                    row_rect,
                    text_px,
                    &text,
                    fg,
                    bg,
                );
            }
        }
    }
    hovered_row
}
