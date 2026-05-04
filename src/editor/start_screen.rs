// editor/start_screen.rs — Launch screen shown when the editor opens with no file.
//
// Screens (state machine):
//   MainMenu    — 3 options: New Project, Open Project, Quit
//   NewName     — wizard step 1: type the project name
//   NewPath     — wizard step 2: type/confirm the project folder path
//   NewTemplate — wizard step 3: choose Empty Canvas or Basic Room
//   OpenProject — browse project folders in the current directory
//   LevelPicker — choose a level within the selected project (when > 1 level)
//
// Mouse support:
//   Hover over menu items / cards / list entries  → updates cursor highlight
//   Left click on an item                         → activates it (same as Enter)

use crate::engine::{GameState, RenderContext, UpdateContext};
use crate::event::EventBus;
use crate::input::Key;
use crate::project::ProjectData;
use crate::renderer::{color::Color, Renderer};
use crate::world::World;

// ── Public result types ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum StartTemplate {
    Empty,
    BasicRoom,
}

#[derive(Debug, Clone)]
pub struct StartResult {
    /// Path to the project folder (e.g. `"my_game"`).
    pub project_folder: String,
    /// Human-readable project name (e.g. `"My Game"`).
    pub project_name:   String,
    /// Full path to the level file to open (e.g. `"my_game/main.level"`).
    pub level_path:     String,
    /// `Some` = create new project + level; `None` = open existing level.
    pub template:       Option<StartTemplate>,
}

// ── Layout constants (all coordinates in character cells, 80×24 screen) ───────

const SCR_W: usize = 80;
const SCR_H: usize = 24;

// Main-menu list
const MENU_W: usize = 56;
const MENU_X: usize = (SCR_W - MENU_W) / 2;   // 12
const MENU_Y: usize = 10;

// Wizard text-input box
const WIZ_W: usize = 62;
const WIZ_X: usize = (SCR_W - WIZ_W) / 2;     // 9
const WIZ_Y: usize = 5;
const WIZ_H: usize = 12;

// Template-picker box
const TBOX_W: usize = 70;
const TBOX_X: usize = (SCR_W - TBOX_W) / 2;   // 5
const TBOX_Y: usize = 4;
const TBOX_H: usize = 15;
const TCARD_W:   usize = 28;
const TCARD_GAP: usize = 4;
const TCARD_X0:  usize = TBOX_X + (TBOX_W - (TCARD_W * 2 + TCARD_GAP)) / 2; // 10
const TCARD_Y:   usize = TBOX_Y + 4;  // 8
const TCARD_H:   usize = 8;

// Browser / picker box (Open Project + Level Picker)
const BROW_W: usize = 60;
const BROW_X: usize = (SCR_W - BROW_W) / 2;   // 10
const BROW_Y: usize = 4;
const BROW_H: usize = 16;
const BROW_LIST_Y: usize = BROW_Y + 2;
const BROW_MAX_VIS: usize = BROW_H - 4;

// Folder browser (New Project step 2)
const FB_W:       usize = 66;
const FB_X:       usize = (SCR_W - FB_W) / 2;  // 7
const FB_Y:       usize = 3;
const FB_H:       usize = 18;
const FB_LIST_Y:  usize = FB_Y + 5;             // 8
const FB_MAX_VIS: usize = FB_H - 7;             // 11

// ── State machine ─────────────────────────────────────────────────────────────
//
// The start screen is implemented as a finite state machine.
// Each variant of `Screen` represents one "page" the user can be on.
// Pressing Enter/Escape or clicking items transitions between states.
//
// Flow diagram:
//
//   MainMenu ─── New Project ──► NewName ──► FolderBrowser ──► NewTemplate ──► (quit → editor)
//            └── Open Project ─► OpenProject ──► LevelPicker ──► (quit → editor)
//                                                             └── (1 level: quit → editor)
//            └── Quit ──► (quit)

#[derive(Debug, Clone, PartialEq)]
enum Screen {
    /// The initial page: "New Project", "Open Project", "Quit".
    MainMenu,
    /// Wizard step 1: type the project name.
    NewName,
    /// Wizard step 2: browse folders to choose where to create the project.
    FolderBrowser,
    /// Wizard step 3: choose a starting template (Empty Canvas or Basic Room).
    NewTemplate,
    /// Browse existing project folders in the current directory.
    OpenProject,
    /// When a project has multiple levels, show a list to pick from.
    LevelPicker,
}

const MENU_LABELS: &[(&str, &str)] = &[
    ("New Project",  "Create a new project folder with levels inside"),
    ("Open Project", "Browse and open an existing Ember2D project"),
    ("Quit",         "Exit Ember2D"),
];

const TEMPLATE_LABELS: &[(&str, &str)] = &[
    ("Empty Canvas", "Blank grid — place everything yourself"),
    ("Basic Room",   "Floor, walls, and a player spawn pre-built"),
];

// ── Text-input key helpers ────────────────────────────────────────────────────

