// editor/mod.rs — Level editor core.

use std::collections::BTreeMap;

pub mod commands;
pub mod grid;
pub mod palette;
pub mod panel;
pub mod start_screen;
pub mod ui;

mod graph_ui;
pub mod helpers;
mod impl_render;
mod impl_state;
mod input;

use commands::UndoStack;
use ember2d::engine::{GameState, RenderContext, Transition, UpdateContext};
use ember2d_sim::level::TileRecord;
use ember2d_sim::scripting::LogEntry;
use grid::LevelGrid;
use palette::TilePalette;
use panel::{PanelId, PanelManager};
pub use ui::HierarchySelection;
use ui::{MenuKind, ToolKind, UiFrame};

/// One in-progress freehand paint/scatter/erase-drag batch's accumulated
/// per-cell edits — see `EditorState::paint_batch`'s own doc comment
/// (7C-6, master plan §5.3, D18). Named so clippy's `type_complexity`
/// lint doesn't flag the field's own type directly.
pub(super) type PaintBatch = BTreeMap<(i32, i32, u8), (Option<TileRecord>, Option<TileRecord>)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteField {
    Name,
    Tag,
    Glyph,
}

/// R14 (7A-2, docs/ember2d-master-plan.md): the seed of the fuller
/// `EditorMode` enum 7C-4 (master plan §5.3) replaced the boolean-soup
/// dispatch with. Stayed a *derived* value, computed in `focus()` below
/// from `mode`/`focused_panel`, rather than becoming a third piece of
/// stored state to keep in sync — exactly what its own original comment
/// here predicted 7C-4 could do without touching any `focus()` call site.
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
pub enum ModalPurpose {
    /// Only shown when `self.unsaved` — see `handle_file_browser_click`'s
    /// own comment (7C-6, master plan §5.3): a switch that would lose
    /// nothing isn't destructive, so it proceeds straight through instead
    /// of asking.
    ConfirmSwitchLevel { path: String },
    /// Always shown, unconditionally — a new level's own name prompt (and
    /// the auto-save it triggers) is easy to click into by mistake before
    /// realizing the current level is about to be replaced (7C-6, master
    /// plan §5.3, CLAUDE.md's "Development Rules": "new level... confirms
    /// first," no `unsaved` qualifier there, unlike level switch above).
    ConfirmNewLevel,
    /// Always shown, unconditionally — deleting a file is irreversible
    /// (no undo stack entry, unlike everything else this phase's own step
    /// added undo for) regardless of whether the CURRENT level has
    /// unsaved edits, so it isn't gated on `unsaved` the way level-switch
    /// is (7C-6, master plan §5.3).
    ConfirmDeleteFile { path: String },
}

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
    TileColliderMask { gx: i32, gy: i32 },
    PlayerColliderLayer,
    PlayerColliderMask,
    NewScriptName,
    PaletteFgCustom,
    PaletteBgCustom,
}

#[derive(Debug)]
pub struct Modal {
    pub title: String,
    pub message: String,
    pub purpose: ModalPurpose,
}

