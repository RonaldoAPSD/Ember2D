# Ember2D — Regression Checklist

**Written against:** the `claude` branch — current phase/step status lives
in `docs/ember2d-master-plan.md` §2, not duplicated here (R36,
docs/ember2d-master-plan.md §3.2).
**Purpose:** the definition of "working" for everything automated tests
still can't see — editor interactions, visual rendering, and anything
needing a live window. **Corrected in Step 4k**: Phase 4 added real
automated tests (headless, via `TurnHarness` and `PlayState` directly —
combat, turn-cadence, determinism, and level integrity), grown
substantially through Phases 5–7A since; run `cargo test --workspace` for
the current count rather than trust a number quoted here, which would go
stale the next time a step adds tests. This list is no longer the *only*
safety net the way it was through Phase 3, but it's still the right net
for anything automated tests can't reach.

**Before first use:** open `demos/roguelike/floor1.level` (or `--editor` it) —
the original `demo/` this checklist targeted is archived at
`docs/archive/demo/` (Phase 4; see that folder's own README) and isn't run
by anything anymore.

Legend: `[ ]` untested · `[✓]` passes · `[✗]` already broken (see §14)

---

## 1. Launch

**Phase 5 Step 5i** (docs/ember2d-phase5-plan.md) split this into a Cargo
workspace (`ember2d-sim`/`ember2d`/`ember2d-editor`/`ember2d-app` — see
CLAUDE.md's "Workspace layout") — every item below still applies unchanged
(one bin target, so plain `cargo run` still resolves it), this is just the
place to notice if it doesn't.

- [✓] `cargo run` opens the start screen
- [✓] `cargo run -- --editor` opens the editor
- [✓] `cargo run -- --editor path/to.level` opens that level
- [✓] `cargo run -- path/to.level` plays directly
- [✓] Bad path prints an error and exits without panicking
- [✓] No args + unrecognised args print usage
- [ ] `project.ron`'s `visual_style` drives `set_sprite_mode` on launch —
      **not exercised**: both shipped demos (`demos/roguelike/`, `demos/shooter/`) use
      `visual_style: ClassicASCII`; nothing in this repo sets `Sprites2D`
      to cross-check against (7B gate, 2026-09-07)
- [✓] `project.ron`'s `gameplay_loop` is applied — confirmed by the two
      demos' actual behavior matching their `project.ron` (roguelike:
      `TurnBased`, advances only on keypress with a visible turn counter;
      shooter: `RealTime`, continuous AI/physics with a live FPS counter)
      (7B gate, 2026-09-07)

## 2. Window and surface

- [✓] Window resize reflows the cell grid and reconfigures the surface
      (7B gate, 2026-09-07 — screenshot before/after a `SetWindowPos` resize)
- [✓] Minimise and restore does not crash or panic on zero-size surface
      (7B gate, 2026-09-07)
- [✓] `SurfaceError::Outdated` / `Lost` recover without a crash — exercised
      by the same minimize/restore cycle above (that's the code path that
      hits it); not independently forced (7B gate, 2026-09-07)
- [ ] DPI scaling: mouse cell position matches the cursor at non-100% scaling —
      **needs a live non-100%-scale display**, not something a synthetic
      input pass can force; unverified this gate (7B gate, 2026-09-07)

## 3. Start screen and projects

- [✓] Create project: name, visual style, gameplay loop all selectable (8 gate, 2026-10-01)
- [✓] `project.ron` written with all four fields (8 gate, 2026-10-01)
- [ ] `BasicRoom` template generates walls, floor, centred spawn
- [✓] Empty template gives a blank grid (8 gate, 2026-10-01)
- [✓] Open project lists folders with `project.ron` or any `.level` (8 gate, 2026-10-01)
- [ ] Project name falls back to folder name when `project.ron` is missing
- [✓] `project.palette.ron` loads if present (8 gate, 2026-10-01)
- [✓] Escape exits cleanly — Esc backs out of every wizard/browser screen; the main menu itself has no Esc (its Quit item exits cleanly) (8 gate, 2026-10-01)

## 4. Editor — painting and tools

- [✓] Left-click/drag paints; right-click/drag erases (8 gate, 2026-10-01)
- [✓] Eraser brush size cycles 1 → 3 → 5 (8 gate, 2026-10-01)
- [✓] Rectangle fill, line tool (Bresenham), flood fill (8 gate, 2026-10-01)
- [✓] Scatter paint (8 gate, 2026-10-01)
- [✓] Palette selection by number and by click (8 gate, 2026-10-01)
- [✓] **Layers:** active layer switching; painting only affects the active layer (8 gate, 2026-10-01)
- [ ] Tiles on different layers at the same (x, y) coexist
- [✓] **Zoom** in/out; painting lands on the correct tile at every zoom level (8 gate, 2026-10-01)
- [✓] Smooth scroll/pan reaches the target and clamps at bounds (8 gate, 2026-10-01)
- [✓] Middle-drag pans (8 gate, 2026-10-01)

## 5. Editor — clipboard and undo

*(The level GRID's own clipboard/undo — `EditorState::clipboard`/`undo`,
tile-shaped. The script editor's text clipboard/undo, added 7C-8, is a
completely separate system — checked in §8 instead.)*

- [✓] Copy-select, cut-select, paste (8 gate, 2026-10-01)
- [✓] Paste flip-X, flip-Y, rotate CW/CCW — found R103 (H also hid the Hierarchy), fixed (8 gate, 2026-10-01)
- [✓] Undo/redo single edits (8 gate, 2026-10-01)
- [ ] Rect fill, line, flood fill, paste, multi-erase each undo as **one** batch
- [✓] Redo stack clears after a new edit (8 gate, 2026-10-01)
- [✓] Undo after save re-marks unsaved (8 gate, 2026-10-01)

## 6. Editor — properties and inspector

- [✓] Rename level; resize level (tiles outside new bounds dropped, spawns clamped) (8 gate, 2026-10-01)
- [ ] Attach script path to a tile; set tag; set glyph; set next-level exit
- [✓] Toggle solid / trigger (8 gate, 2026-10-01)
- [ ] Set collider layer and collider mask on a tile and on the player
- [ ] Save, close, and reopen a level whose tiles use non-default collider
      layers/masks — layer/mask filtering behavior survives the round trip
      unchanged (Phase 6 Step 7's bitmask is `#[serde(skip)]` and only
      recomputed from the saved layer/mask strings on load —
      `ember2d/tests/collision_layers.rs`'s save→load test is the automated
      form of this check; this is the manual editor-side cross-check)
- [ ] Player properties: glyph, tag, script, camera follow, texture
- [ ] Move player spawn; add named spawn
- [ ] Text input: typing, backspace, Enter, Escape — for every `TextInputPurpose`
- [✓] Modal confirm (switch level with unsaved changes) behaves correctly — found R104 (clicking [ YES ] painted on the new level), fixed (8 gate, 2026-10-01)

## 7. Editor — palette

- [✓] Palette scrolls; search field focuses and filters (8 gate, 2026-10-01)
- [✓] Palette editor opens; name/tag/glyph fields editable (8 gate, 2026-10-01)
- [✓] HSV colour picker sets fg and bg; custom colour entry works (8 gate, 2026-10-01)
- [✓] Palette saves to `project.palette.ron` and reloads (8 gate, 2026-10-01)

## 8. Editor — script editor

- [✓] Open a `.rhai` file in the built-in editor (8 gate, 2026-10-01)
- [ ] Type, navigate with cursor keys, scroll
- [✓] Save; unsaved indicator clears (8 gate, 2026-10-01)
- [✓] Create a new script from the file browser (8 gate, 2026-10-01)
- [ ] Edited script takes effect on next play
- [✓] Saving a script with a syntax error highlights the erroring line and
      shows the message; fixing it and saving again clears both (7C-7)
- [ ] Leaving the script unsaved and idle for ~1s also triggers the same
      check, with no explicit save (7C-7)
- [ ] A runtime script error during F5 preview appears in the editor
      console after returning (7C-7, R18)
- [ ] Shift+arrows and Shift+click select text; Ctrl+A selects all; a
      plain arrow move afterward collapses the selection (7C-8)
- [ ] Cut/copy/paste round-trips text, including non-ASCII, within the
      editor; cut/copy also lands on the OS clipboard for pasting into
      another application (7C-8)
- [ ] Typing while a selection is active replaces it
- [ ] Ctrl+Z/Ctrl+Y undo/redo script edits; a burst of typing (or of
      Backspace) undoes as one step, not one per character (7C-8)
- [ ] Ctrl+F opens a find bar; typing searches live and jumps to the
      first match; Enter advances to the next (wrapping); Escape closes it
      (7C-8)
- [ ] A line longer than the panel width scrolls horizontally as the
      cursor moves past the edge, with a `…` marker where it's clipped
      (7C-8)

## 9. Editor — panels, menus, files

- [ ] Panels dock, undock, resize, toggle, focus
- [✓] Panels don't swallow canvas clicks (8 gate, 2026-10-01)
- [✓] Menu bar opens; context menus on file browser, tabs, hierarchy (8 gate, 2026-10-01)
- [✓] (8 gate, 2026-10-01: 2x applied live and persisted) `Theme > UI Scale` (7D-3, docs/ember2d-master-plan.md §5.4): `Auto`/`1x`/`2x`/`3x`/`4x` entries listed after a separator, checkmark on the active one; picking one takes effect immediately (no restart) and persists across a restart (`%APPDATA%\Ember2D\editor_prefs.ron` on Windows). Every panel, bar, dock tab, modal, and the script editor (docked and fullscreen) visibly scales; the level canvas/viewport content does not. A click on any chrome widget still lands correctly at a non-default scale, including one that diverges from the display's own DPI-derived render scale (e.g. `1x` on a 200%-scaled display) — dragging/resizing a panel, opening a dropdown, clicking a file-browser row all still work. A floating (undocked) panel stays fully on-screen after changing scale, even if it was previously positioned near an edge. Chrome text overlapping at an extreme scale on a small window (e.g. `4x` at 1280×720) is expected, not a failure — a mis-click somewhere the overlapping text visually suggests IS one
- [✓] File browser navigates folders; creates `.level`, `.rhai`, folders (8 gate, 2026-10-01)
- [ ] Native file dialog (`rfd`) opens where wired
- [✓] Grid overlay, physics overlay, help screen toggles; Escape closes help — the grid draws under tiles (R100) (8 gate, 2026-10-01)
- [✓] Console shows script log; auto-opens on errors (8 gate, 2026-10-01)
- [✓] (8 gate, 2026-10-01: Save, New Script, Close Project — found R106, Close Project quit the app after an `--editor <level>` launch, fixed) Save, Save-As, New, Open, Close Project — **known gap, see D18** (§14): saving scrambles the level's tile order (`LevelGrid.tiles` is a `HashMap`, not sorted before writing `LevelData.tiles`). Not a reason to fail this item — tile *content* survives correctly, only *order* is unspecified — but don't use an editor-saved level to check for a clean diff, and re-run `cargo run --example gen_roguelike` if a `demos/roguelike/*.level` file gets touched by the editor.

## 10. Editor — node graph

Only until visual scripting is shelved. Afterwards, confirm old levels with graphs still load.

- [✓] Graph editor opens for a tile; add/drag/connect/delete nodes (8 gate, 2026-10-01)
- [ ] Inline parameter editing; node copy/paste
- [ ] Graph saves into the `.level` and reloads
- [ ] Generated Rhai runs in play mode

## 11. Play mode

- [✓] F5 enters play; Escape opens the pause menu (7B gate, 2026-09-07)
- [✓] F5 from the editor shows ONLY the play screen — no editor panels,
      bars, dock tabs, or the editor's own viewport bleeding through where
      play draws nothing (R51, master plan §3.2: the paused editor used to
      be drawn underneath every preview). Esc's pause panel draws OVER the
      still-visible play screen. Back to Editor restores the editor
      cleanly — its menus open and panels respond on the first click — also
      after resizing the window while in play. Check both demos.
- [✓] Pause menu: Resume, Back to Editor, Quit — all three render; only
      Resume actually clicked/keyed through this pass (7B gate, 2026-09-07)
- [ ] Shrink the play window well below the pause panel's own size
      (~400×220 physical px) and press Esc: the panel draws flush to the
      top-left, no panic (R86, master plan §3.2)
- [✓] Tiles spawn with correct solid/trigger/tag/layer/collider layer —
      via `roguelike_level_integrity.rs`/`trigger_collider_layer.rs`
      (automated) plus visual confirmation walls block movement and floor
      doesn't (7B gate, 2026-09-07)
- [✓] Player spawns at spawn point with configured glyph, tag, texture
      (7B gate, 2026-09-07 — green `@` at the level's authored spawn)
- [ ] **Corrected in Step 4k**: movement is entirely script-driven as of
      Phase 4 — `PlayState` itself contains no movement code, no
      tag-specific strings, no score. `demos/roguelike/`'s own `player.rhai` does
      turn-gated grid movement (bump-to-attack instead of colliding; no
      corridor snapping or diagonal normalization — those were
      realtime-AABB-movement concepts and no longer apply to this demo's
      grid-based turn model). A different script-driven project could
      still implement realtime AABB movement itself; the engine no longer
      assumes either.
- [✓] Wall bump consumes no turn; a move onto open floor does — via
      `roguelike_floor1.rs`'s
      `walking_into_a_wall_does_not_move_the_player_or_consume_a_turn`/
      `pressing_w_moves_the_player_one_cell_up_and_triggers_a_turn`
      (automated); live movement confirmed the player's screen position
      advances on open-floor presses, but the demo's own on-screen "Turn N"
      HUD counter (a `player.rhai` display, not engine state) appeared to
      increment on every keypress including an apparent wall bump — not
      re-litigated live since the automated test is the authoritative
      check for the actual engine-side invariant (7B gate, 2026-09-07)
- [ ] Bump-to-attack: walking into a tagged "enemy"/"boss" entity attacks instead of moving, and still consumes a turn — not exercised live this pass (floor2's nearest monster wasn't reachable in a short move sequence); covered by `roguelike_combat.rs` (automated)
- [✓] F3 toggles a debug overlay (level name, position, backend, FPS) — off by default, not a permanent bar (Step 4g; matches the standing preference that this kind of info be a toggle, not always-on chrome) (7B gate, 2026-09-07 — confirmed `Mode:WGPU AS FPS:75`, also a live sanity check that R23's frame-pacing fix didn't peg FPS to some stale cap)
- [ ] Camera follows with lerp and clamps at level edges — not exercised;
      floor2's viewport didn't scroll during this pass (7B gate, 2026-09-07)
- [ ] Camera shake fires and decays — relies on the automated
      `render_time_shake_jitter_never_touches_the_deterministic_rng_stream`
      test; a transient per-frame jitter isn't reliably catchable in a
      static screenshot, not independently confirmed live (7B gate, 2026-09-07)
- [ ] Particles spawn, move, and expire — not exercised live this pass
- [ ] Glyph animation clips play (`register_clip` + `play_clip`/`play_clip_once`) — the legacy `Sprite.frames`/`frame_rate` glyph-cycling fields were removed in Step 3e — not exercised live this pass
- [ ] Texture sprites render when a tile has a texture — neither shipped
      demo uses a textured tile; not exercisable without one
- [✓] Script-drawn HUD (`ctx.draw_hud`) survives opening the pause menu instead of vanishing (Step 4g fixed a real bug here, catalogued as D16) (7B gate, 2026-09-07 — HP/Gold/Potions/Depth/Turn HUD stayed visible with the pause overlay open)
- [ ] Last 3 log lines render at the bottom of the viewport (now full-height as of Step 4g — no bottom bar to sit "above" anymore) — no loggable event triggered this pass
- [✓] Exit trigger loads the next level; relative paths resolve — live
      in the Phase 9 gate pass (2026-10-01): floor 1's stairs to
      `floor2.level#door`, arriving at the named spawn; also covered by
      `an_unlocked_exit_triggers_a_level_transition`/
      `a_locked_exit_does_not_trigger_a_level_transition` (automated)
- [ ] Script log transfers to the editor console on exit — not exercised live

### Phase 9 — scenes, camera, menus/dialogue, spawns (gate pass 2026-10-01)

- [✓] Esc in play pushes the "pause" scene: the engine menu (PAUSED,
      Resume / Back to Editor / Quit Game) in the bundled TTF font, world
      frozen beneath; Up/Down + Enter drive it; Escape resumes. In a
      direct run (`ember2d level.level`) there is no Back to Editor
- [✓] A project's `scenes/pause.rhai` replaces the built-in menu; a scene
      it pushes with `data` reads it back; closing the top scene returns
      the keyboard to the menu beneath; resuming leaves no HUD behind
      (R112)
- [✓] A scene pushed with `pauses_world: false` draws over the level while
      the player still moves
- [✓] Script camera: `set_camera_zoom(2)` zooms around the player,
      `set_camera_target(id)` pans smoothly to that tile,
      `clear_camera_target()` pans back
- [✓] `draw_dialogue`: speaker line, word-wrapped text, three lines a page,
      Enter turns the page and closes it; `menu_open`: title, highlight
      moves with Up/Down (the player doesn't), Enter confirms
- [✓] Named spawns: an exit to `path#spawn` (or `load_level(path, spawn)`)
      enters at that spawn; a v6 level opens in the editor with its named
      spawns in the Hierarchy and saves as v7 (duplicate names renamed
      `_2`)
- [ ] Save mid-scene / mid-menu and load — covered by
      `a_save_made_with_a_scene_open_reopens_it_on_load` and
      `an_open_menu_survives_a_save_and_load` (automated); not live, no
      demo binds a save key yet (9-5's RPG demo will)

## 12. Turn-based mode

**Corrected in Step 4k: promoted to the primary play-mode section** —
`demos/roguelike/` is turn-based (`GameplayLoop::TurnBased`), and this is the
model most of Phase 4's own automated tests (`tests/roguelike_*.rs`)
already exercise headlessly via `TurnHarness`. This section is for what
those tests can't see: how it actually *feels* to play.

**Rewritten in Phase 5 Step 5f** (docs/ember2d-phase5-plan.md): `ctx.trigger_turn()`
is gone, replaced by `TurnScheduler` (`scheduler.rs`) plus the `on_turn`
lifecycle function and `ctx.act(cost)` — see `docs/ember2d-scripting-api.md`'s
"The command boundary" section. The late phase (collisions, `late_update`)
is now gated on whether `TurnScheduler` actually resolved an actor's turn
this step, not on a script explicitly flagging one.

- [ ] A TurnBased project only advances the late phase (collisions,
      `late_update`) on a step that actually resolved an actor's turn —
      physics itself never integrates in turn mode at all anymore (D7,
      fixed Step 5f)
- [ ] Rendering stays responsive while the world is idle
- [ ] FPS (F3 overlay) stays reasonable on floor2/floor3, not just floor1 —
      **regression found and fixed live in Step 5f** (docs/ember2d-phase5-plan.md,
      D11 row of the defect table). Two contributing causes, both fixed:
      (1) Step 5e/5f's `on_input`/`on_turn` passes each rebuilt a full
      `World`-derived `ScriptState` snapshot, turning "one rebuild per step"
      into up to three — fixed by sharing one `WorldSnapshot` (`Rc::clone`,
      O(1)) across all three passes per step instead of each rebuilding its
      own; (2) the much bigger one — `Cargo.toml` had no `[profile.dev]`
      overrides at all, so `wgpu` and every other dependency compiled fully
      unoptimized in a debug build (a well-known wgpu-specific debug-perf
      trap, unrelated to anything in this codebase), *and* `ember2d`'s own
      code (the `WorldSnapshot`/`ScriptState` construction and `apply_ctx`
      loops (1) is about) was itself unoptimized too. Fixed with
      `[profile.dev.package."*"] opt-level = 3` (dependencies) plus
      `[profile.dev] opt-level = 1` (this crate's own code — confirmed via
      headless benchmarking that the dependency-only override alone changed
      nothing; the crate-level one is what cut floor2's per-step
      script-logic cost ~3.75x). floor2 will still run somewhat slower than
      floor1 even now — that's the still-open part of D11 (per-step
      snapshot cost is `O(entities)`, opt-level 1 lowers the constant
      factor, doesn't remove the O(entities) itself) — the bar here is "no
      longer visibly janky on capable hardware," not "identical to floor1."
- [ ] Scripts still run each frame in turn mode (current behaviour — confirmed intended: it's what lets a rat's hp/death check and a stairs tile's lock-state update every frame, even between player turns)
- [ ] One keypress moves the player exactly one cell and advances exactly one turn — no double-moves on a slow frame, no dropped presses on a fast one (automated: `tests/roguelike_floor1.rs`)
- [ ] Enemies visibly act on the frame(s) *after* your turn, not the same frame — should read as "they wait for you," not simultaneous. As of Step 5f each enemy resolves its own turn on its own frame (`TurnScheduler`'s "one actor per step"), so a floor with several enemies takes that many extra frames to finish a round — at 60fps this should still read as instantaneous, not as a visible stagger
- [✓] An asleep enemy (no line of sight yet, Step 4h's amendment) visibly does nothing until it wakes — tinted differently while asleep vs. awake (`DarkRed`/`DarkMagenta` vs `Red`/`Magenta`)
- [✓] Waiting (Space) and quaffing (Q) both visibly cost a turn, same as moving does
- [ ] Death screen appears the instant hp reaches 0; R restarts from floor 1
- [ ] A rat/boss's move visibly slides one cell rather than teleporting (Phase 5.5 Part 3's animation queue, `docs/ember2d-phase5.5-plan.md` — `enemy.rhai` calls `ctx.animate_move` alongside `ctx.set_position`); the player's own movement is deliberately left un-animated (instant, as before) — not a bug if it looks different from an enemy's move
- [ ] The SAME actor's next turn does not advance while its own move animation is still playing (no double-move, no enemy acting twice on top of itself) — automated: `tests/turn_animation.rs`
- [ ] A DIFFERENT actor's turn resolves promptly even while another entity's animation is still draining — on floor2 (3 rats), the player should regain control almost immediately after moving, not feel a stall/freeze that scales with enemy count (D20, fixed in Phase 6 — automated: `tests/turn_animation.rs::two_actors_animations_overlap_instead_of_stacking`)
- [ ] A movement key tapped while the front actor's animation is playing still registers once it's your turn again — it must not need a second press (D19, fixed in Phase 6)

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
- [ ] Loading a save resumes at the right level with entities intact, including per-entity global state (`hp_<id>`, `aware_<id>`, etc.) — **D17 fixed in Phase 5 Step 5c** (docs/ember2d-phase5-plan.md): `SaveState` now carries `globals`/`clips` too, restored directly by `PlayState::from_save` without re-running any script's `on_start`. Automated: `tests/save_load_globals.rs`.
- [ ] `on_start`, `on_input`, `on_update`, `on_turn`, `on_collide` all fire; missing ones don't error
- [ ] **Corrected in Step 4k** — deleted a wrong item that used to read "Per-entity scope persists across frames; removed on despawn." Rhai does not provide this: a script's own `let` does *not* survive between calls (`CallFnOptions::rewind_scope: true`) — see `ember2d-scripting-api.md`'s "Per-entity scope" section, also corrected this step. What actually needs checking instead: per-entity state kept in globals/persistent (e.g. `"hp_" + id`) survives across frames and is cleaned up on despawn (nothing explicitly clears these keys today, but a despawned entity's id is never reused within a level, so stale keys are harmless, not a leak that matters).
- [ ] Hot-reload recompiles on file change and logs
- [ ] Compile errors show the file name; runtime errors log once

Exercise at least one function from every API group (see `ember2d-scripting-api.md` §3):
- [ ] Transform · [ ] Tags · [ ] Glyph/colour/texture/animation · [ ] Input · [ ] Gamepad
- [ ] Globals · [ ] Persistence · [ ] Timers · [ ] RNG · [ ] Spatial queries
- [ ] Raycast · [ ] Pathfinding · [ ] Camera · [ ] Mouse · [ ] HUD widgets
- [ ] Particles · [ ] Audio (incl. spatial) · [ ] Hierarchy · [ ] Save/load · [ ] Turn

## 14. Known defects

**Corrected in 7A-6** (docs/ember2d-master-plan.md §5.1, R36): this used to
keep its own copy of every defect's fixed/unfixed status, which is exactly
the kind of duplicate bookkeeping that drifted from the tree (this table
had gone stale itself before the fix). `docs/ember2d-master-plan.md` §3 is
the one defect register — D1–D22, R1–R40, E1–E6, each with a status marker
(`[x]` fixed, `[~]` in progress, `[ ]` not started, `[-]` dropped) and the
commit or step that fixes it. Check a row there before assuming a manual
test failure is a new regression rather than a known, already-tracked one.

## 15. Determinism / replay gate (Phase 5 Step 5h+)

**Phase 5.5** (docs/ember2d-phase5.5-plan.md Part 1) added CI
(`.github/workflows/ci.yml`, `windows-latest` + `ubuntu-latest`) that ran
`tests/replay.rs` on every push. That config was deleted at some point
before the 2026-09-06 review (R37) and stayed gone until master plan step
7A-7 restored it — the current `.github/workflows/ci.yml` runs
`cargo test --workspace` (which includes `tests/replay.rs` once) and then
`cargo test --test replay` two more times as independent fresh processes
(3× total per OS per push), plus `scripts/check.ps1`/`check.sh`. That is
still short of the 5×-independent-fresh-process discipline below, which
stays a **manual gate**: run it explicitly before trusting a Phase 5+
change that touches sim ordering, deferred writes, or RNG — CI's 3× isn't
sufficient proof for that class of change on its own. `tests/common/mod.rs`'s
`TurnHarness` was also rewritten in Phase 5.5 to drive
`ember2d_sim::simulation::Simulation` directly, with no
`InputManager`/`MouseState`/`GamepadState`/winit `Key` anywhere in it — a
simplification, not a behavior change; nothing in this section's checklist
changes because of it.

- [ ] `cargo test --test replay` passes as 5 independent fresh process runs
      (not `--test-threads=1` reruns within one process — the point is
      catching anything that could only ever vary *between* processes, e.g.
      a stray `HashMap` reintroduced somewhere)
- [ ] If it fails, the reported checkpoint action number narrows down where
      to look before diffing the full RON dump by hand

## 17. Performance baseline (Phase 6, docs/ember2d-phase6-plan.md)

`ember2d-sim/examples/bench_sim.rs` — a headless, sim-only benchmark (no
wgpu/winit/kira in its build), run via
`cargo run --release -p ember2d-sim --example bench_sim` from the repo root.
Not a CI gate (shared runners make ms numbers noise) — a manual perf check
to re-run after any change touching `Simulation::step`/`late_step`,
`WorldSnapshot`, or `World::detect_collisions`, and compare against the
baseline table in `docs/ember2d-phase6-plan.md` §1.

- [x] `cargo run --release -p ember2d-sim --example bench_sim` runs clean
      (warns loudly if accidentally run in debug)
- [x] allocs/step at floor2 has dropped from the ~35,300 baseline (Steps
      3/4/5/6/9 land) — not required to hit a specific number, but should be
      a clear, large drop, not noise. **Final: 35,299 → 6,598 (-81%),
      measured after Step 8** (Steps 9-12 don't touch anything `bench_sim`'s
      counting allocator would see move — see their own write-ups in
      `docs/ember2d-phase6-plan.md`).
- [x] The synthetic n=500 vs. n=2000 allocs/step ratio is closer to 1:1 than
      the ~3.6× baseline, once the non-collision per-step allocation stops
      scaling with entity count. **Final: 3.18× after Step 7** (down from
      3.61× at baseline) — closer to flat but not fully there, since
      `WorldSnapshot::build` is still `O(entities)` by nature, just far
      cheaper per entity; the O(colliders²) term that used to dominate that
      ratio at scale is gone as of Step 8.
- [x] `detect_collisions`'s share of total step time has fallen sharply at
      every scale after the collision-layer bitmask (Step 7) and
      sweep-and-prune (Step 8) land, most visibly at the 10,000-entity
      synthetic level. **Final: floor2 65% → 17%; n=10,000 synthetic 86% →
      22%.**
- [x] Manual: `cargo run -- demos/roguelike/floor2.level` with the F3 debug
      overlay feels smooth (informal cross-check against the bench numbers,
      not a replacement for them — the bench doesn't see the render path).
      **Confirmed at Step 13's conditional check: F3 overlay read `FPS:59`
      on an unoptimized-own-code debug build (`cargo build`/`cargo run`),
      effectively pegged at the engine's 60fps cap — this is what closed
      Step 13 (`DrawList` buffer reuse) as skipped rather than built.**

## 18. Before you start

- [ ] Merge/rename so there is one trunk branch
- [ ] Port `demo/` forward from `main`; confirm all three levels load and play
- [ ] Screen capture: start screen → new project → paint → save → F5 → play → pause → editor → script editor → graph editor
- [ ] Copy `demo/` outside the repo as a migration reference
- [ ] Tag `v0.5.0-pre-refactor`
