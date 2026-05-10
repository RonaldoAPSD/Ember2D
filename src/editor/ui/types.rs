// editor/ui/types.rs — Shared UI types and Layout struct.


// ── Hierarchy selection ───────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum HierarchySelection {
    Player,
    Spawn(usize),
}

// ── Tool / toolbar types ──────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToolKind {
    Paint,
    Select,
    Rect,
    Line,
    Fill,
    Copy,
    Cut,
    Paste,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum MenuKind { File, Edit, Level, View, Tools, Layers }

pub struct MenuState {
    pub can_undo:       bool,
    pub can_redo:       bool,
    pub clipboard_full: bool,
    pub show_palette:   bool,
    pub show_grid:      bool,
    pub show_hierarchy: bool,
    pub show_inspector: bool,
    pub show_console:   bool,
    pub show_stats:     bool,
    pub show_physics:   bool,
    pub active_tool:    ToolKind,
    pub active_layer:   u8,
}

#[derive(Debug, Clone)]
pub enum ToolbarAction {
    SetTool(ToolKind),
    Undo,
    Redo,
    ToggleGrid,
    ToggleInspector,
    ToggleConsole,
    TogglePalette,
    ToggleStats,
    TogglePhysics,
    ToggleHierarchy,
    ToggleHelp,
    New,
    Open,
    Save,
    SaveAs,
    Play,
    CloseProject,
    RenameLevel,
    ResizeLevel,
    SetSpawn,
    AddNamedSpawn,
    NewLevel,
    OpenDocs,
    SetLayer(u8),
}

// ── Dynamic layout ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Layout {
    pub screen_w:    usize,
    pub screen_h:    usize,
    pub canvas_x:    usize,
    pub canvas_y:    usize,
    pub canvas_w:    usize,
    pub canvas_h:    usize,
    pub toolbar_row: usize,
}

impl Layout {
    pub fn new(screen_w: usize, screen_h: usize) -> Self {
        Layout {
            screen_w,
            screen_h,
            canvas_x:    0,
            canvas_y:    2,
            canvas_w:    screen_w,
            canvas_h:    screen_h.saturating_sub(3).max(4),
            toolbar_row: 1,
        }
    }

    pub fn with_canvas(mut self, cx: usize, cy: usize, cw: usize, ch: usize) -> Self {
        self.canvas_x = cx;
        self.canvas_y = cy;
        self.canvas_w = cw;
        self.canvas_h = ch;
        self
    }
}

pub const HIER_W: usize = 14;

pub const INSP_NAME_OFF:    usize = 2;
pub const INSP_GLYPH_OFF:   usize = 3;
pub const INSP_TAG_OFF:     usize = 5;
pub const INSP_FG_OFF:      usize = 6;
pub const INSP_BG_OFF:      usize = 7;
pub const INSP_SOLID_OFF:   usize = 9;
pub const INSP_TRIG_OFF:    usize = 10;
pub const INSP_CAM_OFF:     usize = 11;
pub const INSP_SCRIPT_OFF:  usize = 13;
pub const INSP_EXIT_OFF:    usize = 14;
pub const INSP_GRAPH_BTN:   usize = 17;
