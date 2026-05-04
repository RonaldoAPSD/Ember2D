// editor/mod.rs — Level editor: GameState with all editing tools.
//
// ── CONTROLS ─────────────────────────────────────────────────────────────────
//
//   Left-click / drag       — Paint selected tile
//   Right-click / drag      — Erase (brush size set by E key)
//   Shift + left-drag       — Rectangle fill
//   Alt   + left-drag       — Random scatter paint (~50% fill)
//   1–9                     — Select palette entry
//   U / R                   — Undo / Redo
//   F                       — Flood fill from cursor
//   L                       — Line tool (click anchor, click end to stamp)
//   C                       — Copy-select mode (drag to select, release copies)
//   X                       — Cut-select mode  (drag to select, release cuts)
//   V                       — Paste clipboard (click to stamp)
//   H / [ / ]               — In paste mode: flip-X / rotate-CCW / rotate-CW
//   E                       — Cycle eraser brush size (1→3→5→1)
//   N                       — Rename level
//   T                       — Attach script to tile under cursor
//   D                       — Set exit destination (next level path) on tile under cursor
//   I                       — Edit tag of tile under cursor
//   ;  / '                  — Toggle solid / trigger on tile under cursor
//   /                       — Find & replace: swap all tiles matching cursor tile
//   P                       — Move player spawn to cursor
//   Shift+P                 — Add named entity spawn at cursor
//   S / Shift+S             — Save / Save-as
//   Q                       — Toggle select mode (click to inspect, not paint)
//   Tab                     — Toggle grid overlay
//   F1                      — Toggle console (shows script errors from last play)
//   F2                      — Toggle inspector panel (small screens only; always visible on large)
//   F3                      — Clear console log
//   ~                       — Toggle tile statistics (replaces palette panel)
//   O                       — Open file browser
//   Arrow keys              — Scroll canvas (for levels larger than viewport)
//   F5 / Shift+F5           — Preview / Preview from cursor

pub mod commands;
pub mod grid;
pub mod palette;
pub mod panel;
pub mod node_graph;
pub mod start_screen;
pub mod ui;

mod impl_input;
mod impl_render;

use std::collections::VecDeque;

use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::event::EventBus;
use crate::input::Key;
use crate::level::TileRecord;
use crate::renderer::color::Color;
use crate::scripting::{LogEntry, LogLevel};
use crate::world::World;

use commands::{Command, UndoStack};
use grid::LevelGrid;
use palette::TilePalette;
use panel::{PanelId, PanelManager};
use start_screen::{StartResult, StartTemplate};
use ui::{Layout, ToolKind, ToolbarAction, HierarchySelection, MenuKind, bresenham, transform_offset};

// Default level size (used for initial grid). Independent from the viewport layout.
const DEFAULT_LEVEL_W: usize = 68;
const DEFAULT_LEVEL_H: usize = 22;

// ── Text input ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum TextInputPurpose {
    LevelName,
    SaveAs,
    ScriptPath       { gx: i32, gy: i32 },
    TileNextLevel    { gx: i32, gy: i32 },
    TileTag          { gx: i32, gy: i32 },
    TileGlyph        { gx: i32, gy: i32 },
    NamedSpawn,
    ResizeLevel,
    PlayerTag,
    PlayerScript,
    PlayerGlyph,
    NewLevelName,
}

#[derive(Debug, Clone)]
struct TextInput {
    buffer:  String,
    purpose: TextInputPurpose,
}

// ── EditorState ───────────────────────────────────────────────────────────────

pub struct EditorState {
    grid:    LevelGrid,
    palette: TilePalette,
    undo:    UndoStack,

    save_path: String,
    unsaved:   bool,
    show_grid: bool,

    save_message:       Option<String>,
    save_message_timer: u32,

    pending_transition: Option<Transition>,

    // ── Scroll / canvas pan ───────────────────────────────────────────────────
    scroll: (i32, i32),

    // ── Drawing tools ─────────────────────────────────────────────────────────
    rect_anchor: Option<(i32, i32)>,
    line_anchor: Option<(i32, i32)>,
    erase_size:  usize,   // 1, 3, or 5

    // ── Text input ────────────────────────────────────────────────────────────
    text_input: Option<TextInput>,