const TEXT_KEYS: &[Key] = &[
    Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H,
    Key::I, Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P,
    Key::Q, Key::R, Key::S, Key::T, Key::U, Key::V, Key::W, Key::X,
    Key::Y, Key::Z,
    Key::Key0, Key::Key1, Key::Key2, Key::Key3, Key::Key4,
    Key::Key5, Key::Key6, Key::Key7, Key::Key8, Key::Key9,
    Key::Period, Key::Slash, Key::Backslash, Key::Minus, Key::Space,
];

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

// ── StartScreen ───────────────────────────────────────────────────────────────

pub struct StartScreen {
    /// Which screen/page the user is currently on.
    screen:       Screen,
    /// Highlighted row in the main menu (0 = New, 1 = Open, 2 = Quit).
    menu_cursor:  usize,

    // ── New-project wizard state ──────────────────────────────────────────────

    /// Text the user has typed for the project name (step 1).
    name_buf:     String,
    /// The final project folder path chosen in step 2 (e.g. "projects/my_game").
    folder_buf:   String,
    /// Which template card is highlighted (0 = Empty, 1 = Basic Room).
    template_sel: usize,

    // ── Open-project browser state ────────────────────────────────────────────

    /// Project folder names found by `ProjectData::find_projects(".")`.
    project_list:   Vec<String>,
    /// Currently highlighted project row.
    project_cursor: usize,

    // ── Level picker state (shown when a project has > 1 level) ──────────────

    /// Folder path of the project the user selected in OpenProject.
    sel_project:      String,
    /// Display name of the selected project (from project.ron or folder name).
    sel_project_name: String,
    /// Full paths to all .level files in the selected project.
    level_list:       Vec<String>,
    /// Currently highlighted level row.
    level_cursor:     usize,

    // ── Folder browser state (New Project step 2) ─────────────────────────────

    /// The directory currently being browsed for project placement.
    fb_path:    std::path::PathBuf,
    /// Entries in the browser: special sentinel strings first, then subdirs.
    ///
    /// "\x00SELECT" = "use this folder" action
    /// "\x00PARENT" = navigate to parent directory
    /// Everything else = a subdirectory name the user can navigate into.
    fb_entries: Vec<String>,
    /// Currently highlighted row in the folder browser.
    fb_cursor:  usize,

    /// Set when the user finishes the wizard or picks an existing level.
    /// The engine's run() loop ends, and main.rs reads this value to know
    /// which level to open and whether to apply a template.
    pub result: Option<StartResult>,
}

