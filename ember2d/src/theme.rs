// theme.rs — UI chrome theme resource (7D-1, docs/ember2d-master-plan.md
// §5.4): a fully-typed description of how the editor's OWN chrome (panels,
// menus, modals, inspector rows, text fields — never the tile grid/viewport/
// node graph canvas, which stay on the engine's renderer regardless of theme,
// per the 7C-9 decision gate, §7.1) should look. `Theme` itself only holds
// data and answers lookups; drawing panel/button/etc. chrome FROM a `Theme`
// is 7D-2's job (`draw_panel_chrome`) — this step doesn't touch any existing
// render call site.

use crate::renderer::{AssetManager, TextureId};
use ember2d_sim::color::Color;
use ember2d_sim::math::Rect;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A themeable UI role — what a color is FOR, not a specific value, so a
/// theme file can restyle the whole editor by remapping roles instead of
/// every draw call site naming a raw `Color::Cyan` the way `ember2d-editor`
/// still does today (that migration is 7D-2, not this step).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PaletteRole {
    PanelBg,
    PanelBorder,
    TitleBg,
    TitleText,
    TextPrimary,
    TextDim,
    Accent,
    Danger,
    InputBg,
    InputText,
    Selection,
    TabActive,
    TabInactive,
}

/// Which named 9-slice region of the chrome atlas draws a given widget kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SliceRole {
    Panel,
    TitleBar,
    Button,
    ButtonHover,
    ButtonPressed,
    ButtonDisabled,
    Input,
    TabActive,
    TabInactive,
    Scrollbar,
    Checkbox,
    ResizeGrip,
}

/// One 9-slice region of a chrome atlas: `src` is the source rect in the
/// atlas texture's own pixels; `border` (left, top, right, bottom) is how
/// many of those pixels are the fixed, unstretched corner/edge width —
/// exactly the shape `Renderer::draw_nine_slice`/`nine_slice_quads`
/// (renderer/mod.rs) already take and already have quad-math tests for
/// (`renderer/tests.rs`, from Phase 7 Part 1's own chrome-primitive work) —
/// `NineSlice` is a named, serializable pairing of those same two values,
/// not a new drawing primitive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NineSlice {
    pub src: Rect,
    pub border: (f32, f32, f32, f32),
}

/// Which font a theme draws its own chrome text with — `Bitmap` is the
/// engine's built-in `font8x8` glyph atlas (no file to load), `Ttf` names a
/// file for `TtfFont` (Phase 7 Part 2) to rasterize. Independent of
/// `ui_font_from_env` (renderer/mod.rs) — that's a process-wide, env-var
/// override for development; a `Theme`'s own font choice is what a shipped
/// theme actually wants to look like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FontChoice {
    Bitmap,
    Ttf { path: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FontSizes {
    pub small: f32,
    pub body: f32,
    pub heading: f32,
}

/// Layout constants a theme controls — spacing/border thickness/row height
/// in logical pixels, plus the minimum hit-target size (accessibility-style
/// "don't make buttons smaller than this, no matter how tight the theme's
/// own visual padding is").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub padding: f32,
    pub border: f32,
    pub row_h: f32,
    pub min_target: f32,
}

/// The RON-shaped, on-disk form of a theme — `chrome_path` here becomes
/// `Theme::chrome`'s resolved `TextureId` once `Theme::load` hands it to an
/// `AssetManager`. Kept as a separate type from `Theme` itself rather than
/// `#[serde(skip)]`-ing a `TextureId` field directly: `TextureId` has no
/// meaningful default to skip TO (a stray `0` would alias a real texture),
/// so round-tripping through a path string and resolving it explicitly is
/// the honest shape of what a `.ron` file actually contains.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeData {
    pub name: String,
    pub palette: BTreeMap<PaletteRole, Color>,
    pub chrome_path: String,
    pub slices: BTreeMap<SliceRole, NineSlice>,
    pub font: FontChoice,
    pub font_sizes: FontSizes,
    pub metrics: Metrics,
    pub ui_scale: u8,
}

/// A fully-resolved, ready-to-draw theme (7D-1, master plan §5.4). Built
/// only by `Theme::load` (from a real `themes/<name>/theme.ron`) or
/// `Theme::fallback` (when that load fails) — never constructed directly
/// with a hand-built `TextureId`, since a live one only exists once an
/// `AssetManager` has actually resolved a path.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub palette: BTreeMap<PaletteRole, Color>,
    pub chrome: TextureId,
    pub slices: BTreeMap<SliceRole, NineSlice>,
    pub font: FontChoice,
    pub font_sizes: FontSizes,
    pub metrics: Metrics,
    pub ui_scale: u8,
}

