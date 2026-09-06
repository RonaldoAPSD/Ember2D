// editor/ui/types.rs — Shared UI types and Layout struct.

/// Cell-space text metrics for the editor's still-monospace, 8px-per-cell
/// UI (Phase 7 Part 2c, docs/ember2d-phase7-plan.md) — a thin wrapper over
/// `Font::measure` that returns whole cells instead of pixels, since every
/// `ui::draw_*` call site works in cells. Exact under `BitmapFont` (its
/// own native size, so this reproduces the pre-Part-2c `.len()`
/// arithmetic bit-for-bit); becomes meaningful the day a caller ever hands
/// this a `TtfFont` instead. Shared here rather than duplicated per file,
/// since every `ui/` submodule needs the exact same computation.
pub fn cells(font: &mut dyn ember2d::renderer::Font, text: &str) -> usize {
    (font.measure(text, 8.0).0 / 8.0).round() as usize
}


// ── DockSide ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockSide { None, Left, Right, Bottom }

// ── PanelId ───────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum PanelId {
    Viewport,
    Hierarchy,
    Palette,
    Inspector,
    Console,
    Stats,
    ScriptEditor,
    FileBrowser,
}

// ── Context Menu ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ContextMenuAction {
    // File Browser
    NewLevel,
    NewScript,
    NewFolder,
    DeleteFile(String),
    // Tabs
    CloseTab(PanelId),
    CloseOthers(PanelId),
    FloatPanel(PanelId),
    // Hierarchy
    FocusCamera(HierarchySelection),
    DuplicateEntity(HierarchySelection),
    DeleteEntity(HierarchySelection),
}

pub struct ContextMenu {
    pub x: usize,
    pub y: usize,
    pub items: Vec<(&'static str, ContextMenuAction)>,
    pub selected: usize,
}

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

// `Eq, Hash` added in Phase 7 Part 1d (docs/ember2d-phase7-plan.md) so
// `MenuKind` can be a `WidgetId` field (`ui/frame.rs`) — `WidgetId` itself
// derives both (matching `PanelId`'s existing derives, which `WidgetId`
// already depended on) for the same reason `PanelId` does: cheap value
// equality for `UiFrame::hit`'s comparisons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
    pub show_script_editor: bool,
    pub show_file_browser:  bool,
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
    ToggleScriptEditor,
    ToggleFileBrowser,
    TogglePhysics,
    ToggleHierarchy,
    ToggleHelp,
    Save,
    SaveAs,
    Export,
    Play,
    CloseProject,
    RenameLevel,
    ResizeLevel,
    SetSpawn,
    AddNamedSpawn,
    NewLevel,
    NewScript,
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
    pub zoom:        f32,
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
            zoom:        1.0,
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
pub const INSP_LAYER_OFF:   usize = 21;
pub const INSP_MASK_OFF:    usize = 23;
