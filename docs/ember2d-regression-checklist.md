# Ember2D — Regression Checklist

**Written against:** the `claude` branch, mid-Phase-4 (docs/ember2d-refactor-plan.md).
**Purpose:** the definition of "working" for everything automated tests
still can't see — editor interactions, visual rendering, and anything
needing a live window. **Corrected in Step 4k**: Phase 4 added real
automated tests (`cargo test --lib` — 74 tests — plus `tests/roguelike_*.rs`
— 19 more, covering combat, turn-cadence, determinism, and level integrity
headlessly via `TurnHarness`), so this list is no longer the *only* safety
net the way it was through Phase 3. It's still the right net for anything
those tests can't reach.

**Before first use:** open `roguelike/floor1.level` (or `--editor` it) —
the original `demo/` this checklist targeted is archived at
`docs/archive/demo/` (Phase 4; see that folder's own README) and isn't run
by anything anymore.

Legend: `[ ]` untested · `[✓]` passes · `[✗]` already broken (see §14)

---

## 1. Launch

- [ ] `cargo run` opens the start screen
- [ ] `cargo run -- --editor` opens the editor
- [ ] `cargo run -- --editor path/to.level` opens that level
- [ ] `cargo run -- path/to.level` plays directly
- [ ] Bad path prints an error and exits without panicking
- [ ] No args + unrecognised args print usage
- [ ] `project.ron`'s `visual_style` drives `set_sprite_mode` on launch
- [ ] `project.ron`'s `gameplay_loop` is applied

## 2. Window and surface

- [ ] Window resize reflows the cell grid and reconfigures the surface
- [ ] Minimise and restore does not crash or panic on zero-size surface
- [ ] `SurfaceError::Outdated` / `Lost` recover without a crash
- [ ] DPI scaling: mouse cell position matches the cursor at non-100% scaling

## 3. Start screen and projects

- [ ] Create project: name, visual style, gameplay loop all selectable
- [ ] `project.ron` written with all four fields
- [ ] `BasicRoom` template generates walls, floor, centred spawn
- [ ] Empty template gives a blank grid
- [ ] Open project lists folders with `project.ron` or any `.level`
- [ ] Project name falls back to folder name when `project.ron` is missing
- [ ] `project.palette.ron` loads if present
- [ ] Escape exits cleanly

## 4. Editor — painting and tools

- [ ] Left-click/drag paints; right-click/drag erases
- [ ] Eraser brush size cycles 1 → 3 → 5
- [ ] Rectangle fill, line tool (Bresenham), flood fill
- [ ] Scatter paint
- [ ] Palette selection by number and by click
- [ ] **Layers:** active layer switching; painting only affects the active layer
- [ ] Tiles on different layers at the same (x, y) coexist
- [ ] **Zoom** in/out; painting lands on the correct tile at every zoom level
- [ ] Smooth scroll/pan reaches the target and clamps at bounds
- [ ] Middle-drag pans

## 5. Editor — clipboard and undo

- [ ] Copy-select, cut-select, paste
- [ ] Paste flip-X, flip-Y, rotate CW/CCW
- [ ] Undo/redo single edits
- [ ] Rect fill, line, flood fill, paste, multi-erase each undo as **one** batch
- [ ] Redo stack clears after a new edit
- [ ] Undo after save re-marks unsaved

## 6. Editor — properties and inspector

- [ ] Rename level; resize level (tiles outside new bounds dropped, spawns clamped)
- [ ] Attach script path to a tile; set tag; set glyph; set next-level exit
- [ ] Toggle solid / trigger
- [ ] Set collider layer and collider mask on a tile and on the player
- [ ] Player properties: glyph, tag, script, camera follow, texture
- [ ] Move player spawn; add named spawn
- [ ] Text input: typing, backspace, Enter, Escape — for every `TextInputPurpose`
- [ ] Modal confirm (switch level with unsaved changes) behaves correctly

## 7. Editor — palette

- [ ] Palette scrolls; search field focuses and filters
- [ ] Palette editor opens; name/tag/glyph fields editable
- [ ] HSV colour picker sets fg and bg; custom colour entry works
- [ ] Palette saves to `project.palette.ron` and reloads

## 8. Editor — script editor

- [ ] Open a `.rhai` file in the built-in editor
- [ ] Type, navigate with cursor keys, scroll
- [ ] Save; unsaved indicator clears
- [ ] Create a new script from the file browser
- [ ] Edited script takes effect on next play

## 9. Editor — panels, menus, files

- [ ] Panels dock, undock, resize, toggle, focus
- [ ] Panels don't swallow canvas clicks
- [ ] Menu bar opens; context menus on file browser, tabs, hierarchy
- [ ] File browser navigates folders; creates `.level`, `.rhai`, folders
- [ ] Native file dialog (`rfd`) opens where wired
- [ ] Grid overlay, physics overlay, help screen toggles; Escape closes help
- [ ] Console shows script log; auto-opens on errors
- [ ] Save, Save-As, New, Open, Close Project

## 10. Editor — node graph

Only until visual scripting is shelved. Afterwards, confirm old levels with graphs still load.

- [ ] Graph editor opens for a tile; add/drag/connect/delete nodes
- [ ] Inline parameter editing; node copy/paste
- [ ] Graph saves into the `.level` and reloads
- [ ] Generated Rhai runs in play mode

## 11. Play mode

- [ ] F5 enters play; Escape opens the pause menu
- [ ] Pause menu: Resume, Back to Editor, Quit
- [ ] Tiles spawn with correct solid/trigger/tag/layer/collider layer
- [ ] Player spawns at spawn point with configured glyph, tag, texture
- [ ] **Corrected in Step 4k**: movement is entirely script-driven as of
      Phase 4 — `PlayState` itself contains no movement code, no
      tag-specific strings, no score. `roguelike/`'s own `player.rhai` does
      turn-gated grid movement (bump-to-attack instead of colliding; no
      corridor snapping or diagonal normalization — those were
      realtime-AABB-movement concepts and no longer apply to this demo's
      grid-based turn model). A different script-driven project could
      still implement realtime AABB movement itself; the engine no longer
      assumes either.
- [ ] Wall bump consumes no turn; a move onto open floor does
- [ ] Bump-to-attack: walking into a tagged "enemy"/"boss" entity attacks instead of moving, and still consumes a turn
- [ ] F3 toggles a debug overlay (level name, position, backend, FPS) — off by default, not a permanent bar (Step 4g; matches the standing preference that this kind of info be a toggle, not always-on chrome)
- [ ] Camera follows with lerp and clamps at level edges
- [ ] Camera shake fires and decays
- [ ] Particles spawn, move, and expire
- [ ] Glyph animation clips play (`register_clip` + `play_clip`/`play_clip_once`) — the legacy `Sprite.frames`/`frame_rate` glyph-cycling fields were removed in Step 3e
- [ ] Texture sprites render when a tile has a texture
- [ ] Script-drawn HUD (`ctx.draw_hud`) survives opening the pause menu instead of vanishing (Step 4g fixed a real bug here, catalogued as D16)
- [ ] Last 3 log lines render at the bottom of the viewport (now full-height as of Step 4g — no bottom bar to sit "above" anymore)
- [ ] Exit trigger loads the next level; relative paths resolve
- [ ] Script log transfers to the editor console on exit

## 12. Turn-based mode

**Corrected in Step 4k: promoted to the primary play-mode section** —
`roguelike/` is turn-based (`GameplayLoop::TurnBased`), and this is the
model most of Phase 4's own automated tests (`tests/roguelike_*.rs`)
already exercise headlessly via `TurnHarness`. This section is for what
those tests can't see: how it actually *feels* to play.

- [ ] A TurnBased project only advances physics when `trigger_turn` is called
- [ ] Rendering stays responsive while the world is idle
- [ ] Scripts still run each frame in turn mode (current behaviour — confirmed intended: it's what lets a rat's hp/death check and a stairs tile's lock-state update every frame, even between player turns)
- [ ] One keypress moves the player exactly one cell and advances exactly one turn — no double-moves on a slow frame, no dropped presses on a fast one (automated: `tests/roguelike_floor1.rs`)
- [ ] Enemies visibly act the frame *after* your turn, not the same frame — should read as "they wait for you," not simultaneous
- [ ] An asleep enemy (no line of sight yet, Step 4h's amendment) visibly does nothing until it wakes — tinted differently while asleep vs. awake (`DarkRed`/`DarkMagenta` vs `Red`/`Magenta`)
- [ ] Waiting (Space) and quaffing (Q) both visibly cost a turn, same as moving does
- [ ] Death screen appears the instant hp reaches 0; R restarts from floor 1

## 13. Save/load and scripting

### Input buffering (after Phase 1)
- [ ] One keypress triggers a script action exactly once, under sustained frame drops
- [ ] One keypress in the editor undoes/paints/menu-selects exactly once, under frame drops
- [ ] A press on a frame where the accumulator runs zero steps is still observed
- [ ] Press and release inside a single frame still registers
- [ ] `is_held` stays true for the whole hold and false immediately on release
- [ ] Turn-based: a press while waiting for the turn is honoured when the turn arrives
- [ ] Mouse and gamepad buttons behave the same as keys

- [ ] `save_game` writes a `.ron`; `load_game` restores world + persistent state
- [ ] Loading a save resumes at the right level with entities intact — **known gap, see D17**: globals aren't part of `SaveState` and the `is_loading_save` path never re-runs `on_start` scripts, so any per-entity state that lives in globals (most of the roguelike's own combat state — `hp_<id>`, `acted_<id>`, `aware_<id>`) will not survive a save/load round-trip mid-run. Don't fail this item over that specific gap; it's tracked separately.
- [ ] `on_start`, `on_update`, `on_collide` all fire; missing ones don't error
- [ ] **Corrected in Step 4k** — deleted a wrong item that used to read "Per-entity scope persists across frames; removed on despawn." Rhai does not provide this: a script's own `let` does *not* survive between calls (`CallFnOptions::rewind_scope: true`) — see `ember2d-scripting-api.md`'s "Per-entity scope" section, also corrected this step. What actually needs checking instead: per-entity state kept in globals/persistent (e.g. `"hp_" + id`) survives across frames and is cleaned up on despawn (nothing explicitly clears these keys today, but a despawned entity's id is never reused within a level, so stale keys are harmless, not a leak that matters).
- [ ] Hot-reload recompiles on file change and logs
- [ ] Compile errors show the file name; runtime errors log once

Exercise at least one function from every API group (see `ember2d-scripting-api.md` §3):
- [ ] Transform · [ ] Tags · [ ] Glyph/colour/texture/animation · [ ] Input · [ ] Gamepad
- [ ] Globals · [ ] Persistence · [ ] Timers · [ ] RNG · [ ] Spatial queries
- [ ] Raycast · [ ] Pathfinding · [ ] Camera · [ ] Mouse · [ ] HUD widgets
- [ ] Particles · [ ] Audio (incl. spatial) · [ ] Hierarchy · [ ] Save/load · [ ] Turn

## 14. Known broken before the refactor

Not regressions. Do not chase these.

| Issue | Fixed by |
|---|---|
| D1 — one keypress can fire multiple times per frame, or be dropped on a light frame | Phase 1 (buffer until consumed) |
| D2 — `set_persistent` in `on_start` is discarded | Phase 1 |
| D3 — RNG nondeterministic (scripts, particles, shake) | Phase 1 |
| D4 — trigger colliders default to layer `"solid"` | Phase 1 |
| D5 — equal-z sprites draw in random order | Phase 1 |
| D6 — editor obeys the project's `gameplay_loop` | Phase 1 |
| D8 — hot-reload clears all entity scopes | Phase 1 |
| D9 — failed scripts keep running silently | Phase 1 |
| D10 — `spawn_entity` hardcodes appearance and collider | Phase 1 |
| D12 — `"locked"` magic string on exits | Phase 1 |
| D13 — texture sprites skip viewport culling | Phase 1 |
| D14 — font atlas and textures use different colour spaces | Phase 1 |
| D7 — turn mode integrates physics at `dt = 1.0` | Phase 5 |
| D11 — per-frame clone churn in scripting and collision | Phase 6 |
| D17 — save/load can't round-trip per-entity script globals mid-run | not scheduled — see plan §3 |

## 15. Before you start

- [ ] Merge/rename so there is one trunk branch
- [ ] Port `demo/` forward from `main`; confirm all three levels load and play
- [ ] Screen capture: start screen → new project → paint → save → F5 → play → pause → editor → script editor → graph editor
- [ ] Copy `demo/` outside the repo as a migration reference
- [ ] Tag `v0.5.0-pre-refactor`