/// Replaces ~15 mutually-exclusive `bool`/`Option` fields that used to live
/// directly on `EditorState` (7C-4, master plan §5.3) — `palette_editor_open`,
/// `palette_search_focused`, `text_input`, `modal`, `selecting`, `cutting`,
/// `pasting`, `active_tool` (partly — see `ToolKind`'s own doc comment),
/// `select_mode`, `placing_spawn`, `placing_named_spawn`, `script_mode`,
/// `graph_mode`, `color_picker_open`, `context_menu`. Correctness used to
/// depend on `handle_update`'s if-chain checking them in exactly the right
/// order (E.g. modal before context menu before color picker before...);
/// now there's one field and one `match`.
///
/// A payload lives directly in a variant when it's small and freshly
/// created every time that mode is entered (`Modal`, `ContextMenu`,
/// `TextInputPurpose` — all `Copy` or cheap to move). Payloads that need to
/// persist independently of *which* mode is active right now (the loaded
/// script's buffer, the node graph's pan offset, a prompt's typed text)
/// stay as their own always-present fields on `EditorState`, exactly as
/// before — folding them into the enum would mean projecting a `&mut`
/// through a `match` on every keystroke for no benefit.
///
/// Closing an overlay (`PaletteEditor`'s `ColorPicker` aside, which always
/// returns there — it's the only mode that ever opens one) resets `mode` to
/// `Paint(ToolKind::Paint)`, not whatever was active before the overlay
/// opened. Every one of `PlaceSpawn`/`Script`/`Graph`/`PaletteEditor`/
/// `PaletteSearch`/`Prompt`/`Modal`/`ContextMenu` was already reachable
/// this way (a keyboard shortcut or a panel click that doesn't itself check
/// `mode`), so in principle one could open a confirm modal while mid-paste
/// and, under the old code, resume pasting after dismissing it — an
/// interaction no shipped demo or documented workflow exercises, and one
/// this refactor deliberately no longer preserves rather than adding a mode
/// stack to keep alive.
#[derive(Debug)]
pub enum EditorMode {
    Paint(ToolKind),
    /// Was `select_mode: bool` — click-to-inspect, no painting.
    Inspect,
    /// Was `selecting`/`cutting`/`sel_anchor`.
    Select { start: Option<(i32, i32)>, cutting: bool },
    Paste,
    /// `None` places the single default spawn; `Some(name)` adds a named
    /// one — was `placing_spawn: bool` and `placing_named_spawn:
    /// Option<String>`, two fields that were never both engaged at once.
    PlaceSpawn(Option<String>),
    /// Fullscreen script editor. Was `script_mode: bool` — the *docked*
    /// script panel merely having keyboard focus is a separate, narrower
    /// concern (`EditorFocus`, below) that doesn't touch `mode` at all,
    /// since normal editing stays available around it.
    Script,
    Graph { gx: i32, gy: i32 },
    PaletteEditor,
    /// The Palette panel's search field. Was `palette_search_focused: bool`.
    PaletteSearch,
    /// `true` edits the palette's current foreground color, `false` its
    /// background — always entered from, and always returns to,
    /// `PaletteEditor`.
    ColorPicker { is_fg: bool },
    Prompt(TextInputPurpose),
    Modal(Modal),
    ContextMenu(ui::ContextMenu),
}

impl Default for EditorMode {
    fn default() -> Self {
        EditorMode::Paint(ToolKind::Paint)
    }
}

impl EditorMode {
    /// The top-right toolbar indicator's text — was a `match` on the
    /// separate `active_tool: ToolKind` field, which only ever covered
    /// `Paint`'s four sub-tools plus the four modes that used to be
    /// `ToolKind` values too (`Select`/`Copy`/`Cut`/`Paste`). The other
    /// modes below were never reflected here before (`active_tool` simply
    /// kept whatever value it last had while one of them was active) —
    /// showing their own label instead is a small, arguably clearer
    /// change now that there's one field to ask, not a deliberate
    /// redesign of this indicator.
    pub(super) fn toolbar_label(&self) -> &'static str {
        match self {
            EditorMode::Paint(ToolKind::Paint) => "Paint ",
            EditorMode::Paint(ToolKind::Rect) => "Rect  ",
            EditorMode::Paint(ToolKind::Line) => "Line  ",
            EditorMode::Paint(ToolKind::Fill) => "Fill  ",
            EditorMode::Inspect => "Select",
            EditorMode::Select { cutting: false, .. } => "Copy  ",
            EditorMode::Select { cutting: true, .. } => "Cut   ",
            EditorMode::Paste => "Paste ",
            EditorMode::PlaceSpawn(_) => "Spawn ",
            EditorMode::Script => "Script",
            EditorMode::Graph { .. } => "Graph ",
            EditorMode::PaletteEditor => "Palette",
            EditorMode::PaletteSearch => "Search",
            EditorMode::ColorPicker { .. } => "Color ",
            EditorMode::Prompt(_) => "Prompt",
            EditorMode::Modal(_) => "Modal ",
            EditorMode::ContextMenu(_) => "Menu  ",
        }
    }
}