/// Loud, unmistakable magenta — the fallback for anything a theme doesn't
/// define, at the value level (`Theme::role_color`) and at the whole-theme
/// level (`Theme::fallback`). Never subtle: a missing role/theme should be
/// obvious at a glance, not a slightly-off shade someone ships by accident.
const FALLBACK_MAGENTA: Color = Color::Rgb(255, 0, 255);

impl Theme {
    /// Loads `<dir>/theme.ron`, resolving `chrome_path` (relative to `dir`)
    /// through `assets`. Never fails outward — a missing/unparsable file,
    /// or a chrome image `AssetManager::load` itself can't find, both
    /// already degrade to a loud placeholder one layer down (`ron`
    /// parse error here; `AssetManager::load`'s own 1×1 magenta texture
    /// there) rather than stopping the editor from opening at all.
    pub fn load(assets: &mut AssetManager, dir: &str) -> Theme {
        let ron_path = format!("{dir}/theme.ron");
        let data = std::fs::read_to_string(&ron_path)
            .ok()
            .and_then(|s| ron::from_str::<ThemeData>(&s).ok());

        match data {
            Some(data) => {
                let chrome_path = format!("{dir}/{}", data.chrome_path);
                Theme {
                    name: data.name,
                    palette: data.palette,
                    chrome: assets.load(&chrome_path),
                    slices: data.slices,
                    font: data.font,
                    font_sizes: data.font_sizes,
                    metrics: data.metrics,
                    ui_scale: data.ui_scale,
                }
            }
            None => {
                eprintln!("[theme] failed to load '{}', falling back to a placeholder", ron_path);
                Theme::fallback(assets)
            }
        }
    }

    /// A complete, always-available theme with no file dependency —
    /// magenta chrome, magenta-ish palette, the built-in bitmap font — used
    /// when `load` can't read a real one. Distinct from a per-ROLE lookup
    /// miss (`role_color`/`slice` below): this is "the whole theme is
    /// missing," not "one role in an otherwise-fine theme is."
    pub fn fallback(assets: &mut AssetManager) -> Theme {
        Theme {
            name: "fallback".to_string(),
            palette: BTreeMap::new(),
            chrome: assets.load("__no_theme_chrome__.png"),
            slices: BTreeMap::new(),
            font: FontChoice::Bitmap,
            font_sizes: FontSizes { small: 8.0, body: 8.0, heading: 16.0 },
            metrics: Metrics { padding: 4.0, border: 1.0, row_h: 16.0, min_target: 16.0 },
            ui_scale: 1,
        }
    }

    /// `role`'s color, or loud magenta if this theme doesn't define it —
    /// never a panic, never a silently-wrong default that looks like a
    /// real design choice.
    pub fn role_color(&self, role: PaletteRole) -> Color {
        self.palette.get(&role).copied().unwrap_or(FALLBACK_MAGENTA)
    }

