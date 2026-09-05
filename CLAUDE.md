# Ember2D — Claude Code Instructions

## Project Overview

Ember2D is a 2D/ASCII game engine and editor in Rust. GPU rendering via `wgpu` with
`winit` windowing, `font8x8` glyph atlas, `rhai` scripting, `kira` audio, `gilrs` gamepad.

**Current version:** 0.5.0
**Status:** mid-refactor — read `docs/ember2d-refactor-plan.md` before writing code.

## Documentation — read before starting work

| Document | Purpose |
|---|---|
| `docs/ember2d-refactor-plan.md` | Phases, architecture decisions, known defects (§3). **Read the phase you're working on before writing any code.** |
| `docs/ember2d-scripting-api.md` | The Rhai API. This is the engine's real public contract — treat breaking it like breaking the level format. |
| `docs/ember2d-regression-checklist.md` | Manual test checklist. Run before declaring a phase done. There are no automated tests until Phase 5. |
| `docs/archive/roadmaptoV0.6.md` | Superseded, kept for detail behind the V0.5.x/V0.6 editor items. |

## Build & Run

```
cargo run                                      # Launch editor (start screen)
cargo run -- <path/to/level.level>             # Play a level directly
cargo run -- --editor <path/to/level.level>    # Open a level in the editor
```