pub struct EditorState {
    pub(super) grid: LevelGrid,
    pub(super) palette: TilePalette,
    pub(super) undo: UndoStack,
    pub(super) save_path: String,
    pub(super) unsaved: bool,
    pub(super) show_grid: bool,
    pub(super) active_layer: u8,
    pub(super) palette_scroll: usize,
    pub(super) palette_editing_idx: usize,
    /// Meaningful only while `mode == EditorMode::PaletteEditor`.
    pub(super) palette_editor_focus: Option<PaletteField>,
    /// Snapshot of `self.palette` from the instant the modal palette
    /// editor was opened (7C-6, master plan §5.3, D18) — every field edit
    /// inside it mutates `self.palette` immediately (there's no separate
    /// "buffer" `Escape`'s own "dismiss without writing" comment might
    /// suggest — that comment is about skipping the disk write, not
    /// reverting in-memory state), so the whole open-edit-close session
    /// becomes one `Command::UpdatePalette` pushed on exit, `before` =
    /// this snapshot. `None` when no session is open; also `None` (not
    /// reset) while a nested `ColorPicker` excursion is active, since that
    /// always returns to the SAME `PaletteEditor` session rather than
    /// starting a new one.
    pub(super) palette_edit_before: Option<TilePalette>,
    pub(super) save_message: Option<String>,
    pub(super) save_message_timer: u32,
    pub(super) pending_transition: Option<Transition>,
    pub(super) scroll: (f32, f32),
    pub(super) target_scroll: (f32, f32),
    /// Shift+drag rect-fill anchor — usable as a modifier under any
    /// `Paint` tool, not just `ToolKind::Rect`'s own sticky version, so it
    /// stays its own field rather than folding into `EditorMode::Paint`
    /// (7C-4, master plan §5.3).
    pub(super) rect_anchor: Option<(i32, i32)>,
    pub(super) line_anchor: Option<(i32, i32)>,
    pub(super) erase_size: usize,
    /// A freehand paint/scatter/erase-drag stroke's accumulated per-cell
    /// edits, open between mouse-down and mouse-up (7C-6, master plan
    /// §5.3, D18) — closed into one `Command::Batch` on release instead of
    /// pushing a separate `PlaceTile`/`EraseTile` per cell touched during
    /// the drag. `BTreeMap` (not a `Vec`) so a cell touched more than once
    /// in the same stroke keeps its original `before` and just updates
    /// `after` (via `record_paint_batch_edit`), and so the final
    /// `Command::Batch` gets a deterministic cell order for free — same
    /// reasoning as `LevelGrid::tiles`. `None` when no stroke is open.
    pub(super) paint_batch: Option<PaintBatch>,
    /// The text currently typed into an open `EditorMode::Prompt` — kept
    /// separate from the enum for the same reason `script_buffer` is
    /// (7C-4, master plan §5.3): a growable buffer mutated every keystroke
    /// is awkward to project a `&mut` into out of a `match` on `self.mode`.
    pub(super) prompt_buffer: String,
    pub(super) clipboard: Vec<(i32, i32, TileRecord)>,
    pub(super) paste_flip_x: bool,
    pub(super) paste_flip_y: bool,
    pub(super) paste_rotate: i32,
    pub(super) file_browser_files: Vec<String>,
    pub(super) file_browser_cursor: usize,
    pub(super) file_browser_scroll: usize,
    pub(super) current_folder: String,
    pub(super) script_path: Option<String>,
    pub(super) script_buffer: Vec<String>,
    pub(super) script_cursor: (usize, usize),
    pub(super) script_scroll: usize,
    pub(super) script_unsaved: bool,
    /// First live-compile error in the currently open script, if any (7C-7,
    /// master plan §5.3, R18): `(0-based line, message)`. Refreshed by
    /// `check_script_syntax` (on save, and on the idle timer below); `None`
    /// means "compiles cleanly" as much as "nothing checked yet" — there is
    /// no separate "unknown" state, matching every other status flag here.
    pub(super) script_error: Option<(usize, String)>,
    /// Frames since the script buffer was last edited (7C-7, master plan
    /// §5.3, R18) — `note_script_edit` (input/script_editor.rs) resets this
    /// to 0 on every keystroke; `handle_script_mode_input` increments it
    /// once per frame and triggers `check_script_syntax` when it reaches
    /// `SCRIPT_IDLE_CHECK_FRAMES`, so a live syntax check runs ~500ms after
    /// the user stops typing rather than on every character.
    pub(super) script_idle_timer: u32,
    pub project_folder: Option<String>,
    pub project_name: Option<String>,
    pub(super) console_log: Vec<LogEntry>,
    pub(super) inspected_pos: Option<(i32, i32)>,
    pub(super) selected_pos: Option<(i32, i32)>,
    pub(super) hierarchy_sel: Option<HierarchySelection>,
    pub(super) ignore_drag: bool,
    pub(super) pan_anchor: Option<(usize, usize, i32, i32)>,
    pub(super) scroll_repeat: u32,
    pub(super) panels: PanelManager,
    /// One render pass's worth of interactive-widget hit rects (Phase 7
    /// Part 1d, docs/ember2d-phase7-plan.md) — see `ui/frame.rs`'s header
    /// comment. Rebuilt every `handle_render` call; read by the FOLLOWING
    /// frame's `handle_update`/`handle_panel_input`.
    pub(super) ui_frame: UiFrame,
    /// The editor's active text-metrics source (Phase 7 Part 2c,
    /// docs/ember2d-phase7-plan.md) — a `BitmapFont` today, always; every
    /// `ui::draw_*`/`graph_ui::*` call that used to compute a position
    /// from a string's raw `.len()` now goes through `Font::measure`/
    /// `glyph` on this instead, so a future theme (Part 3) can swap it for
    /// a `TtfFont` without those call sites changing again. `Box<dyn Font>`
    /// rather than a concrete `BitmapFont` for exactly that swap.
    pub(super) font: Box<dyn ember2d::renderer::Font>,
    pub(super) focused_panel: Option<PanelId>,
    pub(super) show_physics: bool,
    pub(super) show_help: bool,
    pub(super) active_menu: Option<MenuKind>,
    pub(super) graph_view_ox: i32,
    pub(super) graph_view_oy: i32,
    pub(super) graph_selected_node: Option<ember2d_sim::graph::NodeId>,
    pub(super) graph_connecting: Option<(ember2d_sim::graph::NodeId, usize)>,
    pub(super) graph_dragging_node: Option<(ember2d_sim::graph::NodeId, i32, i32)>,
    /// The dragged node's tile snapshot from the instant the drag started
    /// (7C-6, master plan §5.3, D18) — dragging used to have no undo
    /// tracking at all; this makes the WHOLE drag one `Command::PlaceTile`
    /// pushed on release, not a push per frame. `None` when no drag is in
    /// progress, same lifetime as `graph_dragging_node`.
    pub(super) graph_drag_before: Option<TileRecord>,
    pub(super) graph_palette_open: Option<(usize, usize)>,
    pub(super) graph_palette_scroll: usize,
    pub(super) graph_palette_cursor: usize,
    pub(super) graph_editing_param: Option<(ember2d_sim::graph::NodeId, String)>,
    pub(super) graph_clipboard: Option<ember2d_sim::graph::Node>,
    /// Meaningful only while `mode == EditorMode::ColorPicker { .. }`.
    pub(super) color_picker_hsv: (f32, f32, f32),
    /// 7C-4 (master plan §5.3): the one field that replaces
    /// `palette_editor_open`/`palette_search_focused`/`text_input`/
    /// `modal`/`selecting`/`cutting`/`sel_anchor`/`pasting`/`active_tool`/
    /// `select_mode`/`placing_spawn`/`placing_named_spawn`/`script_mode`/
    /// `graph_mode`/`color_picker_open`/`context_menu` — see
    /// `EditorMode`'s own doc comment.
    pub(super) mode: EditorMode,
    pub(super) zoom: f32,
}

