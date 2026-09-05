// editor/impl_state.rs — Implementation of non-update/render methods for EditorState.

use std::collections::VecDeque;
use std::path::Path;
use ember2d::engine::Transition;
use ember2d_sim::level::LevelData;
use ember2d::play::resolve_exit_path;
use ember2d_sim::scripting::LogEntry;
use super::EditorState;
use super::commands::Command;
use ember2d_sim::graph as node_graph;
use super::ui::{ToolKind, ToolbarAction, transform_offset, bresenham};
use super::panel::PanelId;

impl EditorState {
    pub(super) fn save(&mut self) {
        let mut data = self.grid.to_level_data();
        self.migrate_graph_sidecars(&mut data);
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
        self.save_palette();
    }

    /// Level format v2 (Step 3d): for each tile carrying a live node-graph
    /// (editor-authoring state — see `TileRecord::graph`'s doc comment),
    /// generate its Rhai source, combine it with whatever `tile.script`
    /// already pointed to (matching `play/spawn.rs::do_on_start`'s runtime
    /// combine, just moved to save time), and write the result to a sidecar
    /// `.rhai` file next to the level file. `tile.script` is repointed at the
    /// sidecar and `tile.graph` is dropped from the serialized record.
    ///
    /// `data` is `self.grid.to_level_data()`'s own fresh clone, so mutating
    /// it here never touches `self.grid` — the live editor keeps every
    /// graph fully editable after a save.
    fn migrate_graph_sidecars(&self, data: &mut LevelData) {
        let level_path = Path::new(&self.save_path);
        let dir = level_path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let stem = level_path.file_stem().and_then(|s| s.to_str()).unwrap_or("level");

        for tile in &mut data.tiles {
            let Some(graph) = tile.graph.take() else { continue };

            let mut source = node_graph::generate_graph(&graph);
            if let Some(ref path) = tile.script {
                let full = resolve_exit_path(path, &self.save_path);
                if let Ok(existing) = std::fs::read_to_string(&full) {
                    source.push('\n');
                    source.push_str(&existing);
                }
            }

            // Keyed by (x, y, layer) — the same tuple `LevelGrid` keys tiles
            // by — so every graph-bearing tile in the level gets a distinct,
            // stable sidecar name across repeated saves.
            let filename = format!("{}_graph_{}_{}_{}.rhai", stem, tile.x, tile.y, tile.layer);
            let sidecar_path = dir.join(&filename);
            if let Err(e) = std::fs::write(&sidecar_path, source) {
                eprintln!("Failed to write graph sidecar '{}': {}", sidecar_path.display(), e);
                continue;
            }
            tile.script = Some(filename);
        }
    }

    pub(super) fn save_palette(&mut self) {
        if let Some(ref folder) = self.project_folder {
            let path = format!("{}/project.palette.ron", folder);
            if let Err(e) = self.palette.save(&path) {
                eprintln!("Failed to save palette: {}", e);
            }
        }
    }

    pub(super) fn mouse_to_grid(&self, cell_x: usize, cell_y: usize) -> Option<(i32, i32)> {
        // 1. Block input if mouse is over any OTHER panel (except Viewport)
        if let Some(pid) = self.panels.panel_at(cell_x, cell_y) {
            if pid != PanelId::Viewport { return None; }
        }

        let l = &self.layout;
        // 2. Localize to canvas space
        if cell_x < l.canvas_x || cell_x >= l.canvas_x + l.canvas_w { return None; }
        if cell_y < l.canvas_y || cell_y >= l.canvas_y + l.canvas_h { return None; }
        
        let local_x = (cell_x - l.canvas_x) as f32;
        let local_y = (cell_y - l.canvas_y) as f32;

        // 3. Project to grid coordinates
        let gx = (local_x / self.zoom + self.scroll.0).floor() as i32;
        let gy = (local_y / self.zoom + self.scroll.1).floor() as i32;
        
        Some((gx, gy))
    }

    pub(super) fn center_on(&mut self, gx: i32, gy: i32) {
        self.target_scroll.0 = (gx as f32 - self.layout.canvas_w as f32 / 2.0 / self.zoom).max(0.0);
        self.target_scroll.1 = (gy as f32 - self.layout.canvas_h as f32 / 2.0 / self.zoom).max(0.0);
        self.clamp_scroll();
    }

