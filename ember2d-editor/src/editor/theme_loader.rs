// editor/theme_loader.rs — loads the editor's own chrome theme (7D-2,
// docs/ember2d-master-plan.md §5.4) and, as of 7D-4, lists and reloads it
// at runtime for View > Theme switching. Split out of `editor/mod.rs`
// purely to keep that file under CLAUDE.md's 750-line hard limit — no
// behavioral change from when this lived there directly.
//
// 7D-3 (master plan §5.4) additions: fonts are now built at a specific
// PHYSICAL raster scale (`ui_scale`, the same S `UiSpace` uses) rather than
// their theme-authored point size directly, with the printable-ASCII range
// pre-warmed at load/rebuild time (mitigates R75 — the first real draw at a
// new scale no longer re-uploads the whole glyph atlas once per never-seen
// character); a theme's optional `code_font` is resolved alongside `font`,
// falling back to a SEPARATE instance of `font`'s own choice when unset;
// and preferences (`EditorPrefs`/`PrefsStore`) are loaded/saved alongside
// the theme they name.

use super::prefs::{EditorPrefs, PrefsStore, UiScaleChoice};
use super::EditorState;
use ember2d::renderer::{
    glyph_atlas_side_for, AssetManager, BitmapFont, DisplayScale, Font, Texture, TtfFont,
};
use ember2d::theme::{FontChoice, FontSizes, Theme};

/// The theme every fresh `EditorState` starts on — `themes/ember-clean`,
/// the one real theme shipped so far (7D-1's own "Landed as" note:
/// `themes/ember-pixel` is deferred, not built). Named here, not just
/// inlined at the one call site, so `list_available_themes`'s "nothing on
/// disk" fallback below names the exact same theme.
pub(super) const DEFAULT_THEME: &str = "ember-clean";

/// The printable ASCII range (space through `~`) pre-warmed into a TTF
/// theme font's atlas at load/rebuild time (7D-3, master plan §5.4,
/// mitigating R75) — every character a theme's own chrome text realistically
/// draws (labels, titles, file names, script source) is Latin/ASCII; a
/// script buffer's rarer characters still rasterize on first use exactly as
/// before, just without the guaranteed-common case paying that cost too.
const ASCII_PREWARM: std::ops::RangeInclusive<u8> = 0x20..=0x7E;

/// Builds one `Font` from a theme's `FontChoice` at the real PHYSICAL
/// raster scale (`ui_scale`, ×`sizes`' three point sizes) — the atlas is
/// sized once via `glyph_atlas_side_for` for the LARGEST size this theme
/// will ever ask of it, then the whole ASCII prewarm set is rasterized
/// immediately at all three sizes so the atlas is fully populated before
/// this font ever draws a live frame.
fn build_font(choice: &FontChoice, ui_scale: u32, sizes: &FontSizes) -> Box<dyn Font> {
    match choice {
        FontChoice::Bitmap => Box::new(BitmapFont::new()),
        FontChoice::Ttf { path } => std::fs::read(path)
            .ok()
            .and_then(|bytes| {
                let max_pt = sizes.small.max(sizes.body).max(sizes.heading);
                let side = glyph_atlas_side_for(max_pt * ui_scale as f32);
                TtfFont::with_atlas_size(&bytes, 0, side, side).ok()
            })
            .map(|mut font| {
                for &pt in &[sizes.small, sizes.body, sizes.heading] {
                    let raster_px = pt * ui_scale as f32;
                    for byte in ASCII_PREWARM {
                        font.glyph(byte as char, raster_px);
                    }
                }
                Box::new(font) as Box<dyn Font>
            })
            .unwrap_or_else(|| Box::new(BitmapFont::new())),
    }
}

/// `(font, code_font)` for `theme`, both built at `ui_scale` — `code_font`
/// resolves `theme.code_font` if the theme names one, otherwise a SEPARATE
/// instance of `theme.font`'s own choice (see `EditorState::code_font`'s
/// own doc comment for why a second instance, not a shared reference).
pub(super) fn load_theme_fonts(theme: &Theme, ui_scale: u32) -> (Box<dyn Font>, Box<dyn Font>) {
    let font = build_font(&theme.font, ui_scale, &theme.font_sizes);
    let code_choice = theme.code_font.as_ref().unwrap_or(&theme.font);
    let code_font = build_font(code_choice, ui_scale, &theme.font_sizes);
    (font, code_font)
}