const DEFAULT_LEVEL_W: usize = 32;
const DEFAULT_LEVEL_H: usize = 20;

/// Placeholder cell-grid size for `PanelManager` at construction time,
/// before a real window (and so a real `renderer.width`/`height`) exists —
/// `EditorState::new` takes only a save path, not a `&Renderer` (Phase 7
/// Part 1e, docs/ember2d-phase7-plan.md, E6). Not a guess at the actual
/// viewport size: `handle_render`'s very first line unconditionally calls
/// `self.panels.apply_layout(renderer.pixel_width, renderer.pixel_height)`
/// before any drawing happens — so whatever this constant is set to is
/// never itself visible on screen. `80x24` was kept as the value precisely
/// because it carries no meaning beyond "some placeholder terminal-shaped
/// size." (7C-3, master plan §5.3, E4: `Layout`, which this comment used to
/// also cite, no longer exists — `PanelManager` alone now owns both panel
/// geometry and the overall screen size, see its own `screen_size_px`.)
const PLACEHOLDER_SCREEN_W: usize = 80;
const PLACEHOLDER_SCREEN_H: usize = 24;

impl EditorState {
    pub fn new(save_path: &str) -> Self {
        EditorState {
            grid: LevelGrid::new(DEFAULT_LEVEL_W, DEFAULT_LEVEL_H),
            palette: TilePalette::default_palette(),
            undo: UndoStack::new(),
            save_path: if save_path.is_empty() {
                "level.level".to_string()
            } else {
                save_path.to_string()
            },
            unsaved: false,
            show_grid: false,
            active_layer: 1,
            palette_scroll: 0,
            palette_editing_idx: 0,
            palette_editor_focus: None,
            palette_edit_before: None,
            save_message: None,
            save_message_timer: 0,
            pending_transition: None,
            scroll: (0.0, 0.0),
            target_scroll: (0.0, 0.0),
            rect_anchor: None,
            line_anchor: None,
            erase_size: 1,
            paint_batch: None,
            prompt_buffer: String::new(),
            clipboard: Vec::new(),
            paste_flip_x: false,
            paste_flip_y: false,
            paste_rotate: 0,
            file_browser_files: Vec::new(),
            file_browser_cursor: 0,
            file_browser_scroll: 0,
            current_folder: ".".to_string(),
            script_path: None,
            script_buffer: Vec::new(),
            script_cursor: (0, 0),
            script_scroll: 0,
            script_unsaved: false,
            script_error: None,
            script_idle_timer: 0,
            project_folder: None,
            project_name: None,
            console_log: Vec::new(),
            inspected_pos: None,
            selected_pos: None,
            hierarchy_sel: None,
            ignore_drag: false,
            pan_anchor: None,
            scroll_repeat: 0,
            panels: PanelManager::new(PLACEHOLDER_SCREEN_W, PLACEHOLDER_SCREEN_H),
            ui_frame: UiFrame::new(),
            font: Box::new(ember2d::renderer::BitmapFont::new()),
            focused_panel: None,
            show_physics: false,
            show_help: false,
            active_menu: None,
            graph_view_ox: 0,
            graph_view_oy: 0,
            graph_selected_node: None,
            graph_connecting: None,
            graph_dragging_node: None,
            graph_drag_before: None,
            graph_palette_open: None,
            graph_palette_scroll: 0,
            graph_palette_cursor: 0,
            graph_editing_param: None,
            graph_clipboard: None,
            color_picker_hsv: (0.0, 1.0, 1.0),
            mode: EditorMode::default(),
            zoom: 1.0,
        }
    }

