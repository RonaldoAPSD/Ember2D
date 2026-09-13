// examples/gen_ember_clean_theme.rs — generates `themes/ember-clean/`'s
// chrome atlas PNG and `theme.ron` (7D-1, docs/ember2d-master-plan.md §5.4).
//
// ── WHY THIS EXISTS ──────────────────────────────────────────────────────────
//
// `ember-clean` pairs the engine's own bitmap-font viewport (unchanged by any
// theme — see the 7C-9 decision gate, §7.1) with Cascadia Code chrome:
// panels, menus, the inspector, text fields read like a modern dev console
// wrapped around a retro game, rather than the whole editor sharing the
// viewport's own coarse pixel font. Same reasoning as `gen_roguelike.rs` for
// generating rather than hand-authoring: a 12-region 9-slice atlas is exact
// pixel arithmetic, not something worth eyeballing in an image editor and
// then hand-transcribing into `theme.ron`'s `Rect` coordinates — this file
// computes both from the same constants, so they can't drift apart, and is
// the reviewable source of truth for the shipped chrome, same as this
// crate's demo levels are for their own dungeon layouts.
//
// ── RUN ──────────────────────────────────────────────────────────────────────
//   cargo run --example gen_ember_clean_theme
//
// Regenerates `themes/ember-clean/chrome.png` and `themes/ember-clean/theme.ron`
// from scratch. Re-run after editing the palette/geometry below; the output
// is committed to git same as hand-authored content would be.

use ember2d::theme::{FontChoice, FontSizes, Metrics, NineSlice, PaletteRole, SliceRole, ThemeData};
use ember2d_sim::color::Color;
use ember2d_sim::math::Rect;
use image::{Rgba, RgbaImage};
use std::collections::BTreeMap;

/// Each 9-slice region occupies one `CELL`x`CELL` cell of the atlas, with a
/// `BORDER`-px fixed corner/edge — the same value on all four sides is
/// plenty for this theme's flat, rectangular chrome (no rounded corners or
/// asymmetric bevels); a richer theme could vary this per-role.
const CELL: u32 = 32;
const BORDER: f32 = 6.0;
const COLS: u32 = 4;

/// One named region: its grid `(col, row)` in the atlas, its fill, and its
/// border-line color. Ordering matches the plan's own `SliceRole` listing.
struct SliceSpec {
    role: SliceRole,
    col: u32,
    row: u32,
    fill: [u8; 4],
    border_color: [u8; 4],
}

const NEUTRAL_BORDER: [u8; 4] = [0x34, 0x3a, 0x45, 0xff];
const AMBER: [u8; 4] = [0xe8, 0xa3, 0x3d, 0xff];
const AMBER_DIM: [u8; 4] = [0x8a, 0x67, 0x2c, 0xff];

fn slices() -> Vec<SliceSpec> {
    vec![
        SliceSpec { role: SliceRole::Panel, col: 0, row: 0, fill: [0x1b, 0x1e, 0x24, 0xff], border_color: NEUTRAL_BORDER },
        SliceSpec { role: SliceRole::TitleBar, col: 1, row: 0, fill: [0x26, 0x2b, 0x33, 0xff], border_color: NEUTRAL_BORDER },
        SliceSpec { role: SliceRole::Button, col: 2, row: 0, fill: [0x20, 0x24, 0x2b, 0xff], border_color: [0x3a, 0x40, 0x49, 0xff] },
        SliceSpec { role: SliceRole::ButtonHover, col: 3, row: 0, fill: [0x30, 0x28, 0x18, 0xff], border_color: AMBER_DIM },
        SliceSpec { role: SliceRole::ButtonPressed, col: 0, row: 1, fill: [0x14, 0x15, 0x19, 0xff], border_color: AMBER },
        SliceSpec { role: SliceRole::ButtonDisabled, col: 1, row: 1, fill: [0x20, 0x22, 0x25, 0xff], border_color: [0x2c, 0x2e, 0x33, 0xff] },
        SliceSpec { role: SliceRole::Input, col: 2, row: 1, fill: [0x14, 0x17, 0x1c, 0xff], border_color: [0x3a, 0x40, 0x49, 0xff] },
        SliceSpec { role: SliceRole::TabActive, col: 3, row: 1, fill: [0x20, 0x24, 0x2b, 0xff], border_color: AMBER },
        SliceSpec { role: SliceRole::TabInactive, col: 0, row: 2, fill: [0x18, 0x1a, 0x1f, 0xff], border_color: NEUTRAL_BORDER },
        SliceSpec { role: SliceRole::Scrollbar, col: 1, row: 2, fill: [0x2a, 0x2f, 0x38, 0xff], border_color: NEUTRAL_BORDER },
        SliceSpec { role: SliceRole::Checkbox, col: 2, row: 2, fill: [0x14, 0x17, 0x1c, 0xff], border_color: [0x3a, 0x40, 0x49, 0xff] },
        SliceSpec { role: SliceRole::ResizeGrip, col: 3, row: 2, fill: [0x1b, 0x1e, 0x24, 0xff], border_color: AMBER_DIM },
    ]
}

