// editor/ui/menu.rs — Menu system rendering and logic.

use crate::renderer::{color::Color, Renderer};
use super::types::*;

pub const MENU_W: usize = 22;

pub enum MenuEntry {
    Item { label: &'static str, shortcut: &'static str, action: ToolbarAction },
    Sep,
}

fn menu_label_defs() -> &'static [(usize, &'static str, MenuKind)] {
    &[
        (1,  "File",   MenuKind::File),
        (7,  "Edit",   MenuKind::Edit),
        (13, "Level",  MenuKind::Level),
        (20, "View",   MenuKind::View),
        (26, "Tools",  MenuKind::Tools),
        (33, "Layers", MenuKind::Layers),
    ]
}

fn menu_label_col(kind: MenuKind) -> usize {
    menu_label_defs().iter()
        .find(|(_, _, k)| *k == kind)
        .map(|(col, _, _)| *col)
        .unwrap_or(1)
}

pub fn menu_entries(kind: MenuKind) -> Vec<MenuEntry> {
    use MenuEntry::*;
    use ToolbarAction::*;
    use ToolKind::*;
    match kind {
        MenuKind::File => vec![
            Item { label: "New Level",  shortcut: "    ", action: NewLevel },
            Item { label: "New Script", shortcut: "    ", action: NewScript },
            Item { label: "Save",       shortcut: "S   ", action: Save      },

            Item { label: "Save As...", shortcut: "S+S ", action: SaveAs },
            Sep,
            Item { label: "Export Game...", shortcut: "    ", action: Export },
            Sep,
            Item { label: "Play",       shortcut: "F5  ", action: Play   },
            Sep,
            Item { label: "Close Project", shortcut: "    ", action: CloseProject },
        ],
        MenuKind::Edit => vec![
            Item { label: "Undo",         shortcut: "U/^Z", action: Undo        },
            Item { label: "Redo",         shortcut: "R/^Y", action: Redo        },
            Sep,
            Item { label: "Copy Select",  shortcut: "C   ", action: SetTool(Copy)  },
            Item { label: "Cut Select",   shortcut: "X   ", action: SetTool(Cut)   },
            Item { label: "Paste",        shortcut: "V   ", action: SetTool(Paste) },
        ],
        MenuKind::Level => vec![
            Item { label: "New Level",    shortcut: "    ", action: NewLevel      },
            Sep,
            Item { label: "Rename Level", shortcut: "N   ", action: RenameLevel  },
            Item { label: "Resize Level", shortcut: "Z   ", action: ResizeLevel  },
            Sep,
            Item { label: "Set Spawn",    shortcut: "P   ", action: SetSpawn     },
            Item { label: "Add Spawn...", shortcut: "S+P ", action: AddNamedSpawn},
        ],
        MenuKind::View => vec![
            Item { label: "Hierarchy", shortcut: "H   ", action: ToggleHierarchy },
            Item { label: "Palette",   shortcut: "B   ", action: TogglePalette   },
            Item { label: "Grid",      shortcut: "Tab ", action: ToggleGrid      },
            Item { label: "Physics",   shortcut: "G   ", action: TogglePhysics   },
            Item { label: "Stats",     shortcut: "`   ", action: ToggleStats     },
            Item { label: "Inspector", shortcut: "F2  ", action: ToggleInspector },
            Item { label: "Console",   shortcut: "F1  ", action: ToggleConsole   },
            Item { label: "Scripter",  shortcut: "    ", action: ToggleScriptEditor },
            Item { label: "Files",     shortcut: "    ", action: ToggleFileBrowser },
            Sep,
            Item { label: "Shortcuts", shortcut: "?   ", action: ToggleHelp      },
            Sep,
            Item { label: "API Docs",  shortcut: "    ", action: OpenDocs        },
        ],
        MenuKind::Tools => vec![
            Item { label: "Paint",  shortcut: "    ", action: SetTool(Paint)  },
            Item { label: "Select", shortcut: "Q   ", action: SetTool(Select) },
            Item { label: "Rect",   shortcut: "    ", action: SetTool(Rect)   },
            Item { label: "Line",   shortcut: "L   ", action: SetTool(Line)   },
            Item { label: "Fill",   shortcut: "F   ", action: SetTool(Fill)   },
        ],
        MenuKind::Layers => vec![
            Item { label: "Background", shortcut: "1   ", action: SetLayer(0) },
            Item { label: "Main",       shortcut: "2   ", action: SetLayer(1) },
            Item { label: "Foreground", shortcut: "3   ", action: SetLayer(2) },
        ],
    }
}

pub fn menu_label_at(col: usize) -> Option<MenuKind> {
    for &(start, label, kind) in menu_label_defs() {
        if col >= start && col < start + label.len() {
            return Some(kind);
        }
    }
    None
}