    /// `role`'s 9-slice region, or `None` if this theme doesn't define it —
    /// `None`, not a fallback `NineSlice`, because there's no geometry that
    /// would make sense to stretch across an arbitrary chrome atlas; the
    /// caller (7D-2's `draw_panel_chrome`) is what actually knows how to
    /// draw a loud placeholder in its place (e.g. a flat `role_color`-tinted
    /// rect instead of attempting a 9-slice draw with fabricated coordinates).
    pub fn slice(&self, role: SliceRole) -> Option<&NineSlice> {
        self.slices.get(&role)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data() -> ThemeData {
        let mut palette = BTreeMap::new();
        palette.insert(PaletteRole::PanelBg, Color::Rgb(20, 20, 24));
        palette.insert(PaletteRole::Accent, Color::Cyan);

        let mut slices = BTreeMap::new();
        slices.insert(
            SliceRole::Panel,
            NineSlice { src: Rect::new(0.0, 0.0, 24.0, 24.0), border: (4.0, 4.0, 4.0, 4.0) },
        );

        ThemeData {
            name: "sample".to_string(),
            palette,
            chrome_path: "chrome.png".to_string(),
            slices,
            font: FontChoice::Ttf { path: "font.ttf".to_string() },
            font_sizes: FontSizes { small: 10.0, body: 12.0, heading: 18.0 },
            metrics: Metrics { padding: 6.0, border: 2.0, row_h: 20.0, min_target: 24.0 },
            ui_scale: 2,
        }
    }

    #[test]
    fn theme_data_round_trips_through_ron() {
        let data = sample_data();
        let serialized = ron::to_string(&data).expect("ThemeData must serialize to RON");
        let restored: ThemeData = ron::from_str(&serialized).expect("must parse back");

        assert_eq!(restored.name, data.name);
        assert_eq!(restored.palette, data.palette);
        assert_eq!(restored.chrome_path, data.chrome_path);
        assert_eq!(restored.slices, data.slices);
        assert_eq!(restored.font, data.font);
        assert_eq!(restored.font_sizes, data.font_sizes);
        assert_eq!(restored.metrics, data.metrics);
        assert_eq!(restored.ui_scale, data.ui_scale);
    }

    #[test]
    fn loading_a_real_theme_file_resolves_its_chrome_texture() {
        let dir = std::env::temp_dir().join(format!("ember2d-theme-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("test temp dir must be creatable");
        std::fs::write(dir.join("theme.ron"), ron::to_string(&sample_data()).unwrap())
            .expect("write theme.ron");
        // The chrome file itself doesn't need to be a real PNG for this
        // test — `AssetManager::load` already falls back to a placeholder
        // for an unreadable path, which is exactly what a bare touch here
        // produces; this test is only about theme.ron's own path handling.
        std::fs::write(dir.join("chrome.png"), []).expect("write placeholder chrome");

        let mut assets = AssetManager::new();
        let theme = Theme::load(&mut assets, &dir.to_string_lossy());

        assert_eq!(theme.name, "sample");
        assert!(assets.get(theme.chrome).is_some(), "chrome_path must resolve to a real texture handle");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_shipped_ember_clean_theme_loads_with_every_slice_and_a_real_chrome_texture() {
        // `themes/ember-clean/` is checked-in, generated content
        // (`ember2d/examples/gen_ember_clean_theme.rs`) — this is the one
        // test that actually loads it, catching a generator/theme.ron
        // mismatch or a corrupt PNG that every other test here (which all
        // use synthetic fixtures) structurally can't.
        let _ = std::env::set_current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let mut assets = AssetManager::new();
        let theme = Theme::load(&mut assets, "themes/ember-clean");

        assert_eq!(theme.name, "ember-clean");
        assert_ne!(theme.name, "fallback", "the real theme file must have been found and parsed");
        let tex = assets.get(theme.chrome).expect("chrome texture must resolve");
        assert_ne!((tex.width, tex.height), (1, 1), "the real chrome.png must decode, not fall back to AssetManager's 1x1 placeholder");

        for role in [
            SliceRole::Panel,
            SliceRole::TitleBar,
            SliceRole::Button,
            SliceRole::ButtonHover,
            SliceRole::ButtonPressed,
            SliceRole::ButtonDisabled,
            SliceRole::Input,
            SliceRole::TabActive,
            SliceRole::TabInactive,
            SliceRole::Scrollbar,
            SliceRole::Checkbox,
            SliceRole::ResizeGrip,
        ] {
            assert!(theme.slice(role).is_some(), "ember-clean must define every SliceRole, missing {role:?}");
        }
        assert!(matches!(theme.font, FontChoice::Ttf { .. }));
    }

    #[test]
    fn loading_a_missing_theme_directory_falls_back_without_panicking() {
        let mut assets = AssetManager::new();
        let theme = Theme::load(&mut assets, "__no_such_theme_dir__");
        assert_eq!(theme.name, "fallback");
    }

    #[test]
    fn a_role_missing_from_the_palette_falls_back_to_magenta() {
        let data = sample_data();
        let theme = Theme {
            name: data.name,
            palette: data.palette,
            chrome: TextureId(0),
            slices: data.slices,
            font: data.font,
            font_sizes: data.font_sizes,
            metrics: data.metrics,
            ui_scale: data.ui_scale,
        };
        // PanelBg and Accent were defined; TitleBg was not.
        assert_eq!(theme.role_color(PaletteRole::PanelBg), Color::Rgb(20, 20, 24));
        assert_eq!(theme.role_color(PaletteRole::TitleBg), FALLBACK_MAGENTA);
    }

    #[test]
    fn a_slice_missing_from_the_theme_returns_none_not_a_guess() {
        let data = sample_data();
        let theme = Theme {
            name: data.name,
            palette: data.palette,
            chrome: TextureId(0),
            slices: data.slices,
            font: data.font,
            font_sizes: data.font_sizes,
            metrics: data.metrics,
            ui_scale: data.ui_scale,
        };
        assert!(theme.slice(SliceRole::Panel).is_some());
        assert!(theme.slice(SliceRole::Scrollbar).is_none());
    }
}
