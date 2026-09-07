# Ember2D — Phase 7: Pixel-Space UI Foundation and Editor Rebuild

**Follows** Phase 6 (performance and data-model hardening), complete.
**Replaces** the earlier Phase 7 draft, which planned editor features on the existing cell-based UI. See §0.2 for why that changed.
**Refactor plan reference:** `docs/ember2d-refactor-plan.md` §7 Phase 7, §6.
**Absorbs** `docs/archive/roadmaptoV0.6.md` V0.5.3–V0.5.9, minus the deferrals in §0.5.

---

## 0. Framing

### 0.1 The problem

The editor's entire UI is quantized to 8×16 character cells. Panel positions, sizes, hit-tests, text, and borders are all `usize` cell coordinates. Borders are drawn by placing `|`, `-`, and `+` characters. Text is one fixed-size monospace bitmap font.

This is limiting in ways that compound:

- **Nothing can be drawn smaller than a cell.** Phase 8's asset preview is deferred specifically because a sprite thumbnail doesn't fit in a character cell.
- **Text is one size.** No hierarchy, no small labels, no large headers.
- **Style is code, not data.** Restyling means editing `draw_panel_chrome`'s character placement.
- **Drawing and hit-testing are separate code paths that agree by convention.** `draw_panel_chrome` computes where the close button goes; `Panel::on_close_btn` recomputes it independently. Same for tabs, toolbar buttons, palette entries. Every bug in the old `Issues.txt` was this class — a click landing somewhere other than where the thing was drawn.

### 0.2 Why now, and why this replaces the earlier plan

The previous Phase 7 draft built a property grid, toasts, tooltips, a command palette, and rulers — roughly 2,000 lines of new UI, all cell-based, all of which this rebuild would immediately invalidate. Foundation first, features second. The features come back in Part 5, on the new base.

### 0.3 The two aims, and how they coexist

Stated goals: a retro look that ages well, **and** text at multiple sizes like Unity/Unreal. Those pull against each other — crisp pixel-art UI wants a font designed at one size drawn at whole multiples; TTF at arbitrary sizes with antialiasing is a cleaner, more flexible, less chunky aesthetic.

**Resolution: the font is a theme asset.** The engine supports both a bitmap font (exact pixels, integer scale) and a TTF font (rasterized at any size). A pixel-styled theme ships the former; a modern theme ships the latter. UI code never branches on which it got. This satisfies the actual requirement — not being boxed in later — without forcing the aesthetic call now.

### 0.4 Aesthetic direction

**9-slice beveled panels, tight palette, pixel font.** Workbench / Deluxe Paint / SNES-menu territory. It ages well because it comes from a real constraint (chunky pixels, limited colors) rather than being a costume.

**Avoid anything that simulates a display**: scanlines, phosphor glow, CRT curvature, terminal-green-on-black. Those read as an effect applied on top, and effects date badly.

The practical payoff of 9-slice: panel chrome becomes a texture instead of character-placement code. `draw_panel_chrome` collapses from manual `|`/`-`/`+` placement into nine quads from an atlas, and restyling becomes swapping a PNG.

### 0.5 Scope

**In:** Parts 1–6 below.

**Deferred:**
- **Asset preview / drag & drop** (V0.5.5) → Phase 8. Unblocked by this phase (thumbnails become drawable), but there's still no asset model to preview — textures are path strings, clips aren't serialized.
- **The `EditorUi` abstraction** (refactor plan §6) → **not built, and now formally dropped.** It existed as an escape hatch to egui. This phase commits to owning the chrome, and the `Theme` resource covers the real need (restyling without touching layout code). An abstraction with one implementation and no second consumer is speculative structure.
- **Migrating the script-facing HUD API to pixels** → see §0.7.

### 0.6 Guardrail — it changes shape this phase

Phases 5–6 verified `ember2d-editor/` had **zero** diff. That inverts: editor changes are the work now.

- **`ember2d-sim/` must have zero diff.** If a step seems to need one, it's misplaced.
- **`ember2d/` changes are expected but must be additive** — new renderer primitives and the font module. No existing signature changes; `draw_char`, `draw_str`, and the cell-based helpers stay exactly as they are, because `PlayState`'s HUD and both demos depend on them.

The 5×-fresh-process replay gate is not required per step (nothing here touches the deterministic sim path). Run it once at phase end.

### 0.7 The script-facing HUD stays cell-based

