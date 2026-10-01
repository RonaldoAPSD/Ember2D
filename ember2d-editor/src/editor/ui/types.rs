// editor/ui/types.rs — Shared UI types.

/// Cell-space text metrics for the editor's still-monospace, 8px-per-cell
/// UI (Phase 7 Part 2c, docs/ember2d-phase7-plan.md) — a thin wrapper over
/// `Font::measure` that returns whole cells instead of pixels, since every
/// `ui::draw_*` call site works in cells. Exact under `BitmapFont` (its
/// own native size, so this reproduces the pre-Part-2c `.len()`
/// arithmetic bit-for-bit); becomes meaningful the day a caller ever hands
/// this a `TtfFont` instead. Shared here rather than duplicated per file,
/// since every `ui/` submodule needs the exact same computation —
/// `start_screen/drawing.rs` used to keep its own byte-for-byte copy
/// (before it had any dependency on `ui`), now reaches in for this one
/// like everything else does (7C-2, master plan §5.3, E2).
pub fn cells(font: &mut dyn ember2d::renderer::Font, text: &str) -> usize {
    (font.measure(text, ember2d::renderer::CELL_W as f32).0 / ember2d::renderer::CELL_W as f32)
        .round() as usize
}

// ── DockSide ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DockSide {
    None,
    Left,
    Right,
    Bottom,
}

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

#[derive(Debug)]
pub struct ContextMenu {
    pub x: usize,
    pub y: usize,
    pub items: Vec<(&'static str, ContextMenuAction)>,
    pub selected: usize,
}

// ── Hierarchy selection ───────────────────────────────────────────────────────

// 7C-1 (master plan §5.3): `Eq`/`Hash` added so this can be a `WidgetId`
// variant's own payload (`WidgetId::HierarchyRow`) — `WidgetId` itself
// derives both.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum HierarchySelection {
    Player,
    Spawn(usize),
}

// ── Tool / toolbar types ──────────────────────────────────────────────────────

/// The persistent "paint brush" a click on the canvas uses — 7C-4 (master
/// plan §5.3): shrunk from its old 8-variant self (`Select`/`Copy`/`Cut`/
/// `Paste` used to live here too, as values of `EditorState::active_tool`
/// kept manually in sync with the *separate* `select_mode`/`selecting`/
/// `cutting`/`pasting` bools that actually drove behavior — the exact
/// "two sources of truth" `EditorMode` replaces). Those four are now
/// `EditorMode` variants in their own right (`Inspect`, `Select`, `Paste`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ToolKind {
    Paint,
    Rect,
    Line,
    Fill,
}

// `Eq, Hash` added in Phase 7 Part 1d (docs/ember2d-phase7-plan.md) so
// `MenuKind` can be a `WidgetId` field (`ui/frame.rs`) — `WidgetId` itself
// derives both (matching `PanelId`'s existing derives, which `WidgetId`
// already depended on) for the same reason `PanelId` does: cheap value
// equality for `UiFrame::hit`'s comparisons.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MenuKind {
    File,
    Edit,
    Level,
    View,
    Tools,
    Layers,
    /// 7D-4 (master plan §5.4): the one menu whose entries aren't a fixed
    /// compile-time list — see `menu.rs`'s own `theme_menu_entries`.
    Theme,
}

pub struct MenuState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub clipboard_full: bool,
    pub show_palette: bool,
    pub show_grid: bool,
    pub show_hierarchy: bool,
    pub show_inspector: bool,
    pub show_console: bool,
    pub show_stats: bool,
    pub show_script_editor: bool,
    pub show_file_browser: bool,
    pub show_physics: bool,
    /// Meaningful only when `mode == EditorMode::Paint(_)` — which of the
    /// four `ToolKind`s. The other four checkmarks below (7C-4, master plan
    /// §5.3) cover the modes `ToolKind` itself used to also represent.
    pub active_tool: ToolKind,
    pub inspecting: bool,
    pub copying: bool,
    pub cutting: bool,
    pub pasting: bool,
    pub active_layer: u8,
    /// 7D-4 (master plan §5.4): `self.theme.name` — which `View > Theme`
    /// entry (`menu_checkmark`) gets the active checkmark.
    pub current_theme: String,
    /// 7D-3 checkpoint 7 (master plan §5.4): `self.prefs.ui_scale` — which
    /// Theme menu Auto/1x/2x/3x/4x entry (`menu_checkmark`) gets the
    /// active checkmark. The PREFERENCE, not `effective_ui_scale()`'s own
    /// resolved number — `Auto` itself is what should show checked while
    /// `Auto` is selected, regardless of which physical scale it currently
    /// resolves to.
    pub current_ui_scale: crate::editor::prefs::UiScaleChoice,
}

#[derive(Debug, Clone)]
pub enum ToolbarAction {
    SetTool(ToolKind),
    /// The four menu/shortcut entries that used to be `SetTool(ToolKind::
    /// Select/Copy/Cut/Paste)` before `ToolKind` shrank (7C-4, master plan
    /// §5.3) — each maps to an `EditorMode` variant of the same shape
    /// instead of a `ToolKind` value.
    EnterInspect,
    EnterCopy,
    EnterCut,
    EnterPaste,
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
    /// Step 8-2: File > Import Tileset... (OS image picker, then the
    /// importer dialog).
    ImportTileset,
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
    /// 7D-4 (master plan §5.4): switch to `themes/<name>/` at runtime — the
    /// one `ToolbarAction` a `MenuEntry` gets from `theme_menu_entries`
    /// rather than `menu_entries`'s fixed per-`MenuKind` lists.
    SetTheme(String),
    /// 7D-3 checkpoint 7 (master plan §5.4): the Theme menu's own
    /// Auto/1x/2x/3x/4x picker — `theme_menu_entries` appends these after
    /// the theme list itself, same "runtime-built list" reasoning as
    /// `SetTheme` above.
    SetUiScale(crate::editor::prefs::UiScaleChoice),
}

// ── Chrome row constants ────────────────────────────────────────────────────────

// `TOOLBAR_ROW` (the menu bar's own fixed cell row) removed (7D-3,
// docs/ember2d-master-plan.md §5.4) — the toolbar's real row position is
// `theme.metrics.row_h`-based now (`ui/menu.rs::draw_menu_toolbar`), and
// its click gate reads `WidgetId::MenuBar`/`MenuLabel` from `UiFrame`
// instead of comparing against a cell-row constant
// (`input/panels/menu_bar.rs`).

pub const HIER_W: usize = 14;

pub const INSP_NAME_OFF: usize = 2;
pub const INSP_GLYPH_OFF: usize = 3;
pub const INSP_TAG_OFF: usize = 5;
pub const INSP_FG_OFF: usize = 6;
pub const INSP_BG_OFF: usize = 7;
pub const INSP_SOLID_OFF: usize = 9;
pub const INSP_TRIG_OFF: usize = 10;
pub const INSP_CAM_OFF: usize = 11;
pub const INSP_SCRIPT_OFF: usize = 13;
pub const INSP_EXIT_OFF: usize = 14;
pub const INSP_GRAPH_BTN: usize = 17;
pub const INSP_LAYER_OFF: usize = 21;
pub const INSP_MASK_OFF: usize = 23;
