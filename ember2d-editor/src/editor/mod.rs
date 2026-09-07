// editor/mod.rs — Level editor core.

pub mod commands;
pub mod grid;
pub mod palette;
pub mod panel;
pub mod start_screen;
pub mod ui;

pub mod helpers;
mod graph_ui;
mod input;
mod impl_render;
mod impl_state;

pub use ui::HierarchySelection;
use ember2d::engine::{GameState, RenderContext, Transition, UpdateContext};
use ember2d_sim::level::TileRecord;
use ember2d_sim::scripting::LogEntry;
use grid::LevelGrid;
use palette::TilePalette;
use panel::{PanelId, PanelManager};
use commands::UndoStack;
use ui::{Layout, MenuKind, ToolKind, UiFrame};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteField { Name, Tag, Glyph }

/// R14 (7A-2, docs/ember2d-master-plan.md): the seed of the fuller `Mode`
/// enum 7C-4 replaces the boolean-soup dispatch with. Only the two variants
/// this step actually needs — derived from existing state (`focus()` below)
/// rather than stored, so there's no new field to keep in sync with
/// `script_mode`/`focused_panel`; 7C-4 can turn this into real stored state
/// later without touching any call site that reads `focus()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorFocus {
    /// Nothing has exclusive keyboard focus — panels, canvas tools, and
    /// global shortcuts all apply normally.
    Canvas,
    /// The script editor (fullscreen or docked) owns the keyboard — global
    /// shortcuts must not fire while it does (R14: typing 's' used to save,
    /// 'f' used to switch tools, etc., instead of being typed).
    ScriptPanel,
}

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
    /// One render pass's worth of interactive-widget hit rects (Phase 7
    /// Part 1d, docs/ember2d-phase7-plan.md) — see `ui/frame.rs`'s header
    /// comment. Rebuilt every `handle_render` call; read by the FOLLOWING
    /// frame's `handle_update`/`handle_panel_input`.
    pub(super) ui_frame:    UiFrame,
    /// The editor's active text-metrics source (Phase 7 Part 2c,
    /// docs/ember2d-phase7-plan.md) — a `BitmapFont` today, always; every
    /// `ui::draw_*`/`graph_ui::*` call that used to compute a position
    /// from a string's raw `.len()` now goes through `Font::measure`/
    /// `glyph` on this instead, so a future theme (Part 3) can swap it for
    /// a `TtfFont` without those call sites changing again. `Box<dyn Font>`
    /// rather than a concrete `BitmapFont` for exactly that swap.
    pub(super) font:        Box<dyn ember2d::renderer::Font>,
    pub(super) focused_panel: Option<PanelId>,
    pub(super) show_physics: bool,
    pub(super) show_help:    bool,
    pub(super) active_menu:  Option<MenuKind>,
    pub(super) graph_mode:          Option<(i32, i32)>,
    pub(super) graph_view_ox:       i32,
    pub(super) graph_view_oy:       i32,
    pub(super) graph_selected_node: Option<ember2d_sim::graph::NodeId>,
    pub(super) graph_connecting:    Option<(ember2d_sim::graph::NodeId, usize)>,
    pub(super) graph_dragging_node: Option<(ember2d_sim::graph::NodeId, i32, i32)>,
    pub(super) graph_palette_open:  Option<(usize, usize)>,
    pub(super) graph_palette_scroll: usize,
    pub(super) graph_palette_cursor: usize,
    pub(super) graph_editing_param: Option<(ember2d_sim::graph::NodeId, String)>,
    pub(super) graph_clipboard:     Option<ember2d_sim::graph::Node>,
    pub(super) script_mode: bool,
    pub(super) color_picker_open: Option<bool>,
    pub(super) color_picker_hsv: (f32, f32, f32),
    pub(super) context_menu: Option<ui::ContextMenu>,
    pub(super) layout: Layout,
    pub(super) zoom:   f32,
}

