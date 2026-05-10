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

## v0.4.4: The Project Wizard & UI Update
*Focus: Setting the foundation for multiple engine modes and UI rendering.*
*   **Project Wizard Overhaul:** A step-by-step setup when creating a new project.
    *   **Visual Style:** Choose between Classic ASCII or 2D Sprites.
    *   **Gameplay Loop:** Choose between Real-time or Turn-based.
*   **Project Configuration:** Saving these choices into `project.ron` to govern engine behavior.
*   **UI Components:** Built-in drawing tools for menus, panels, and selectable lists.

## v0.4.5: The Logic & AI Update
*Focus: Deepening the gameplay possibilities.*
*   **Pathfinding API:** Built-in A* pathfinding (`ctx.get_path(start, target)`) exposed to Rhai.
*   **Raycasting:** `ctx.raycast()` to easily check line-of-sight for enemy vision.
*   **Collision Masks:** Define exactly what collides with what (e.g., "Arrows hit enemies but pass through items").

## v0.4.6: The Audio & Export Update
*Focus: Immersion and getting the game to players.*
*   **Spatial Audio:** Sounds get quieter the further away the entity is from the camera.
*   **Export Tools:** A simple CLI command or editor button to bundle the project into a distributable standalone folder.

## v0.4.7: The Render Abstraction Update
*Focus: Engine architecture prep for non-ASCII rendering.*
*   **Render Traits:** Abstracting the custom pixel-buffer renderer to prepare for the "2D Sprites" mode chosen in the project wizard.
*   **Asset Management:** Support for loading actual image textures (PNGs) into memory.

## v0.4.8: The Simulation Core Update
*Focus: Engine architecture prep for the Turn-Based loop.*
*   **Decoupling Logic & Frame Rate:** Ensuring physics, timers, and input can be paused while waiting for a turn.
*   **Script Queuing:** Refactoring how Rhai scripts execute so they can run sequentially rather than concurrently every frame.

## v0.4.9: Pre-Release Polish
*Focus: Stability before the major milestone.*
*   **Bug Squashing & Profiling:** Dedicated performance profiling and fixing edge cases introduced during the v0.4.x cycle.
*   **API Finalization:** Ensuring all Scripting API methods are consistent and fully documented.

## v0.5.0: The Scripting & Workspace Update
*Focus: Professional in-engine development tools transforming the editor into a complete workspace.*
*   **In-Engine Script Editor:** A dedicated multi-line text editor panel built directly into the engine, allowing you to create, edit, and save `.rhai` scripts without leaving the workspace.
*   **Project File Manager:** A new browser panel to manage all assets within a project folder. Easily switch between editing `.level` files and `.rhai` scripts, and handle file operations (create, rename, delete) natively.
*   **Panel System Expansion:** Expanding the existing docking/floating window system to accommodate these new large-format panels and manage workflow focus.