`ctx.draw_hud(x, y, ...)`, `draw_box`, `fill_rect`, `draw_panel`, `draw_menu` all take cell coordinates and are the public scripting contract documented in `docs/ember2d-scripting-api.md`.

**Leave them alone this phase.** They're genuinely fine for games, both demos depend on them, and changing them is a breaking API change that would double the migration surface. They become thin wrappers over the pixel path, so they benefit from the new renderer without their signatures moving. A pixel-space script HUD API is a later, additive question.

---

## Part 1 — Pixel-space layout foundation

**The hardest part, and the one with a hard success criterion: appearance must not change.** Screenshots before and after Part 1 should be identical. This is a pure refactor with a working checkpoint at the end.

### 1a. Renderer primitives (`ember2d/src/renderer/`, additive)

Three new pixel-space primitives. `backend.draw_texture` already takes pixel position, per-axis size, rotation, tint, and a UV sub-rect, so most of this is plumbing rather than backend work.

```rust
/// Solid filled rectangle in pixels. Backed by a 1×1 white texture scaled
/// to size and tinted — the same instanced path glyphs and sprites use, so
/// it batches with them rather than forcing its own draw call.
pub fn fill_rect_px(&mut self, rect: Rect, color: Color);

/// Texture sub-rect blit in pixels. The 9-slice primitive.
pub fn draw_texture_px(&mut self, dest: Rect, texture: &Texture, src: Option<Rect>, tint: Color);

/// Nine-slice: corners drawn 1:1, edges stretched along one axis, center
/// stretched both ways. `border` is the inset in source pixels.
pub fn draw_nine_slice(&mut self, dest: Rect, texture: &Texture, border: (f32, f32, f32, f32), tint: Color);
```

Add a built-in 1×1 white texture to `AssetManager` at startup if one doesn't already exist — `fill_rect_px` needs it and so does any future tinted quad.