const DEFAULT_LEVEL_W: usize = 32;
const DEFAULT_LEVEL_H: usize = 20;

/// Placeholder cell-grid size for `PanelManager`/`Layout` at construction
/// time, before a real window (and so a real `renderer.width`/`height`)
/// exists — `EditorState::new` takes only a save path, not a `&Renderer`
/// (Phase 7 Part 1e, docs/ember2d-phase7-plan.md, E6). Not a guess at the
/// actual viewport size: `handle_render`'s very first lines unconditionally
/// call `self.panels.apply_layout(renderer.pixel_width, renderer.pixel_height)`
/// and rebuild `self.layout` from `renderer.width`/`height`, before any
/// drawing happens — so whatever this constant is set to is never itself
/// visible on screen. `80x24` was kept as the value precisely because it
/// carries no meaning beyond "some placeholder terminal-shaped size."
const PLACEHOLDER_SCREEN_W: usize = 80;
const PLACEHOLDER_SCREEN_H: usize = 24;

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
            panels:       PanelManager::new(PLACEHOLDER_SCREEN_W, PLACEHOLDER_SCREEN_H),
            ui_frame:     UiFrame::new(),
            font:         Box::new(ember2d::renderer::BitmapFont::new()),
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
            layout: Layout::new(PLACEHOLDER_SCREEN_W, PLACEHOLDER_SCREEN_H),
            zoom: 1.0,
        }
    }

    pub fn load(path: &str) -> Result<Self, String> {
        use ember2d_sim::level::LevelData;
        let data = LevelData::load(path).map_err(|e| format!("Failed to load '{}': {}", path, e))?;
        let mut editor = EditorState::new(path);
        editor.grid = LevelGrid::from_level_data(&data);
        Ok(editor)
    }

    pub(super) fn load_palette(&mut self) {
        if let Some(ref folder) = self.project_folder {
            let path = format!("{}/project.palette.ron", folder);
            // R20 (7A-2, docs/ember2d-master-plan.md): only attempt a load
            // (and only report a failure) when the file actually exists —
            // a brand-new project with no custom palette yet is the common
            // case and must stay silent, keeping whatever `EditorState::new`
            // already set (the built-in default). A file that DOES exist
            // but fails to parse, or parses with no tiles, previously left
            // `self.palette` on the built-in default silently too — now it
            // says so, since `TilePalette::current()` panicking on an empty
            // palette was reachable only through a malformed file exactly
            // like this.
            if std::path::Path::new(&path).exists() {
                match TilePalette::load(&path) {
                    Ok(pal) => self.palette = pal,
                    Err(e) => {
                        self.console_log.push(LogEntry::error(format!("Palette '{}' invalid ({}) — using defaults", path, e)));
                        self.palette = TilePalette::default_palette();
                    }
                }
            }
        }
    }

    /// R14 (7A-2, docs/ember2d-master-plan.md) — see `EditorFocus`'s own
    /// doc comment for why this is derived rather than a stored field.
    pub(crate) fn focus(&self) -> EditorFocus {
        if self.script_mode || self.focused_panel == Some(PanelId::ScriptEditor) {
            EditorFocus::ScriptPanel
        } else {
            EditorFocus::Canvas
        }
    }

    pub fn new_from_result(result: ember2d::project::StartResult) -> Result<Self, String> {
        use ember2d::project::{ProjectData, StartTemplate};
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
    fn on_start(&mut self, _world: &mut ember2d_sim::world::World, _events: &mut ember2d_sim::event::EventBus, _viewport_width: usize, _viewport_height: usize, _persistent: &mut std::collections::BTreeMap<String, rhai::Dynamic>) {}
    fn update(&mut self, ctx: UpdateContext) { self.handle_update(ctx); }
    fn render(&mut self, ctx: RenderContext) { self.handle_render(ctx); }
    fn take_transition(&mut self) -> Option<Transition> { self.pending_transition.take() }
}
