// editor/start_screen/mod.rs — Start screen module orchestration.

use crate::engine::{GameState, RenderContext, UpdateContext, Transition};
use crate::event::EventBus;
use crate::world::World;

mod drawing;
mod logic;

pub use drawing::DEFAULT_SCR_W as SCR_W;
pub use drawing::DEFAULT_SCR_H as SCR_H;

#[derive(Debug, Clone, PartialEq)]
pub enum StartTemplate { Empty, BasicRoom }

#[derive(Debug, Clone)]
pub struct StartResult {
    pub project_folder: String,
    pub project_name:   String,
    pub level_path:     String,
    pub template:       Option<StartTemplate>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Screen { MainMenu, NewName, FolderBrowser, NewTemplate, OpenProject, LevelPicker }

mod mod_types {
    
    pub const MENU_LABELS: &[(&str, &str)] = &[
        ("New Project",  "Create a new project folder with levels inside"),
        ("Open Project", "Browse and open an existing Ember2D project"),
        ("Quit",         "Exit Ember2D"),
    ];
    pub const TEMPLATE_LABELS: &[(&str, &str)] = &[
        ("Empty Canvas", "Blank grid — place everything yourself"),
        ("Basic Room",   "Floor, walls, and a player spawn pre-built"),
    ];
}

pub struct StartScreen {
    screen:       Screen,
    menu_cursor:  usize,
    name_buf:     String,
    folder_buf:   String,
    template_sel: usize,
    project_list:   Vec<String>,
    project_cursor: usize,
    sel_project:      String,
    sel_project_name: String,
    level_list:       Vec<String>,
    level_cursor:     usize,
    fb_path:    std::path::PathBuf,
    fb_entries: Vec<String>,
    fb_cursor:  usize,
    pub result: Option<StartResult>,

    // Last known window dimensions for hit-testing in update()
    last_sw: usize,
    last_sh: usize,
}

impl StartScreen {
    pub fn new() -> Self {
        StartScreen {
            screen: Screen::MainMenu, menu_cursor: 0, name_buf: String::new(), folder_buf: String::new(), template_sel: 0,
            project_list: Vec::new(), project_cursor: 0, sel_project: String::new(), sel_project_name: String::new(),
            level_list: Vec::new(), level_cursor: 0, fb_path: std::path::PathBuf::new(), fb_entries: Vec::new(), fb_cursor: 0,
            result: None,
            last_sw: 80,
            last_sh: 24,
        }
    }
}

impl GameState for StartScreen {
    fn on_start(&mut self, _world: &mut World, _events: &mut EventBus) {}
    fn update(&mut self, ctx: UpdateContext) {
        let mut q = *ctx.quit;
        let sw = self.last_sw;
        let sh = self.last_sh;
        self.update_logic(ctx.input, ctx.mouse, sw, sh, &mut q);
        *ctx.quit = q;
    }
    fn render(&mut self, ctx: RenderContext) {
        use drawing::*;
        let renderer = ctx.renderer;
        let sw = renderer.width;
        let sh = renderer.height;
        self.last_sw = sw;
        self.last_sh = sh;

        renderer.draw_rect_filled(0, 0, sw, sh, ' ', crate::renderer::color::Color::Reset, crate::renderer::color::Color::Black);
        draw_header(renderer, sw);
        match self.screen {
            Screen::MainMenu => draw_main_menu(renderer, sw, sh, ctx.elapsed, self.menu_cursor),
            Screen::NewName => draw_text_step(renderer, sw, sh, 1, 3, "Project Name", "Enter a name for your new project:", "This becomes the folder name and appears in the editor title bar.", "Enter: next  |  Esc: back", &self.name_buf),
            Screen::FolderBrowser => draw_folder_browser(renderer, sw, sh, &self.fb_path, &self.fb_entries, self.fb_cursor, &self.auto_folder()),
            Screen::NewTemplate => draw_template_step(renderer, sw, sh, self.template_sel),
            Screen::OpenProject => draw_browser(renderer, sw, sh, " OPEN PROJECT ", &self.project_list, self.project_cursor, "No Ember2D projects found in the current directory.", "Tip: cd into your projects folder, or create a New Project first.", "Up/Down: navigate  |  Enter: open  |  Esc: back", true),
            Screen::LevelPicker => draw_browser(renderer, sw, sh, &format!(" LEVELS IN: {} ", self.sel_project_name), &self.level_list, self.level_cursor, "No levels found in this project.", "", "Up/Down: navigate  |  Enter: open  |  Esc: back", false),
        }
    }
    fn take_transition(&mut self) -> Option<Transition> { None }
}
