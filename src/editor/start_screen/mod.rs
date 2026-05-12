// editor/start_screen/mod.rs — Start screen module orchestration.

use crate::engine::{GameState, RenderContext, UpdateContext, Transition};
use crate::event::EventBus;
use crate::world::World;
use crate::project::{VisualStyle, GameplayLoop, StartResult, StartTemplate};

mod drawing;
mod logic;

pub use drawing::DEFAULT_SCR_W as SCR_W;
pub use drawing::DEFAULT_SCR_H as SCR_H;

#[derive(Debug, Clone, PartialEq)]
pub enum Screen { MainMenu, NewName, FolderBrowser, NewStyle, NewLoop, NewTemplate, OpenProject, LevelPicker, NewFolder }

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
    pub const STYLE_LABELS: &[(&str, &str)] = &[
        ("Classic ASCII", "The iconic character-cell grid aesthetic"),
        ("2D Sprites",    "Future-ready high resolution pixel art (coming soon)"),
    ];
    pub const LOOP_LABELS: &[(&str, &str)] = &[
        ("Real-Time",     "Standard updates every frame (action/platformer)"),
        ("Turn-Based",    "World only advances when you act (roguelike)"),
    ];
}

pub struct StartScreen {
    screen:       Screen,
    menu_cursor:  usize,
    name_buf:     String,
    folder_buf:   String,
    template_sel: usize,
    style_sel:    usize,
    loop_sel:     usize,
    sel_project:      String,
    sel_project_name: String,
    level_list:       Vec<String>,
    level_cursor:     usize,
    fb_path:    std::path::PathBuf,
    fb_entries: Vec<String>,
    fb_cursor:  usize,
    pub pending_transition: Option<Transition>,
    last_sw: usize,
    last_sh: usize,
}

impl StartScreen {
    pub fn new() -> Self {
        StartScreen {
            screen: Screen::MainMenu, menu_cursor: 0, name_buf: String::new(), folder_buf: String::new(), 
            template_sel: 0, style_sel: 0, loop_sel: 0,
            sel_project: String::new(), sel_project_name: String::new(),
            level_list: Vec::new(), level_cursor: 0, fb_path: std::path::PathBuf::new(), fb_entries: Vec::new(), fb_cursor: 0,
            pending_transition: None,
            last_sw: 80,
            last_sh: 24,
        }
    }
}

impl GameState for StartScreen {
    fn on_start(&mut self, _world: &mut World, _events: &mut EventBus, _viewport_width: usize, _viewport_height: usize) {}
    
    fn update(&mut self, ctx: UpdateContext) {
        let mut q = *ctx.quit;
        self.update_logic(ctx.input, ctx.mouse, self.last_sw, self.last_sh, &mut q);
        if q { self.pending_transition = Some(Transition::Quit); }
        *ctx.quit = q;
    }

    fn render(&mut self, ctx: RenderContext) {
        use drawing::*;
        let sw = ctx.renderer.width;
        let sh = ctx.renderer.height;
        self.last_sw = sw;
        self.last_sh = sh;

        ctx.renderer.draw_rect_filled(0, 0, sw, sh, ' ', crate::renderer::color::Color::Reset, crate::renderer::color::Color::Reset);
        draw_header(ctx.renderer, sw);
        match self.screen {
            Screen::MainMenu => draw_main_menu(ctx.renderer, sw, sh, ctx.elapsed, self.menu_cursor),
            Screen::NewName => draw_text_step(ctx.renderer, sw, sh, 1, 5, "Project Name", "Enter a name for your new project:", "This becomes the folder name and appears in the editor title bar.", "Enter: next  |  Esc: back", &self.name_buf),
            Screen::NewStyle => draw_style_step(ctx.renderer, sw, sh, self.style_sel),
            Screen::NewLoop => draw_loop_step(ctx.renderer, sw, sh, self.loop_sel),
            Screen::FolderBrowser => draw_folder_browser(ctx.renderer, sw, sh, &self.fb_path, &self.fb_entries, self.fb_cursor, &self.auto_folder()),
            Screen::NewFolder => draw_text_step(ctx.renderer, sw, sh, 4, 5, "New Folder", "Enter a name for the new folder:", "A new directory will be created inside the current location.", "Enter: create  |  Esc: cancel", &self.folder_buf),
            Screen::NewTemplate => draw_template_step(ctx.renderer, sw, sh, self.template_sel),
            Screen::OpenProject => draw_browser(ctx.renderer, sw, sh, " OPEN PROJECT ", &self.fb_entries, self.fb_cursor, "No projects or folders found.", "Tip: Use [..] to go up or the OS Browser to find your project.", "Up/Down: navigate  |  Enter: open  |  Esc: back", true, &self.fb_path),
            Screen::LevelPicker => draw_browser(ctx.renderer, sw, sh, &format!(" LEVELS IN: {} ", self.sel_project_name), &self.level_list, self.level_cursor, "No levels found in this project.", "", "Up/Down: navigate  |  Enter: open  |  Esc: back", false, &self.fb_path),
        }
    }

    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }
}