    pub fn load(path: &str) -> Result<Self, String> {
        use ember2d_sim::level::LevelData;
        let data =
            LevelData::load(path).map_err(|e| format!("Failed to load '{}': {}", path, e))?;
        let mut editor = EditorState::new(path);
        editor.grid = LevelGrid::from_level_data(&data);
        Ok(editor)
    }

    /// R45 (7A-12, docs/ember2d-master-plan.md §5.1): `EditorState::load`
    /// alone (used directly by `ember2d-app/src/main.rs`'s `--editor
    /// <path>` CLI branch) never sets `project_folder` — only
    /// `new_from_result` did, for the start-screen "Open Project" flow,
    /// by setting the field and calling `load_palette`/
    /// `refresh_project_files` itself afterward. A level opened via the
    /// direct CLI path loaded and rendered fine, but the Files panel
    /// showed "(empty folder)" and creating a new script silently did
    /// nothing (`impl_state/mod.rs`'s new-script/save-script paths both
    /// require `project_folder` to be `Some`). `pub` (not `pub(super)`,
    /// unlike `load_palette`/`refresh_project_files` themselves) so
    /// `ember2d-app`, a different crate, can call it after `load`/`new`.
    pub fn open_project_folder(&mut self, folder: String) {
        self.project_folder = Some(folder);
        self.load_palette();
        self.refresh_project_files();
    }

    // ── Read-only accessors (7C-5, master plan §5.3) ───────────────────────
    //
    // `EditorState`'s fields are `pub(super)` — deliberately narrow, since
    // most of them are mutated through dozens of call sites that all rely
    // on `EditorMode`/undo/panel invariants holding. `EditorHarness`
    // (`ember2d-editor/tests/common/mod.rs`) is a genuinely external crate
    // (an integration test binary), so it can only see `pub` items —
    // these are exactly the read-only observations that step's own tests
    // need to assert on, and no more: what mode is active, what got
    // painted, which widget frame a click would see, and the current
    // panel layout. None of these exist to be mutated from outside; there
    // is no `pub fn set_mode` or similar alongside them.