**Done when:** each primitive has a unit test on its coordinate math (testable without a GPU, following `screen_cell_to_pixel`'s existing precedent), and a scratch call draws a beveled box on screen.

### 1b. `UiRect` and the compatibility bridge

This is what makes Part 1 survivable.

```rust
/// Pixel-space rectangle. f32 for layout math; rounded to whole pixels at
/// draw time so nothing lands on a half-pixel and blurs.
#[derive(Clone, Copy)]
pub struct UiRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl UiRect {
    /// A cell rect converted to pixels — CELL_W=8, CELL_H=16. Every panel
    /// starts life going through this, so Part 1 is a coordinate-system
    /// change with zero visual change. Panels migrate to arbitrary pixel
    /// positions one at a time afterward, and this goes away at the end of
    /// Part 4.
    pub fn from_cells(cx: i32, cy: i32, cw: usize, ch: usize) -> Self;
    pub fn contains(self, px: f32, py: f32) -> bool;
    pub fn inset(self, by: f32) -> Self;
    pub fn split_left(self, w: f32) -> (Self, Self);   // and split_right/top/bottom
}
```

Because a cell is exactly 8×16 pixels, `from_cells` is lossless. Every existing panel keeps its exact position and size while the system underneath becomes pixel-based.

### 1c. Migrate `Panel` and `PanelManager` to `UiRect`

- `Panel.x/y/w/h: i32/usize` → `rect: UiRect`.
- `contains`, `on_title_bar`, `on_close_btn`, `on_resize_handle` all take pixel coordinates.
- `apply_layout` computes in pixels. Master-fill for the viewport works identically.
- `DOCK_THRESHOLD`, `MIN_W`, `MIN_H` become pixel constants (`3` cells → `24.0` px, etc.).

Mouse input arrives as pixels already — `MouseState` currently derives `cell_x`/`cell_y` from them. Keep both; the editor reads pixels, the cell fields stay for the script HUD path.

**This resolves defect E4** (two sources of truth for the canvas rect): `PanelManager` owns it, and `Layout`'s canvas fields go away or become a projection.

### 1d. One pass produces both drawing and hit-testing

The structural fix, and the reason a rebuild is worth doing at all.

Every interactive element gets its rect computed **once**, into a per-frame list:

```rust
pub struct UiHit { pub id: WidgetId, pub rect: UiRect }

pub struct UiFrame {
    hits: Vec<UiHit>,   // cleared each frame, pushed in draw order
}
impl UiFrame {
    pub fn push(&mut self, id: WidgetId, rect: UiRect);
    /// Topmost hit — last pushed wins, matching draw order.
    pub fn hit(&self, px: f32, py: f32) -> Option<WidgetId>;
}
```

Drawing pushes the rect it just drew into. Hit-testing queries the list from the previous frame. A widget can no longer be drawn in one place and clicked in another, because there is only one rect.

**This resolves E5** (tab hitboxes computed independently of tab drawing) structurally rather than by keeping two functions in sync.

**Migration order:** panel chrome and tabs first (E5's actual site), then the toolbar, then palette entries, then inspector rows. Each is independently verifiable by clicking things.

### 1e. Editor canvas coordinate cleanup

With pixel layout in place, three defects from the earlier review dissolve or become trivial:

- **E1** — `draw_cursor_highlight` derives its grid cell from screen position then draws with `scroll = (0.0, 0.0)`, so the highlight drifts off the tile during a smooth pan at fractional scroll. Fix by computing in world coordinates and drawing with the real `scroll`, like every other call in the file. **Pin this with a test** (see 1f) — it's the bug most likely to silently return.
- **E2** — `grid_to_pixel` hardcodes `8.0`/`16.0`. Use the renderer's `CELL_W`/`CELL_H` (export them; the one allowed additive `ember2d/` change here) or, better, the theme's cell metrics once Part 3 lands.
- **E3** — `draw_extra_spawns` computes label position by integer-dividing pixels by 8/16, correct only at zoom 1.0. Derive from the same `grid_to_pixel` result the marker uses.

Also fix **E6**: `PanelManager::new(80, 24)` and `Layout::new(80, 24)` hardcode a terminal-era size, corrected on the first `apply_layout`. Construct from the real viewport or comment that the value is provisional.

### 1f. Editor tests

`ember2d-editor` has **one** library test. This phase is almost entirely layout and coordinate math — the most testable code in the project and the easiest to break invisibly.

- **Coordinate round-trip:** world → pixel → world is the identity across scroll values, zoom levels, and canvas origins. This is the test that catches E1.
- `UiRect::from_cells` matches the old cell math exactly for every existing panel — the Part 1 "appearance unchanged" property, pinned.
- `apply_layout`: viewport fills the gap for every docked-panel combination; `validate_active_panels` recovers when the active panel is hidden.
- `UiFrame::hit` returns the topmost widget when rects overlap.
- `UndoStack` batching: rect fill, line, flood fill, paste, and multi-erase each undo as one unit; redo clears on a new edit.

**Done when Part 1 completes:** the editor looks pixel-identical to before, every panel docks/resizes/tabs as before, all editor tests pass, and `grep` finds no `8.0`/`16.0` cell literals in `ember2d-editor/`.

---

## Part 2 — Font system

### 2a. The `Font` trait

```rust
pub trait Font {
    /// Rasterize (or fetch from cache) one glyph at one pixel size.
    /// Returns its atlas sub-rect and layout metrics.
    fn glyph(&mut self, ch: char, px: f32) -> Option<GlyphInfo>;
    /// Width and height of `text` at `px`, WITHOUT drawing it.
    fn measure(&mut self, text: &str, px: f32) -> (f32, f32);
    fn line_height(&self, px: f32) -> f32;
    fn ascent(&self, px: f32) -> f32;
}

pub struct GlyphInfo {
    pub atlas_rect: Rect,   // pixels in the glyph atlas
    pub offset: Vec2,       // from pen position to the quad's top-left
    pub advance: f32,       // how far the pen moves after this glyph
}
```

Two implementations:

- **`BitmapFont`** — wraps the existing 8×8 font atlas. `advance` is constant, `measure` is `len * 8 * scale`. Only honours integer multiples of its native size; a request for 12px snaps to 8 or 16 rather than blurring.
- **`TtfFont`** — `fontdue`, rasterized on demand into a dynamic atlas.

UI code calls the trait and never branches on which it got.

### 2b. Dynamic glyph atlas

Cache keyed `(font_id, char, px_size_bits)`. On a miss, rasterize and pack into a texture (shelf packer is plenty — glyphs are similar heights per size). Upload once on insert.

No eviction initially. An editor uses a few hundred distinct glyphs across a handful of sizes and will never fill a 1024×1024 sheet. Log a warning if the atlas fills; solve it if it ever happens.

`px_size` must be quantized (e.g. to 0.5px) before it becomes a cache key, or continuous zoom generates a new atlas entry per frame.

### 2c. `measure_text` is the important part

The single most limiting thing about the current system isn't that text is one size — it's that **width is assumed to be `len * 8` inline, everywhere**. Every one of those sites is wrong under proportional text.

Audit and route through `measure`: centering, right-alignment, tab widths, truncation with `…`, text-input cursor positioning, the file browser, the script editor's column math.

Then build on it:

```rust
pub fn wrap_text(&mut self, text: &str, px: f32, max_w: f32) -> Vec<String>;
```

Word wrap is exactly what `docs/ember2d-rpg-demo-feasibility.md` §2.5 flags as missing for dialogue. Building it here means the RPG genre gets it for free later.

### 2d. Baseline positioning

Text draws from a **baseline**, not a top-left corner. Mixed sizes on one line only align correctly on a shared baseline, and that's the whole point of having multiple sizes. `ascent()` converts between the two for call sites that think in boxes.

**Done when:** the editor renders through the `Font` trait with `BitmapFont`, appearance unchanged; swapping to `TtfFont` renders legibly at 10/12/16/24px; `measure` is exercised by a test comparing against known widths for both implementations; `wrap_text` has tests for long words, exact-fit lines, and empty input.

---

## Part 3 — Theme

### 3a. The resource

```rust
pub struct Theme {
    pub palette: BTreeMap<String, Color>,   // named roles, not raw colors
    pub chrome: TextureId,                  // 9-slice atlas
    pub slices: BTreeMap<String, NineSlice>,// "panel", "button", "button_pressed", "input", "tab_active"…
    pub font: FontHandle,
    pub font_sizes: FontSizes,              // small / body / heading, in px
    pub metrics: Metrics,                   // padding, border width, row height, min touch target
}
```

Loaded from `themes/<name>/theme.ron` plus its PNG. **Palette entries are roles, not colors** — `panel_bg`, `text_primary`, `text_dim`, `accent`, `danger` — so a theme swap doesn't require every call site to reinterpret what `DarkBlue` meant.

**This makes V0.5.8 (Color Themes) a file, not a feature.**

### 3b. Chrome through 9-slice

`draw_panel_chrome` stops placing characters and becomes: one `draw_nine_slice` for the frame, one for the title bar, `measure`-centered title text, one slice for the close button. Buttons, inputs, tabs, and scrollbars all follow the same pattern.

### 3c. Integer UI scale

Pixel-art UI shimmers at fractional scale. Make the UI scale an explicit integer multiplier (1×, 2×, 3×), separate from `SCALE` and from the canvas zoom. Nearest-neighbor sampling for bitmap fonts and 9-slice; TTF fonts rasterize at the scaled size instead of being scaled up.

**Done when:** two themes exist (one pixel-styled with a bitmap font, one with a TTF at multiple sizes), switching between them changes only data, and 1×/2× both render crisply.

---

## Part 4 — Restyle

Now that appearance is data, actually design it. 9-slice beveled panels per §0.4.

- Draw the chrome atlas: panel frame, title bar, button (normal/hover/pressed/disabled), text input, tab (active/inactive), scrollbar, checkbox, resize grip.
- Panels get real padding rather than one-cell borders. Text gets a hierarchy (heading / body / small).
- Toolbar buttons become icons rather than characters.
- Remove `UiRect::from_cells`. Panels now size to content and to theme metrics rather than to a cell grid.

**Done when:** the editor no longer looks cell-quantized, `from_cells` is gone, and the full regression checklist §§3–9 passes.

---

## Part 5 — Rebuild the editor features on the new foundation

These were in the earlier Phase 7 draft and are unchanged in intent — only the base they're built on is different.

**5a — Rulers and selection (V0.5.3).** Coordinate indices along the viewport's top and left edges, ticking every 5 cells to match `draw_grid_overlay`'s existing convention, respecting scroll and zoom, togglable. Cursor highlight becomes bracketed rather than a solid fill, so the tile underneath stays readable.

**5b — Inspector 2.0 (V0.5.4).** Row-based property grid computed from `(label, widget)` pairs instead of hardcoded offsets. Collapsible sections: Transform, Sprite, Physics, Script, Exits. Inline widgets — toggles for solid/trigger/visible, numeric steppers for collider size and layer. `TextInputPurpose` has 20+ variants, many of which are per-property modal prompts; inline editing should retire several of them.

**5c — Toasts and tooltips (V0.5.6).** Replace `save_message`/`save_message_timer` with a queue: stacked messages, independent timers, severity levels, drawn over the viewport. Tooltips on hover delay for toolbar and inspector fields, drawn above everything and respecting panel z-order.

**5d — Command palette (V0.5.7).** `Ctrl+P` for files, `Ctrl+Shift+P` for commands, fuzzy-filtered. Higher leverage than it looks: `input/shortcuts.rs` is ~350 lines of bindings with no discoverability. Reuse the existing modal infrastructure rather than building a second overlay system.

---

## Part 6 — Performance, audit, docs

**6a — Editor rendering performance (V0.5.9).** `draw_grid` and `draw_physics_overlay` both do three full passes over every tile in the level per frame, filtering by layer:

```rust
for l in 0..3 { for (&(gx, gy, lyr), tile) in &grid.tiles { if lyr != l { continue; } ... } }
```

`LevelGrid.tiles` is keyed `(x, y, layer)`, so a `BTreeMap` range query over the visible rect — or a per-layer index built on edit — removes the scan entirely. Same shape as Phase 6's Step 8, much smaller. Measure before/after on `floor2.level` at zoom 1.0 and 0.25, and **state the numbers in the commit message**, per project convention.

**6b — Undo/redo audit (V0.5.9).** Every mutating action confirmed present in the undo stack and batching correctly. Newly relevant: anything 5b's inline editing added, since inline widgets make it much easier to mutate without going through the command path.

**6c — Docs.**
- `docs/ember2d-regression-checklist.md`: rulers, toasts, command palette, inspector sections, theme switching; correct §15/§17's stale "CI runs replay per-push" text (`.github/` was deleted).
- `docs/ember2d-refactor-plan.md` §7: Phase 7 amendment block recording what shipped versus this plan, including the formal drop of the `EditorUi` abstraction (§0.5) and the reasoning.
- `docs/ember2d-scripting-api.md`: note that the cell-based HUD API is unchanged and now wraps the pixel path (§0.7).
- New `docs/ember2d-theming.md`: the theme file format, the palette roles, how to author a chrome atlas.
- `docs/HANDOFF.md`: rewrite for the Phase 7 → Phase 8 transition.

---

## 7. Verification

**Per step:** `cargo build --workspace --examples`; `cargo test --workspace --lib`; all named integration tests; manual editor smoke test; `git diff --stat` confirming `ember2d-sim/` untouched and `ember2d/` additive only.

**End of Part 1 specifically:** side-by-side screenshots against the pre-Part-1 build. Any visual difference is a bug, not an improvement.

**End of phase:**
1. `cargo test --test replay` once, as a sanity check.
2. Full manual pass of `docs/ember2d-regression-checklist.md` §§3–9.
3. Load, edit, save, reload `roguelike/floor1.level` and `floor2.level`; confirm `tests/roguelike_level_integrity.rs` still passes — that's what catches an editor round-trip silently dropping a `LevelData` field.
4. Both demos play unchanged: `cargo run -- roguelike/floor2.level`, `cargo run -- shooter/arena.level`. The script HUD path must be untouched.
5. Theme switch at runtime, or at minimum by editing the theme file and restarting.

---

## 8. Notes

**On CI.** `.github/` was deleted because Actions reported billing as unavailable. Actions minutes are free and unlimited for public repositories, and this repo is public — likely a settings issue rather than a real block. Worth checking Settings → Actions → General and the account's billing spending limits. It matters less this phase than any other (nothing here touches the deterministic sim path), but a great deal for Phase 9.

**On square world units.** The refactor plan §4 called for rasterizing the font at its true 8×8 so world units are square; `CELL_W=8`/`CELL_H=16` means a 1.0×1.0 sprite is still 8×16 pixels, so vertical motion covers twice the visual distance of horizontal motion per unit. That's a *gameplay* coordinate issue, not a UI one, and it belongs with the platformer demo rather than here. But this phase makes it cheaper: once UI layout no longer assumes 8×16 cells, changing the cell aspect stops rippling into the editor.

**On the 600-line limit.** Parts 1–3 add substantial new code to `ember2d-editor/`. `ui/panels.rs` is already ~1,065 lines and `input/panels.rs` ~943 — both over. Split as you go, using the precedents in `ember2d-sim/src/scripting/` and `simulation/`.
