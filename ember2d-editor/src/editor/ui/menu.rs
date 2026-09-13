// editor/ui/menu.rs — Menu system rendering and logic.

use super::frame::{UiFrame, WidgetId};
use super::rect::UiRect;
use super::types::*;
use ember2d::renderer::{color::Color, DrawSurface, Font};
use ember2d::theme::{PaletteRole, Theme};

pub const MENU_W: usize = 22;

pub enum MenuEntry {
    Item { label: &'static str, shortcut: &'static str, action: ToolbarAction },
    /// A runtime-known label (7D-4, master plan §5.4) — `theme_menu_entries`
    /// is the one place that builds these, since a shipped theme's
    /// directory name isn't a compile-time `&'static str` the way every
    /// other menu's entries are.
    DynamicItem { label: String, action: ToolbarAction },
    Sep,
}

fn menu_label_defs() -> &'static [(usize, &'static str, MenuKind)] {
    &[
        (1, "File", MenuKind::File),
        (7, "Edit", MenuKind::Edit),
        (13, "Level", MenuKind::Level),
        (20, "View", MenuKind::View),
        (26, "Tools", MenuKind::Tools),
        (33, "Layers", MenuKind::Layers),
        (41, "Theme", MenuKind::Theme),
    ]
}

/// `MenuKind::Theme`'s entries — one per `EditorState::available_themes`
/// (7D-4, master plan §5.4), unlike every other menu's fixed
/// `menu_entries` list. Both real call sites (`draw_menu_dropdown` below,
/// and `handle_menu_dropdown_click`'s re-resolve-by-index in
/// `input/panels/menu_bar.rs`) special-case `MenuKind::Theme` to call this
/// instead of `menu_entries`, so the two stay in sync the same way
/// `menu_entries` itself already had to for every other menu.
pub fn theme_menu_entries(available: &[String]) -> Vec<MenuEntry> {
    available
        .iter()
        .map(|name| MenuEntry::DynamicItem { label: name.clone(), action: ToolbarAction::SetTheme(name.clone()) })
        .collect()
}

fn menu_label_col(kind: MenuKind) -> usize {
    menu_label_defs().iter().find(|(_, _, k)| *k == kind).map(|(col, _, _)| *col).unwrap_or(1)
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
/// for every mode this indicator shows (`ui/menu.rs` has no reason to
/// depend on `editor::EditorMode` just to render six words).
pub fn draw_menu_toolbar(
    renderer: &mut dyn DrawSurface,
    font: &mut dyn Font,
    theme: &Theme,
    active_menu: Option<MenuKind>,
    mode_label: &str,
    frame: &mut UiFrame,
) {
    let row = TOOLBAR_ROW;
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let accent = theme.role_color(PaletteRole::Accent);
    renderer.draw_rect_filled(0, row, renderer.width(), 1, ' ', Color::White, panel_bg);
    for &(col, label, kind) in menu_label_defs() {
        let open = active_menu == Some(kind);
        // No themed "text-on-accent" role — see `chrome.rs`'s
        // `draw_dock_tabs` comment on this same gap.
        let (fg, bg) = if open { (Color::Black, accent) } else { (text_fg, panel_bg) };
        let padded = format!(" {} ", label);
        let draw_col = col.saturating_sub(1);
        renderer.draw_str(draw_col, row, &padded, fg, bg);
        frame.push(
            WidgetId::MenuLabel(kind),
            UiRect::from_cells(draw_col as i32, row as i32, cells(font, &padded), 1),
        );
    }
    let indicator = format!("[ {} ]", mode_label);
    let col = renderer.width().saturating_sub(cells(font, &indicator) + 1);
    renderer.draw_str(col, row, &indicator, accent, panel_bg);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_menu_dropdown(
    renderer: &mut dyn DrawSurface,
    theme: &Theme,
    menu: MenuKind,
    available_themes: &[String],
    mouse_col: usize,
    mouse_row: usize,
    ms: &MenuState,
    frame: &mut UiFrame,
) {
    let start_col = menu_label_col(menu);
    let start_row = TOOLBAR_ROW + 1;
    // `MenuKind::Theme`'s entries are runtime-known (`available_themes`),
    // not the fixed per-kind list every other menu draws from — see
    // `theme_menu_entries`'s own doc comment.
    let entries =
        if menu == MenuKind::Theme { theme_menu_entries(available_themes) } else { menu_entries(menu) };
    let panel_bg = theme.role_color(PaletteRole::PanelBg);
    let text_fg = theme.role_color(PaletteRole::TextPrimary);
    let dim = theme.role_color(PaletteRole::TextDim);
    let accent = theme.role_color(PaletteRole::Accent);
    renderer.draw_rect_filled(start_col, start_row, MENU_W, entries.len(), ' ', Color::White, panel_bg);
    for (i, entry) in entries.iter().enumerate() {
        let row = start_row + i;
        match entry {
            MenuEntry::Sep => {
                let line: String = std::iter::repeat_n('-', MENU_W).collect();
                renderer.draw_str(start_col, row, &line, dim, panel_bg);
                // No hit pushed — a separator was never clickable (the old
                // `menu_item_at` returned `None` for a `Sep` row too).
            }
            MenuEntry::Item { label, shortcut, action } => {
                let hovered =
                    mouse_row == row && mouse_col >= start_col && mouse_col < start_col + MENU_W;
                let enabled = is_action_enabled(action, ms);
                let check = menu_checkmark(action, ms);
                // No themed "text-on-accent" role — same gap as above.
                let (fg, bg) = if !enabled {
                    (dim, panel_bg)
                } else if hovered {
                    (Color::Black, accent)
                } else {
                    (text_fg, panel_bg)
                };
                let text = format!(" {} {:<11} {} ", check, label, shortcut);
                renderer.draw_str(start_col, row, &text, fg, bg);
                // Pushed at the same MENU_W-wide, one-row rect the
                // background fill above already covers for this row —
                // matches the old `menu_item_at`'s `start_col..start_col+
                // MENU_W` extent exactly (that one was never mismatched;
                // a disabled/greyed item was always still clickable,
                // no-opping harmlessly downstream — see this file's own
                // note on `is_action_enabled` being cosmetic only).
                frame.push(
                    WidgetId::MenuItem(menu, i),
                    UiRect::from_cells(start_col as i32, row as i32, MENU_W, 1),
                );
            }
            MenuEntry::DynamicItem { label, action } => {
                let hovered =
                    mouse_row == row && mouse_col >= start_col && mouse_col < start_col + MENU_W;
                let check = menu_checkmark(action, ms);
                // No themed "text-on-accent" role — same gap as above.
                let (fg, bg) = if hovered { (Color::Black, accent) } else { (text_fg, panel_bg) };
                let text = format!(" {} {:<width$} ", check, label, width = MENU_W.saturating_sub(4));
                let text: String = text.chars().take(MENU_W).collect();
                renderer.draw_str(start_col, row, &text, fg, bg);
                frame.push(
                    WidgetId::MenuItem(menu, i),
                    UiRect::from_cells(start_col as i32, row as i32, MENU_W, 1),
                );
            }
        }
    }
}
