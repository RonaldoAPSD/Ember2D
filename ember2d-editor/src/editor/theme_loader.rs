// editor/theme_loader.rs — loads the editor's own chrome theme (7D-2,
// docs/ember2d-master-plan.md §5.4) and, as of 7D-4, lists and reloads it
// at runtime for View > Theme switching. Split out of `editor/mod.rs`
// purely to keep that file under CLAUDE.md's 750-line hard limit — no
// behavioral change from when this lived there directly.

use super::EditorState;
use ember2d::renderer::{AssetManager, BitmapFont, Font, Texture, TtfFont};
use ember2d::theme::{FontChoice, Theme};

/// The theme every fresh `EditorState` starts on — `themes/ember-clean`,
/// the one real theme shipped so far (7D-1's own "Landed as" note:
/// `themes/ember-pixel` is deferred, not built). Named here, not just
/// inlined at the one call site, so `list_available_themes`'s "nothing on
/// disk" fallback below names the exact same theme.
pub(super) const DEFAULT_THEME: &str = "ember-clean";

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
pub(super) fn load_editor_theme_named(name: &str) -> (Theme, Texture, Box<dyn Font>) {
    let mut assets = AssetManager::new();
    let theme = Theme::load(&mut assets, &format!("themes/{name}"));
    let chrome_tex = assets.get(theme.chrome).cloned().unwrap_or_else(|| Texture::solid(0xFFFF00FF));
    let font: Box<dyn Font> = match &theme.font {
        FontChoice::Bitmap => Box::new(BitmapFont::new()),
        FontChoice::Ttf { path } => std::fs::read(path)
            .ok()
            .and_then(|bytes| TtfFont::from_bytes(&bytes, 0).ok())
            .map(|f| Box::new(f) as Box<dyn Font>)
            .unwrap_or_else(|| Box::new(BitmapFont::new())),
    };
    (theme, chrome_tex, font)
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
    /// Reloads `theme`/`theme_chrome_tex`/`font` from `themes/<name>/`
    /// without restarting the editor (7D-4, master plan §5.4) — the
    /// `View > Theme` menu's `ToolbarAction::SetTheme` handler
    /// (`input/panels/menu_bar.rs`) is the one caller.
    pub(super) fn switch_theme(&mut self, name: &str) {
        let (theme, theme_chrome_tex, font) = load_editor_theme_named(name);
        self.theme = theme;
        self.theme_chrome_tex = theme_chrome_tex;
        self.font = font;
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
        let dir = std::env::temp_dir().join(format!("ember2d-{}-no-themes-dir", std::process::id()));
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
