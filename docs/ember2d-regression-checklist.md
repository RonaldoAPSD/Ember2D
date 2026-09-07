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

**Before first use:** open `roguelike/floor1.level` (or `--editor` it) —
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
- [ ] Save, close, and reopen a level whose tiles use non-default collider
      layers/masks — layer/mask filtering behavior survives the round trip
      unchanged (Phase 6 Step 7's bitmask is `#[serde(skip)]` and only
      recomputed from the saved layer/mask strings on load —
      `ember2d/tests/collision_layers.rs`'s save→load test is the automated
      form of this check; this is the manual editor-side cross-check)
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
- [ ] Save, Save-As, New, Open, Close Project — **known gap, see D18** (§14): saving scrambles the level's tile order (`LevelGrid.tiles` is a `HashMap`, not sorted before writing `LevelData.tiles`). Not a reason to fail this item — tile *content* survives correctly, only *order* is unspecified — but don't use an editor-saved level to check for a clean diff, and re-run `cargo run --example gen_roguelike` if a `roguelike/*.level` file gets touched by the editor.

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
- [ ] An asleep enemy (no line of sight yet, Step 4h's amendment) visibly does nothing until it wakes — tinted differently while asleep vs. awake (`DarkRed`/`DarkMagenta` vs `Red`/`Magenta`)
- [ ] Waiting (Space) and quaffing (Q) both visibly cost a turn, same as moving does
- [ ] Death screen appears the instant hp reaches 0; R restarts from floor 1
- [ ] A rat/boss's move visibly slides one cell rather than teleporting (Phase 5.5 Part 3's animation queue, `docs/ember2d-phase5.5-plan.md` — `enemy_rat.rhai`/`enemy_boss.rhai` call `ctx.animate_move` alongside `ctx.set_position`); the player's own movement is deliberately left un-animated (instant, as before) — not a bug if it looks different from an enemy's move
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
- [x] Manual: `cargo run -- roguelike/floor2.level` with the F3 debug
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
