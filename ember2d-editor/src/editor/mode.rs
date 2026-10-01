// editor/mode.rs — `EditorMode`, the one enum that says what the editor is
// doing right now, plus the small payload types its variants carry
// (`Modal`/`ModalPurpose`, `TextInputPurpose`).
//
// Moved out of editor/mod.rs at Step 8-2 (docs/ember2d-master-plan.md §5.7)
// — that file sat at exactly CLAUDE.md's 750-line limit, and 8-2 needs to
// add a mode (the tileset importer) and state to `EditorState` there. Pure
// relocation: re-exported from mod.rs (`pub use mode::*`), so every
// `crate::editor::EditorMode`/`super::EditorMode` path still resolves and
// nothing about these types changed.

use super::ui::{self, ToolKind};

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
    Select {
        start: Option<(i32, i32)>,
        cutting: bool,
    },
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
    Graph {
        gx: i32,
        gy: i32,
    },
    PaletteEditor,
    /// The Palette panel's search field. Was `palette_search_focused: bool`.
    PaletteSearch,
    /// `true` edits the palette's current foreground color, `false` its
    /// background — always entered from, and always returns to,
    /// `PaletteEditor`.
    ColorPicker {
        is_fg: bool,
    },
    Prompt(TextInputPurpose),
    Modal(Modal),
    ContextMenu(ui::ContextMenu),
    /// Step 8-2: the tileset importer dialog. Its in-progress state is
    /// `EditorState::tileset_import` (edited every frame, so a field, not a
    /// payload here — see this enum's own doc comment).
    TilesetImport,
    /// Step 8-3: the animation clip editor dialog. State lives in
    /// `EditorState::clip_editor`, same reasoning as `TilesetImport`.
    ClipEditor,
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
            EditorMode::TilesetImport => "Import",
            EditorMode::ClipEditor => "Clips ",
        }
    }
}
