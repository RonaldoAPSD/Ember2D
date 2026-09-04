// editor/mod.rs — Level editor core.

pub mod commands;
pub mod grid;
pub mod palette;
pub mod panel;
pub mod node_graph;
pub mod start_screen;
pub mod ui;

pub mod helpers;
mod input;
mod impl_render;
mod impl_state;

pub use ui::HierarchySelection;
use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::level::TileRecord;
use crate::scripting::LogEntry;
use grid::LevelGrid;
use palette::TilePalette;
use panel::{PanelId, PanelManager};
use commands::UndoStack;
use ui::{Layout, MenuKind, ToolKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteField { Name, Tag, Glyph }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalPurpose { ConfirmSwitchLevel { path: String } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputPurpose {
    LevelName,
    SaveAs,
    ScriptPath { gx: i32, gy: i32 },
    TileNextLevel { gx: i32, gy: i32 },
    TileTag { gx: i32, gy: i32 },
    TileGlyph { gx: i32, gy: i32 },
    NamedSpawn,
    ResizeLevel,
    PlayerTag,
    PlayerScript,
    PlayerGlyph,
    NewLevelName,
    PaletteName,
    TileColliderLayer { gx: i32, gy: i32 },
    TileColliderMask  { gx: i32, gy: i32 },
    PlayerColliderLayer,
    PlayerColliderMask,
    NewScriptName,
    PaletteFgCustom,
    PaletteBgCustom,
}

pub struct TextInput {
    pub buffer:  String,
    pub purpose: TextInputPurpose,
}

pub struct Modal {
    pub title:   String,
    pub message: String,
    pub purpose: ModalPurpose,
}

pub struct EditorState {
    pub(super) grid:         LevelGrid,
    pub(super) palette:      TilePalette,
    pub(super) undo:         UndoStack,
    pub(super) save_path:    String,
    pub(super) unsaved:      bool,
    pub(super) show_grid:    bool,
    pub(super) active_layer: u8,
    pub(super) palette_scroll: usize,
    pub(super) palette_editor_open: bool,
    pub(super) palette_editing_idx: usize,
    pub(super) palette_editor_focus: Option<PaletteField>,
    pub(super) palette_search_focused: bool,
    pub(super) save_message:       Option<String>,
    pub(super) save_message_timer: u32,
    pub(super) pending_transition: Option<Transition>,
    pub(super) scroll: (f32, f32),
    pub(super) target_scroll: (f32, f32),
    pub(super) rect_anchor: Option<(i32, i32)>,
    pub(super) line_anchor: Option<(i32, i32)>,
    pub(super) erase_size:  usize,
    pub(super) text_input: Option<TextInput>,
    pub(super) modal:      Option<Modal>,
    pub(super) selecting:  bool,
    pub(super) cutting:    bool,
    pub(super) sel_anchor: Option<(i32, i32)>,
    pub(super) clipboard:  Vec<(i32, i32, TileRecord)>,
    pub(super) pasting:    bool,
    pub(super) paste_flip_x: bool,
    pub(super) paste_flip_y: bool,
    pub(super) paste_rotate: i32,
    pub(super) file_browser_files:  Vec<String>,
    pub(super) file_browser_cursor: usize,
    pub(super) file_browser_scroll: usize,
    pub(super) current_folder:      String,
    pub(super) script_path:    Option<String>,
    pub(super) script_buffer:  Vec<String>,
    pub(super) script_cursor:  (usize, usize),
    pub(super) script_scroll:  usize,
    pub(super) script_unsaved: bool,
    pub project_folder: Option<String>,
    pub project_name:   Option<String>,
    pub(super) console_log: Vec<LogEntry>,
    pub(super) inspected_pos:  Option<(i32, i32)>,
    pub(super) active_tool:  ToolKind,
    pub(super) select_mode:  bool,
    pub(super) selected_pos: Option<(i32, i32)>,
    pub(super) hierarchy_sel: Option<HierarchySelection>,
    pub(super) placing_spawn: bool,
    pub(super) placing_named_spawn: Option<String>,
    pub(super) ignore_drag:  bool,
    pub(super) pan_anchor:   Option<(usize, usize, i32, i32)>,
    pub(super) scroll_repeat: u32,
    pub(super) panels:      PanelManager,
    pub(super) focused_panel: Option<PanelId>,
    pub(super) show_physics: bool,
    pub(super) show_help:    bool,
    pub(super) active_menu:  Option<MenuKind>,
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
    pub(super) graph_clipboard:     Option<node_graph::Node>,
    pub(super) script_mode: bool,
    pub(super) color_picker_open: Option<bool>,
    pub(super) color_picker_hsv: (f32, f32, f32),
    pub(super) context_menu: Option<ui::ContextMenu>,
    pub(super) layout: Layout,
    pub(super) zoom:   f32,
}

const DEFAULT_LEVEL_W: usize = 32;
const DEFAULT_LEVEL_H: usize = 20;

impl EditorState {
    pub fn new(save_path: &str) -> Self {
        EditorState {
            grid:    LevelGrid::new(DEFAULT_LEVEL_W, DEFAULT_LEVEL_H),
            palette: TilePalette::default_palette(),
            undo:    UndoStack::new(),
            save_path: if save_path.is_empty() { "level.level".to_string() } else { save_path.to_string() },
            unsaved:      false,
            show_grid:    false,
            active_layer: 1,
            palette_scroll: 0,
            palette_editor_open: false,
            palette_editing_idx: 0,
            palette_editor_focus: None,
            palette_search_focused: false,
            save_message:       None,
            save_message_timer: 0,
            pending_transition: None,
            scroll:      (0.0, 0.0),
            target_scroll: (0.0, 0.0),
            rect_anchor: None,
            line_anchor: None,
            erase_size:  1,
            text_input:  None,
            modal:       None,
            selecting:   false,
            cutting:     false,
            sel_anchor:  None,
            clipboard:   Vec::new(),
            pasting:     false,
            paste_flip_x: false,
            paste_flip_y: false,
            paste_rotate: 0,
            file_browser_files:  Vec::new(),
            file_browser_cursor: 0,
            file_browser_scroll: 0,
            current_folder:      ".".to_string(),
            script_path:    None,
            script_buffer:  Vec::new(),
            script_cursor:  (0, 0),
            script_scroll:  0,
            script_unsaved: false,
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
            focused_panel: None,
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
            graph_clipboard:     None,
            script_mode: false,
            color_picker_open: None,
            color_picker_hsv: (0.0, 1.0, 1.0),
            context_menu: None,
            layout: Layout::new(80, 24),
            zoom: 1.0,
        }
    }

    pub fn load(path: &str) -> Result<Self, String> {
        use crate::level::LevelData;
        let data = LevelData::load(path).map_err(|e| format!("Failed to load '{}': {}", path, e))?;
        let mut editor = EditorState::new(path);
        editor.grid = LevelGrid::from_level_data(&data);
        Ok(editor)
    }

    pub(super) fn load_palette(&mut self) {
        if let Some(ref folder) = self.project_folder {
            let path = format!("{}/project.palette.ron", folder);
            if let Ok(pal) = TilePalette::load(&path) { self.palette = pal; }
        }
    }

    pub fn new_from_result(result: crate::project::StartResult) -> Result<Self, String> {
        use crate::project::{ProjectData, StartTemplate};
        use helpers::apply_basic_room;
        match result.template {
            None => {
                let mut editor = EditorState::load(&result.level_path)?;
                editor.project_folder = Some(result.project_folder);
                editor.project_name   = Some(result.project_name);
                editor.load_palette();
                editor.refresh_project_files();
                Ok(editor)
            }
            Some(template) => {
                std::fs::create_dir_all(&result.project_folder).map_err(|e| format!("Cannot create '{}': {}", result.project_folder, e))?;
                ProjectData::new(&result.project_name, result.visual_style, result.gameplay_loop).save(&result.project_folder).map_err(|e| format!("Cannot write project.ron: {}", e))?;
                let mut editor = EditorState::new(&result.level_path);
                editor.grid.name    = result.project_name.clone();
                editor.project_folder = Some(result.project_folder);
                editor.project_name   = Some(result.project_name);
                editor.load_palette();
                editor.refresh_project_files();
                if template == StartTemplate::BasicRoom { apply_basic_room(&mut editor.grid); }
                editor.unsaved = true;
                Ok(editor)
            }
        }
    }
}

impl GameState for EditorState {
    fn on_start(&mut self, _world: &mut crate::world::World, _events: &mut crate::event::EventBus, _viewport_width: usize, _viewport_height: usize, _persistent: &mut std::collections::HashMap<String, rhai::Dynamic>) {}
    fn update(&mut self, ctx: UpdateContext) { self.handle_update(ctx); }
    fn render(&mut self, ctx: RenderContext) { self.handle_render(ctx); }
    fn take_transition(&mut self) -> Option<Transition> { self.pending_transition.take() }
}
