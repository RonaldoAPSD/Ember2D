// editor/start_screen/logic.rs — State machine logic and helper methods for StartScreen.

use crate::project::ProjectData;
use crate::input::Key;
use super::mod_types::*;
use super::drawing::*;
use crate::editor::key_to_char;
use super::*;

impl StartScreen {
    pub(super) fn auto_folder(&self) -> String {
        if self.name_buf.is_empty() { return "untitled_project".to_string(); }
        self.name_buf.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' }).collect()
    }

    pub(super) fn init_fb(&mut self) {
        self.fb_path   = std::env::current_dir().unwrap_or_default();
        self.fb_cursor = 0;
        self.refresh_fb_entries();
    }

    pub(super) fn refresh_fb_entries(&mut self) {
        self.fb_entries.clear();
        self.fb_entries.push("\x00SELECT".to_string());
        if self.fb_path.parent().is_some() { self.fb_entries.push("\x00PARENT".to_string()); }
        if let Ok(rd) = std::fs::read_dir(&self.fb_path) {
            let mut dirs: Vec<String> = rd.filter_map(|e| e.ok()).filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.')).map(|e| e.file_name().to_string_lossy().to_string()).collect();
            dirs.sort(); self.fb_entries.extend(dirs);
        }
    }

    pub(super) fn refresh_projects(&mut self) { self.project_list = ProjectData::find_projects("."); self.project_cursor = 0; }

    pub(super) fn open_project(&mut self, folder: &str, quit: &mut bool) {
        let name   = ProjectData::name_for(folder);
        let levels = ProjectData::levels_in(folder);
        match levels.len() {
            0 => { self.result = Some(StartResult { project_folder: folder.to_string(), project_name: name, level_path: format!("{}/main.level", folder), template: Some(StartTemplate::Empty) }); *quit = true; }
            1 => { self.result = Some(StartResult { project_folder: folder.to_string(), project_name: name, level_path: levels.into_iter().next().unwrap(), template: None }); *quit = true; }
            _ => { self.sel_project = folder.to_string(); self.sel_project_name = name; self.level_list = levels; self.level_cursor = 0; self.screen = Screen::LevelPicker; }
        }
    }

    pub(super) fn confirm_template(&mut self, quit: &mut bool) {
        let tmpl = if self.template_sel == 0 { StartTemplate::Empty } else { StartTemplate::BasicRoom };
        let folder = self.folder_buf.clone();
        let level_path = std::path::Path::new(&folder).join("main.level").to_string_lossy().into_owned();
        self.result = Some(StartResult { level_path, project_folder: folder, project_name: self.name_buf.clone(), template: Some(tmpl) });
        *quit = true;
    }

