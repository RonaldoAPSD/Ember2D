# Ember2D — Editor Theming Reference

**Written against:** `claude` branch, v0.5.7-b, `ember2d/src/theme.rs` (7D-1), `ember2d-editor/src/editor/theme_loader.rs` (7D-2/7D-4), `ember2d/examples/gen_ember_clean_theme.rs`, `ember2d/src/renderer/{ui_space,ui_painter}.rs` (7D-3, all 7 checkpoints landed).
**Status:** the loading/switching mechanism, color/9-slice theming, and panel LAYOUT (every dock panel, bar, menu, modal, and the script editor, all in UI points sized from the theme's own `Metrics` — 7D-3, docs/ember2d-master-plan.md §5.4) are all built and live, including a real, user-facing `Theme > UI Scale` picker. `UiRect::from_cells` — the old fixed 8×16 character-cell grid every panel used to lay out on regardless of theme — is gone from every chrome surface; only the level canvas/viewport, play mode, the node graph, and the pre-project start screen still draw on a fixed cell grid, by design (§7). UI scale itself is a `Renderer`/editor-preference concept (`UiSpace`, `ember2d-editor/src/editor/prefs.rs`), not a theme property — see §6.

---

## 1. What a theme controls

A theme is the editor's OWN chrome: panels, menus, modals, the inspector, text fields. It never touches:

- **The tile grid / viewport / node graph canvas** — these stay on the engine's own renderer regardless of theme (the 7C-9 decision gate, master plan §7.1). A theme can make the panel frame around the viewport look however it wants; the ASCII/sprite content inside it is unrelated.
- **Semantic content colors** — console log levels (error/warning/info), hierarchy entity-kind colors (player/spawn), file-browser icon-kind colors (dir/level/script), Rhai syntax highlighting, node-graph port-kind colors (exec/data-in/data-out), and the raw HSV/RGB swatches in the color picker. These signal MEANING, not decorative chrome, and stay hardcoded at their own call sites regardless of theme — each one has an inline comment saying so.

Everything else — panel/title-bar/status-bar fills, dock tabs, buttons, text-input/confirm/context-menu chrome, the palette editor and color-picker's own frames, the script editor's chrome, the node graph's flat fills — draws through the active `Theme`.

## 2. File format

A theme lives at `themes/<name>/`, with two files:

- `theme.ron` — a serialized `ThemeData` (`ember2d/src/theme.rs`):
  ```rust
  pub struct ThemeData {
      pub name: String,
      pub palette: BTreeMap<PaletteRole, Color>,
      pub chrome_path: String,       // relative to the theme's own directory
      pub slices: BTreeMap<SliceRole, NineSlice>,
      pub font: FontChoice,
      pub code_font: Option<FontChoice>,  // 7D-3 — None falls back to `font`
      pub font_sizes: FontSizes,
      pub metrics: Metrics,
  }
  ```
  `ui_scale` (a `u8`) used to live here — removed at 7D-3 (docs/ember2d-master-plan.md §5.4): it's a user/display preference, not a property of a theme, and now lives in `EditorPrefs` (`ember2d-editor/src/editor/prefs.rs`) instead. `#[serde(default)]` on `code_font` and serde's own default "ignore unknown fields" behavior mean a `theme.ron` written before this step — no `code_font` key, a stray `ui_scale: N,` line — still loads unchanged.
- The chrome atlas PNG named by `chrome_path` (by convention, `chrome.png`, sitting next to `theme.ron`).

`Theme::load(&mut AssetManager, dir)` reads `<dir>/theme.ron`, resolves `chrome_path` through the given `AssetManager`, and never fails outward — a missing or unparsable `theme.ron`, or a chrome image the `AssetManager` itself can't find, both degrade to `Theme::fallback()` (loud magenta chrome, an empty palette, the built-in bitmap font) rather than stopping the editor from opening. A role missing from an otherwise-valid theme's palette is a separate, per-lookup fallback: `Theme::role_color` returns loud magenta for that one role; `Theme::slice` returns `None` (the caller draws a flat fallback fill, never fabricated 9-slice geometry).

## 3. Palette roles

`PaletteRole` (`ember2d/src/theme.rs`) is what a color is FOR, not a specific value — a theme restyles the whole editor by remapping roles, not by every draw call site naming a raw RGB literal.

| Role | Used for |
|---|---|
| `PanelBg` | Panel/dock/menu background fills |
| `PanelBorder` | Flat borders where no 9-slice art is used (e.g. the node graph's frame line) |
| `TitleBg` / `TitleText` | Title-bar strips (the main title bar, panel titles, modal titles) |
| `TextPrimary` | Ordinary body text |
| `TextDim` | Secondary/hint text, separators, disabled items |
| `Accent` | Highlighted/active state — the active dock tab, a hovered menu item, a focused field, the mode indicator |
| `Danger` | Destructive actions and error states (a Delete button, the script editor's error line) |
| `InputBg` / `InputText` | Editable-field surfaces — text-input modals, the inspector's value rows, the script editor's buffer |
| `Selection` | Selected text/row highlight |
| `TabActive` / `TabInactive` | Reserved for a future themed tab-strip role beyond the flat `Accent`/`TextDim` pairing `draw_dock_tabs` uses today |

**Known gap:** no role exists for "text drawn ON an `Accent` background" (an active dock tab's label, a selected menu row, a selected node's title). Every such site uses a literal `Color::Black` instead, each documented inline with a cross-reference to this same gap — `Theme` has no way to express it yet.

## 4. Slice roles and authoring a chrome atlas

`SliceRole` names which 9-slice region of the chrome atlas draws a given widget kind: `Panel`, `TitleBar`, `Button`, `ButtonHover`, `ButtonPressed`, `ButtonDisabled`, `Input`, `TabActive`, `TabInactive`, `Scrollbar`, `Checkbox`, `ResizeGrip`.

Each `NineSlice { src: Rect, border: (f32, f32, f32, f32) }` names a source rectangle in the atlas texture and how many of its edge pixels are the fixed, unstretched border (left, top, right, bottom) — the rest of the rect stretches to fill whatever destination size a draw call asks for. This is exactly the shape `Renderer::draw_nine_slice`/`nine_slice_quads` already take.

Single-row-tall bars (the title bar, status bar, dock tab strip, menu bar/dropdown) deliberately stay FLAT theme-colored fills rather than 9-slice, even though `TitleBar` has a slice defined — a themed border thick enough to read as a border would consume most of a 16px-tall row. 9-slice is reserved for genuinely box-shaped chrome: the panel frame itself, text-input/confirm modals, the context menu, the palette editor, and the color picker.

**How `ember-clean` was built** (`ember2d/examples/gen_ember_clean_theme.rs`): a theme's chrome atlas doesn't need to be hand-drawn pixel art. `ember-clean`'s 12-region atlas is generated from a small Rust program — a `COLS`-wide grid of `CELL`×`CELL` cells (128×96px, 4 columns × 3 rows, 32px cells), each with a `BORDER`-px (6px) solid-color border and a solid-color fill, computed from the same constants that write `theme.ron`'s `Rect`/`border` values — so the PNG and the RON file can't drift apart the way hand-transcribing pixel coordinates into a text file risks. Run `cargo run --example gen_ember_clean_theme` to regenerate it after editing the palette or geometry in that file; the output is committed to git like any other generated content in this repo (`gen_roguelike.rs`'s demo levels are the same pattern).

A hand-painted pixel-art atlas is equally valid — `Theme::load` only cares that `chrome_path` resolves to a real image and that `theme.ron`'s `slices` name real sub-rects within it. The generator approach is what this repo happens to use for its own flat, modern-dev-console-styled theme; a more ornate theme (rounded corners, gradients, per-role border asymmetry) is just a different `NineSlice` per role in a hand-authored PNG.

## 5. Font

`FontChoice` is either `Bitmap` (the engine's built-in `font8x8` glyph atlas, no file to load) or `Ttf { path }` (a TTF file `TtfFont` rasterizes). `path` is NOT joined with the theme's own directory the way `chrome_path` is — a theme's font isn't "inside" the theme directory the way its own atlas is, so the path is repo-root-relative like any other on-disk path this codebase stores (level `script`/`next_level` fields, demo audio paths). `ember-clean` uses `ember2d/assets/fonts/CascadiaMono.ttf`.

`code_font: Option<FontChoice>` (7D-3, docs/ember2d-master-plan.md §5.4) is the script editor's own monospace font — `None` (every theme before this step, `ember-clean` included) falls back to `font` itself. Kept separate because a theme's body text can be proportional while the script editor's own column math needs a genuinely monospace face; `ember-clean` leaves it `None` since its body font, Cascadia MONO, already is one.

`font_sizes: FontSizes { small, body, heading }` names three logical pixel sizes. Only `font_sizes.body` is read anywhere today (`draw_panel_chrome`'s title text, the one call site drawing through `DrawSurface::draw_text_px` instead of the fixed-cell `draw_str`/`draw_char`). Everything else — every panel's actual content — renders at a fixed size regardless of what a theme's `font_sizes` says, since it's still cell-grid text (see §7).

## 6. Metrics and UI scale

`Metrics { padding, border, row_h, min_target }` is fully defined in `ThemeData`, round-trips through RON, and is exactly what `ChromeMetrics::from_theme` (`ember2d-editor/src/editor/ui/metrics.rs`) builds every panel/bar/modal/script-editor layout from (7D-3, docs/ember2d-master-plan.md §5.4, landed across that step's 7 checkpoints) — `row_h` in particular sets the height of every title bar, menu row, dock tab strip, and list row in the editor; `ember-clean`'s `row_h: 20` (up from the old fixed 16px cell) is a real, deliberate visual change from every screenshot taken before this step.

**UI scale is not a theme property.** `ui_scale` used to be a `ThemeData` field (removed at 7D-3's first checkpoint, §2 above) — a display/user preference doesn't belong to a shipped theme file, since two people using the same theme on different monitors want different scales. It's `UiScaleChoice` (`Auto`, a fixed whole step `1..=4`, or the dedicated `OnePointFive` — 7D-4 follow-up, docs/ember2d-master-plan.md §5.4) in `EditorPrefs` (`ember2d-editor/src/editor/prefs.rs`), persisted per-user at `%APPDATA%\Ember2D\editor_prefs.ron` on Windows (`$XDG_CONFIG_HOME/ember2d/` else `$HOME/.config/ember2d/` elsewhere) alongside the active theme name, resolved against the display's real DPI reading for `Auto` (`round(os_scale_factor * 2)`, clamped `1..=8` — 100% OS scale resolves to `2`, matching the size the editor always drew at before this preference existed). `UiSpace::ui_scale`/`UiScaleChoice::resolve` are `f32`, not `u32` — `1.5` needed a real (if still fully discrete — the menu offers no free-entry value) fractional scale to flow through.

**Changing it live.** `Theme > UI Scale` (after a separator, below the theme list) offers `Auto`/`1x`/`1.5x`/`2x`/`3x`/`4x` with a checkmark on whichever is active. Picking one persists immediately and takes effect on the very next frame — no restart, and the choice survives one (`editor_prefs.ron` is read back on the next launch). 1 point = the chosen scale's own count of PHYSICAL pixels, independent of the display's own DPI-derived render scale (`R`, floored at `MIN_UI_SCALE = 2` — see R48/R21 in §3.2) — the two only coincide by construction at `Auto`'s own default resolution; picking a fixed value that diverges from `R` (say `1x` on a 200%-scaled display, where `R` is already `4`) is exactly the case this step's own `UiSpace` (`ember2d::renderer::ui_space.rs`) exists to make correct, not just the common `S == R` one.

A UI scale much larger than the window can comfortably fit will overlap chrome TEXT (two independently-positioned labels on the same bar can collide once each is drawn at a much bigger physical size) — an expected consequence of choosing an extreme scale on a small window, the same way real OS-level display scaling can truncate text in an app that wasn't designed for it, not a hit-testing defect: every panel, button, and menu item stays exactly where it's drawn and exactly as clickable, however cramped the text inside it looks.

## 7. What still ignores UI scale

The level canvas/viewport, play mode, and `ember2d-sim` stay on the engine's own fixed logical `CELL_W`×`CELL_H` (8×16) glyph grid forever — a deliberate boundary (docs/ember2d-master-plan.md §7.1's decision gate), not a gap: the canvas is the GAME's own presentation surface, unrelated to how big the editor's own chrome draws around it. Two further surfaces are logged, not fixed, exclusions from this step's own scope rather than an oversight: the node graph editor (`graph_ui.rs`) and the pre-project start screen (`start_screen/`) both still draw on that same fixed cell grid and don't yet follow `ui_scale` (R79/R80, §3.2) — every other editor surface (every dock panel, every modal, the menu bar, the script editor both docked and fullscreen) draws in UI points and scales with the preference above.

## 8. Switching themes at runtime

The `Theme` menu (top-level, after `Layers` — not a submenu under `View`, since the menu system has no submenu concept) lists every subdirectory of `themes/` with a real `theme.ron` in it, scanned once at editor startup (`theme_loader::list_available_themes`) — a theme dropped into `themes/` while the editor is already running needs a restart to appear. Picking an entry calls `EditorState::switch_theme`, which reloads `theme`/`theme_chrome_tex`/`font` from that theme's directory in place, no restart needed. The active theme gets a checkmark in its own menu entry.

A missing/unreadable `themes/` directory (or one with no valid theme subdirectory) still leaves one selectable entry — `ember-clean` — rather than leaving the menu empty, the same "always something to fall back to" contract `Theme::load` itself keeps at the single-theme level.

## 9. Shipped themes

- **`ember-clean`** — the one theme shipping today. Flat, modern-dev-console styling: dark neutral panels (`#1b1e24` background), an amber accent (`#e8a33d`), Cascadia Code for all chrome text, the engine's own bitmap-font viewport left untouched (the "hybrid" style, picked over an all-bitmap alternative after comparing both live in an interactive mockup).
- **`ember-pixel`** — named in the original plan (all-bitmap, no TTF) but not built. Deferred, not abandoned: a genuine pixel-art style is a design decision a human should make, not something a coding agent should guess at and ship as placeholder art passed off as a real second theme.