    // ── Copy / paste / cut ────────────────────────────────────────────────────
    selecting:  bool,
    cutting:    bool,
    sel_anchor: Option<(i32, i32)>,
    clipboard:  Vec<(i32, i32, TileRecord)>,
    pasting:    bool,
    paste_flip_x: bool,
    paste_flip_y: bool,
    paste_rotate: i32,    // 0=0°, 1=90°CW, 2=180°, 3=270°CW

    // ── File browser ──────────────────────────────────────────────────────────
    browsing:     bool,
    file_list:    Vec<String>,
    file_cursor:  usize,

    // ── Project context ───────────────────────────────────────────────────────
    project_folder: Option<String>,
    project_name:   Option<String>,

    // ── Console / inspector log ───────────────────────────────────────────────
    console_log: Vec<LogEntry>,

    // ── Inspector ─────────────────────────────────────────────────────────────
    /// Grid position of the last tile the mouse hovered over on the canvas.
    inspected_pos:  Option<(i32, i32)>,
    // ── Modes ─────────────────────────────────────────────────────────────────
    active_tool:  ToolKind,
    select_mode:  bool,
    selected_pos: Option<(i32, i32)>,
    hierarchy_sel: Option<HierarchySelection>,

    // ── Panning ───────────────────────────────────────────────────────────────
    pan_anchor:   Option<(usize, usize, i32, i32)>,
    scroll_repeat: u32,

    // ── UI state ──────────────────────────────────────────────────────────────
    /// Floating panel windows (Inspector, Palette, Console, Stats).
    panels:      PanelManager,
    show_physics: bool,
    show_help:    bool,
    active_menu:  Option<MenuKind>,

    // ── Graph editor mode ─────────────────────────────────────────────────────
    /// Some((gx, gy)) when the graph editor is open for that tile.
    graph_mode:          Option<(i32, i32)>,
    graph_view_ox:       i32,
    graph_view_oy:       i32,
    graph_selected_node: Option<node_graph::NodeId>,
    /// (from_node_id, from_port_dir_idx) — wire being drawn
    graph_connecting:    Option<(node_graph::NodeId, usize)>,
    /// (node_id, mouse_col_offset, mouse_row_offset)
    graph_dragging_node: Option<(node_graph::NodeId, i32, i32)>,
    graph_palette_open:  Option<(usize, usize)>,
    graph_palette_scroll: usize,
    graph_palette_cursor: usize,
    /// Inline param editing: (node_id, buffer)
    graph_editing_param: Option<(node_graph::NodeId, String)>,

    // ── Dynamic layout (updated every render frame from renderer size) ─────────
    layout: Layout,
}

impl EditorState {
    pub fn new(save_path: &str) -> Self {
        EditorState {
            grid:    LevelGrid::new(DEFAULT_LEVEL_W, DEFAULT_LEVEL_H),
            palette: TilePalette::default_palette(),
            undo:    UndoStack::new(),
            save_path: if save_path.is_empty() { "level.level".to_string() } else { save_path.to_string() },
            unsaved:      false,
            show_grid:    false,
            save_message:       None,
            save_message_timer: 0,
            pending_transition: None,
            scroll:      (0, 0),
            rect_anchor: None,
            line_anchor: None,
            erase_size:  1,
            text_input:  None,
            selecting:   false,
            cutting:     false,
            sel_anchor:  None,
            clipboard:   Vec::new(),
            pasting:     false,
            paste_flip_x: false,
            paste_flip_y: false,
            paste_rotate: 0,
            browsing:    false,
            file_list:   Vec::new(),
            file_cursor: 0,
            project_folder: None,
            project_name:   None,
            console_log:       Vec::new(),
            inspected_pos:     None,
            active_tool:    ToolKind::Paint,
            select_mode:    false,
            selected_pos:   None,
            hierarchy_sel:  None,
            pan_anchor:     None,
            scroll_repeat:  0,
            panels:       PanelManager::new(80, 24),
            show_physics: false,
            show_help:    false,
            active_menu:  None,
            graph_mode:          None,
            graph_view_ox:       0,
            graph_view_oy:       0,
            graph_selected_node: None,
            graph_connecting:    None,
            graph_dragging_node: None,
            graph_palette_open:  None,
            graph_palette_scroll: 0,
            graph_palette_cursor: 0,
            graph_editing_param: None,
            layout:       Layout::new(80, 24),
        }
    }

