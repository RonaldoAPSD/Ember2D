# Roadmap to V0.6.0: The Creator Experience Update

This roadmap outlines the evolution of Ember2D from V0.5.1 into the V0.6.0 "Creator Update," focusing on professionalizing the Editor UI and enhancing workflow efficiency.

---

## V0.5.x: The UI & Functionality Cycle

### V0.5.2: Contextual Workflow
- [ ] **File Manager Right-Click**: Create new `.level`, `.rhai`, or folders directly from the browser.
- [ ] **Tab Context Menus**: Right-click tabs to "Close", "Close Others", or "Float".
- [ ] **Hierarchy Right-Click**: "Teleport Camera to", "Duplicate", or "Delete" entities.

### V0.5.3: Viewport & Navigation Polish
- [ ] **Smooth Camera**: Implement eased lerping for camera movements.
- [ ] **Pivot-based Zoom**: Refine zooming to always center on the mouse cursor.
- [ ] **Grid Snap Toggle**: Enable/disable strict snapping for spawn placement.
- [ ] **Minimap Overlay**: A small navigational overview for large levels.

### V0.5.4: Inspector 2.0 (Dynamic Layout)
- [ ] **Property Grid**: Shift from hardcoded offsets to a dynamic row-based layout.
- [ ] **Collapsible Sections**: Group properties (Transform, Script, Physics) into toggleable headers.
- [ ] **Inline Components**: Add support for simple sliders and toggle switches in the panel.

### V0.5.5: Asset Management
- [ ] **Asset Preview**: Hover over files in the browser to see tile/script previews.
- [ ] **Drag & Drop**: Drag scripts/levels from the browser to assign them to tiles or spawn properties.

### V0.5.6: Visual Feedback & Hints
- [ ] **Hover Tooltips**: Minimalist popups for toolbar icons and complex inspector fields.
- [ ] **Toast Notifications**: Replace status-row messages with non-intrusive, timed popups for "Saved" or "Error" events.

### V0.5.7: Command Palette (Global Search)
- [ ] **Ctrl+P / Ctrl+Shift+P**: A global modal to quickly jump to files, levels, or execute editor commands (e.g., "Toggle Physics", "Reset Layout").

### V0.5.8: Theming & Personalization
- [ ] **Color Themes**: Support for "Classic Dark", "Terminal Green", and "Solarized" ASCII palettes.
- [ ] **Layout Profiles**: Save/Load custom dock arrangements (e.g., "Level Design" vs "Scripting" layouts).

### V0.5.9: Stability & Performance Pass
- [ ] **Undo/Redo Blitz**: Exhaustive verification of all editor actions in the undo stack.
- [ ] **Low-End Optimization**: Profile and optimize UI rendering to maintain 60FPS on older hardware.
- [ ] **Bug Blitz**: Final sweep of all "Known Issues" from V0.5.0.

---

## V0.6.0: The Asset & Animation Update
- [ ] **Sprite Animation Editor**: Dedicated UI for creating and previewing flip-book animations.
- [ ] **Tileset Importer**: Support for importing external sprite sheets/tilesets.
- [ ] **Engine API Expansion**: Expose animation and advanced physics controls to Rhai.
- [ ] **Final V0.6 Documentation**.
