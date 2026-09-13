# Ember2D — Editor Theming Reference

**Written against:** `claude` branch, v0.5.7-b, `ember2d/src/theme.rs` (7D-1), `ember2d-editor/src/editor/theme_loader.rs` (7D-2/7D-4), `ember2d/examples/gen_ember_clean_theme.rs`.
**Status:** the loading/switching mechanism and color/9-slice theming are built. The panel LAYOUT itself is still the pre-theme 8×16 character-cell grid (`UiRect::from_cells`, docs/ember2d-master-plan.md §5.4, 7D-2) — a theme changes what colors and 9-slice art draw, not where things sit on screen or how big text renders. `Theme.ui_scale`/`font_sizes`/`metrics` are read from a theme file today but not yet applied anywhere (7D-3, blocked on that same layout work — see the master plan's own note on why).

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
      pub font_sizes: FontSizes,
      pub metrics: Metrics,
      pub ui_scale: u8,
  }
  ```
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

`font_sizes: FontSizes { small, body, heading }` names three logical pixel sizes. Only `font_sizes.body` is read anywhere today (`draw_panel_chrome`'s title text, the one call site drawing through `DrawSurface::draw_text_px` instead of the fixed-cell `draw_str`/`draw_char`). Everything else — every panel's actual content — renders at a fixed size regardless of what a theme's `font_sizes` says, since it's still cell-grid text (see §7).

## 6. Metrics and UI scale

`Metrics { padding, border, row_h, min_target }` and `ui_scale: u8` are both fully defined in `ThemeData` and round-trip through RON, but neither is consumed by any draw call yet — inert fields, loaded and available for a future step (7D-3, and the `from_cells` layout rewrite it depends on) to read. Don't infer that setting a large `ui_scale` in a hand-authored `theme.ron` today does anything visible; it doesn't yet.

## 7. Cell-grid layout, still

Almost the entire editor's panel content — the inspector, hierarchy, console, file browser, palette, dock tabs, every modal, the script editor, the node graph — draws through `UiRect::from_cells` and the fixed-8×16-pixel-cell `draw_str`/`draw_char` primitives, colored by the active theme but laid out exactly the same regardless of it. A `Theme` today answers "what color, what 9-slice art" — never "how big" or "where." Replacing that cell grid with layout sized to real content and `Metrics` is its own future step (docs/ember2d-master-plan.md §5.4, 7D-2's own "STILL NOT done" note) — large enough (35 `from_cells` call sites across 11 files) that it's deliberately not bundled into any theming work done so far.

## 8. Switching themes at runtime

The `Theme` menu (top-level, after `Layers` — not a submenu under `View`, since the menu system has no submenu concept) lists every subdirectory of `themes/` with a real `theme.ron` in it, scanned once at editor startup (`theme_loader::list_available_themes`) — a theme dropped into `themes/` while the editor is already running needs a restart to appear. Picking an entry calls `EditorState::switch_theme`, which reloads `theme`/`theme_chrome_tex`/`font` from that theme's directory in place, no restart needed. The active theme gets a checkmark in its own menu entry.

A missing/unreadable `themes/` directory (or one with no valid theme subdirectory) still leaves one selectable entry — `ember-clean` — rather than leaving the menu empty, the same "always something to fall back to" contract `Theme::load` itself keeps at the single-theme level.

## 9. Shipped themes

- **`ember-clean`** — the one theme shipping today. Flat, modern-dev-console styling: dark neutral panels (`#1b1e24` background), an amber accent (`#e8a33d`), Cascadia Code for all chrome text, the engine's own bitmap-font viewport left untouched (the "hybrid" style, picked over an all-bitmap alternative after comparing both live in an interactive mockup).
- **`ember-pixel`** — named in the original plan (all-bitmap, no TTF) but not built. Deferred, not abandoned: a genuine pixel-art style is a design decision a human should make, not something a coding agent should guess at and ship as placeholder art passed off as a real second theme.
