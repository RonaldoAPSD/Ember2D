// editor/mod.rs — Level editor core.

pub mod commands;
pub mod grid;
pub mod palette;
pub mod panel;
pub mod node_graph;
pub mod start_screen;
pub mod ui;

mod input;
mod impl_render;
mod helpers;
mod impl_state;

pub use helpers::*;
pub use ui::bresenham;

use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::event::EventBus;
use crate::level::TileRecord;
use crate::scripting::LogEntry;
use crate::world::World;

use commands::UndoStack;
use grid::LevelGrid;
use palette::TilePalette;
use panel::PanelManager;
use start_screen::{StartResult, StartTemplate};
use ui::{Layout, ToolKind, MenuKind, HierarchySelection};

// Default level size (used for initial grid). Independent from the viewport layout.
pub const DEFAULT_LEVEL_W: usize = 68;
pub const DEFAULT_LEVEL_H: usize = 22;

// ── Text input ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum TextInputPurpose {
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
pub struct TextInput {
    pub buffer:  String,
    pub purpose: TextInputPurpose,
}

// ── EditorState ───────────────────────────────────────────────────────────────

pub struct EditorState {
    pub(super) grid:    LevelGrid,
    pub(super) palette: TilePalette,
    pub(super) undo:    UndoStack,

    pub(super) save_path: String,
    pub(super) unsaved:   bool,
    pub(super) show_grid: bool,

    pub(super) save_message:       Option<String>,
    pub(super) save_message_timer: u32,

    pub(super) pending_transition: Option<Transition>,

    // ── Scroll / canvas pan ───────────────────────────────────────────────────
    pub(super) scroll: (i32, i32),

    // ── Drawing tools ─────────────────────────────────────────────────────────
    pub(super) rect_anchor: Option<(i32, i32)>,
    pub(super) line_anchor: Option<(i32, i32)>,
    pub(super) erase_size:  usize,

    // ── Text input ────────────────────────────────────────────────────────────
    pub(super) text_input: Option<TextInput>,

    // ── Copy / paste / cut ────────────────────────────────────────────────────
    pub(super) selecting:  bool,
    pub(super) cutting:    bool,
    pub(super) sel_anchor: Option<(i32, i32)>,
    pub(super) clipboard:  Vec<(i32, i32, TileRecord)>,
    pub(super) pasting:    bool,
    pub(super) paste_flip_x: bool,
    pub(super) paste_flip_y: bool,
    pub(super) paste_rotate: i32,

    // ── File browser ──────────────────────────────────────────────────────────
    pub(super) browsing:     bool,
    pub(super) file_list:    Vec<String>,
    pub(super) file_cursor:  usize,

    // ── Project context ───────────────────────────────────────────────────────
    pub(super) project_folder: Option<String>,
    pub(super) project_name:   Option<String>,

    // ── Console / inspector log ───────────────────────────────────────────────
    pub(super) console_log: Vec<LogEntry>,

    // ── Inspector ─────────────────────────────────────────────────────────────
    pub(super) inspected_pos:  Option<(i32, i32)>,
    // ── Modes ─────────────────────────────────────────────────────────────────
    pub(super) active_tool:  ToolKind,
    pub(super) select_mode:  bool,
    pub(super) selected_pos: Option<(i32, i32)>,
    pub(super) hierarchy_sel: Option<HierarchySelection>,
    pub(super) placing_spawn: bool,
    pub(super) placing_named_spawn: Option<String>,
    pub(super) ignore_drag:  bool,

    // ── Panning ───────────────────────────────────────────────────────────────
    pub(super) pan_anchor:   Option<(usize, usize, i32, i32)>,
    pub(super) scroll_repeat: u32,

    // ── UI state ──────────────────────────────────────────────────────────────
    pub(super) panels:      PanelManager,
    pub(super) show_physics: bool,
    pub(super) show_help:    bool,
    pub(super) active_menu:  Option<MenuKind>,

    // ── Graph editor mode ─────────────────────────────────────────────────────
    pub(super) graph_mode:          Option<(i32, i32)>,
    pub(super) graph_view_ox:       i32,
    pub(super) graph_view_oy:       i32,
    pub(super) graph_selected_node: Option<node_graph::NodeId>,
    pub(super) graph_connecting:    Option<(node_graph::NodeId, usize)>,
    pub(super) graph_dragging_node: Option<(node_graph::NodeId, i32, i32)>,
    pub(super) graph_palette_open:  Option<(usize, usize)>,
    pub(super) graph_palette_scroll: usize,
    pub(super) graph_palette_cursor: usize,
    pub(super) graph_editing_param: Option<(node_graph::NodeId, String)>,

    pub(super) layout: Layout,
    pub(super) zoom:   f32,
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
            placing_spawn:  false,
            placing_named_spawn: None,
            ignore_drag:    false,
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
            zoom:         1.0,
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
}

impl GameState for EditorState {
    fn on_start(&mut self, _world: &mut World, _events: &mut EventBus) {}
    fn update(&mut self, ctx: UpdateContext) { self.handle_update(ctx); }
    fn render(&mut self, ctx: RenderContext) { self.handle_render(ctx); }
    fn take_transition(&mut self) -> Option<Transition> { self.pending_transition.take() }
}