/// The editor's chrome theme is loaded and resolved ONCE per switch, rather
/// than lazily on the first real render: `AssetManager` needs no live
/// GPU/window (`AssetManager::new()` is two empty `HashMap`s) — only the
/// eventual GPU *upload* of a texture does, which happens on demand inside
/// `Renderer::draw_texture_px` itself, not here — so there's no reason to
/// defer this until a real `Renderer` exists. The resolved `Texture` is
/// cloned out and the loading `AssetManager` is dropped immediately:
/// keeping it alive just to re-resolve the same `TextureId` every frame
/// would be pure overhead for a texture that never changes between
/// switches. This also means `EditorHarness` (headless tests) gets the
/// exact same real theme-loading path the live app does — no separate
/// test-only code path.
///
/// `ui_scale` (7D-3, master plan §5.4): the physical raster scale to build
/// `font`/`code_font` at — see `build_font`'s own doc comment. Callers
/// before a real `UiSpace` exists (`EditorState::new`, before any window)
/// pass `1`; `EditorState::rebuild_fonts_if_scale_changed` re-derives fonts
/// ALONE (via `load_theme_fonts`, not this whole function) once the real
/// scale is known, without re-reading the theme file or re-decoding the
/// chrome PNG.
///
/// 7D-2's own doc comment on this reserved `Renderer.ui_assets` (7D-1) as
/// what runtime switching "DOES need... the way this one-shot load
/// doesn't," expecting a leak otherwise — investigated before writing
/// 7D-4: `Texture::id` (renderer/texture.rs) comes from a process-wide
/// `AtomicU64`, not anything scoped to one `AssetManager` instance, so a
/// throwaway `AssetManager` on every switch still hands out a genuinely
/// unique id every time — no collision, no stale-texture bug. The old
/// theme's GPU-resident texture does become unreferenced until
/// `WgpuBackend`'s own LRU `texture_budget` (R26, master plan §5.2)
/// evicts it, rather than being freed immediately — acceptable for a
/// rarely-used, user-initiated dev-tool action switching between a
/// handful of small chrome atlases, not worth a second `AssetManager`
/// wired through `EditorState` just to avoid.
pub(super) fn load_editor_theme_named(
    name: &str,
    ui_scale: u32,
) -> (Theme, Texture, Box<dyn Font>, Box<dyn Font>) {
    let mut assets = AssetManager::new();
    let theme = Theme::load(&mut assets, &format!("themes/{name}"));
    let chrome_tex =
        assets.get(theme.chrome).cloned().unwrap_or_else(|| Texture::solid(0xFFFF00FF));
    let (font, code_font) = load_theme_fonts(&theme, ui_scale);
    (theme, chrome_tex, font, code_font)
}

/// Every subdirectory of `themes/` that actually has a `theme.ron` in it —
/// what the View > Theme menu lists (`ui::theme_menu_entries`). Sorted for
/// a stable menu order across runs (directory read order isn't guaranteed).
/// Never empty: a missing/unreadable `themes/` directory, or one with no
/// valid theme subdirectory, still returns `[DEFAULT_THEME]` — the editor
/// already tolerates `DEFAULT_THEME` itself resolving to `Theme::fallback`
/// (`Theme::load`'s own contract), so the menu always has at least one
/// selectable entry rather than silently disappearing.
pub(super) fn list_available_themes() -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir("themes") {
        for entry in read_dir.flatten() {
            if entry.path().join("theme.ron").is_file() {
                if let Ok(name) = entry.file_name().into_string() {
                    names.push(name);
                }
            }
        }
    }
    names.sort();
    if names.is_empty() {
        names.push(DEFAULT_THEME.to_string());
    }
    names
}

impl EditorState {
    /// Reloads `theme`/`theme_chrome_tex`/`font`/`code_font` from
    /// `themes/<name>/` without restarting the editor (7D-4, master plan
    /// §5.4), at whatever raster scale this state's fonts are already
    /// built at. Does NOT persist the choice — see `switch_theme` below,
    /// the real entry point for a user-initiated switch.
    fn apply_theme(&mut self, name: &str) {
        let (theme, theme_chrome_tex, font, code_font) =
            load_editor_theme_named(name, self.font_raster_scale);
        self.theme = theme;
        self.theme_chrome_tex = theme_chrome_tex;
        self.font = font;
        self.code_font = code_font;
    }

    /// `apply_theme` plus persisting the choice to `self.prefs`/
    /// `self.prefs_store` (7D-3, master plan §5.4 — was a bare
    /// `apply_theme` call before preferences existed) — the `View > Theme`
    /// menu's `ToolbarAction::SetTheme` handler (`input/panels/menu_bar.rs`)
    /// is the one caller.
    pub(super) fn switch_theme(&mut self, name: &str) {
        self.apply_theme(name);
        self.prefs.theme = name.to_string();
        self.prefs_store.save(&self.prefs);
    }

    /// Loads `store`'s persisted preferences onto this already-constructed
    /// `EditorState` (7D-3, master plan §5.4) — applying the saved theme
    /// (if it's still one of `available_themes`) WITHOUT re-saving what was
    /// just read back. Called exactly once, right after construction, by
    /// `ember2d-app/src/app.rs`'s `run_editor_app` (the only caller of
    /// `PrefsStore::user()`); every `EditorState` built directly via `new`/
    /// `load`/`new_from_result`, including every test's, stays on
    /// `PrefsStore::InMemory(EditorPrefs::default())` unless it opts in
    /// here — which is what keeps tests off the real prefs file.
    pub fn with_prefs(mut self, store: PrefsStore) -> Self {
        let prefs = store.load();
        if prefs.theme != self.theme.name && self.available_themes.iter().any(|n| n == &prefs.theme)
        {
            self.apply_theme(&prefs.theme);
        }
        self.prefs = prefs;
        self.prefs_store = store;
        self
    }