    pub(super) fn clamp_scroll(&mut self) {
        let max_x = (self.grid.width  as f32 - self.layout.canvas_w as f32 / self.zoom).max(0.0);
        let max_y = (self.grid.height as f32 - self.layout.canvas_h as f32 / self.zoom).max(0.0);
        self.target_scroll.0 = self.target_scroll.0.clamp(0.0, max_x);
        self.target_scroll.1 = self.target_scroll.1.clamp(0.0, max_y);
    }

    pub(super) fn apply_command(&mut self, cmd: &Command) {
        match cmd {
            Command::PlaceTile { after, .. } => { self.grid.place(after.x, after.y, after.layer, after.clone()); }
            Command::EraseTile { before }    => { self.grid.erase(before.x, before.y, before.layer); }
            Command::Batch { cells } => {
                for &(x, y, layer, _, ref after) in cells {
                    match after {
                        Some(t) => { self.grid.place(x, y, layer, t.clone()); }
                        None    => { self.grid.erase(x, y, layer); }
                    }
                }
            }
            Command::ResizeLevel { after_w, after_h, .. } => {
                self.grid.resize(*after_w, *after_h);
            }
            Command::MoveSpawn { after, .. } => {
                self.grid.spawn_point = *after;
            }
            Command::UpdatePlayer { after, .. } => {
                self.grid.player = after.clone();
            }
            Command::UpdateExtraSpawns { after, .. } => {
                self.grid.extra_spawns = after.clone();
            }
        }
    }

    pub(super) fn reverse_command(&mut self, cmd: &Command) {
        match cmd {
            Command::PlaceTile { before, after } => {
                self.grid.erase(after.x, after.y, after.layer);
                if let Some(prev) = before { self.grid.place(prev.x, prev.y, prev.layer, prev.clone()); }
            }
            Command::EraseTile { before } => { self.grid.place(before.x, before.y, before.layer, before.clone()); }
            Command::Batch { cells } => {
                for &(x, y, layer, ref before, _) in cells {
                    match before {
                        Some(t) => { self.grid.place(x, y, layer, t.clone()); }
                        None    => { self.grid.erase(x, y, layer); }
                    }
                }
            }
            Command::ResizeLevel { before_w, before_h, before_tiles, .. } => {
                self.grid.resize(*before_w, *before_h);
                // Restoration of lost tiles. resize() clears outside bounds, so we put them back.
                for t in before_tiles {
                    self.grid.place(t.x, t.y, t.layer, t.clone());
                }
            }
            Command::MoveSpawn { before, .. } => {
                self.grid.spawn_point = *before;
            }
            Command::UpdatePlayer { before, .. } => {
                self.grid.player = before.clone();
            }
            Command::UpdateExtraSpawns { before, .. } => {
                self.grid.extra_spawns = before.clone();
            }
        }
    }

