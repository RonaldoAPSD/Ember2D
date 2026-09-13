// editor/theme_loader.rs — loads the editor's own chrome theme (7D-2,
// docs/ember2d-master-plan.md §5.4). Split out of `editor/mod.rs` purely to
// keep that file under CLAUDE.md's 750-line hard limit — no behavioral
// change from when this lived there directly.

use ember2d::renderer::{AssetManager, BitmapFont, Font, Texture, TtfFont};
use ember2d::theme::{FontChoice, Theme};

/// The editor's chrome theme is loaded and resolved ONCE, here, rather than
/// lazily on the first real render: `AssetManager` needs no live GPU/window
/// (`AssetManager::new()` is two empty `HashMap`s) — only the eventual GPU
/// *upload* of a texture does, which happens on demand inside
/// `Renderer::draw_texture_px` itself, not here — so there's no reason to
/// defer this until a real `Renderer` exists. The resolved `Texture` is
/// cloned out and the loading `AssetManager` is dropped immediately:
/// keeping it alive just to re-resolve the same `TextureId` every frame
/// would be pure overhead for a single texture that never changes for this
/// editor instance's lifetime. This also means `EditorHarness` (headless
/// tests) gets the exact same real theme `EditorState::new` does — no
/// separate test-only code path, and no `Renderer.ui_assets` (7D-1)
/// dependency at all for this narrow first slice of the chrome rewrite;
/// that field stays reserved for 7D-4's runtime theme-switching, which
/// DOES need a persistent, evictable `AssetManager` the way this one-shot
/// load doesn't.
pub(super) fn load_editor_theme() -> (Theme, Texture, Box<dyn Font>) {
    let mut assets = AssetManager::new();
    let theme = Theme::load(&mut assets, "themes/ember-clean");
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