    pub(super) fn update_logic(&mut self, input: &crate::input::InputManager, mouse: &crate::mouse::MouseState, sw: usize, _sh: usize, quit: &mut bool) {
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let mx = mouse.cell_x; let my = mouse.cell_y;
        let click = mouse.in_bounds && mouse.left_just_pressed();

        match self.screen {
            Screen::MainMenu => {
                if input.just_pressed(Key::Up) && self.menu_cursor > 0 { self.menu_cursor -= 1; }
                if input.just_pressed(Key::Down) && self.menu_cursor + 1 < MENU_LABELS.len() { self.menu_cursor += 1; }
                if input.just_pressed(Key::Key1) { self.menu_cursor = 0; }
                if input.just_pressed(Key::Key2) { self.menu_cursor = 1; }
                if input.just_pressed(Key::Key3) { self.menu_cursor = 2; }
                if mouse.in_bounds { for i in 0..MENU_LABELS.len() { if menu_item_hit(sw, mx, my, i) { self.menu_cursor = i; } } }
                if input.just_pressed(Key::Enter) || (click && menu_item_hit(sw, mx, my, self.menu_cursor)) {
                    match self.menu_cursor {
                        0 => { self.name_buf.clear(); self.screen = Screen::NewName; }
                        1 => { self.refresh_projects(); self.screen = Screen::OpenProject; }
                        _ => { *quit = true; }
                    }
                }
            }
            Screen::NewName => {
                for &key in crate::editor::TEXT_INPUT_KEYS { if input.just_pressed(key) { if let Some(ch) = key_to_char(key, shift) { self.name_buf.push(ch); } } }
                if input.just_pressed(Key::Backspace) { self.name_buf.pop(); }
                if input.just_pressed(Key::Escape) { self.screen = Screen::MainMenu; }
                if input.just_pressed(Key::Enter) && !self.name_buf.is_empty() { self.init_fb(); self.screen = Screen::FolderBrowser; }
            }
            Screen::FolderBrowser => {
                if input.just_pressed(Key::Escape) { self.screen = Screen::NewName; }
                if input.just_pressed(Key::Up) && self.fb_cursor > 0 { self.fb_cursor -= 1; }
                if input.just_pressed(Key::Down) && self.fb_cursor + 1 < self.fb_entries.len() { self.fb_cursor += 1; }
                let offset = if self.fb_cursor >= 11 { self.fb_cursor - 11 + 1 } else { 0 };
                if mouse.in_bounds { for vis_i in 0..11 { if folder_item_hit(sw, mx, my, vis_i) { self.fb_cursor = offset + vis_i; } } }
                let confirm = input.just_pressed(Key::Enter) || (click && !self.fb_entries.is_empty() && folder_item_hit(sw, mx, my, self.fb_cursor.saturating_sub(offset)));
                if confirm && !self.fb_entries.is_empty() {
                    match self.fb_entries[self.fb_cursor].as_str() {
                        "\x00SELECT" => { self.folder_buf = self.fb_path.join(self.auto_folder()).to_string_lossy().into_owned(); self.template_sel = 0; self.screen = Screen::NewTemplate; }
                        "\x00PARENT" => { if let Some(parent) = self.fb_path.parent() { self.fb_path = parent.to_path_buf(); self.fb_cursor = 0; self.refresh_fb_entries(); } }
                        dir_name => { self.fb_path = self.fb_path.join(dir_name); self.fb_cursor = 0; self.refresh_fb_entries(); }
                    }
                }
            }
            Screen::NewTemplate => {
                if input.just_pressed(Key::Left) || input.just_pressed(Key::Up) { if self.template_sel > 0 { self.template_sel -= 1; } }
                if input.just_pressed(Key::Right) || input.just_pressed(Key::Down) { if self.template_sel + 1 < TEMPLATE_LABELS.len() { self.template_sel += 1; } }
                if input.just_pressed(Key::Escape) { self.screen = Screen::FolderBrowser; }
                if input.just_pressed(Key::Enter) { self.confirm_template(quit); }
                if mouse.in_bounds { for i in 0..TEMPLATE_LABELS.len() { if template_item_hit(sw, mx, my, i) { self.template_sel = i; if click { self.confirm_template(quit); } } } }
            }
            Screen::OpenProject => {
                if input.just_pressed(Key::Escape) { self.screen = Screen::MainMenu; }
                if input.just_pressed(Key::Up) && self.project_cursor > 0 { self.project_cursor -= 1; }
                if input.just_pressed(Key::Down) && self.project_cursor + 1 < self.project_list.len() { self.project_cursor += 1; }
                let offset = if self.project_cursor >= 12 { self.project_cursor - 12 + 1 } else { 0 };
                if mouse.in_bounds { 
                    for list_i in 0..12 { 
                        if browser_item_hit(sw, mx, my, list_i) { 
                            let idx = offset + list_i;
                            if idx < self.project_list.len() {
                                self.project_cursor = idx; 
                            }
                        } 
                    } 
                }
                let confirm = input.just_pressed(Key::Enter) || (click && !self.project_list.is_empty() && browser_item_hit(sw, mx, my, self.project_cursor.saturating_sub(offset)));
                if confirm && !self.project_list.is_empty() { 
                    let folder = self.project_list[self.project_cursor].clone(); 
                    self.open_project(&folder, quit); 
                }
            }
            Screen::LevelPicker => {
                if input.just_pressed(Key::Escape) { self.screen = Screen::OpenProject; }
                if input.just_pressed(Key::Up) && self.level_cursor > 0 { self.level_cursor -= 1; }
                if input.just_pressed(Key::Down) && self.level_cursor + 1 < self.level_list.len() { self.level_cursor += 1; }
                let offset = if self.level_cursor >= 12 { self.level_cursor - 12 + 1 } else { 0 };
                if mouse.in_bounds { 
                    for list_i in 0..12 { 
                        if browser_item_hit(sw, mx, my, list_i) { 
                            let idx = offset + list_i;
                            if idx < self.level_list.len() {
                                self.level_cursor = idx; 
                            }
                        } 
                    } 
                }
                let confirm = input.just_pressed(Key::Enter) || (click && !self.level_list.is_empty() && browser_item_hit(sw, mx, my, self.level_cursor.saturating_sub(offset)));
                if confirm && !self.level_list.is_empty() { 
                    let path = self.level_list[self.level_cursor].clone(); 
                    self.result = Some(StartResult { project_folder: self.sel_project.clone(), project_name: self.sel_project_name.clone(), level_path: path, template: None }); 
                    *quit = true; 
                }
            }
        }
    }
}