    pub(super) fn stamp_rect(&mut self, anchor: (i32, i32), current: (i32, i32)) {
        let x0 = anchor.0.min(current.0);
        let y0 = anchor.1.min(current.1);
        let x1 = anchor.0.max(current.0);
        let y1 = anchor.1.max(current.1);
        let mut cells = Vec::new();
        let lyr = self.active_layer;
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if !self.grid.in_bounds(gx, gy) { continue; }
                let new_tile = self.palette.current().to_tile_record(gx, gy);
                let before   = self.grid.get(gx, gy, lyr).cloned();
                self.grid.place(gx, gy, lyr, new_tile.clone());
                cells.push((gx, gy, lyr, before, Some(new_tile)));
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    pub(super) fn stamp_line(&mut self, anchor: (i32, i32), end: (i32, i32)) {
        let mut cells = Vec::new();
        let lyr = self.active_layer;
        for (gx, gy) in bresenham(anchor, end) {
            if !self.grid.in_bounds(gx, gy) { continue; }
            let new_tile = self.palette.current().to_tile_record(gx, gy);
            let before   = self.grid.get(gx, gy, lyr).cloned();
            self.grid.place(gx, gy, lyr, new_tile.clone());
            cells.push((gx, gy, lyr, before, Some(new_tile)));
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    pub(super) fn flood_fill(&mut self, sx: i32, sy: i32) {
        let lyr = self.active_layer;
        let target_tile = self.grid.get(sx, sy, lyr).cloned();
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
            let cell = self.grid.get(gx, gy, lyr).cloned();
            let matches = match (&cell, &target_tile) {
                (None, None)       => true,
                (Some(a), Some(b)) => a.glyph == b.glyph && a.solid == b.solid && a.tag == b.tag,
                _                  => false,
            };
            if !matches { continue; }
            let new_tile = new_def.to_tile_record(gx, gy);
            self.grid.place(gx, gy, lyr, new_tile.clone());
            cells.push((gx, gy, lyr, cell, Some(new_tile)));
            for (nx, ny) in [(gx-1,gy),(gx+1,gy),(gx,gy-1),(gx,gy+1)] {
                if !visited.contains(&(nx, ny)) { visited.insert((nx, ny)); queue.push_back((nx, ny)); }
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    pub(super) fn erase_brush(&mut self, gx: i32, gy: i32) {
        let half = (self.erase_size as i32) / 2;
        let mut cells = Vec::new();
        let lyr = self.active_layer;
        for dy in -half..=half {
            for dx in -half..=half {
                let (ex, ey) = (gx + dx, gy + dy);
                if let Some(removed) = self.grid.erase(ex, ey, lyr) {
                    cells.push((ex, ey, lyr, Some(removed), None));
                }
            }
        }
        if !cells.is_empty() {
            if self.erase_size == 1 {
                let (_x, _y, _l, before, _) = cells.remove(0);
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
        let lyr = self.active_layer;
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
            new_tile.layer = lyr;
            let before = self.grid.get(gx, gy, lyr).cloned();
            self.grid.place(gx, gy, lyr, new_tile.clone());
            cells.push((gx, gy, lyr, before, Some(new_tile)));
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
        let lyr = self.active_layer;
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if let Some(tile) = self.grid.get(gx, gy, lyr).cloned() {
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
        let lyr = self.active_layer;
        for gy in y0..=y1 {
            for gx in x0..=x1 {
                if let Some(removed) = self.grid.erase(gx, gy, lyr) {
                    cells.push((gx, gy, lyr, Some(removed), None));
                }
            }
        }
        if !cells.is_empty() { self.undo.push(Command::Batch { cells }); self.unsaved = true; }
    }

    pub(super) fn refresh_project_files(&mut self) {
        if let Some(ref root) = self.project_folder {
            let mut files = Vec::new();
            let current_path = if self.current_folder == "." {
                std::path::PathBuf::from(root)
            } else {
                std::path::Path::new(root).join(&self.current_folder)
            };

            // Add ".." if not at root
            if self.current_folder != "." {
                files.push(".. [UP]".to_string());
            }

            if let Ok(entries) = std::fs::read_dir(&current_path) {
                let mut dirs = Vec::new();
                let mut other = Vec::new();

                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                    
                    if path.is_dir() {
                        dirs.push(format!("/ {} ", name));
                    } else if name.ends_with(".level") {
                        other.push(format!("[] {} ", name));
                    } else if name.ends_with(".rhai") {
                        other.push(format!("{{}} {} ", name));
                    } else if name == "project.ron" || name.ends_with(".palette.ron") {
                        other.push(format!(":: {} ", name));
                    }
                }
                
                dirs.sort();
                other.sort();
                files.extend(dirs);
                files.extend(other);
            }

            self.file_browser_files = files;
            // Clamp cursor
            if self.file_browser_cursor >= self.file_browser_files.len() {
                self.file_browser_cursor = self.file_browser_files.len().saturating_sub(1);
            }
        }
    }

    pub(super) fn load_script(&mut self, name: &str) {
        if let Some(ref folder) = self.project_folder {
            let path = format!("{}/{}", folder, name);
            if let Ok(content) = std::fs::read_to_string(&path) {
                self.script_path    = Some(name.to_string());
                self.script_buffer  = content.lines().map(|s| s.to_string()).collect();
                if self.script_buffer.is_empty() { self.script_buffer.push(String::new()); }
                self.script_cursor  = (0, 0);
                self.script_scroll  = 0;
                self.script_unsaved = false;
                self.script_mode    = true;
                self.focused_panel  = Some(PanelId::ScriptEditor);
            } else {
                self.console_log.push(LogEntry::error(format!("Failed to load script: {}", name)));
            }
        }
    }

    pub(super) fn save_script(&mut self) {
        if let (Some(ref folder), Some(ref name)) = (&self.project_folder, &self.script_path) {
            let path = format!("{}/{}", folder, name);
            let content = self.script_buffer.join("\n");
            if let Err(e) = std::fs::write(&path, content) {
                self.console_log.push(LogEntry::error(format!("Failed to save script: {}", e)));
            } else {
                self.script_unsaved = false;
                self.save_message = Some(format!("Saved script: {}", name));
                self.save_message_timer = 0;
            }
        }
    }

    pub(super) fn make_player_tile_record(&self) -> ember2d_sim::level::TileRecord {
        let sp = self.grid.spawn_point;
        let pr = &self.grid.player;
        ember2d_sim::level::TileRecord {
            x: sp.0 as i32, y: sp.1 as i32,
            layer: 1,
            glyph: pr.glyph, fg: pr.fg, bg: pr.bg,
            solid: pr.solid, trigger: pr.trigger,
            tag: pr.tag.clone(), script: pr.script.clone(),
            collider_layer: pr.collider_layer.clone(),
            collider_mask: pr.collider_mask.clone(),
            camera_follow: pr.camera_follow,
            next_level: None,
            graph: None,
            texture: pr.texture.clone(),
            // The editor never authors an actor on the player tile — the
            // player is implicitly Local(0) always (see
            // TileRecord::actor's doc comment).
            actor: None,
        }
    }

    pub(super) fn start_text_input(&mut self, purpose: super::TextInputPurpose) {
        let initial = match &purpose {
            super::TextInputPurpose::LevelName => self.grid.name.clone(),
            super::TextInputPurpose::SaveAs => self.save_path.clone(),
            _ => String::new(),
        };
        self.text_input = Some(super::TextInput { buffer: initial, purpose });
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
            ToolbarAction::ToggleHierarchy => { self.panels.toggle(PanelId::Hierarchy); }
            ToolbarAction::ToggleScriptEditor => { self.panels.toggle(PanelId::ScriptEditor); }
            ToolbarAction::ToggleFileBrowser  => { self.panels.toggle(PanelId::FileBrowser); }
            ToolbarAction::TogglePhysics   => { self.show_physics = !self.show_physics; }
            ToolbarAction::ToggleStats     => { self.panels.toggle(PanelId::Stats); }
            ToolbarAction::ToggleHelp      => { self.show_help = !self.show_help; }
            ToolbarAction::Save   => { self.save(); }
            ToolbarAction::SaveAs => { self.text_input = Some(TextInput { buffer: self.save_path.clone(), purpose: TextInputPurpose::SaveAs }); }
            ToolbarAction::Export => { self.export_game(); }
            ToolbarAction::NewLevel  => { self.grid = LevelGrid::new(super::DEFAULT_LEVEL_W, super::DEFAULT_LEVEL_H); self.undo = UndoStack::new(); self.unsaved = false; self.save_message = Some("New level created".to_string()); self.save_message_timer = 0; }
            ToolbarAction::NewScript => {
                self.text_input = Some(TextInput { buffer: String::new(), purpose: TextInputPurpose::NewScriptName });
            }
            ToolbarAction::Play => {

                let mut data = self.grid.to_level_data();
                data.path = self.save_path.clone();
                self.pending_transition = Some(Transition::ToPlay(data));
            }
            ToolbarAction::OpenDocs => {
                #[cfg(target_os = "windows")]
                {
                    let _ = std::process::Command::new("cmd").args(&["/C", "start", "index.html"]).spawn();
                }
                #[cfg(target_os = "macos")]
                {
                    let _ = std::process::Command::new("open").arg("index.html").spawn();
                }
                #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                {
                    let _ = std::process::Command::new("xdg-open").arg("index.html").spawn();
                }
                self.save_message = Some("Opening documentation...".to_string());
                self.save_message_timer = 0;
            }
            ToolbarAction::SetLayer(l) => {
                self.active_layer = l;
                let name = match l { 0 => "Background", 1 => "Main", 2 => "Foreground", _ => "Unknown" };
                self.save_message = Some(format!("LAYER: {}", name));
                self.save_message_timer = 0;
            }
            _ => {}
        }
    }

    pub fn receive_log(&mut self, entries: Vec<LogEntry>) {
        self.console_log.extend(entries);
        if self.console_log.len() > 200 {
            let drain = self.console_log.len() - 200;
            self.console_log.drain(..drain);
        }
    }

    pub(super) fn export_game(&mut self) {
        let Some(project_path) = self.project_folder.clone() else {
            self.console_log.push(LogEntry::error("Cannot export: No project folder open."));
            return;
        };

        // Ensure current level is saved first
        self.save();

        #[cfg(debug_assertions)]
        self.console_log.push(LogEntry::warn(
            "Exporting a DEBUG build. For a release build run: cargo build --release, then export from target/release/ember2d."
        ));

        let picked = rfd::FileDialog::new()
            .set_title("Export Standalone Game - Pick Destination Folder")
            .pick_folder();

        if let Some(out_dir) = picked {
            let project_name = std::path::Path::new(&project_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "MyGame".to_string());
            
            let export_root = out_dir.join(format!("{}_Export", project_name));
            if let Err(e) = std::fs::create_dir_all(&export_root) {
                self.console_log.push(LogEntry::error(format!("Export failed (create dir): {}", e)));
                return;
            }

            // 1. Copy executable
            if let Ok(exe_path) = std::env::current_exe() {
                let mut target_exe = export_root.join(&project_name);
                if cfg!(windows) { target_exe.set_extension("exe"); }
                if let Err(e) = std::fs::copy(&exe_path, &target_exe) {
                    self.console_log.push(LogEntry::warn(format!("Executable copy failed: {}. You may need to copy it manually.", e)));
                }
            }

            // 2. Copy assets (recursive)
            let asset_folders = ["audio", "scripts"];
            for folder in asset_folders {
                let src = std::path::Path::new(&project_path).join(folder);
                if src.exists() {
                    let dst = export_root.join(folder);
                    if let Err(e) = copy_dir_all(&src, &dst) {
                        self.console_log.push(LogEntry::warn(format!("Failed to copy {}: {}", folder, e)));
                    }
                }
            }

            // 3. Copy project.ron, palette, and all levels
            if let Ok(entries) = std::fs::read_dir(&project_path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() {
                        let name = p.file_name().unwrap().to_string_lossy();
                        if name == "project.ron" || name.ends_with(".level") || name.ends_with(".palette.ron") {
                            let _ = std::fs::copy(&p, export_root.join(&*name));
                        }
                    }
                }
            }

            // 4. Create .standalone marker
            if let Err(e) = std::fs::write(export_root.join(".standalone"), "") {
                self.console_log.push(LogEntry::error(format!("Failed to create marker: {}", e)));
            } else {
                self.console_log.push(LogEntry::info(format!("SUCCESS: Game exported to {:?}", export_root)));
                self.panels.show(PanelId::Console);
            }
        }
    }
}

fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
        } else {
            std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ember2d_sim::level::TileRecord;
    use ember2d::renderer::color::Color;

    /// Step 3d's "done when": a level round-trip where a tile carries a live
    /// node-graph confirms `graph` never reaches the saved `.level` output
    /// and the sidecar `.rhai` file actually contains the generated code.
    #[test]
    fn graph_migrates_to_a_sidecar_script_and_never_appears_in_saved_output() {
        let dir = std::env::temp_dir().join("ember2d_test_graph_sidecar");
        std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
        let level_path = dir.join("test_level.level");
        let sidecar_path = dir.join("test_level_graph_5_5_1.rhai");
        let _ = std::fs::remove_file(&level_path);
        let _ = std::fs::remove_file(&sidecar_path);

        let mut editor = EditorState::new(level_path.to_str().unwrap());

        let mut graph = node_graph::NodeGraph::default();
        let on_start = graph.add_node(node_graph::NodeKind::OnStart, 0, 0);
        let log_node = graph.add_node(node_graph::NodeKind::Log, 100, 0);
        let str_lit  = graph.add_node(node_graph::NodeKind::StringLit { value: "hello from graph".to_string() }, 100, 50);
        graph.add_edge(on_start, 0, log_node, 0); // OnStart.Out -> Log.In
        graph.add_edge(str_lit, 0, log_node, 1);  // StringLit.Value -> Log.Msg

        let mut tile = TileRecord::new(5, 5, 1, '#', Color::White, Color::Reset, false, false, "npc");
        tile.graph = Some(graph);
        editor.grid.place(5, 5, 1, tile);

        editor.save();

        let saved = std::fs::read_to_string(&level_path).expect("level file must be written");
        assert!(!saved.contains("graph:"), "a saved level must never carry a `graph` field");
        assert!(saved.contains("test_level_graph_5_5_1.rhai"), "the tile's script field must point at the sidecar");

        let sidecar = std::fs::read_to_string(&sidecar_path).expect("sidecar script must be written");
        assert!(sidecar.contains("hello from graph"), "the sidecar must contain the generated Rhai source");

        // Mutating `to_level_data()`'s own clone must never touch the live
        // grid — the node-graph editor keeps working on the real graph.
        assert!(editor.grid.get(5, 5, 1).unwrap().graph.is_some());

        let _ = std::fs::remove_file(&level_path);
        let _ = std::fs::remove_file(&sidecar_path);
    }
}
