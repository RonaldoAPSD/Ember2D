// editor/mod.rs — Level editor core.

use std::collections::BTreeMap;

mod accessors;
pub mod assets;
pub mod commands;
pub mod grid;
pub mod palette;
pub mod panel;
pub mod prefs;
pub mod start_screen;
pub mod ui;

mod graph_ui;
pub mod helpers;
mod impl_render;
mod impl_state;
pub mod clip_editor;
pub mod importer;
mod input;
mod mode;
pub mod sprites;
mod theme_loader;

use commands::UndoStack;
use ember2d::engine::{GameState, RenderContext, Transition, UpdateContext};
use ember2d::renderer::{Font, Texture, UiSpace};
use ember2d::theme::Theme;
use ember2d_sim::level::TileRecord;
use ember2d_sim::scripting::LogEntry;
use grid::LevelGrid;
use palette::TilePalette;
use panel::{PanelId, PanelManager};
use prefs::{EditorPrefs, PrefsStore};
use theme_loader::{list_available_themes, load_editor_theme_named, DEFAULT_THEME};
pub use ui::HierarchySelection;
pub use mode::{EditorMode, Modal, ModalPurpose, TextInputPurpose};
use ui::{MenuKind, UiFrame};

/// One in-progress freehand paint/scatter/erase-drag batch's accumulated
/// per-cell edits — see `EditorState::paint_batch`'s own doc comment
/// (7C-6, master plan §5.3, D18). Named so clippy's `type_complexity`
/// lint doesn't flag the field's own type directly.
pub(super) type PaintBatch = BTreeMap<(i32, i32, u8), (Option<TileRecord>, Option<TileRecord>)>;

// `Hash` (7D-3, master plan §5.4): lets this double as a `WidgetId`
// variant's own payload (`WidgetId::PaletteEditorField`), the same reason
// `HierarchySelection`/`InspectorField` derive it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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

/// Which kind of edit the script buffer's undo stack last checkpointed
/// (7C-8, master plan §5.3) — consecutive edits of the SAME group coalesce
/// into one undo step (so typing a whole word doesn't cost a Ctrl+Z per
/// character), but switching group (including moving the cursor without
/// editing) always starts a fresh one. `Enter`/cut/paste/undo/redo always
/// force `None` afterward rather than being a group of their own, so they
/// never coalesce with whatever comes next either.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScriptEditGroup {
    None,
    Insert,
    Delete,
}

// `ModalPurpose`, `TextInputPurpose`, `Modal`, and `EditorMode` itself
// live in `mode.rs` (Step 8-2, docs/ember2d-master-plan.md §5.7 — this
// file was at the 750-line limit); re-exported here so every existing path
// to them is unchanged.