    pub fn mode(&self) -> &EditorMode {
        &self.mode
    }

    pub fn ui_frame(&self) -> &UiFrame {
        &self.ui_frame
    }

    pub fn panels(&self) -> &PanelManager {
        &self.panels
    }

    pub fn active_menu(&self) -> Option<MenuKind> {
        self.active_menu
    }

    pub fn grid(&self) -> &LevelGrid {
        &self.grid
    }

    pub fn focused_panel(&self) -> Option<PanelId> {
        self.focused_panel
    }

    /// `true` when global shortcuts apply — `false` while the script
    /// editor (fullscreen or docked-and-focused) owns the keyboard. A
    /// plain `bool` rather than exposing `EditorFocus` itself, which is
    /// `pub(crate)` (see its own doc comment) — this is the one bit of it
    /// a test actually needs.
    pub fn focus_is_canvas(&self) -> bool {
        self.focus() == EditorFocus::Canvas
    }

    pub fn script_buffer(&self) -> &[String] {
        &self.script_buffer
    }

    pub fn file_browser_files(&self) -> &[String] {
        &self.file_browser_files
    }

    pub fn prompt_buffer(&self) -> &str {
        &self.prompt_buffer
    }

    pub fn rect_anchor(&self) -> Option<(i32, i32)> {
        self.rect_anchor
    }

    pub fn show_physics(&self) -> bool {
        self.show_physics
    }

    pub fn palette_tile_count(&self) -> usize {
        self.palette.tiles.len()
    }

    pub fn show_grid(&self) -> bool {
        self.show_grid
    }

    pub fn active_layer(&self) -> u8 {
        self.active_layer
    }

    pub fn unsaved(&self) -> bool {
        self.unsaved
    }

    pub fn script_unsaved(&self) -> bool {
        self.script_unsaved
    }

    /// The current script buffer's first live-compile error, if any (7C-7,
    /// master plan §5.3, R18).
    pub fn script_error(&self) -> Option<(usize, &str)> {
        self.script_error.as_ref().map(|(line, msg)| (*line, msg.as_str()))
    }

    pub fn console_log(&self) -> &[LogEntry] {
        &self.console_log
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.undo.redo_len()
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
                        self.console_log.push(LogEntry::error(format!(
                            "Palette '{}' invalid ({}) — using defaults",
                            path, e
                        )));
                        self.palette = TilePalette::default_palette();
                    }
                }
            }
        }
    }

    /// R14 (7A-2, docs/ember2d-master-plan.md) — see `EditorFocus`'s own
    /// doc comment for why this is derived rather than a stored field.
    pub(crate) fn focus(&self) -> EditorFocus {
        if matches!(self.mode, EditorMode::Script) || self.focused_panel == Some(PanelId::ScriptEditor)
        {
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
                editor.project_name = Some(result.project_name);
                editor.load_palette();
                editor.refresh_project_files();
                Ok(editor)
            }
            Some(template) => {
                std::fs::create_dir_all(&result.project_folder)
                    .map_err(|e| format!("Cannot create '{}': {}", result.project_folder, e))?;
                ProjectData::new(&result.project_name, result.visual_style, result.gameplay_loop)
                    .save(&result.project_folder)
                    .map_err(|e| format!("Cannot write project.ron: {}", e))?;
                let mut editor = EditorState::new(&result.level_path);
                editor.grid.name = result.project_name.clone();
                editor.project_folder = Some(result.project_folder);
                editor.project_name = Some(result.project_name);
                editor.load_palette();
                editor.refresh_project_files();
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
    fn on_start(
        &mut self,
        _world: &mut ember2d_sim::world::World,
        _events: &mut ember2d_sim::event::EventBus,
        _viewport_width: usize,
        _viewport_height: usize,
        _persistent: &mut std::collections::BTreeMap<String, rhai::Dynamic>,
    ) {
    }
    fn update(&mut self, ctx: UpdateContext) {
        self.handle_update(ctx);
    }
    fn render(&mut self, ctx: RenderContext) {
        self.handle_render(ctx);
    }
    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }

    // 7C-7 (master plan §5.3, R18): reuses the existing `receive_log`.
    fn receive_script_log(&mut self, entries: Vec<LogEntry>) {
        self.receive_log(entries);
    }
}
