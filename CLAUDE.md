# Ember2D — Claude Code Instructions

## Project Overview

Ember2D is a 2D/ASCII game engine and editor in Rust. GPU rendering via `wgpu` with
`winit` windowing, `font8x8` glyph atlas, `rhai` scripting, `kira` audio, `gilrs` gamepad.

**Current version:** 0.5.0
**Status:** mid-refactor — read `docs/refactor-plan.md` before writing code.

## Documentation — read before starting work

| Document | Purpose |
|---|---|
| `docs/refactor-plan.md` | Phases, architecture decisions, known defects (§3). **Read the phase you're working on before writing any code.** |
| `docs/scripting-api.md` | The Rhai API. This is the engine's real public contract — treat breaking it like breaking the level format. |
| `docs/regression.md` | Manual test checklist. Run before declaring a phase done. There are no automated tests until Phase 5. |
| `docs/archive/roadmap-v0.6.md` | Superseded, kept for detail behind the V0.5.x/V0.6 editor items. |

## Build & Run

```
cargo run                                      # Launch editor (start screen)
cargo run -- <path/to/level.level>             # Play a level directly
cargo run -- --editor <path/to/level.level>    # Open a level in the editor
```

## Architecture

- **ECS-ish world** — `src/world.rs`: entities are `u64`, components in `HashMap`s under `src/components/`. Supports parenting via `Transform.parent`.
- **Renderer** — `src/renderer/`: `wgpu` instanced quads, batching by texture, font atlas built at startup. `RenderBackend` trait with `WgpuBackend`.
  **Note:** the public draw API is still cell-based (`draw_char(x, y)` in cells). World-space rendering and a real camera arrive in Phase 2.
- **Engine loop** — `src/engine.rs`: `winit` via `pump_events`, fixed-timestep accumulator, state stack (push/pop/pause/resume).
- **Level format** — RON via `serde`/`ron`, `.level` files. No version field yet (added in Phase 3).
- **Scripting** — `src/scripting/`: ~95 registered functions. Deferred mutation queue.
- **Editor** — `src/editor/`: layer-aware grid, float zoom, dockable panels, palette editor, in-engine script editor, node-based visual scripter (generates Rhai).
- **Save system** — `src/save.rs`: full `World` serialization.
- **Prelude** — `use ember2d::prelude::*;`

## Key Source Locations

| Area | Path |
|------|------|
| Core engine loop | src/engine.rs |
| World / ECS | src/world.rs |
| Components | src/components/ |
| Renderer | src/renderer/ |
| GPU backend + shader | src/renderer/backend.rs, shader.wgsl |
| Scripting API | src/scripting/api.rs |
| Scripting engine | src/scripting/engine.rs |
| Play mode | src/play.rs, src/play/spawn.rs |
| Editor core | src/editor/mod.rs, impl_state.rs |
| Editor panels & UI | src/editor/ui/ |
| Editor input | src/editor/input/ |
| Node graph | src/editor/node_graph/ |
| Start screen | src/editor/start_screen/ |
| Level serialization | src/level.rs |
| Project settings | src/project.rs |
| Math utilities | src/math.rs |

## Development Rules

- **File size hard limit:** no `.rs` file may exceed **600 lines**. Split into sub-modules when approaching it.
- **One phase per branch, one logical change per commit.** A large mechanical refactor reviewed as a single diff will not be reviewed at all.
- **Preserve the comment style.** This codebase is heavily commented as a deliberate learning artifact. When code changes, *rewrite the comment to match* — never delete it, never leave one describing behaviour that no longer exists.
- **The engine must run at the end of every phase.** If a phase can't land working, split it.
- **Run the regression checklist** before declaring a phase done.
- **Don't opportunistically refactor** outside the current phase. Note it and move on.
- **Ask before changing public API shape** — scripts and level files depend on it.
- **No half-finished features.** Complete or skip.
- **UI/editor changes:** be mindful of the docking system in `src/editor/panel.rs` — panels must not render over menu-bar dropdowns.
- **Painting guard:** any mouse interaction must be guarded so menu/panel clicks don't bleed into the canvas paint tool.
- **Error handling:** `Result` plus `eprintln!` for warnings; prioritize editor stability. A broken script should never take down the editor.

## Determinism (matters from Phase 5 onward)

The simulation must be reproducible — replay, save/load, and 2-player netcode all depend on it.

- **No `HashMap` iteration in simulation code.** Order varies between processes and will desync two machines. Use `BTreeMap` or collect-and-sort by `EntityId`.
- **No ambient randomness or wall-clock time in game logic.** RNG is world-owned and seeded.
- **Avoid `sin`/`cos`/`atan2`/`exp`/`powf` in the sim** — platform libm differs. See `docs/refactor-plan.md` §5.2.

## Current State

Branch `gemini` is trunk at v0.5.0. `main` is stale at v0.3.4.

Known defects are catalogued in `docs/refactor-plan.md` §3 (D1–D14) and mirrored in
`docs/regression.md` §14 — check there before assuming something is a new regression.
