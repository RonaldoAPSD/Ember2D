// editor/accessors.rs — `EditorState`'s read-only accessors (7C-5, master
// plan §5.3), split out of `editor/mod.rs` (7D-3, master plan §5.4) once
// that file had no room left under the 750-line hard limit (CLAUDE.md) for
// the new `EditorPrefs`/`UiSpace`/`code_font`/`ScriptLayout` fields this
// step adds. Purely a location move — every method here is unchanged from
// `mod.rs`'s own copy; see the original "Read-only accessors" comment
// below, preserved verbatim, for why these exist and why they're `pub`
// while most of `EditorState`'s fields stay `pub(super)`.

use super::grid::LevelGrid;
use super::panel::{PanelId, PanelManager};
use super::ui::{MenuKind, UiFrame};
use super::{EditorFocus, EditorMode, EditorState};
use ember2d::renderer::UiSpace;
use ember2d_sim::scripting::LogEntry;

impl EditorState {
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

    /// Which row of the currently-open dropdown was actually drawn with
    /// the hovered highlight on the last real draw (R89, §3 in the master
    /// plan) — `None` if no menu is open, or the mouse isn't over any row.
    pub fn hovered_menu_item(&self) -> Option<usize> {
        self.menu_hover_item
    }

    // `theme()`/`available_themes()` moved to `theme_loader.rs` (7D-4,
    // same accessor contract) purely to keep this file under 750 lines.

    /// The active `UiSpace` (points<->logical<->physical conversion, 7D-3,
    /// master plan §5.4) as of the last real draw — see
    /// `EditorState::ui_space`'s own doc comment (field, `mod.rs`) for why
    /// it's captured at draw time and read here, not recomputed on demand.
    pub fn ui_space(&self) -> UiSpace {
        self.ui_space
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

    /// `(char_idx, line_idx)` — see `ui::ScriptPos`'s own doc comment.
    pub fn script_cursor(&self) -> (usize, usize) {
        self.script_cursor
    }

    pub fn script_scroll(&self) -> usize {
        self.script_scroll
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

    pub fn palette_tile(&self, idx: usize) -> &super::palette::TileDefinition {
        &self.palette.tiles[idx]
    }

    /// Which tile the palette editor modal is currently open on
    /// (`EditorMode::PaletteEditor`'s own implicit argument, not carried by
    /// the mode itself) — a test opening the editor via the panel's own
    /// `[Edit]` button needs this to read back the right tile afterward.
    pub fn palette_editing_idx(&self) -> usize {
        self.palette_editing_idx
    }

    /// Row count of the built (headers + visible items) Palette layout —
    /// what `draw_palette_panel`/`handle_palette_click` both scroll against
    /// (`WidgetId::PaletteRow(idx)` indexes into this same layout), distinct
    /// from `palette_tile_count`'s raw tile count.
    pub fn palette_layout_len(&self) -> usize {
        self.palette.build_layout().len()
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

    /// The script buffer's own selection, normalized `(start, end)` with
    /// `start <= end` in reading order — `None` when nothing is selected
    /// (7C-8, master plan §5.3).
    pub fn script_selection(&self) -> Option<((usize, usize), (usize, usize))> {
        let anchor = self.script_selection_anchor?;
        let cursor = self.script_cursor;
        // Compare `(line, char)`, not `(char, line)` — line order first
        // matches reading order; `script_cursor`/`script_selection_anchor`
        // are stored `(char, line)` for consistency with `script_cursor`
        // everywhere else, so the tuple itself can't be compared directly.
        if (anchor.1, anchor.0) <= (cursor.1, cursor.0) {
            Some((anchor, cursor))
        } else {
            Some((cursor, anchor))
        }
    }

    pub fn script_clipboard(&self) -> &str {
        &self.script_clipboard
    }

    pub fn script_hscroll(&self) -> usize {
        self.script_hscroll
    }

    pub fn script_undo_len(&self) -> usize {
        self.script_undo.len()
    }

    pub fn script_redo_len(&self) -> usize {
        self.script_redo.len()
    }

    pub fn script_find_active(&self) -> bool {
        self.script_find_active
    }

    pub fn script_find_query(&self) -> &str {
        &self.script_find_query
    }
}