pub struct EditorState {
    /// The editor's own chrome — panels, menus, modals, text fields; never
    /// the tile grid/viewport/node graph canvas (7C-9 decision gate, §7.1).
    /// Loaded by `load_editor_theme_named`; a missing/broken theme
    /// directory falls back to `Theme::fallback` internally rather than
    /// leaving this field unusable (7D-1, §5.4). Reloaded in place by
    /// `switch_theme` (7D-4) when the user picks a `View > Theme` entry.
    pub(super) theme: Theme,
    /// `self.theme.chrome` (a `TextureId`), already resolved to real
    /// pixel data — see `load_editor_theme_named`'s own doc comment for why
    /// this is a plain owned `Texture`, not a kept-alive `AssetManager`.
    pub(super) theme_chrome_tex: Texture,
    /// Every `themes/*` subdirectory with a real `theme.ron` in it, scanned
    /// ONCE at startup (`list_available_themes`) — what `View > Theme`
    /// lists (7D-4, §5.4). Not re-scanned while running, so a theme
    /// dropped into `themes/` mid-session needs a restart to appear.
    pub(super) available_themes: Vec<String>,
    /// The script editor's own monospace font (7D-3, master plan §5.4) —
    /// resolved from `self.theme.code_font`, or a SEPARATE freshly-built
    /// instance of `self.theme.font` when the theme leaves `code_font`
    /// unset (`load_theme_fonts`, `theme_loader.rs`) — kept as its own
    /// field, not a second reference to `self.font`, since a theme's body
    /// and code fonts can be genuinely different faces/files, and each
    /// `Font` impl owns its own mutable rasterization cache. Unused until
    /// this step's own script-editor checkpoint actually draws through it.
    pub(super) code_font: Box<dyn Font>,
    /// User preferences — UI scale choice and active theme name — loaded
    /// once at construction via `prefs_store` and saved back to it on
    /// change (7D-3, master plan §5.4). Defaults (`EditorPrefs::default`)
    /// until `with_prefs` (`theme_loader.rs`) overwrites them; see that
    /// method's own doc comment for the "only `ember2d-app` ever calls
    /// `PrefsStore::user()`" contract that keeps tests off the real file.
    pub(super) prefs: EditorPrefs,
    /// Where `prefs` persists to — `PrefsStore::InMemory` by default (every
    /// test, and `EditorState::new`/`load`/`new_from_result` before
    /// `with_prefs` runs), `PrefsStore::File` only after `with_prefs` is
    /// called with a real path (`ember2d-app/src/app.rs`, the only caller).
    pub(super) prefs_store: PrefsStore,
    /// The points<->logical<->physical conversion (7D-3, master plan §5.4)
    /// as of the LAST real draw — captured once per frame at the top of
    /// `handle_render` (`UiSpace::from_surface`) and read back during the
    /// FOLLOWING frame's `handle_update` for chrome hit-testing, the same
    /// one-frame-lag contract `ui_frame` already keeps (its own doc
    /// comment) — a chrome click is always tested against what was ACTUALLY
    /// drawn, not a value recomputed fresh (and potentially different, if
    /// the window resized mid-frame) during input handling. Starts at
    /// `UiSpace::identity` (points == logical pixels) before the first
    /// frame ever renders, harmless since `ui_frame` is equally empty then.
    pub(super) ui_space: UiSpace,
    pub(super) menu_hover_item: Option<usize>, // R89 §3 — see hovered_menu_item()
    /// The `ui_scale` (physical pixels per point) `font`/`code_font` are
    /// CURRENTLY built at (7D-3, master plan §5.4) — compared against
    /// `effective_ui_scale`'s result once per draw
    /// (`rebuild_fonts_if_scale_changed`, `theme_loader.rs`) so a DPI or
    /// scale-preference change rebuilds the fonts at their new real size
    /// instead of drawing stale-scale glyphs until the next theme switch.
    pub(super) font_raster_scale: f32,
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
    /// Step 8-2 (docs/ember2d-master-plan.md §5.7): the project's tilesets
    /// and their sheet images — what the canvas draws sprite tiles with and
    /// the palette draws thumbnails with. Reloaded with the palette on
    /// project open (`load_palette`) and after every import.
    pub(super) sprites: sprites::SpriteAssets,
    /// Step 8-2: the tileset importer's in-progress state, `Some` exactly
    /// while `mode == EditorMode::TilesetImport`.
    pub(super) tileset_import: Option<importer::TilesetImport>,
    /// Step 8-3: seconds of real (presentation) time since the editor
    /// started — the clock animated tiles, palette thumbnails and the clip
    /// editor's preview play against. Editor-only; never reaches the sim.
    pub(super) anim_time: f32,
    /// Step 8-3: the clip editor's in-progress state, `Some` exactly while
    /// `mode == EditorMode::ClipEditor`.
    pub(super) clip_editor: Option<clip_editor::ClipEditor>,
    /// Step 8-4: a drag started on a File Browser asset row (an image, a
    /// tileset, a clip) — `Some` from the press until the release, when it
    /// drops onto the palette or a canvas tile (`impl_state/asset_drop.rs`).
    pub(super) asset_drag: Option<assets::AssetDrag>,
    /// Step 8-4: the File Browser's image thumbnails for the folder it's
    /// showing, by project-relative path — loaded by `refresh_project_files`
    /// (so drawing never touches the disk), replaced on every refresh.
    pub(super) file_thumbs: std::collections::BTreeMap<String, ember2d::renderer::Texture>,
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
    /// R109 (Phase 9 gate pass): how the open script's file ended its lines
    /// (`"\r\n"` or `"\n"`) and whether it ended with one — remembered at
    /// load so `save_script` writes them back. `str::lines` drops both, and
    /// saving used to rewrite every CRLF file to LF and strip its final
    /// newline, a whole-file diff for a one-character edit.
    pub(super) script_eol: &'static str,
    pub(super) script_trailing_newline: bool,
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
    /// The OTHER end of an in-progress selection, `(char_idx, line_idx)` —
    /// same shape as `script_cursor`, which is always the moving end
    /// (7C-8, master plan §5.3). `None` means no selection. Order relative
    /// to `script_cursor` is never assumed; `script_selection_range`
    /// normalizes it.
    pub(super) script_selection_anchor: Option<(usize, usize)>,
    /// Columns scrolled off the left of the script editor's text area
    /// (7C-8, master plan §5.3) — the horizontal twin of `script_scroll`,
    /// auto-tracked the same way so the cursor always stays visible.
    pub(super) script_hscroll: usize,
    /// The script editor's OWN clipboard (7C-8, master plan §5.3) —
    /// deliberately separate from `self.clipboard` (the level grid's tile
    /// clipboard, a completely different shape) and deliberately the real
    /// source of truth for paste, not `arboard`'s live OS clipboard: a
    /// paste always reads this field, so a cut/copy → paste round trip
    /// inside the editor never depends on a display/clipboard server
    /// existing (headless CI, most notably) even though cut/copy also
    /// best-effort mirrors this text out to the OS clipboard via
    /// `arboard` for pasting into (not from) another application.
    pub(super) script_clipboard: String,
    /// Undo/redo stacks for the script text buffer (7C-8, master plan
    /// §5.3) — each entry is a full `(buffer, cursor)` snapshot taken
    /// BEFORE the edit it precedes, not a diff: scripts are small text
    /// files, so the simplicity of "restore the whole thing" comfortably
    /// outweighs the memory cost a diff-based stack would save. Entirely
    /// separate from `commands::UndoStack` (the level grid's own undo,
    /// 7C-6) — that one's `Command` variants are grid-edit-shaped
    /// (`PlaceTile`, `UpdateExtraSpawns`, ...) and have nothing to do with
    /// text. `redo` is cleared by any new checkpoint (`checkpoint_script_edit`),
    /// matching how the grid's own undo/redo already behaves.
    pub(super) script_undo: Vec<(Vec<String>, (usize, usize))>,
    pub(super) script_redo: Vec<(Vec<String>, (usize, usize))>,
    /// Which group the LAST script-buffer checkpoint belonged to (7C-8,
    /// master plan §5.3) — see `ScriptEditGroup`'s own doc comment.
    pub(super) script_undo_group: ScriptEditGroup,
    /// Ctrl+F opens a one-line find bar in place of the topmost text row
    /// (7C-8, master plan §5.3) — `true` while it's showing and consuming
    /// typed input instead of the script buffer.
    pub(super) script_find_active: bool,
    pub(super) script_find_query: String,
    /// Where the cursor was when Ctrl+F opened (7C-8, master plan §5.3) —
    /// live-as-you-type search always re-searches from here (not from
    /// wherever the previous keystroke's match landed), so growing or
    /// shrinking the query re-searches consistently instead of drifting.
    /// Enter (`script_find_next`) searches from the current match's end
    /// instead, advancing through the buffer.
    pub(super) script_find_origin: (usize, usize),
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
    /// The editor's active text-metrics AND (as of 7D-2, master plan
    /// §5.4) drawing source — every `ui::draw_*`/`graph_ui::*` call that
    /// used to compute a position from a string's raw `.len()` goes
    /// through `Font::measure`/`glyph` on this (Phase 7 Part 2c), and a
    /// growing set of chrome call sites now draw through it directly via
    /// `draw_text_px` too. This IS the theme's own loaded font
    /// (`load_editor_theme_named`, `theme_loader.rs`) — exactly the swap
    /// this field's original Part 2c doc comment predicted ("so a future
    /// theme can swap it for a `TtfFont` without call sites changing
    /// again"): one field, not a second `theme_font` alongside it. Also
    /// reassigned by `switch_theme` (7D-4) when the theme changes at runtime.
    pub(super) font: Box<dyn Font>,
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

/// Placeholder point-space screen size for `PanelManager` at construction
/// time, before a real window (and so a real `renderer.pixel_width`/
/// `height`) exists — `EditorState::new` takes only a save path, not a
/// `&Renderer` (Phase 7 Part 1e, docs/ember2d-phase7-plan.md, E6). Not a
/// guess at the actual viewport size: `handle_render`'s very first line
/// unconditionally calls `self.panels.apply_layout(renderer.pixel_width,
/// renderer.pixel_height, ...)` before any drawing happens — so whatever
/// this constant is set to is never itself visible on screen. `640x384`
/// (7D-3, master plan §5.4 — was `80x24` CELLS, i.e. these exact same
/// numbers times `CELL_W`/`CELL_H`) carries no meaning beyond "some
/// placeholder terminal-shaped size," same as before. (7C-3, master plan
/// §5.3, E4: `Layout`, which this comment used to also cite, no longer
/// exists — `PanelManager` alone now owns both panel geometry and the
/// overall screen size, see its own `screen_size_pt`.)
const PLACEHOLDER_SCREEN_W: f32 = 640.0;
const PLACEHOLDER_SCREEN_H: f32 = 384.0;

impl EditorState {
    pub fn new(save_path: &str) -> Self {
        // `1.0` (7D-3, master plan §5.4): no real window/`UiSpace` exists yet
        // at construction (same reasoning as `PLACEHOLDER_SCREEN_W/H`
        // below) — `rebuild_fonts_if_scale_changed` (`theme_loader.rs`)
        // re-derives these at the real scale on the first actual draw.
        let (theme, theme_chrome_tex, theme_font, theme_code_font) =
            load_editor_theme_named(DEFAULT_THEME, 1.0);
        let placeholder_metrics = ui::ChromeMetrics::from_theme(&theme);
        EditorState {
            theme,
            theme_chrome_tex,
            available_themes: list_available_themes(),
            code_font: theme_code_font,
            prefs: EditorPrefs::default(),
            prefs_store: PrefsStore::InMemory(EditorPrefs::default()),
            ui_space: UiSpace::identity((0.0, 0.0)),
            menu_hover_item: None,
            font_raster_scale: 1.0,
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
            sprites: sprites::SpriteAssets::default(),
            tileset_import: None,
            anim_time: 0.0,
            clip_editor: None,
            asset_drag: None,
            file_thumbs: std::collections::BTreeMap::new(),
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
            script_eol: "\n",
            script_trailing_newline: true,
            script_cursor: (0, 0),
            script_scroll: 0,
            script_unsaved: false,
            script_error: None,
            script_idle_timer: 0,
            script_selection_anchor: None,
            script_hscroll: 0,
            script_clipboard: String::new(),
            script_undo: Vec::new(),
            script_redo: Vec::new(),
            script_undo_group: ScriptEditGroup::None,
            script_find_active: false,
            script_find_query: String::new(),
            script_find_origin: (0, 0),
            project_folder: None,
            project_name: None,
            console_log: Vec::new(),
            inspected_pos: None,
            selected_pos: None,
            hierarchy_sel: None,
            ignore_drag: false,
            pan_anchor: None,
            scroll_repeat: 0,
            panels: PanelManager::new(
                PLACEHOLDER_SCREEN_W,
                PLACEHOLDER_SCREEN_H,
                &placeholder_metrics,
            ),
            ui_frame: UiFrame::new(),
            font: theme_font,
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

    // Read-only accessors (7C-5, master plan §5.3) moved to `accessors.rs`
    // (7D-3, master plan §5.4) — see that file's own header comment for why.

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
        // Step 8-2: the palette's sprite thumbnails (and the canvas's
        // sprite tiles) draw from the project's tilesets — loaded alongside
        // it, on every path that opens a project.
        for problem in self.sprites.reload(self.project_folder.as_deref()) {
            self.console_log.push(LogEntry::warn(problem));
        }
    }

    /// R14 (7A-2, docs/ember2d-master-plan.md) — see `EditorFocus`'s own
    /// doc comment for why this is derived rather than a stored field.
    pub(crate) fn focus(&self) -> EditorFocus {
        if matches!(self.mode, EditorMode::Script)
            || self.focused_panel == Some(PanelId::ScriptEditor)
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
    /// Back on top after an F5 preview (R51, master plan §3.2): `draw` — where
    /// `ui_frame`/`ui_space`/`apply_layout` are refreshed — no longer runs
    /// while `PlayState` is stacked above this, so the first frame back runs
    /// `update()` against whatever `UiFrame` was captured before F5, stale
    /// if the window was resized during play. Cleared here so that one frame
    /// hit-tests nothing (the same first-frame contract `ui/frame.rs`'s own
    /// header describes) rather than pre-preview rects; `draw` rebuilds
    /// everything at the end of that same frame.
    fn on_resume(
        &mut self,
        _world: &mut ember2d_sim::world::World,
        _events: &mut ember2d_sim::event::EventBus,
        _viewport_width: usize,
        _viewport_height: usize,
    ) {
        self.ui_frame.clear();
    }
    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }

    // 7C-7 (master plan §5.3, R18): reuses the existing `receive_log`.
    fn receive_script_log(&mut self, entries: Vec<LogEntry>) {
        self.receive_log(entries);
    }
}