pub fn menu_item_at(menu: MenuKind, click_col: usize, click_row: usize, layout: &Layout) -> Option<ToolbarAction> {
    let start_col = menu_label_col(menu);
    if click_col < start_col || click_col >= start_col + MENU_W { return None; }
    let item_idx  = click_row.saturating_sub(layout.toolbar_row + 1);
    match menu_entries(menu).into_iter().nth(item_idx) {
        Some(MenuEntry::Item { action, .. }) => Some(action),
        _ => None,
    }
}

fn menu_checkmark(action: &ToolbarAction, ms: &MenuState) -> char {
    match action {
        ToolbarAction::SetTool(t)      => if *t == ms.active_tool { '>' } else { ' ' },
        ToolbarAction::TogglePalette   => if ms.show_palette   { 'x' } else { ' ' },
        ToolbarAction::ToggleGrid      => if ms.show_grid       { 'x' } else { ' ' },
        ToolbarAction::TogglePhysics   => if ms.show_physics    { 'x' } else { ' ' },
        ToolbarAction::ToggleInspector => if ms.show_inspector  { 'x' } else { ' ' },
        ToolbarAction::ToggleConsole   => if ms.show_console    { 'x' } else { ' ' },
        ToolbarAction::ToggleStats     => if ms.show_stats      { 'x' } else { ' ' },
        ToolbarAction::ToggleHierarchy => if ms.show_hierarchy  { 'x' } else { ' ' },
        ToolbarAction::ToggleScriptEditor => if ms.show_script_editor { 'x' } else { ' ' },
        ToolbarAction::ToggleFileBrowser  => if ms.show_file_browser  { 'x' } else { ' ' },
        ToolbarAction::SetLayer(l)     => if *l == ms.active_layer { 'x' } else { ' ' },
        _ => ' ',
    }
}

fn is_action_enabled(action: &ToolbarAction, ms: &MenuState) -> bool {
    match action {
        ToolbarAction::Undo                    => ms.can_undo,
        ToolbarAction::Redo                    => ms.can_redo,
        ToolbarAction::SetTool(ToolKind::Paste)=> ms.clipboard_full,
        _ => true,
    }
}

pub fn draw_menu_toolbar(renderer: &mut Renderer, active_menu: Option<MenuKind>, active_tool: ToolKind, layout: &Layout) {
    let row = layout.toolbar_row;
    renderer.draw_rect_filled(0, row, renderer.width, 1, ' ', Color::White, Color::DarkGrey);
    for &(col, label, kind) in menu_label_defs() {
        let open = active_menu == Some(kind);
        let (fg, bg) = if open { (Color::Black, Color::Cyan) } else { (Color::White, Color::DarkGrey) };
        let padded = format!(" {} ", label);
        renderer.draw_str(col.saturating_sub(1), row, &padded, fg, bg);
    }
    let tool_name = match active_tool {
        ToolKind::Paint  => "Paint ",
        ToolKind::Select => "Select",
        ToolKind::Rect   => "Rect  ",
        ToolKind::Line   => "Line  ",
        ToolKind::Fill   => "Fill  ",
        ToolKind::Copy   => "Copy  ",
        ToolKind::Cut    => "Cut   ",
        ToolKind::Paste  => "Paste ",
    };
    let indicator = format!("[ {} ]", tool_name);
    let col = renderer.width.saturating_sub(indicator.len() + 1);
    renderer.draw_str(col, row, &indicator, Color::Cyan, Color::DarkGrey);
}

pub fn draw_menu_dropdown(renderer: &mut Renderer, menu: MenuKind, mouse_col: usize, mouse_row: usize, ms: &MenuState, layout: &Layout) {
    let start_col = menu_label_col(menu);
    let start_row = layout.toolbar_row + 1;
    let entries   = menu_entries(menu);
    renderer.draw_rect_filled(start_col, start_row, MENU_W, entries.len(), ' ', Color::White, Color::Black);
    for (i, entry) in entries.iter().enumerate() {
        let row = start_row + i;
        match entry {
            MenuEntry::Sep => {
                let line: String = std::iter::repeat('-').take(MENU_W).collect();
                renderer.draw_str(start_col, row, &line, Color::DarkGrey, Color::Black);
            }
            MenuEntry::Item { label, shortcut, action } => {
                let hovered = mouse_row == row && mouse_col >= start_col && mouse_col < start_col + MENU_W;
                let enabled = is_action_enabled(action, ms);
                let check = menu_checkmark(action, ms);
                let (fg, bg) = if !enabled { (Color::DarkGrey, if hovered { Color::DarkBlue } else { Color::Black }) }
                else if hovered { (Color::Black, Color::Cyan) }
                else { (Color::White, Color::Black) };
                let text = format!(" {} {:<11} {} ", check, label, shortcut);
                renderer.draw_str(start_col, row, &text, fg, bg);
            }
        }
    }
}
