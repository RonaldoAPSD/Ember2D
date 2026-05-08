# Ember2D Project Instructions

Ember2D is a 2D game engine and editor built with Rust, focusing on a character-cell based aesthetic with a pixel-buffer renderer.

## Project Overview

- **Core Technology**: Rust, `minifb` for windowing and framebuffer, `rhai` for scripting, `kira` for audio.
- **Renderer**: Custom pixel-buffer renderer in `src/renderer/`. It uses `font8x8` to render text characters into the buffer.
- **Editor**: Built-in level editor located in `src/editor/`.
- **Scripting**: Rhai scripts (located in `demo/scripts/` and `scripts/`) provide entity logic.

## Build and Run

- **Launch Editor**: `cargo run`
- **Play Level**: `cargo run -- <path/to/level.level>`
- **Edit Level**: `cargo run -- --editor <path/to/level.level>`

## Architecture & Conventions

- **Prelude**: Use `use ember2d::prelude::*;` to import common types.
- **ECS-ish**: The engine uses a simple entity database in `src/world.rs` with components in `src/components/`.
- **Mode Switching**: Handled in `src/app.rs` and `src/main.rs`.

## Development Guidelines

- **File Size Limit**: Each source file (`.rs`) MUST NOT exceed 600 lines of code. Prefer splitting into sub-modules.
- **UI/Editor Changes**: Be mindful of the docking system and panel management in `src/editor/`.
- **Performance**: The renderer blits a raw `u32` buffer; keep operations within the frame budget.
- **Error Handling**: Use `Result` and `eprintln!` for warnings; prioritize stability in the editor.
