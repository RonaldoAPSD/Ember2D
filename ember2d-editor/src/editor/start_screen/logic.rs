// editor/start_screen/logic.rs — State machine logic and helper methods for StartScreen.

use super::super::ui::WidgetId;
use super::mod_types::*;
use super::*;
use crate::editor::helpers::{key_to_char, TEXT_INPUT_KEYS};
use ember2d::input::Key;
use ember2d::project::ProjectData;

/// Found live by the user (2026-09-12): both the New Project folder
/// browser and Open Project's own listing used to start at
/// `std::env::current_dir()` — the repo root, when launched via `cargo
/// run` — meaning every new/opened project browse began by staring at the
/// engine's own source tree. `Projects/` (repo root, alongside `demos/`
/// and `docs/` — see CLAUDE.md's "Workspace layout") is now the one
/// default location both flows start from instead; created on demand
/// (best-effort — a failure here just leaves the browser showing an empty
/// listing at a path that doesn't exist yet, not a hard error) so it's
/// there to browse into even on a fresh clone.
fn default_projects_dir() -> std::path::PathBuf {
    let dir = std::env::current_dir().unwrap_or_default().join("Projects");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

impl StartScreen {
    pub(super) fn auto_folder(&self) -> String {
        if self.name_buf.is_empty() {
            return "untitled_project".to_string();
        }
        self.name_buf
            .chars()
            .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect()
    }

    pub(super) fn init_fb(&mut self) {
        self.fb_path = default_projects_dir();
        self.fb_cursor = 0;
        self.refresh_fb_entries();
    }

    pub(super) fn refresh_fb_entries(&mut self) {
        self.fb_entries.clear();
        self.fb_entries.push("\x00SELECT".to_string());
        self.fb_entries.push("\x00OS_BROWSER".to_string());
        self.fb_entries.push("\x00NEW_FOLDER".to_string());
        if self.fb_path.parent().is_some() {
            self.fb_entries.push("\x00PARENT".to_string());
        }
        if let Ok(rd) = std::fs::read_dir(&self.fb_path) {
            let mut dirs: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            dirs.sort();
            self.fb_entries.extend(dirs);
        }
    }

    pub(super) fn refresh_projects(&mut self) {
        self.fb_entries.clear();
        self.fb_entries.push("\x00OS_BROWSER".to_string());
        if self.fb_path.parent().is_some() {
            self.fb_entries.push("\x00PARENT".to_string());
        }
        if let Ok(rd) = std::fs::read_dir(&self.fb_path) {
            let mut entries: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect();
            entries.sort();
            self.fb_entries.extend(entries);
        }
        self.fb_cursor = 0;
    }

    pub(super) fn open_project(&mut self, folder: &str, _quit: &mut bool) {
        let name = ProjectData::name_for(folder);
        let levels = ProjectData::levels_in(folder);
        let config = ProjectData::load(folder).unwrap_or_else(|_| {
            ProjectData::new(name.clone(), VisualStyle::ClassicASCII, GameplayLoop::RealTime)
        });

        let mut result = StartResult {
            project_folder: folder.to_string(),
            project_name: name.clone(),
            level_path: String::new(),
            template: None,
            visual_style: config.visual_style,
            gameplay_loop: config.gameplay_loop,
        };

        if let Some(ref start) = config.start_level {
            let path = std::path::Path::new(folder).join(start);
            if path.exists() {
                result.level_path = path.to_string_lossy().into_owned();
                self.pending_transition = Some(Transition::ToEditorWithResult(result));
                return;
            }
            if let Ok(rd) = std::fs::read_dir(folder) {
                for entry in rd.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.eq_ignore_ascii_case(start) {
                        result.level_path = entry.path().to_string_lossy().into_owned();
                        self.pending_transition = Some(Transition::ToEditorWithResult(result));
                        return;
                    }
                }
            }
        }

        match levels.len() {
            0 => {
                result.level_path = format!("{}/main.level", folder);
                result.template = Some(StartTemplate::Empty);
                self.pending_transition = Some(Transition::ToEditorWithResult(result));
            }
            1 => {
                result.level_path = levels.into_iter().next().unwrap();
                self.pending_transition = Some(Transition::ToEditorWithResult(result));
            }
            _ => {
                self.sel_project = folder.to_string();
                self.sel_project_name = name;
                self.level_list = levels;
                self.level_cursor = 0;
                self.screen = Screen::LevelPicker;
            }
        }
    }

    pub(super) fn confirm_template(&mut self, _quit: &mut bool) {
        let tmpl =
            if self.template_sel == 0 { StartTemplate::Empty } else { StartTemplate::BasicRoom };
        let folder = self.folder_buf.clone();
        let level_path =
            std::path::Path::new(&folder).join("main.level").to_string_lossy().into_owned();
        let visual_style =
            if self.style_sel == 0 { VisualStyle::ClassicASCII } else { VisualStyle::Sprites2D };
        let gameplay_loop =
            if self.loop_sel == 0 { GameplayLoop::RealTime } else { GameplayLoop::TurnBased };

        self.pending_transition = Some(Transition::ToEditorWithResult(StartResult {
            level_path,
            project_folder: folder,
            project_name: self.name_buf.clone(),
            template: Some(tmpl),
            visual_style,
            gameplay_loop,
        }));
    }

    pub(super) fn update_logic(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
        _sw: usize,
        _sh: usize,
        quit: &mut bool,
    ) {
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let click = mouse.in_bounds && mouse.left_just_pressed();
        // 7C-1 (master plan §5.3): every hover/click check below reads this
        // one `UiFrame::hit` instead of a per-screen removed `*_item_hit`
        // function recomputing its own draw-side layout math (E5).
        let hit = if mouse.in_bounds { self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) } else { None };

        match self.screen {
            Screen::MainMenu => {
                if input.just_pressed(Key::Up) && self.menu_cursor > 0 {
                    self.menu_cursor -= 1;
                }
                if input.just_pressed(Key::Down) && self.menu_cursor + 1 < MENU_LABELS.len() {
                    self.menu_cursor += 1;
                }
                if input.just_pressed(Key::Key1) {
                    self.menu_cursor = 0;
                }
                if input.just_pressed(Key::Key2) {
                    self.menu_cursor = 1;
                }
                if input.just_pressed(Key::Key3) {
                    self.menu_cursor = 2;
                }
                if let Some(WidgetId::StartMenuItem(i)) = hit {
                    self.menu_cursor = i;
                }
                if input.just_pressed(Key::Enter) || (click && matches!(hit, Some(WidgetId::StartMenuItem(_))))
                {
                    match self.menu_cursor {
                        0 => {
                            self.name_buf.clear();
                            self.screen = Screen::NewName;
                        }
                        1 => {
                            self.fb_path = default_projects_dir();
                            self.refresh_projects();
                            self.screen = Screen::OpenProject;
                        }
                        _ => {
                            *quit = true;
                        }
                    }
                }
            }
            Screen::NewName => {
                for &key in TEXT_INPUT_KEYS {
                    if input.just_pressed(key) {
                        if let Some(ch) = key_to_char(key, shift) {
                            self.name_buf.push(ch);
                        }
                    }
                }
                if input.just_pressed(Key::Backspace) {
                    self.name_buf.pop();
                }
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::MainMenu;
                }
                if input.just_pressed(Key::Enter) && !self.name_buf.is_empty() {
                    self.style_sel = 0;
                    self.screen = Screen::NewStyle;
                }
            }
            Screen::NewStyle => {
                if (input.just_pressed(Key::Left) || input.just_pressed(Key::Up))
                    && self.style_sel > 0
                {
                    self.style_sel -= 1;
                }
                if (input.just_pressed(Key::Right) || input.just_pressed(Key::Down))
                    && self.style_sel + 1 < STYLE_LABELS.len()
                {
                    self.style_sel += 1;
                }
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::NewName;
                }
                if input.just_pressed(Key::Enter) {
                    self.loop_sel = 0;
                    self.screen = Screen::NewLoop;
                }
                if let Some(WidgetId::StartTemplateItem(i)) = hit {
                    if i < STYLE_LABELS.len() {
                        self.style_sel = i;
                        if click {
                            self.loop_sel = 0;
                            self.screen = Screen::NewLoop;
                        }
                    }
                }
            }
            Screen::NewLoop => {
                if (input.just_pressed(Key::Left) || input.just_pressed(Key::Up))
                    && self.loop_sel > 0
                {
                    self.loop_sel -= 1;
                }
                if (input.just_pressed(Key::Right) || input.just_pressed(Key::Down))
                    && self.loop_sel + 1 < LOOP_LABELS.len()
                {
                    self.loop_sel += 1;
                }
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::NewStyle;
                }
                if input.just_pressed(Key::Enter) {
                    self.init_fb();
                    self.screen = Screen::FolderBrowser;
                }
                if let Some(WidgetId::StartTemplateItem(i)) = hit {
                    if i < LOOP_LABELS.len() {
                        self.loop_sel = i;
                        if click {
                            self.init_fb();
                            self.screen = Screen::FolderBrowser;
                        }
                    }
                }
            }
            Screen::FolderBrowser => {
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::NewLoop;
                }
                if input.just_pressed(Key::Up) && self.fb_cursor > 0 {
                    self.fb_cursor -= 1;
                }
                if input.just_pressed(Key::Down) && self.fb_cursor + 1 < self.fb_entries.len() {
                    self.fb_cursor += 1;
                }
                if let Some(WidgetId::StartFolderItem(idx)) = hit {
                    if idx < self.fb_entries.len() {
                        self.fb_cursor = idx;
                    }
                }
                let confirm = input.just_pressed(Key::Enter)
                    || (click
                        && !self.fb_entries.is_empty()
                        && matches!(hit, Some(WidgetId::StartFolderItem(_))));
                if confirm && !self.fb_entries.is_empty() {
                    if let Some(entry) = self.fb_entries.get(self.fb_cursor) {
                        match entry.as_str() {
                            "\x00SELECT" => {
                                self.folder_buf = self
                                    .fb_path
                                    .join(self.auto_folder())
                                    .to_string_lossy()
                                    .into_owned();
                                self.template_sel = 0;
                                self.screen = Screen::NewTemplate;
                            }
                            "\x00PARENT" => {
                                if let Some(parent) = self.fb_path.parent() {
                                    self.fb_path = parent.to_path_buf();
                                    self.fb_cursor = 0;
                                    self.refresh_fb_entries();
                                }
                            }
                            "\x00OS_BROWSER" => {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.fb_path = path;
                                    self.fb_cursor = 0;
                                    self.refresh_fb_entries();
                                }
                            }
                            "\x00NEW_FOLDER" => {
                                self.folder_buf.clear();
                                self.screen = Screen::NewFolder;
                            }
                            dir_name => {
                                self.fb_path = self.fb_path.join(dir_name);
                                self.fb_cursor = 0;
                                self.refresh_fb_entries();
                            }
                        }
                    }
                }
            }
            Screen::NewFolder => {
                for &key in TEXT_INPUT_KEYS {
                    if input.just_pressed(key) {
                        if let Some(ch) = key_to_char(key, shift) {
                            self.folder_buf.push(ch);
                        }
                    }
                }
                if input.just_pressed(Key::Backspace) {
                    self.folder_buf.pop();
                }
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::FolderBrowser;
                }
                if input.just_pressed(Key::Enter) && !self.folder_buf.is_empty() {
                    let new_path = self.fb_path.join(&self.folder_buf);
                    let _ = std::fs::create_dir_all(&new_path);
                    self.fb_path = new_path;
                    self.refresh_fb_entries();
                    self.screen = Screen::FolderBrowser;
                }
            }
            Screen::NewTemplate => {
                if (input.just_pressed(Key::Left) || input.just_pressed(Key::Up))
                    && self.template_sel > 0
                {
                    self.template_sel -= 1;
                }
                if (input.just_pressed(Key::Right) || input.just_pressed(Key::Down))
                    && self.template_sel + 1 < TEMPLATE_LABELS.len()
                {
                    self.template_sel += 1;
                }
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::FolderBrowser;
                }
                if input.just_pressed(Key::Enter) {
                    self.confirm_template(quit);
                }
                if let Some(WidgetId::StartTemplateItem(i)) = hit {
                    if i < TEMPLATE_LABELS.len() {
                        self.template_sel = i;
                        if click {
                            self.confirm_template(quit);
                        }
                    }
                }
            }
            Screen::OpenProject => {
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::MainMenu;
                }
                if input.just_pressed(Key::Up) && self.fb_cursor > 0 {
                    self.fb_cursor -= 1;
                }
                if input.just_pressed(Key::Down) && self.fb_cursor + 1 < self.fb_entries.len() {
                    self.fb_cursor += 1;
                }
                if let Some(WidgetId::StartBrowserItem(idx)) = hit {
                    if idx < self.fb_entries.len() {
                        self.fb_cursor = idx;
                    }
                }
                let confirm = input.just_pressed(Key::Enter)
                    || (click
                        && !self.fb_entries.is_empty()
                        && matches!(hit, Some(WidgetId::StartBrowserItem(_))));
                if confirm && !self.fb_entries.is_empty() {
                    if let Some(entry) = self.fb_entries.get(self.fb_cursor).cloned() {
                        match entry.as_str() {
                            "\x00PARENT" => {
                                if let Some(p) = self.fb_path.parent() {
                                    self.fb_path = p.to_path_buf();
                                    self.refresh_projects();
                                }
                            }
                            "\x00OS_BROWSER" => {
                                if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                    self.open_project(&path.to_string_lossy(), quit);
                                }
                            }
                            dir_name => {
                                let folder = self.fb_path.join(dir_name);
                                if folder.join("project.ron").exists() {
                                    self.open_project(&folder.to_string_lossy(), quit);
                                } else {
                                    self.fb_path = folder;
                                    self.refresh_projects();
                                }
                            }
                        }
                    }
                }
            }
            Screen::LevelPicker => {
                if input.just_pressed(Key::Escape) {
                    self.screen = Screen::OpenProject;
                }
                if input.just_pressed(Key::Up) && self.level_cursor > 0 {
                    self.level_cursor -= 1;
                }
                if input.just_pressed(Key::Down) && self.level_cursor + 1 < self.level_list.len() {
                    self.level_cursor += 1;
                }
                if let Some(WidgetId::StartBrowserItem(idx)) = hit {
                    if idx < self.level_list.len() {
                        self.level_cursor = idx;
                    }
                }
                let confirm = input.just_pressed(Key::Enter)
                    || (click
                        && !self.level_list.is_empty()
                        && matches!(hit, Some(WidgetId::StartBrowserItem(_))));
                if confirm && !self.level_list.is_empty() {
                    let path = self.level_list[self.level_cursor].clone();
                    let config = ProjectData::load(&self.sel_project).unwrap_or_else(|_| {
                        ProjectData::new(
                            self.sel_project_name.clone(),
                            VisualStyle::ClassicASCII,
                            GameplayLoop::RealTime,
                        )
                    });
                    self.pending_transition = Some(Transition::ToEditorWithResult(StartResult {
                        project_folder: self.sel_project.clone(),
                        project_name: self.sel_project_name.clone(),
                        level_path: path,
                        template: None,
                        visual_style: config.visual_style,
                        gameplay_loop: config.gameplay_loop,
                    }));
                }
            }
        }
    }
}
