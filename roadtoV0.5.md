# Ember2D Roadmap: v0.4.1 to v0.5.0

## DONE v0.4.1: The Documentation & API Update
*Focus: Finalizing the core API and making the engine accessible to users.*
*   **Full Scripting API Implementation:** Ensure all core engine features are exposed to Rhai.
*   **"Your First Game" Tutorial:** A step-by-step guide for new developers.
*   **API Reference Site:** Comprehensive local documentation in `index.html`.

## DONE v0.4.2: The Editor Polish Update
*Focus: Robust tools and quality-of-life for the developer.*
*   **Undo / Redo System:** Essential for both the level painter and the node graph.
*   **Node Graph UX:** Wire routing/snapping, box-selection for moving multiple nodes, and copy/paste functionality.
*   **Visual Tile Layers:** Dedicated Editor layers (Background, Foreground, Logic/Triggers) to keep complex levels organized.

## DONE v0.4.2.5: The Palette Overhaul
*Focus: Dynamic asset management.*
*   **Persistent Custom Palettes:** Palettes are now saved to `project.palette.ron` for each project.
*   **In-Editor Creation:** `[+ New Item]` button to expand the palette on the fly.
*   **Integrated Palette Editor:** The Inspector handles palette definition editing (name, glyph, fg/bg color cycling) when no grid tile is selected.
*   **UI Polish:** Added mouse-wheel scrolling to the palette panel.

## DONE v0.4.3: The Workspace UI Overhaul
*Focus: Professional window-based aesthetics and interactive resizing.*
*   **Panel Borders:** Full ASCII box borders around all workspace panels.
*   **Interactive Resizing:** Draggable handles in the bottom-right corner of every panel.
*   **Standardized Layout:** Unified content area calculations for more reliable panel spacing.
*   **Modal Color Picker:** A dedicated 16-color grid modal for precise asset customization.

## DONE v0.4.3.5: The Visual Polish Update
*Focus: Making the games look dynamic and alive without writing complex scripts.*
*   **Sprite/Glyph Animation:** Engine support for cycling characters (e.g., `|`, `/`, `-`, `\`) over time.
*   **Particle API:** `ctx.emit_particles(x, y, glyph, color)` for short-lived ASCII effects (dust, hits, magic).
*   **Advanced Camera:** Camera boundaries (stop panning at map edges) and smooth lerping/following.

## DONE v0.4.4: The Project Wizard & UI Update
*Focus: Setting the foundation for multiple engine modes and UI rendering.*
*   **Project Wizard Overhaul:** A step-by-step setup when creating a new project.
    *   **Visual Style:** Choose between Classic ASCII or 2D Sprites.
    *   **Gameplay Loop:** Choose between Real-time or Turn-based.
*   **Project Configuration:** Saving these choices into `project.ron` to govern engine behavior.
*   **UI Components:** Built-in drawing tools for menus, panels, and selectable lists.

## DONE v0.4.5: The Logic & AI Update
*Focus: Deepening the gameplay possibilities.*
*   **Pathfinding API:** Built-in A* pathfinding (`ctx.get_path(start, target)`) exposed to Rhai. Returns coordinates for smarter entity movement.
*   **Raycasting:** `ctx.raycast(x1, y1, x2, y2, mask)` to check line-of-sight for enemy vision or projectile paths.
*   **Collision Masks:** Defined exactly what collides with what. Colliders now have `layer` and `mask` for fine-grained interaction.
*   **Persistence:** Level files now store collision settings for tiles and the player.

## DONE v0.4.6: The Audio & Export Update
*Focus: Distribution and atmosphere.*
*   **Spatial Audio:** New `ctx.play_sound_at(path, x, y)` API. The engine automatically attenuates volume based on distance to the camera (linear falloff, 20-unit radius).
*   **Standalone Export Tool:** Integrated `File -> Export Game...` tool in the editor.
    *   Automatically bundles assets (`audio/`, `scripts/`), levels, and `project.ron`.
    *   Copies and renames the engine executable to the project name.
    *   Creates a `.standalone` marker for zero-config distribution.
*   **Direct Boot:** The engine now detects standalone mode and launches directly into the game, bypassing all editor UI.

## DONE v0.4.7: The Render Abstraction Update
*Focus: Engine architecture prep for non-ASCII rendering.*
*   **Render Backend Trait:** Created the `RenderBackend` trait and moved character rasterization into `AsciiBackend`.
*   **Asset Manager:** Implemented `Texture` loading (PNG support) and `AssetManager` for caching textures in memory.
*   **2D Sprite Support:** Updated the `Sprite` component and `PlayState` to support drawing textures instead of just characters.
*   **Backend Switching:** The engine now automatically selects between `AsciiBackend` and `SpriteBackend` based on project settings.

## DONE v0.4.8: The Simulation Core Update
*Focus: Engine architecture prep for the Turn-Based loop.*
*   **Decoupling Logic & Frame Rate:** Ensuring physics, timers, and input can be paused while waiting for a turn.
*   **Script Queuing:** Refactoring how Rhai scripts execute so they can run sequentially rather than concurrently every frame.

## DONE v0.4.9: Pre-Release Polish
*Focus: Stability before the major milestone.*
*   **Bug Squashing & Profiling:** Fixed Turn-Based physics integration and optimized AssetManager.
*   **API Finalization:** Added missing getters/setters for all entity properties (position, velocity, glyph, color, tag, collider).
*   **Documentation:** Fully updated `index.html` with the current 0.4.9 API reference.

## DONE v0.5.0: The Scripting & Workspace Update
*Focus: Professional in-engine development tools transforming the editor into a complete workspace.*
*   **In-Engine Script Editor:** A dedicated multi-line text editor panel built directly into the engine, allowing you to create, edit, and save `.rhai` scripts without leaving the workspace. Includes navigation, typing, and keyboard shortcuts (`Ctrl+S`).
*   **Project File Manager:** A new browser panel to manage all assets within a project folder. Easily switch between editing `.level` files and `.rhai` scripts, and handle file operations (create script) natively.
*   **Panel System Expansion:** Expanded docking/floating window system to support the new workspace panels. Added focus tracking for keyboard input routing.