    /// Sets and persists a new UI-scale preference (7D-3, master plan §5.4)
    /// — called live by the `Theme > UI Scale` menu entries
    /// (`input/panels/menu_bar.rs`'s `ToolbarAction::SetUiScale` handler).
    /// Sanitizes (`UiScaleChoice::sanitized`) before storing, so a corrupt/
    /// hand-edited prefs file can never persist an out-of-range `Fixed`
    /// value forward.
    pub(super) fn set_ui_scale(&mut self, choice: UiScaleChoice) {
        self.prefs.ui_scale = choice.sanitized();
        self.prefs_store.save(&self.prefs);
    }

    /// The UI scale actually in effect this frame, given the display's real
    /// `DisplayScale` (7D-3, master plan §5.4). Was **pinned** to
    /// `display.render_scale` through checkpoints 2-6 of this step — a
    /// deliberate mid-step gate so no checkpoint before this one (the
    /// step's final "live UI scale" checkpoint) ever shipped a half-wired
    /// preference the menu couldn't actually reach yet, since every draw-
    /// and input-side caller already read this method, never
    /// `self.prefs.ui_scale` directly, and could be migrated one file at a
    /// time without ui_scale/render_scale ever actually diverging under
    /// them. Now reads the real preference.
    pub(super) fn effective_ui_scale(&self, display: DisplayScale) -> u32 {
        self.prefs.ui_scale.resolve(display.os_scale_factor)
    }

    /// Rebuilds `font`/`code_font` at `ui_scale` if it differs from what
    /// they're currently built at (7D-3, master plan §5.4) — called once
    /// per real draw (`impl_render.rs`'s `draw`, before any mode dispatch)
    /// so a DPI change (a monitor move, today; a live scale-preference
    /// change once un-pinned) takes effect on the very next frame without
    /// needing a restart or a manual re-open of the theme. A no-op re-read
    /// of the theme file isn't needed here — `load_theme_fonts` only touches
    /// the already-loaded `self.theme`, not disk.
    pub(super) fn rebuild_fonts_if_scale_changed(&mut self, ui_scale: u32) {
        if ui_scale == self.font_raster_scale {
            return;
        }
        let (font, code_font) = load_theme_fonts(&self.theme, ui_scale);
        self.font = font;
        self.code_font = code_font;
        self.font_raster_scale = ui_scale;
    }

    // ── Read-only accessors — see `mod.rs`'s own header comment on this
    // pattern (7C-5, master plan §5.3): `pub`, not `pub(super)`, so the
    // genuinely external `EditorHarness` (an integration-test crate) can
    // read them; nothing here is mutable from outside.

    /// The currently active chrome theme — tests assert on `.name` after a
    /// `View > Theme` switch.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Every `themes/*` entry the `View > Theme` menu lists — what a test
    /// picks a name from to exercise `ToolbarAction::SetTheme`.
    pub fn available_themes(&self) -> &[String] {
        &self.available_themes
    }

    /// The active `code_font` — `&mut`, unlike every other accessor here,
    /// since `ScriptLayout::compute`/`Font::measure` need mutable access
    /// to rasterize/cache a glyph the first time it's measured. A test
    /// building its own oracle `ScriptLayout` (the script editor's own
    /// regression tests, `tests/editor_script.rs`) is the only real caller
    /// — production code always reaches `self.code_font` directly, being
    /// `pub(super)` within this same crate.
    pub fn code_font(&mut self) -> &mut dyn Font {
        self.code_font.as_mut()
    }

    /// This state's own persisted preferences (7D-3, master plan §5.4) —
    /// what a test asserts changed after `switch_theme`/`set_ui_scale`.
    pub fn prefs(&self) -> &EditorPrefs {
        &self.prefs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one CWD-mutating test in this file — deliberately the only one,
    /// so there's no cross-test race over a shared process-wide CWD to
    /// worry about (every OTHER theme-loading test either doesn't touch
    /// CWD, or — like `ember2d-editor/tests/editor_theme.rs`'s integration
    /// tests — always sets it to the same repo-root target, which is safe
    /// to race on since every racer converges on the same value; a
    /// "missing directory" case like this one does NOT converge with
    /// those, so it gets its own isolated temp dir instead of touching the
    /// real `themes/` at all).
    #[test]
    fn list_available_themes_falls_back_to_the_default_name_when_the_directory_is_missing() {
        let dir =
            std::env::temp_dir().join(format!("ember2d-{}-no-themes-dir", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
        let original = std::env::current_dir().expect("must have a CWD to restore");
        std::env::set_current_dir(&dir).expect("must be able to cd into the empty temp dir");

        let names = list_available_themes();

        std::env::set_current_dir(&original).expect("must restore CWD before this test returns");
        assert_eq!(
            names,
            vec![DEFAULT_THEME.to_string()],
            "a directory with no themes/ subdirectory at all must still return one selectable name"
        );
    }
}
