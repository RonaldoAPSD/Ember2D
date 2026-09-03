# Ember2D — Claude Code Instructions

## Project Overview

Ember2D is a 2D ASCII game engine and editor built in Rust. It uses a character-cell aesthetic rendered into a raw pixel buffer via `minifb`, with `font8x8` for glyph rendering, `rhai` for game scripting, and `kira` for audio.

**Current version:** 0.4.4
**Target:** v0.5.0 (see roadtoV0.5.md for phased plan)

## Build & Run

```
cargo run                                      # Launch editor (start screen)
cargo run -- <path/to/level.level>             # Play a level directly
cargo run -- --editor <path/to/level.level>    # Open a level in the editor
```

## Architecture

- **ECS-ish world** — `src/world.rs`: entities are `u64`, components stored in `HashMap`s in `src/components/`
- **Renderer** — `src/renderer/`: raw `u32` framebuffer, double-buffered diff, 8×8 bitmap glyphs
- **Level format** — RON serialization via `serde`/`ron`, files end in `.level`
- **Scripting** — Rhai embedded in `src/scripting/`; scripts live in `demo/scripts/` and `scripts/`
- **Editor** — `src/editor/`: dockable panels, tile palette, node-based visual scripter (generates Rhai), undo/redo stack
- **Audio** — `src/audio.rs` wraps `kira`; assets are `.ogg`/`.mp3`
- **App mode switching** — `src/app.rs` and `src/main.rs`
- **Prelude** — `use ember2d::prelude::*;` imports all common types

## Key Source Locations

| Area | Path |
|------|------|
| Core engine loop | src/engine.rs |
| World / ECS | src/world.rs |
| Components | src/components/ |
| Renderer | src/renderer/ |
| Scripting API | src/scripting/ |
| Editor core | src/editor/mod.rs |
| Editor panels & UI | src/editor/ui/ |
| Editor input | src/editor/input/ |
| Node graph (visual scripting) | src/editor/node_graph/ |
| Start screen | src/editor/start_screen/ |
| Math utilities | src/math.rs |
| Level serialization | src/level.rs |
| Audio | src/audio.rs |

## Development Rules

- **File size hard limit:** No `.rs` file may exceed **600 lines**. Split into sub-modules when approaching the limit.
- **No redundant features:** Don't implement half-finished features; complete or skip.
- **Performance:** The renderer blits a raw `u32` buffer every frame — keep per-frame work lean.
- **Error handling:** Use `Result` and `eprintln!` for warnings; prioritize editor stability.
- **UI/Editor changes:** Be mindful of the panel docking system in `src/editor/panel.rs` — panels must not render over the menu bar dropdowns.
- **Painting guard:** Any mouse interaction must be guarded so menu/panel clicks don't bleed into the canvas paint tool.

## Current State (as of 2026-05-09)

All 16 items in updates.txt are complete, notably:

- Full scripting API implemented (`src/scripting/`)
- Editor refactored — all files under 600 lines
- Start screen is dynamic
- Scroll wheel is zoom in/out (with reset zoom button)
- Help menu is up to date
- Fixed: paint bleed, panel z-order, copy/cut select, draw-outside-map, mouse in menus

## Known Issues

See `Issues.txt` in project root for the current bug tracker.

## Roadmap

See `roadtoV0.5.md` — next phases include documentation, editor polish, visual effects, UI, AI/pathfinding, audio, and roguelike toolkit features targeting v0.5.0.
