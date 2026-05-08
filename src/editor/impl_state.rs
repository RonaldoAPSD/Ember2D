// editor/impl_state.rs — Implementation of non-update/render methods for EditorState.

use std::collections::VecDeque;
use crate::engine::Transition;
use crate::scripting::{LogEntry, LogLevel};
use super::EditorState;
use super::commands::Command;
use super::ui::{ToolKind, ToolbarAction, transform_offset, bresenham};
use super::panel::PanelId;

impl EditorState {
    pub(super) fn save(&mut self) {
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

    pub(super) fn mouse_to_grid(&self, cell_x: usize, cell_y: usize) -> Option<(i32, i32)> {
        if self.panels.is_point_on_panel(cell_x, cell_y) { return None; }
        let l = &self.layout;
        if cell_x < l.canvas_x || cell_x >= l.canvas_x + l.canvas_w { return None; }
        if cell_y < l.canvas_y || cell_y >= l.canvas_y + l.canvas_h { return None; }
        let gx = (cell_x - l.canvas_x) as i32 + self.scroll.0;
        let gy = (cell_y - l.canvas_y) as i32 + self.scroll.1;
        Some((gx, gy))
    }

    pub(super) fn center_on(&mut self, gx: i32, gy: i32) {
        self.scroll.0 = (gx - self.layout.canvas_w as i32 / 2).max(0);
        self.scroll.1 = (gy - self.layout.canvas_h as i32 / 2).max(0);
        self.clamp_scroll();
    }

    pub(super) fn clamp_scroll(&mut self) {
        let max_x = (self.grid.width  as i32 - self.layout.canvas_w as i32).max(0);
        let max_y = (self.grid.height as i32 - self.layout.canvas_h as i32).max(0);
        self.scroll.0 = self.scroll.0.clamp(0, max_x);
        self.scroll.1 = self.scroll.1.clamp(0, max_y);
    }

    pub(super) fn apply_command(&mut self, cmd: &Command) {
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

    pub(super) fn reverse_command(&mut self, cmd: &Command) {
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

    pub(super) fn stamp_rect(&mut self, anchor: (i32, i32), current: (i32, i32)) {
        let x0 = anchor.0.min(current.0);
        let y0 = anchor.1.min(current.1);
        let x1 = anchor.0.max(current.0);
        let y1 = anchor.1.max(current.1);
        let mut cells = Vec::new();
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if !self.grid.in_bounds(gx, gy) { continue; }
                let new_tile = self.palette.current().to_tile_record(gx, gy);
                let before   = self.grid.get(gx, gy).cloned();
                self.grid.place(gx, gy, new_tile.clone());
                cells.push((gx, gy, before, Some(new_tile)));
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    pub(super) fn stamp_line(&mut self, anchor: (i32, i32), end: (i32, i32)) {
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

    pub(super) fn flood_fill(&mut self, sx: i32, sy: i32) {
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

    pub(super) fn erase_brush(&mut self, gx: i32, gy: i32) {
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

    pub(super) fn stamp_paste(&mut self, cursor: (i32, i32)) {
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

    pub(super) fn copy_selection(&mut self, anchor: (i32, i32), current: (i32, i32)) {
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

    pub(super) fn cut_selection(&mut self, anchor: (i32, i32), current: (i32, i32)) {
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

    pub(super) fn refresh_file_list(&mut self) {
        use crate::project::ProjectData;
        let dir = self.project_folder.as_deref().unwrap_or(".");
        self.file_list   = ProjectData::levels_in(dir);
        self.file_cursor = 0;
    }

    pub(super) fn make_player_tile_record(&self) -> crate::level::TileRecord {
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

    pub(super) fn clear_tool_modes(&mut self) {
        self.select_mode = false;
        self.selecting   = false;
        self.cutting     = false;
        self.pasting     = false;
        self.sel_anchor  = None;
        self.line_anchor = None;
        self.rect_anchor = None;
    }

    pub(super) fn dispatch_toolbar_action(&mut self, action: ToolbarAction) {
        use super::grid::LevelGrid;
        use super::commands::UndoStack;
        use super::TextInput;
        use super::TextInputPurpose;
        match action {
            ToolbarAction::SetTool(ToolKind::Paint) => { self.clear_tool_modes(); self.active_tool = ToolKind::Paint; }
            ToolbarAction::SetTool(ToolKind::Select) => { self.clear_tool_modes(); self.select_mode = true; self.active_tool = ToolKind::Select; }
            ToolbarAction::SetTool(ToolKind::Rect) => { self.clear_tool_modes(); self.active_tool = ToolKind::Rect; }
            ToolbarAction::SetTool(ToolKind::Line) => { self.clear_tool_modes(); self.active_tool = ToolKind::Line; }
            ToolbarAction::SetTool(ToolKind::Fill) => { self.clear_tool_modes(); self.active_tool = ToolKind::Fill; }
            ToolbarAction::SetTool(ToolKind::Copy) => { self.clear_tool_modes(); self.selecting = true; self.active_tool = ToolKind::Copy; }
            ToolbarAction::SetTool(ToolKind::Cut) => { self.clear_tool_modes(); self.cutting = true; self.active_tool = ToolKind::Cut; }
            ToolbarAction::SetTool(ToolKind::Paste) => { if !self.clipboard.is_empty() { self.clear_tool_modes(); self.pasting = true; self.active_tool = ToolKind::Paste; } }
            ToolbarAction::Undo => { if let Some(cmd) = self.undo.pop_undo() { self.reverse_command(&cmd); self.unsaved = true; } }
            ToolbarAction::Redo => { if let Some(cmd) = self.undo.pop_redo() { self.apply_command(&cmd); self.unsaved = true; } }
            ToolbarAction::ToggleGrid      => { self.show_grid = !self.show_grid; }
            ToolbarAction::ToggleInspector => { self.panels.toggle(PanelId::Inspector); }
            ToolbarAction::ToggleConsole   => { self.panels.toggle(PanelId::Console); }
            ToolbarAction::TogglePalette   => { self.panels.toggle(PanelId::Palette); }
            ToolbarAction::TogglePhysics   => { self.show_physics = !self.show_physics; }
            ToolbarAction::ToggleStats     => { self.panels.toggle(PanelId::Stats); }
            ToolbarAction::ToggleHelp      => { self.show_help = !self.show_help; }
            ToolbarAction::Save   => { self.save(); }
            ToolbarAction::SaveAs => { self.text_input = Some(TextInput { buffer: self.save_path.clone(), purpose: TextInputPurpose::SaveAs }); }
            ToolbarAction::Open => { self.refresh_file_list(); self.browsing = true; }
            ToolbarAction::New  => { self.grid = LevelGrid::new(super::DEFAULT_LEVEL_W, super::DEFAULT_LEVEL_H); self.undo = UndoStack::new(); self.unsaved = false; self.save_message = Some("New level created".to_string()); self.save_message_timer = 0; }
            ToolbarAction::Play => {
                let mut data = self.grid.to_level_data();
                data.path = self.save_path.clone();
                self.pending_transition = Some(Transition::ToPlay(data));
            }
            _ => {}
        }
    }

    pub fn receive_log(&mut self, entries: Vec<LogEntry>) {
        let had_errors = entries.iter().any(|e| e.level == LogLevel::Error);
        self.console_log.extend(entries);
        if self.console_log.len() > 200 {
            let drain = self.console_log.len() - 200;
            self.console_log.drain(..drain);
        }
        if had_errors { self.panels.show(PanelId::Console); }
    }
}