fn main() {
    let specs = slices();
    let rows = 1 + specs.iter().map(|s| s.row).max().unwrap_or(0);
    let (atlas_w, atlas_h) = (COLS * CELL, rows * CELL);

    let mut img = RgbaImage::from_pixel(atlas_w, atlas_h, Rgba([0, 0, 0, 0]));
    let mut slice_map: BTreeMap<SliceRole, NineSlice> = BTreeMap::new();

    for spec in &specs {
        let (x0, y0) = (spec.col * CELL, spec.row * CELL);
        for py in 0..CELL {
            for px in 0..CELL {
                let on_border = px < BORDER as u32
                    || py < BORDER as u32
                    || px >= CELL - BORDER as u32
                    || py >= CELL - BORDER as u32;
                let color = if on_border { spec.border_color } else { spec.fill };
                img.put_pixel(x0 + px, y0 + py, Rgba(color));
            }
        }
        slice_map.insert(
            spec.role,
            NineSlice {
                src: Rect::new(x0 as f32, y0 as f32, CELL as f32, CELL as f32),
                border: (BORDER, BORDER, BORDER, BORDER),
            },
        );
    }

    std::fs::create_dir_all("themes/ember-clean").expect("create themes/ember-clean");
    img.save("themes/ember-clean/chrome.png").expect("write chrome.png");

    let mut palette = BTreeMap::new();
    palette.insert(PaletteRole::PanelBg, Color::Rgb(0x1b, 0x1e, 0x24));
    palette.insert(PaletteRole::PanelBorder, Color::Rgb(0x34, 0x3a, 0x45));
    palette.insert(PaletteRole::TitleBg, Color::Rgb(0x26, 0x2b, 0x33));
    palette.insert(PaletteRole::TitleText, Color::Rgb(0xe6, 0xe4, 0xde));
    palette.insert(PaletteRole::TextPrimary, Color::Rgb(0xd9, 0xda, 0xe0));
    palette.insert(PaletteRole::TextDim, Color::Rgb(0x7d, 0x83, 0x8f));
    palette.insert(PaletteRole::Accent, Color::Rgb(0xe8, 0xa3, 0x3d));
    palette.insert(PaletteRole::Danger, Color::Rgb(0xe0, 0x61, 0x6b));
    palette.insert(PaletteRole::InputBg, Color::Rgb(0x14, 0x17, 0x1c));
    palette.insert(PaletteRole::InputText, Color::Rgb(0xd9, 0xda, 0xe0));
    palette.insert(PaletteRole::Selection, Color::Rgb(0x8a, 0x67, 0x2c));
    palette.insert(PaletteRole::TabActive, Color::Rgb(0xe6, 0xe4, 0xde));
    palette.insert(PaletteRole::TabInactive, Color::Rgb(0x7d, 0x83, 0x8f));

    let data = ThemeData {
        name: "ember-clean".to_string(),
        palette,
        chrome_path: "chrome.png".to_string(),
        slices: slice_map,
        // Repo-root-relative, matching every other on-disk path this
        // codebase stores (level `script`/`next_level` fields, demo audio
        // paths) — `Theme::load` doesn't path-join `font` the way it does
        // `chrome_path` (a theme's font isn't "inside" the theme
        // directory the way its own atlas is; the bundled Cascadia file
        // lives under `ember2d/assets/`, not under `themes/` at all).
        font: FontChoice::Ttf { path: "ember2d/assets/fonts/CascadiaMono.ttf".to_string() },
        font_sizes: FontSizes { small: 11.0, body: 13.0, heading: 16.0 },
        metrics: Metrics { padding: 6.0, border: BORDER, row_h: 20.0, min_target: 22.0 },
        ui_scale: 1,
    };

    let ron_str = ron::ser::to_string_pretty(&data, ron::ser::PrettyConfig::default())
        .expect("serialize theme.ron");
    std::fs::write("themes/ember-clean/theme.ron", ron_str).expect("write theme.ron");

    println!("Generated themes/ember-clean/chrome.png ({atlas_w}x{atlas_h}) and theme.ron");
}