impl StartScreen {
    pub fn new() -> Self {
        StartScreen {
            screen:           Screen::MainMenu,
            menu_cursor:      0,
            name_buf:         String::new(),
            folder_buf:       String::new(),
            template_sel:     0,
            project_list:     Vec::new(),
            project_cursor:   0,
            sel_project:      String::new(),
            sel_project_name: String::new(),
            level_list:       Vec::new(),
            level_cursor:     0,
            fb_path:          std::path::PathBuf::new(),
            fb_entries:       Vec::new(),
            fb_cursor:        0,
            result:           None,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Convert the project name the user typed into a filesystem-safe folder name.
    ///
    /// Replaces spaces and non-alphanumeric characters with underscores and
    /// lowercases everything. "My Cool Game" → "my_cool_game".
    /// Returns "untitled_project" if the name buffer is empty.
    fn auto_folder(&self) -> String {
        if self.name_buf.is_empty() { return "untitled_project".to_string(); }
        self.name_buf.chars()
            .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect()
    }

    /// Initialize the folder browser, starting at the current working directory.
    ///
    /// Called when the user moves from step 1 (name) to step 2 (folder choice).
    fn init_fb(&mut self) {
        self.fb_path   = std::env::current_dir().unwrap_or_default();
        self.fb_cursor = 0;
        self.refresh_fb_entries();
    }

    /// Rebuild the folder browser entry list from the current `fb_path`.
    ///
    /// Always starts with special sentinel entries:
    ///   "\x00SELECT" — "create the project here" action
    ///   "\x00PARENT" — navigate up to the parent directory (if one exists)
    ///
    /// Then lists all non-hidden subdirectories, sorted alphabetically.
    /// Hidden directories (names starting with '.') are excluded.
    fn refresh_fb_entries(&mut self) {
        self.fb_entries.clear();
        // The "\x00" prefix is a sentinel that the drawing code checks for special styling.
        self.fb_entries.push("\x00SELECT".to_string());
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

    /// Scan the current directory for project folders and populate `project_list`.
    fn refresh_projects(&mut self) {
        self.project_list   = ProjectData::find_projects(".");
        self.project_cursor = 0;
    }

    /// Handle the user selecting a project folder in the Open Project browser.
    ///
    /// - 0 levels: creates a blank main.level and opens it immediately.
    /// - 1 level:  opens it directly (no picker needed).
    /// - 2+ levels: transitions to the LevelPicker screen.
    fn open_project(&mut self, folder: &str, quit: &mut bool) {
        let name   = ProjectData::name_for(folder);
        let levels = ProjectData::levels_in(folder);

        match levels.len() {
            0 => {
                // Project exists but has no levels yet — create main.level.
                self.result = Some(StartResult {
                    project_folder: folder.to_string(),
                    project_name:   name,
                    level_path:     format!("{}/main.level", folder),
                    template:       Some(StartTemplate::Empty),
                });
                *quit = true;
            }
            1 => {
                // Exactly one level — open it directly without a picker.
                self.result = Some(StartResult {
                    project_folder: folder.to_string(),
                    project_name:   name,
                    level_path:     levels.into_iter().next().unwrap(),
                    template:       None,
                });
                *quit = true;
            }
            _ => {
                // Multiple levels — show the level picker screen.
                self.sel_project      = folder.to_string();
                self.sel_project_name = name;
                self.level_list       = levels;
                self.level_cursor     = 0;
                self.screen           = Screen::LevelPicker;
            }
        }
    }

    /// Finish the wizard: record the chosen template and trigger the quit flag.
    ///
    /// The engine's run() call in main.rs returns after this. main.rs then reads
    /// `start.result` to know which project/level/template to open.
    fn confirm_template(&mut self, quit: &mut bool) {
        let tmpl = if self.template_sel == 0 {
            StartTemplate::Empty
        } else {
            StartTemplate::BasicRoom
        };
        let folder     = self.folder_buf.clone();
        // The first level in a new project is always called "main.level".
        let level_path = std::path::Path::new(&folder)
            .join("main.level")
            .to_string_lossy()
            .into_owned();
        self.result = Some(StartResult {
            level_path,
            project_folder: folder,
            project_name:   self.name_buf.clone(),
            template:       Some(tmpl),
        });
        *quit = true;
    }

    // ── Mouse hit tests ───────────────────────────────────────────────────────

    /// Return true if the mouse cursor (mx, my) is inside the rectangle (x, y, w, h).
    ///
    /// Used throughout `update()` to detect clicks on buttons, list items, and cards.
    fn hit(mx: usize, my: usize, x: usize, y: usize, w: usize, h: usize) -> bool {
        mx >= x && mx < x + w && my >= y && my < y + h
    }
}

// ── GameState ─────────────────────────────────────────────────────────────────

impl GameState for StartScreen {
    fn on_start(&mut self, _world: &mut World, _events: &mut EventBus) {}

    fn update(&mut self, ctx: UpdateContext) {
        let UpdateContext { input, mouse, quit, .. } = ctx;
        let shift = input.is_held(Key::LeftShift) || input.is_held(Key::RightShift);
        let mx    = mouse.cell_x;
        let my    = mouse.cell_y;
        let click = mouse.in_bounds && mouse.left_just_pressed();

        match self.screen {
            // ── Main menu ─────────────────────────────────────────────────────
            Screen::MainMenu => {
                if input.just_pressed(Key::Up)   && self.menu_cursor > 0 {
                    self.menu_cursor -= 1;
                }
                if input.just_pressed(Key::Down) && self.menu_cursor + 1 < MENU_LABELS.len() {
                    self.menu_cursor += 1;
                }
                if input.just_pressed(Key::Key1) { self.menu_cursor = 0; }
                if input.just_pressed(Key::Key2) { self.menu_cursor = 1; }
                if input.just_pressed(Key::Key3) { self.menu_cursor = 2; }
                if input.just_pressed(Key::Escape) { *quit = true; }

                // Mouse hover
                if mouse.in_bounds {
                    for i in 0..MENU_LABELS.len() {
                        let row = MENU_Y + 2 + i * 3;
                        if Self::hit(mx, my, MENU_X, row, MENU_W, 2) {
                            self.menu_cursor = i;
                        }
                    }
                }

                let activate = input.just_pressed(Key::Enter)
                    || (click && {
                        let row = MENU_Y + 2 + self.menu_cursor * 3;
                        Self::hit(mx, my, MENU_X, row, MENU_W, 2)
                    });

                if activate {
                    match self.menu_cursor {
                        0 => { self.name_buf.clear(); self.screen = Screen::NewName; }
                        1 => { self.refresh_projects(); self.screen = Screen::OpenProject; }
                        _ => { *quit = true; }
                    }
                }
            }

            // ── Wizard: project name ───────────────────────────────────────────
            Screen::NewName => {
                for &key in TEXT_KEYS {
                    if input.just_pressed(key) {
                        if let Some(ch) = key_to_char(key, shift) { self.name_buf.push(ch); }
                    }
                }
                if input.just_pressed(Key::Backspace) { self.name_buf.pop(); }
                if input.just_pressed(Key::Escape)    { self.screen = Screen::MainMenu; }
                if input.just_pressed(Key::Enter) && !self.name_buf.is_empty() {
                    self.init_fb();
                    self.screen = Screen::FolderBrowser;
                }
            }

            // ── Wizard: folder browser ────────────────────────────────────────
            Screen::FolderBrowser => {
                if input.just_pressed(Key::Escape) { self.screen = Screen::NewName; }
                if input.just_pressed(Key::Up) && self.fb_cursor > 0 {
                    self.fb_cursor -= 1;
                }
                if input.just_pressed(Key::Down)
                    && self.fb_cursor + 1 < self.fb_entries.len()
                {
                    self.fb_cursor += 1;
                }

                // Mouse hover over list rows
                if mouse.in_bounds {
                    let offset = if self.fb_cursor >= FB_MAX_VIS {
                        self.fb_cursor - FB_MAX_VIS + 1
                    } else { 0 };
                    for vis_i in 0..FB_MAX_VIS {
                        let list_i = offset + vis_i;
                        if list_i >= self.fb_entries.len() { break; }
                        if Self::hit(mx, my, FB_X, FB_LIST_Y + vis_i, FB_W, 1) {
                            self.fb_cursor = list_i;
                        }
                    }
                }

                let confirm = input.just_pressed(Key::Enter)
                    || (click && !self.fb_entries.is_empty() && {
                        let offset = if self.fb_cursor >= FB_MAX_VIS {
                            self.fb_cursor - FB_MAX_VIS + 1
                        } else { 0 };
                        let vis = self.fb_cursor.saturating_sub(offset);
                        Self::hit(mx, my, FB_X, FB_LIST_Y + vis, FB_W, 1)
                    });

                if confirm && !self.fb_entries.is_empty() {
                    let entry = self.fb_entries[self.fb_cursor].clone();
                    match entry.as_str() {
                        "\x00SELECT" => {
                            self.folder_buf = self.fb_path.join(self.auto_folder())
                                .to_string_lossy().into_owned();
                            self.template_sel = 0;
                            self.screen       = Screen::NewTemplate;
                        }
                        "\x00PARENT" => {
                            if let Some(parent) = self.fb_path.parent() {
                                self.fb_path   = parent.to_path_buf();
                                self.fb_cursor = 0;
                                self.refresh_fb_entries();
                            }
                        }
                        dir_name => {
                            self.fb_path   = self.fb_path.join(dir_name);
                            self.fb_cursor = 0;
                            self.refresh_fb_entries();
                        }
                    }
                }
            }

            // ── Wizard: template choice ───────────────────────────────────────
            Screen::NewTemplate => {
                if input.just_pressed(Key::Left)  || input.just_pressed(Key::Up) {
                    if self.template_sel > 0 { self.template_sel -= 1; }
                }
                if input.just_pressed(Key::Right) || input.just_pressed(Key::Down) {
                    if self.template_sel + 1 < TEMPLATE_LABELS.len() { self.template_sel += 1; }
                }
                if input.just_pressed(Key::Escape) { self.screen = Screen::FolderBrowser; }
                if input.just_pressed(Key::Enter)  { self.confirm_template(quit); }

                // Mouse: hover over cards changes selection; click confirms
                if mouse.in_bounds {
                    for i in 0..TEMPLATE_LABELS.len() {
                        let cx = TCARD_X0 + i * (TCARD_W + TCARD_GAP);
                        if Self::hit(mx, my, cx, TCARD_Y, TCARD_W, TCARD_H) {
                            self.template_sel = i;
                            if click { self.confirm_template(quit); }
                        }
                    }
                }
            }

            // ── Open project browser ───────────────────────────────────────────
            Screen::OpenProject => {
                if input.just_pressed(Key::Escape) { self.screen = Screen::MainMenu; }
                if input.just_pressed(Key::Up)   && self.project_cursor > 0 {
                    self.project_cursor -= 1;
                }
                if input.just_pressed(Key::Down)
                    && self.project_cursor + 1 < self.project_list.len()
                {
                    self.project_cursor += 1;
                }

                // Mouse hover over project list
                if mouse.in_bounds {
                    let offset = if self.project_cursor >= BROW_MAX_VIS {
                        self.project_cursor - BROW_MAX_VIS + 1
                    } else { 0 };
                    for (vis_i, list_i) in (offset..).zip(0..BROW_MAX_VIS) {
                        if vis_i >= self.project_list.len() { break; }
                        let row = BROW_LIST_Y + list_i;
                        if Self::hit(mx, my, BROW_X, row, BROW_W, 1) {
                            self.project_cursor = vis_i;
                        }
                    }
                }

                let confirm = input.just_pressed(Key::Enter)
                    || (click && !self.project_list.is_empty() && {
                        let offset = if self.project_cursor >= BROW_MAX_VIS {
                            self.project_cursor - BROW_MAX_VIS + 1
                        } else { 0 };
                        let vis = self.project_cursor.saturating_sub(offset);
                        Self::hit(mx, my, BROW_X, BROW_LIST_Y + vis, BROW_W, 1)
                    });

                if confirm && !self.project_list.is_empty() {
                    let folder = self.project_list[self.project_cursor].clone();
                    self.open_project(&folder, quit);
                }
            }

            // ── Level picker ──────────────────────────────────────────────────
            Screen::LevelPicker => {
                if input.just_pressed(Key::Escape) { self.screen = Screen::OpenProject; }
                if input.just_pressed(Key::Up)   && self.level_cursor > 0 {
                    self.level_cursor -= 1;
                }
                if input.just_pressed(Key::Down)
                    && self.level_cursor + 1 < self.level_list.len()
                {
                    self.level_cursor += 1;
                }

                // Mouse hover over level list
                if mouse.in_bounds {
                    let offset = if self.level_cursor >= BROW_MAX_VIS {
                        self.level_cursor - BROW_MAX_VIS + 1
                    } else { 0 };
                    for (vis_i, list_i) in (offset..).zip(0..BROW_MAX_VIS) {
                        if vis_i >= self.level_list.len() { break; }
                        let row = BROW_LIST_Y + list_i;
                        if Self::hit(mx, my, BROW_X, row, BROW_W, 1) {
                            self.level_cursor = vis_i;
                        }
                    }
                }

                let confirm = input.just_pressed(Key::Enter)
                    || (click && !self.level_list.is_empty() && {
                        let offset = if self.level_cursor >= BROW_MAX_VIS {
                            self.level_cursor - BROW_MAX_VIS + 1
                        } else { 0 };
                        let vis = self.level_cursor.saturating_sub(offset);
                        Self::hit(mx, my, BROW_X, BROW_LIST_Y + vis, BROW_W, 1)
                    });

                if confirm && !self.level_list.is_empty() {
                    let path = self.level_list[self.level_cursor].clone();
                    self.result = Some(StartResult {
                        project_folder: self.sel_project.clone(),
                        project_name:   self.sel_project_name.clone(),
                        level_path:     path,
                        template:       None,
                    });
                    *quit = true;
                }
            }
        }
    }

    fn render(&mut self, ctx: RenderContext) {
        let RenderContext { renderer, elapsed, .. } = ctx;
        renderer.draw_rect_filled(0, 0, SCR_W, SCR_H, ' ', Color::Reset, Color::Black);
        draw_header(renderer);

        match self.screen {
            Screen::MainMenu    => draw_main_menu(renderer, elapsed, self.menu_cursor),
            Screen::NewName     => draw_text_step(
                renderer, 1, 3, "Project Name",
                "Enter a name for your new project:",
                "This becomes the folder name and appears in the editor title bar.",
                "Enter: next  |  Esc: back",
                &self.name_buf,
            ),
            Screen::FolderBrowser => draw_folder_browser(
                renderer,
                &self.fb_path,
                &self.fb_entries,
                self.fb_cursor,
                &self.auto_folder(),
            ),
            Screen::NewTemplate => draw_template_step(renderer, self.template_sel),
            Screen::OpenProject => draw_browser(
                renderer,
                " OPEN PROJECT ",
                &self.project_list,
                self.project_cursor,
                "No Ember2D projects found in the current directory.",
                "Tip: cd into your projects folder, or create a New Project first.",
                "Up/Down: navigate  |  Enter: open  |  Esc: back",
                true,  // show project names via ProjectData::name_for
            ),
            Screen::LevelPicker => draw_browser(
                renderer,
                &format!(" LEVELS IN: {} ", self.sel_project_name),
                &self.level_list,
                self.level_cursor,
                "No levels found in this project.",
                "",
                "Up/Down: navigate  |  Enter: open  |  Esc: back",
                false, // show raw filenames
            ),
        }
    }
}

// ── Drawing helpers ───────────────────────────────────────────────────────────
//
// Each `draw_*` function renders one complete screen state into the renderer's
// back buffer. They're called from `render()` based on `self.screen`.
// None of them take `&self` — they only need the data they're drawing.

/// Render the persistent 2-row title header shown on every screen.
fn draw_header(renderer: &mut Renderer) {
    renderer.draw_rect_filled(0, 0, SCR_W, 2, ' ', Color::White, Color::DarkBlue);
    let title = "EMBER2D  LEVEL  EDITOR";
    renderer.draw_str(SCR_W / 2 - title.len() / 2, 0, title, Color::White, Color::DarkBlue);
    let sub   = "Game Development Toolkit";
    renderer.draw_str(SCR_W / 2 - sub.len() / 2, 1, sub, Color::Cyan, Color::DarkBlue);
}

/// Render the hint/shortcut bar along the bottom edge of the screen.
///
/// Each screen passes its own `hint` string showing the relevant shortcuts.
fn draw_hint_bar(renderer: &mut Renderer, hint: &str) {
    renderer.draw_rect_filled(0, SCR_H - 1, SCR_W, 1, ' ', Color::White, Color::DarkGrey);
    let x = (SCR_W / 2).saturating_sub(hint.len() / 2);
    renderer.draw_str(x, SCR_H - 1, hint, Color::White, Color::DarkGrey);
}

/// Render the main menu: animated logo box and the 3 menu options.
///
/// `elapsed` drives the pulsing border color (alternates Cyan/Yellow ~1.5× per second).
/// `cursor` is the currently highlighted row.
fn draw_main_menu(renderer: &mut Renderer, elapsed: f32, cursor: usize) {
    // Logo box
    let box_w: usize = 44;
    let box_x = (SCR_W - box_w) / 2;
    let box_y: usize = 3;
    let pulse = if (elapsed * 1.5) as u32 % 2 == 0 { Color::Cyan } else { Color::Yellow };
    renderer.draw_rect_outline(box_x, box_y, box_w, 5, pulse, Color::Black);
    let t1 = "* E M B E R  2 D *";
    renderer.draw_str(box_x + (box_w - t1.len()) / 2, box_y + 1, t1, Color::Yellow, Color::Black);
    let t2 = "L E V E L   E D I T O R";
    renderer.draw_str(box_x + (box_w - t2.len()) / 2, box_y + 2, t2, Color::White, Color::Black);
    let t3 = "v0.1";
    renderer.draw_str(box_x + (box_w - t3.len()) / 2, box_y + 3, t3, Color::DarkGrey, Color::Black);

    // Dividers
    let sep = "-".repeat(MENU_W);
    renderer.draw_str(MENU_X, MENU_Y, &sep, Color::DarkGrey, Color::Black);
    renderer.draw_str(MENU_X, MENU_Y + 2 + MENU_LABELS.len() * 3, &sep, Color::DarkGrey, Color::Black);

    // Menu items
    for (i, &(label, desc)) in MENU_LABELS.iter().enumerate() {
        let row = MENU_Y + 2 + i * 3;
        if i == cursor {
            let line = format!("  >  {}. {:<44}", i + 1, label);
            renderer.draw_str(MENU_X, row, &line, Color::Black, Color::Cyan);
            // Highlight desc row too for a bigger click target feel
            renderer.draw_str(MENU_X, row + 1,
                &format!("{:width$}", "", width = MENU_W), Color::Black, Color::DarkBlue);
            renderer.draw_str(MENU_X + 9, row + 1, desc, Color::Cyan, Color::DarkBlue);
        } else {
            renderer.draw_str(MENU_X, row,
                &format!("     {}. {}", i + 1, label), Color::White, Color::Black);
            renderer.draw_str(MENU_X + 9, row + 1, desc, Color::DarkGrey, Color::Black);
        }
    }

    draw_hint_bar(renderer, "Up/Down or hover: navigate  |  Enter or click: select  |  Esc: quit");
}

/// Render a single-field wizard step: a border box with a text input field.
///
/// Used for "Project Name" (step 1) — any step that collects one text value.
/// `step`/`total` drives the "Step N/M:" label in the header.
/// `buffer` is the current typed text, shown with a `|` cursor at the end.
/// When the buffer is too long to fit, it scrolls to show the end.
fn draw_text_step(
    renderer:  &mut Renderer,
    step:      usize,
    total:     usize,
    step_name: &str,
    prompt:    &str,
    info:      &str,
    hint:      &str,
    buffer:    &str,
) {
    renderer.draw_rect_outline(WIZ_X, WIZ_Y, WIZ_W, WIZ_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(WIZ_X + 1, WIZ_Y + 1, WIZ_W - 2, WIZ_H - 2,
        ' ', Color::White, Color::Black);

    let header = format!(" NEW PROJECT - Step {}/{}: {} ", step, total, step_name);
    renderer.draw_str(WIZ_X + 1, WIZ_Y, &header, Color::Black, Color::Cyan);

    renderer.draw_str(WIZ_X + 3, WIZ_Y + 3, prompt, Color::White, Color::Black);

    let field_w = WIZ_W - 6;
    renderer.draw_rect_filled(WIZ_X + 3, WIZ_Y + 5, field_w, 1, ' ', Color::Black, Color::White);
    let display = if buffer.len() + 3 >= field_w {
        format!("{}|", &buffer[buffer.len() + 3 - field_w..])
    } else {
        format!("{}|", buffer)
    };
    renderer.draw_str(WIZ_X + 4, WIZ_Y + 5, &display, Color::Black, Color::White);

    renderer.draw_str(WIZ_X + 3, WIZ_Y + 8, info, Color::DarkGrey, Color::Black);

    draw_hint_bar(renderer, hint);
}

/// Render the template picker: two side-by-side cards (Empty Canvas, Basic Room).
///
/// The selected card has a Cyan border and a "click or Enter" action button.
/// `selected` is 0 for the left card or 1 for the right card.
fn draw_template_step(renderer: &mut Renderer, selected: usize) {
    renderer.draw_rect_outline(TBOX_X, TBOX_Y, TBOX_W, TBOX_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(TBOX_X + 1, TBOX_Y + 1, TBOX_W - 2, TBOX_H - 2,
        ' ', Color::White, Color::Black);
    renderer.draw_str(TBOX_X + 1, TBOX_Y,
        " NEW PROJECT - Step 3/3: Starting Template ", Color::Black, Color::Cyan);
    renderer.draw_str(TBOX_X + 3, TBOX_Y + 2,
        "Choose how to start your first level:", Color::White, Color::Black);

    for (i, &(name, desc)) in TEMPLATE_LABELS.iter().enumerate() {
        let cx = TCARD_X0 + i * (TCARD_W + TCARD_GAP);
        let cy = TCARD_Y;
        let is_sel = i == selected;
        let (border_c, hdr_bg) = if is_sel {
            (Color::Cyan, Color::Cyan)
        } else {
            (Color::DarkGrey, Color::DarkGrey)
        };

        renderer.draw_rect_outline(cx, cy, TCARD_W, TCARD_H, border_c, Color::Black);
        renderer.draw_rect_filled(cx + 1, cy + 1, TCARD_W - 2, 1, ' ', Color::Black, hdr_bg);

        let title = format!(" {} {}", if is_sel { "[*]" } else { "[ ]" }, name);
        renderer.draw_str(cx + 1, cy + 1, &title, Color::Black, hdr_bg);

        let text_fg = if is_sel { Color::White } else { Color::Grey };
        let max_w   = TCARD_W - 4;
        let mut lines: Vec<String> = Vec::new();
        let mut cur   = String::new();
        for word in desc.split_whitespace() {
            if cur.is_empty() {
                cur = word.to_string();
            } else if cur.len() + 1 + word.len() <= max_w {
                cur.push(' ');
                cur.push_str(word);
            } else {
                lines.push(cur.clone());
                cur = word.to_string();
            }
        }
        if !cur.is_empty() { lines.push(cur); }
        for (li, line) in lines.iter().take(4).enumerate() {
            renderer.draw_str(cx + 2, cy + 3 + li, line, text_fg, Color::Black);
        }

        // "Click to confirm" hint on selected card
        if is_sel {
            renderer.draw_str(cx + 2, cy + TCARD_H - 2,
                "  click or Enter  ", Color::Black, Color::DarkGreen);
        }
    }

    draw_hint_bar(renderer,
        "Left/Right or hover: choose  |  Click or Enter: confirm  |  Esc: back");
}

/// Render the filesystem folder browser (New Project step 2).
///
/// Shows the current directory path, the projected project folder name,
/// and a scrollable list of subdirectories the user can navigate into.
/// The "\x00SELECT" sentinel entry is styled green to stand out as the "confirm" action.
fn draw_folder_browser(
    renderer:       &mut Renderer,
    fb_path:        &std::path::PathBuf,
    fb_entries:     &[String],
    fb_cursor:      usize,
    project_folder: &str,
) {
    renderer.draw_rect_outline(FB_X, FB_Y, FB_W, FB_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(FB_X + 1, FB_Y + 1, FB_W - 2, FB_H - 2,
        ' ', Color::White, Color::Black);
    renderer.draw_str(FB_X + 1, FB_Y,
        " NEW PROJECT - Step 2/3: Choose Location ", Color::Black, Color::Cyan);

    // Current location
    let path_str  = fb_path.to_string_lossy();
    let avail     = FB_W.saturating_sub(14);
    let path_disp = if path_str.len() > avail {
        format!("...{}", &path_str[path_str.len().saturating_sub(avail - 3)..])
    } else {
        path_str.to_string()
    };
    renderer.draw_str(FB_X + 2, FB_Y + 1, "Location:", Color::DarkGrey, Color::Black);
    renderer.draw_str(FB_X + 12, FB_Y + 1, &path_disp, Color::Cyan, Color::Black);

    // Projected project folder
    let creates    = fb_path.join(project_folder).to_string_lossy().to_string();
    let crt_avail  = FB_W.saturating_sub(14);
    let crt_disp   = if creates.len() > crt_avail {
        format!("...{}", &creates[creates.len().saturating_sub(crt_avail - 3)..])
    } else {
        creates
    };
    renderer.draw_str(FB_X + 2, FB_Y + 2, "Creates: ", Color::DarkGrey, Color::Black);
    renderer.draw_str(FB_X + 12, FB_Y + 2, &crt_disp, Color::Yellow, Color::Black);

    // Divider + column header
    let sep = format!("{:-<width$}", "", width = FB_W - 4);
    renderer.draw_str(FB_X + 2, FB_Y + 3, &sep, Color::DarkGrey, Color::Black);
    renderer.draw_str(FB_X + 2, FB_Y + 4, "Subdirectories:", Color::DarkGrey, Color::Black);

    // Entry list
    let offset = if fb_cursor >= FB_MAX_VIS { fb_cursor - FB_MAX_VIS + 1 } else { 0 };
    let item_w = FB_W - 4;

    for vis_i in 0..FB_MAX_VIS {
        let list_i = offset + vis_i;
        if list_i >= fb_entries.len() { break; }
        let row    = FB_LIST_Y + vis_i;
        let is_sel = list_i == fb_cursor;

        let (label, text_fg, text_bg, sel_fg, sel_bg): (&str, Color, Color, Color, Color) =
            match fb_entries[list_i].as_str() {
                "\x00SELECT" => (
                    "[ Confirm: create project here ]",
                    Color::DarkGreen, Color::Black, Color::Black, Color::DarkGreen,
                ),
                "\x00PARENT" => (
                    "[..] Go up to parent folder",
                    Color::Yellow, Color::Black, Color::Yellow, Color::DarkBlue,
                ),
                name => (
                    name,
                    Color::White, Color::Black, Color::White, Color::DarkBlue,
                ),
            };

        let padded  = format!("{:<width$}", label, width = item_w.saturating_sub(2));
        let display = if padded.len() > item_w - 2 { &padded[..item_w - 2] } else { &padded };

        if is_sel {
            renderer.draw_str(FB_X + 1, row, " >", sel_fg, sel_bg);
            renderer.draw_str(FB_X + 3, row, display, sel_fg, sel_bg);
        } else {
            renderer.draw_str(FB_X + 1, row, "  ", text_fg, text_bg);
            renderer.draw_str(FB_X + 3, row, display, text_fg, text_bg);
        }
    }

    // Scroll indicator
    if fb_entries.len() > FB_MAX_VIS {
        let pct = fb_cursor * (FB_H - 8) / fb_entries.len().max(1);
        renderer.draw_char(FB_X + FB_W - 1, FB_Y + 5 + pct, '#', Color::DarkGrey, Color::Black);
    }

    draw_hint_bar(renderer,
        "Up/Down: navigate  |  Enter/click folder: open  |  Enter on Confirm: select  |  Esc: back");
}

/// Generic scrollable list browser used for both Open Project and Level Picker.
///
/// `use_name_for` controls how each item is displayed:
///   true  → call `ProjectData::name_for(path)` to get a human-readable project name
///   false → strip the path prefix and show just the filename (for level files)
///
/// A `#` scroll indicator appears on the right edge when the list is longer than the box.
fn draw_browser(
    renderer:      &mut Renderer,
    title:         &str,
    items:         &[String],
    cursor:        usize,
    empty_msg:     &str,
    empty_hint:    &str,
    hint:          &str,
    use_name_for:  bool,   // if true, use ProjectData::name_for for display
) {
    renderer.draw_rect_outline(BROW_X, BROW_Y, BROW_W, BROW_H, Color::Cyan, Color::Black);
    renderer.draw_rect_filled(BROW_X + 1, BROW_Y + 1, BROW_W - 2, BROW_H - 2,
        ' ', Color::White, Color::Black);
    renderer.draw_str(BROW_X + 1, BROW_Y, title, Color::Black, Color::Cyan);

    if items.is_empty() {
        renderer.draw_str(BROW_X + 3, BROW_Y + 4, empty_msg, Color::DarkGrey, Color::Black);
        if !empty_hint.is_empty() {
            renderer.draw_str(BROW_X + 3, BROW_Y + 6, empty_hint, Color::DarkGrey, Color::Black);
        }
    } else {
        let offset  = if cursor >= BROW_MAX_VIS { cursor - BROW_MAX_VIS + 1 } else { 0 };
        let item_w  = BROW_W - 4;

        for (vis_i, list_i) in (offset..).zip(0..BROW_MAX_VIS) {
            if list_i >= items.len() { break; }
            let row = BROW_LIST_Y + vis_i - offset;
            let raw = &items[list_i];

            // Display: if use_name_for → ProjectData::name_for; else just filename
            let display = if use_name_for {
                let name = ProjectData::name_for(raw);
                format!("  {:width$}", name, width = item_w - 2)
            } else {
                // Strip path prefix, show just the filename
                let fname = raw.rfind('/').or_else(|| raw.rfind('\\'))
                    .map(|i| &raw[i+1..])
                    .unwrap_or(raw.as_str());
                format!("  {:width$}", fname, width = item_w - 2)
            };

            if list_i == cursor {
                renderer.draw_str(BROW_X + 1, row, " >", Color::Cyan, Color::DarkBlue);
                renderer.draw_str(BROW_X + 3, row, &display[2..], Color::White, Color::DarkBlue);
            } else {
                renderer.draw_str(BROW_X + 1, row, &display, Color::White, Color::Black);
            }
        }

        // Scroll indicator
        if items.len() > BROW_MAX_VIS {
            let pct = (cursor * (BROW_H - 6)) / items.len().max(1);
            renderer.draw_char(BROW_X + BROW_W - 1, BROW_Y + 2 + pct, '#', Color::DarkGrey, Color::Black);
        }
    }

    draw_hint_bar(renderer, hint);
}