Same commands as always — Phase 5 Step 5i (docs/ember2d-phase5-plan.md §5.5)
split this into a Cargo **workspace** (see "Workspace layout" below), but
there's still exactly one bin target (`ember2d-app`, `[[bin]] name =
"ember2d"`), so plain `cargo run` from the repo root still resolves
unambiguously and still produces `target/debug/ember2d.exe`.

## Workspace layout

Four member crates (Phase 5 Step 5i, docs/ember2d-phase5-plan.md §5.5) — see
each crate's own `src/lib.rs` for the full reasoning, including the two
prep gaps that step found beyond what the plan called "mechanical" (a
`Color` import path most sim-side files never updated after Step 5a moved
the type, and `scripting` needing `MouseSnapshot`/`GamepadSnapshot` —
sim-safe twins of `MouseState`/`GamepadState` — so it never references
`winit`/`gilrs` types directly):

| Crate | Contains | Depends on |
|---|---|---|
| `ember2d-sim/` | math, color, world, components, level, save, scripting, command, scheduler, graph, event. **No renderer, window, or input-device dependency anywhere in its tree** — `cargo build -p ember2d-sim` succeeding with that property is the constraint this split exists to enforce (docs/ember2d-refactor-plan.md §5.5). | — |
| `ember2d/` | engine, renderer, input, mouse, gamepad, audio, play, project, camera, `sim.rs` (the per-step orchestration loop — see that file's own doc comment for why it's here, not in `ember2d-sim`, despite the name). | `ember2d-sim` |
| `ember2d-editor/` | The level/script editor (`editor/`). | `ember2d-sim`, `ember2d` |
| `ember2d-app/` | `main.rs` + `app.rs` (Editor↔Play orchestration — lives here, not in `ember2d`, because it's the one place needing both `PlayState` and `EditorState`, and `ember2d` can't depend on `ember2d-editor` without the workspace cycling). | `ember2d`, `ember2d-editor` |

`ember2d`'s `prelude` re-exports `ember2d-sim` types alongside its own, so
`use ember2d::prelude::*;` still gives one-shot access to everything a game
(or a test) needs — exactly the pre-split experience. It does **not**
re-export `EditorState`/`run_editor_app`/`run_play_app`/`StartScreen` —
those aren't reachable from `ember2d` at all anymore; use
`ember2d_editor::prelude` and `ember2d-app`'s own `app` module.

`roguelike/` and `docs/` stay at the **repo root**, not inside any crate —
`cargo run`'s CWD is wherever it's invoked from (the repo root, by
convention), so `cargo run -- roguelike/floor1.level` keeps working
unchanged. `cargo test`, however, runs each integration test binary with
CWD set to *that package's own directory* (`ember2d/`, one level below the
repo root) — `ember2d/tests/common/mod.rs`'s `ensure_workspace_root_cwd`
fixes this for every test that loads real level content; a level-path
constant used directly (not through `TurnHarness`) should be
`concat!(env!("CARGO_MANIFEST_DIR"), "/../roguelike/...")`, not a bare
`"roguelike/..."` literal — see `ember2d/tests/replay.rs`'s own comment on
this for the full explanation.

## Architecture

- **ECS-ish world** — `ember2d-sim/src/world.rs`: entities are `u64`, components in `BTreeMap`s under `ember2d-sim/src/components/` (`HashMap` until Step 5b's determinism pass — docs/ember2d-phase5-plan.md §5.2 H1). Supports parenting via `Transform.parent`.
- **Renderer** — `ember2d/src/renderer/`: `wgpu` instanced quads, batching by texture, font atlas built at startup. `RenderBackend` trait with `WgpuBackend`.
  Phase 2 added `ember2d/src/camera.rs` (`Camera`: world<->screen, zoom, viewport origin) and world-space entry points
  (`Renderer::draw_char_world`/`draw_texture_world`), plus a `DrawList` sorted by `(space, z, texture)` for batching.
  `draw_char(x, y)` (cells) still works unchanged — the editor, HUD chrome, and node graph stay screen-space only
  until Phase 7.
- **Engine loop** — `ember2d/src/engine.rs`: `winit` via `pump_events`, fixed-timestep accumulator, state stack (push/pop/pause/resume). The actual per-step sequence (consume input, update, conditionally physics/collisions/late_update) lives in `ember2d/src/sim.rs` (Step 5d) — shared with the headless `TurnHarness` test harness so there's only one copy of it.
- **Level format** — RON via `serde`/`ron`, `.level` files, `LEVEL_FORMAT_VERSION` (currently 2 — Step 5f added `TileRecord::actor`).
- **Scripting** — `ember2d-sim/src/scripting/`: ~100 registered functions, deferred mutation queue. `on_input`/`on_turn` (Phase 5 Steps 5e/5f) split "read input" from "act" from the older `on_update`/`on_start`/`on_collide` lifecycle — see `docs/ember2d-scripting-api.md`.
- **Turn scheduling** — `ember2d-sim/src/scheduler.rs`: `TurnScheduler`, a deterministic min-heap turn queue (Step 5f).
- **Editor** — `ember2d-editor/src/editor/`: layer-aware grid, float zoom, dockable panels, palette editor, in-engine script editor, node-based visual scripter (generates Rhai via `ember2d-sim/src/graph/`).
- **Save system** — `ember2d-sim/src/save.rs`: full `World` serialization, plus script `globals`/`clips` (Step 5c, D17 fix).
- **Prelude** — `use ember2d::prelude::*;` (see "Workspace layout" above for what it does and doesn't re-export).

## Key Source Locations

| Area | Path |
|------|------|
| Core engine loop | ember2d/src/engine.rs |
| Shared per-step simulation sequence | ember2d/src/sim.rs |
| Turn scheduler | ember2d-sim/src/scheduler.rs |
| World / ECS | ember2d-sim/src/world.rs |
| Components | ember2d-sim/src/components/ |
| Renderer | ember2d/src/renderer/ |
| GPU backend + shader | ember2d/src/renderer/backend.rs, shader.wgsl |
| Scripting API | ember2d-sim/src/scripting/api.rs |
| Scripting engine | ember2d-sim/src/scripting/engine.rs |
| Commands / input snapshots | ember2d-sim/src/command.rs |
| Play mode | ember2d/src/play.rs, ember2d/src/play/spawn.rs |
| Editor core | ember2d-editor/src/editor/mod.rs, impl_state.rs |
| Editor panels & UI | ember2d-editor/src/editor/ui/ |
| Editor input | ember2d-editor/src/editor/input/ |
| Node graph (data + codegen) | ember2d-sim/src/graph/ |
| Node graph editor UI | ember2d-editor/src/editor/graph_ui.rs |
| Start screen | ember2d-editor/src/editor/start_screen/ |
| Level serialization | ember2d-sim/src/level.rs |
| Project settings | ember2d/src/project.rs |
| Math utilities | ember2d-sim/src/math.rs |
| Top-level Editor↔Play orchestration | ember2d-app/src/app.rs |

## Development Rules

- **File size hard limit:** no `.rs` file may exceed **600 lines**. Split into sub-modules when approaching it.
- **One phase per branch, one logical change per commit.** A large mechanical refactor reviewed as a single diff will not be reviewed at all.
- **Preserve the comment style.** This codebase is heavily commented as a deliberate learning artifact. When code changes, *rewrite the comment to match* — never delete it, never leave one describing behaviour that no longer exists.
- **The engine must run at the end of every phase.** If a phase can't land working, split it.
- **Run the regression checklist** before declaring a phase done.
- **Don't opportunistically refactor** outside the current phase. Note it and move on.
- **Ask before changing public API shape** — scripts and level files depend on it.
- **No half-finished features.** Complete or skip.
- **UI/editor changes:** be mindful of the docking system in `ember2d-editor/src/editor/panel.rs` — panels must not render over menu-bar dropdowns.
- **Painting guard:** any mouse interaction must be guarded so menu/panel clicks don't bleed into the canvas paint tool.
- **Error handling:** `Result` plus `eprintln!` for warnings; prioritize editor stability. A broken script should never take down the editor.

## Determinism (matters from Phase 5 onward)

The simulation must be reproducible — replay, save/load, and 2-player netcode all depend on it.

- **No `HashMap` iteration in simulation code.** Order varies between processes and will desync two machines. Use `BTreeMap` or collect-and-sort by `EntityId`.
- **No ambient randomness or wall-clock time in game logic.** RNG is world-owned and seeded.
- **Avoid `sin`/`cos`/`atan2`/`exp`/`powf` in the sim** — platform libm differs. See `docs/ember2d-refactor-plan.md` §5.2.

## Current State

`main` is trunk at v0.5.0 (`gemini` was merged in before this refactor started). Refactor work
happens on the `claude` branch. Phases 0–2 are done: demo content recovered, the D1–D14 defect
sweep closed except D7/D11 (deferred to Phases 5/6 by design), and Phase 2's world-space camera
landed. Phase 3 (sprite/asset model) is next.

Known defects are catalogued in `docs/ember2d-refactor-plan.md` §3 (D1–D14) and mirrored in
`docs/ember2d-regression-checklist.md` §14 — check there before assuming something is a new
regression, but note most are already fixed as of the `claude` branch; the plan doc doesn't
track per-defect status itself, so verify against the actual code/tests if in doubt.