    pub fn load(path: &str) -> Result<Self, String> {
        use crate::level::LevelData;
        let data = LevelData::load(path).map_err(|e| format!("Failed to load '{}': {}", path, e))?;
        let mut editor = EditorState::new(path);
        editor.grid = LevelGrid::from_level_data(&data);
        Ok(editor)
    }

    pub fn new_from_result(result: StartResult) -> Result<Self, String> {
        use crate::project::ProjectData;
        match result.template {
            None => {
                let mut editor = EditorState::load(&result.level_path)?;
                editor.project_folder = Some(result.project_folder);
                editor.project_name   = Some(result.project_name);
                Ok(editor)
            }
            Some(template) => {
                std::fs::create_dir_all(&result.project_folder)
                    .map_err(|e| format!("Cannot create '{}': {}", result.project_folder, e))?;
                ProjectData::new(&result.project_name)
                    .save(&result.project_folder)
                    .map_err(|e| format!("Cannot write project.ron: {}", e))?;

                let mut editor = EditorState::new(&result.level_path);
                editor.grid.name    = result.project_name.clone();
                editor.project_folder = Some(result.project_folder);
                editor.project_name   = Some(result.project_name);

                if template == StartTemplate::BasicRoom {
                    apply_basic_room(&mut editor.grid);
                }
                editor.unsaved = true;
                Ok(editor)
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn save(&mut self) {
        let data = self.grid.to_level_data();
        match data.save(&self.save_path) {
            Ok(()) => {
                self.unsaved            = false;
                self.save_message       = Some(format!("Saved → {}", self.save_path));
                self.save_message_timer = 0;
            }
            Err(e) => {
                self.save_message       = Some(format!("Save FAILED: {}", e));
                self.save_message_timer = 0;
            }
        }
    }

    fn mouse_to_grid(&self, cell_x: usize, cell_y: usize) -> Option<(i32, i32)> {
        if self.panels.is_point_on_panel(cell_x, cell_y) { return None; }
        let l = &self.layout;
        if cell_x < l.canvas_x || cell_x >= l.canvas_x + l.canvas_w { return None; }
        if cell_y < l.canvas_y || cell_y >= l.canvas_y + l.canvas_h { return None; }
        let gx = (cell_x - l.canvas_x) as i32 + self.scroll.0;
        let gy = (cell_y - l.canvas_y) as i32 + self.scroll.1;
        Some((gx, gy))
    }

    fn center_on(&mut self, gx: i32, gy: i32) {
        self.scroll.0 = (gx - self.layout.canvas_w as i32 / 2).max(0);
        self.scroll.1 = (gy - self.layout.canvas_h as i32 / 2).max(0);
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let max_x = (self.grid.width  as i32 - self.layout.canvas_w as i32).max(0);
        let max_y = (self.grid.height as i32 - self.layout.canvas_h as i32).max(0);
        self.scroll.0 = self.scroll.0.clamp(0, max_x);
        self.scroll.1 = self.scroll.1.clamp(0, max_y);
    }

    fn apply_command(&mut self, cmd: &Command) {
        match cmd {
            Command::PlaceTile { after, .. } => { self.grid.place(after.x, after.y, after.clone()); }
            Command::EraseTile { before }    => { self.grid.erase(before.x, before.y); }
            Command::Batch { cells } => {
                for (x, y, _before, after) in cells {
                    match after {
                        Some(t) => { self.grid.place(*x, *y, t.clone()); }
                        None    => { self.grid.erase(*x, *y); }
                    }
                }
            }
        }
    }

    fn reverse_command(&mut self, cmd: &Command) {
        match cmd {
            Command::PlaceTile { before, after } => {
                self.grid.erase(after.x, after.y);
                if let Some(prev) = before { self.grid.place(prev.x, prev.y, prev.clone()); }
            }
            Command::EraseTile { before } => { self.grid.place(before.x, before.y, before.clone()); }
            Command::Batch { cells } => {
                for (x, y, before, _after) in cells {
                    match before {
                        Some(t) => { self.grid.place(*x, *y, t.clone()); }
                        None    => { self.grid.erase(*x, *y); }
                    }
                }
            }
        }
    }

    fn stamp_rect(&mut self, anchor: (i32, i32), current: (i32, i32)) {
        let x0 = anchor.0.min(current.0);
        let y0 = anchor.1.min(current.1);
        let x1 = anchor.0.max(current.0);
        let y1 = anchor.1.max(current.1);
        let mut cells = Vec::new();
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                let new_tile = self.palette.current().to_tile_record(gx, gy);
                let before   = self.grid.get(gx, gy).cloned();
                self.grid.place(gx, gy, new_tile.clone());
                cells.push((gx, gy, before, Some(new_tile)));
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    fn stamp_line(&mut self, anchor: (i32, i32), end: (i32, i32)) {
        let mut cells = Vec::new();
        for (gx, gy) in bresenham(anchor, end) {
            if !self.grid.in_bounds(gx, gy) { continue; }
            let new_tile = self.palette.current().to_tile_record(gx, gy);
            let before   = self.grid.get(gx, gy).cloned();
            self.grid.place(gx, gy, new_tile.clone());
            cells.push((gx, gy, before, Some(new_tile)));
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    fn flood_fill(&mut self, sx: i32, sy: i32) {
        let target_tile = self.grid.get(sx, sy).cloned();
        let new_def     = self.palette.current();
        if let Some(t) = &target_tile {
            if t.glyph == new_def.glyph && t.solid == new_def.solid && t.tag == new_def.tag { return; }
        }
        let mut cells   = Vec::new();
        let mut queue   = VecDeque::new();
        let mut visited = std::collections::HashSet::new();
        queue.push_back((sx, sy));
        visited.insert((sx, sy));
        while let Some((gx, gy)) = queue.pop_front() {
            if !self.grid.in_bounds(gx, gy) { continue; }
            let cell = self.grid.get(gx, gy).cloned();
            let matches = match (&cell, &target_tile) {
                (None, None)       => true,
                (Some(a), Some(b)) => a.glyph == b.glyph && a.solid == b.solid && a.tag == b.tag,
                _                  => false,
            };
            if !matches { continue; }
            let new_tile = new_def.to_tile_record(gx, gy);
            self.grid.place(gx, gy, new_tile.clone());
            cells.push((gx, gy, cell, Some(new_tile)));
            for (nx, ny) in [(gx-1,gy),(gx+1,gy),(gx,gy-1),(gx,gy+1)] {
                if !visited.contains(&(nx, ny)) { visited.insert((nx, ny)); queue.push_back((nx, ny)); }
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    fn find_replace_at(&mut self, gx: i32, gy: i32) {
        let target = match self.grid.get(gx, gy).cloned() {
            Some(t) => t,
            None    => return,
        };
        let new_def = self.palette.current();
        let positions: Vec<(i32, i32)> = self.grid.tiles.iter()
            .filter(|(_, t)| t.glyph == target.glyph && t.tag == target.tag)
            .map(|(&pos, _)| pos)
            .collect();
        let mut cells = Vec::new();
        for (px, py) in positions {
            let new_tile = new_def.to_tile_record(px, py);
            let before   = self.grid.get(px, py).cloned();
            self.grid.place(px, py, new_tile.clone());
            cells.push((px, py, before, Some(new_tile)));
        }
        if !cells.is_empty() {
            let count = cells.len();
            self.undo.push(Command::Batch { cells });
            self.unsaved     = true;
            self.save_message = Some(format!("Replaced {} tile(s)", count));
            self.save_message_timer = 0;
        }
    }

    fn erase_brush(&mut self, gx: i32, gy: i32) {
        let half = (self.erase_size as i32) / 2;
        let mut cells = Vec::new();
        for dy in -half..=half {
            for dx in -half..=half {
                let (ex, ey) = (gx + dx, gy + dy);
                if let Some(removed) = self.grid.erase(ex, ey) {
                    cells.push((ex, ey, Some(removed), None));
                }
            }
        }
        if !cells.is_empty() {
            if self.erase_size == 1 {
                let (_x, _y, before, _) = cells.remove(0);
                self.undo.push(Command::EraseTile { before: before.unwrap() });
            } else {
                self.undo.push(Command::Batch { cells });
            }
            self.unsaved = true;
        }
    }

    fn stamp_paste(&mut self, cursor: (i32, i32)) {
        let max_dx = self.clipboard.iter().map(|(dx,_,_)| *dx).max().unwrap_or(0);
        let max_dy = self.clipboard.iter().map(|(_,dy,_)| *dy).max().unwrap_or(0);
        let mut cells = Vec::new();
        let clipboard = std::mem::take(&mut self.clipboard);
        for (dx, dy, ref tile) in &clipboard {
            let (tdx, tdy) = transform_offset(*dx, *dy, max_dx, max_dy,
                                              self.paste_flip_x, self.paste_flip_y, self.paste_rotate);
            let gx = cursor.0 + tdx;
            let gy = cursor.1 + tdy;
            if !self.grid.in_bounds(gx, gy) { continue; }
            let mut new_tile = tile.clone();
            new_tile.x = gx;
            new_tile.y = gy;
            let before = self.grid.get(gx, gy).cloned();
            self.grid.place(gx, gy, new_tile.clone());
            cells.push((gx, gy, before, Some(new_tile)));
        }
        self.clipboard = clipboard;
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    fn copy_selection(&mut self, anchor: (i32, i32), current: (i32, i32)) {
        let x0 = anchor.0.min(current.0);
        let y0 = anchor.1.min(current.1);
        let x1 = anchor.0.max(current.0);
        let y1 = anchor.1.max(current.1);
        self.clipboard.clear();
        self.paste_flip_x = false;
        self.paste_flip_y = false;
        self.paste_rotate = 0;
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if let Some(tile) = self.grid.get(gx, gy).cloned() {
                    self.clipboard.push((gx - x0, gy - y0, tile));
                }
            }
        }
    }

    fn cut_selection(&mut self, anchor: (i32, i32), current: (i32, i32)) {
        self.copy_selection(anchor, current);
        let x0 = anchor.0.min(current.0);
        let y0 = anchor.1.min(current.1);
        let x1 = anchor.0.max(current.0);
        let y1 = anchor.1.max(current.1);
        let mut cells = Vec::new();
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if let Some(removed) = self.grid.erase(gx, gy) {
                    cells.push((gx, gy, Some(removed), None));
                }
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    fn refresh_file_list(&mut self) {
        use crate::project::ProjectData;
        let dir = self.project_folder.as_deref().unwrap_or(".");
        self.file_list   = ProjectData::levels_in(dir);
        self.file_cursor = 0;
    }

    fn make_player_tile_record(&self) -> crate::level::TileRecord {
        let sp = self.grid.spawn_point;
        let pr = &self.grid.player;
        crate::level::TileRecord {
            x: sp.0 as i32, y: sp.1 as i32,
            glyph: pr.glyph, fg: pr.fg, bg: pr.bg,
            solid: pr.solid, trigger: pr.trigger,
            tag: pr.tag.clone(), script: pr.script.clone(),
            camera_follow: pr.camera_follow,
            next_level: None,
            graph: None,
        }
    }

    fn clear_tool_modes(&mut self) {
        self.select_mode = false;
        self.selecting   = false;
        self.cutting     = false;
        self.pasting     = false;
        self.sel_anchor  = None;
        self.line_anchor = None;
        self.rect_anchor = None;
    }

    fn dispatch_toolbar_action(&mut self, action: ToolbarAction) {
        match action {
            ToolbarAction::SetTool(ToolKind::Paint) => {
                self.clear_tool_modes();
                self.active_tool = ToolKind::Paint;
            }
            ToolbarAction::SetTool(ToolKind::Select) => {
                self.clear_tool_modes();
                self.select_mode = true;
                self.active_tool = ToolKind::Select;
            }
            ToolbarAction::SetTool(ToolKind::Rect) => {
                self.clear_tool_modes();
                self.active_tool = ToolKind::Rect;
            }
            ToolbarAction::SetTool(ToolKind::Line) => {
                self.clear_tool_modes();
                self.active_tool = ToolKind::Line;
            }
            ToolbarAction::SetTool(ToolKind::Fill) => {
                self.clear_tool_modes();
                self.active_tool = ToolKind::Fill;
            }
            ToolbarAction::SetTool(ToolKind::Copy) => {
                self.clear_tool_modes();
                self.selecting   = true;
                self.active_tool = ToolKind::Copy;
            }
            ToolbarAction::SetTool(ToolKind::Cut) => {
                self.clear_tool_modes();
                self.cutting     = true;
                self.active_tool = ToolKind::Cut;
            }
            ToolbarAction::SetTool(ToolKind::Paste) => {
                if !self.clipboard.is_empty() {
                    self.clear_tool_modes();
                    self.pasting     = true;
                    self.active_tool = ToolKind::Paste;
                }
            }
            ToolbarAction::Undo => {
                if let Some(cmd) = self.undo.pop_undo() {
                    self.reverse_command(&cmd);
                    self.unsaved = true;
                }
            }
            ToolbarAction::Redo => {
                if let Some(cmd) = self.undo.pop_redo() {
                    self.apply_command(&cmd);
                    self.unsaved = true;
                }
            }
            ToolbarAction::ToggleGrid      => { self.show_grid = !self.show_grid; }
            ToolbarAction::ToggleInspector => { self.panels.toggle(PanelId::Inspector); }
            ToolbarAction::ToggleConsole   => { self.panels.toggle(PanelId::Console); }
            ToolbarAction::TogglePalette   => { self.panels.toggle(PanelId::Palette); }
            ToolbarAction::TogglePhysics   => { self.show_physics = !self.show_physics; }
            ToolbarAction::ToggleStats     => { self.panels.toggle(PanelId::Stats); }
            ToolbarAction::ToggleHelp      => { self.show_help = !self.show_help; }
            ToolbarAction::Save   => { self.save(); }
            ToolbarAction::SaveAs => {
                self.text_input = Some(TextInput {
                    buffer:  self.save_path.clone(),
                    purpose: TextInputPurpose::SaveAs,
                });
            }
            ToolbarAction::Open => { self.refresh_file_list(); self.browsing = true; }
            ToolbarAction::New  => {
                self.grid = LevelGrid::new(DEFAULT_LEVEL_W, DEFAULT_LEVEL_H);
                self.undo = UndoStack::new();
                self.unsaved = false;
                self.save_message = Some("New level created".to_string());
                self.save_message_timer = 0;
            }
            ToolbarAction::Play => {
                let mut data = self.grid.to_level_data();
                data.path = self.save_path.clone();
                self.pending_transition = Some(Transition::ToPlay(data));
            }
            // Quit/SetSpawn/AddNamedSpawn/FindReplace/RenameLevel/ResizeLevel
            // need context (mouse pos or quit flag) — handled inline in update()
            // before dispatch_toolbar_action is called, so these are unreachable here.
            ToolbarAction::CloseProject
            | ToolbarAction::FindReplace
            | ToolbarAction::SetSpawn
            | ToolbarAction::AddNamedSpawn
            | ToolbarAction::NewLevel
            | ToolbarAction::RenameLevel
            | ToolbarAction::ResizeLevel => {}
        }
    }

    /// Called by app.rs after play mode ends. Appends script log entries and
    /// auto-opens the console if there are any errors.
    pub fn receive_log(&mut self, entries: Vec<LogEntry>) {
        let had_errors = entries.iter().any(|e| e.level == LogLevel::Error);
        self.console_log.extend(entries);
        // Cap at 200 entries to avoid unbounded growth.
        if self.console_log.len() > 200 {
            let drain = self.console_log.len() - 200;
            self.console_log.drain(..drain);
        }
        if had_errors { self.panels.show(PanelId::Console); }
    }
}

// ── Key → char ────────────────────────────────────────────────────────────────

fn key_to_char(key: Key, shift: bool) -> Option<char> {
    let ch = match key {
        Key::A=>'a', Key::B=>'b', Key::C=>'c', Key::D=>'d', Key::E=>'e',
        Key::F=>'f', Key::G=>'g', Key::H=>'h', Key::I=>'i', Key::J=>'j',
        Key::K=>'k', Key::L=>'l', Key::M=>'m', Key::N=>'n', Key::O=>'o',
        Key::P=>'p', Key::Q=>'q', Key::R=>'r', Key::S=>'s', Key::T=>'t',
        Key::U=>'u', Key::V=>'v', Key::W=>'w', Key::X=>'x', Key::Y=>'y',
        Key::Z=>'z',
        Key::Key0=>'0', Key::Key1=>'1', Key::Key2=>'2', Key::Key3=>'3',
        Key::Key4=>'4', Key::Key5=>'5', Key::Key6=>'6', Key::Key7=>'7',
        Key::Key8=>'8', Key::Key9=>'9',
        Key::Period    => '.',
        Key::Slash     => '/',
        Key::Backslash => '\\',
        Key::Minus     => if shift { '_' } else { '-' },
        Key::Space     => ' ',
        _ => return None,
    };
    Some(if shift && ch.is_ascii_alphabetic() { ch.to_ascii_uppercase() } else { ch })
}

const TEXT_INPUT_KEYS: &[Key] = &[
    Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
    Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P,
    Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X,
    Key::Y, Key::Z,
    Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4,
    Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9,
    Key::Period, Key::Slash, Key::Backslash, Key::Minus, Key::Space,
];

// ── Template helpers ──────────────────────────────────────────────────────────

fn apply_basic_room(grid: &mut LevelGrid) {
    let w = grid.width as i32;
    let h = grid.height as i32;
    for gy in 0..h {
        for gx in 0..w {
            let on_edge = gx == 0 || gx == w - 1 || gy == 0 || gy == h - 1;
            let tile = if on_edge {
                TileRecord::new(gx, gy, '#', Color::Grey, Color::Reset, true, false, "wall")
            } else {
                TileRecord::new(gx, gy, '.', Color::DarkGrey, Color::Reset, false, false, "floor")
            };
            grid.place(gx, gy, tile);
        }
    }
    grid.spawn_point = (w as f32 / 2.0, h as f32 / 2.0);
}

// ── Graph helper fns ──────────────────────────────────────────────────────────

fn apply_param_edit(kind: &mut node_graph::NodeKind, buf: &str) {
    use node_graph::NodeKind;
    match kind {
        NodeKind::OnKeyHeld   { key }    => *key = buf.to_string(),
        NodeKind::OnKeyPress  { key }    => *key = buf.to_string(),
        NodeKind::OnCollide   { tag_filter } => *tag_filter = buf.to_string(),
        NodeKind::LoadLevel   { path }   => *path = buf.to_string(),
        NodeKind::PlaySound   { path }   => *path = buf.to_string(),
        NodeKind::FloatLit    { value }  => *value = buf.parse().unwrap_or(0.0),
        NodeKind::StringLit   { value }  => *value = buf.to_string(),
        NodeKind::GetVar      { name }   => *name = buf.to_string(),
        NodeKind::SetVar      { name }   => *name = buf.to_string(),
        NodeKind::Sequence { outputs }   => *outputs = buf.parse().unwrap_or(3).clamp(2, 8),
        _ => {}
    }
}

fn param_default_for(kind: &node_graph::NodeKind) -> String {
    use node_graph::NodeKind;
    match kind {
        NodeKind::OnKeyHeld   { key }    => key.clone(),
        NodeKind::OnKeyPress  { key }    => key.clone(),
        NodeKind::OnCollide   { tag_filter } => tag_filter.clone(),
        NodeKind::LoadLevel   { path }   => path.clone(),
        NodeKind::PlaySound   { path }   => path.clone(),
        NodeKind::FloatLit    { value }  => format!("{}", value),
        NodeKind::StringLit   { value }  => value.clone(),
        NodeKind::GetVar      { name }   => name.clone(),
        NodeKind::SetVar      { name }   => name.clone(),
        NodeKind::Sequence { outputs }   => format!("{}", outputs),
        _ => String::new(),
    }
}

// ── GameState ─────────────────────────────────────────────────────────────────

impl GameState for EditorState {
    fn on_start(&mut self, _world: &mut World, _events: &mut EventBus) {}

    fn update(&mut self, ctx: UpdateContext) {
        self.handle_update(ctx);
    }

    fn render(&mut self, ctx: RenderContext) {
        self.handle_render(ctx);
    }

    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }
}
