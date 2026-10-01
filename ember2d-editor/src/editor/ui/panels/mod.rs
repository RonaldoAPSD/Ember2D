// editor/ui/panels/mod.rs — Drawing functions for editor panels, split
// into three thematic files in Phase 7 Part 1f (docs/ember2d-phase7-plan.md)
// purely to keep each under CLAUDE.md's 600-line hard limit (this file was
// already 727 lines before this split, and Phase 7's own additions weren't
// the ones that put it there — it was 662 before Phase 7 touched it at
// all). No behavioral change from being split; this file just re-exports
// everything so every existing `ui::draw_*` call site keeps working
// unchanged.
//
//   chrome — small, always-present widgets (title bar, status bar, dock
//            tabs, text-input prompt, confirm modal, right-click menu)
//   dock   — the big dockable content panels (Palette, Stats, Console,
//            Inspector, Hierarchy, File Browser)
//   modals — full-screen/floating overlays (palette editor, advanced color
//            picker + its swatch grid, keyboard-shortcuts help screen)
//   palette_panel — the Palette panel, out of `dock` since Step 8-2 (dock
//            was at the 750-line limit; see that file's header)
//   file_browser — the File Browser panel and the asset drag ghost, out of
//            `dock` since Step 8-4 (see that file's header)

mod chrome;
mod clip_editor_panel;
mod dock;
mod file_browser;
mod importer_panel;
mod inspector;
mod modals;
mod palette_panel;
mod project_settings_panel;

pub use chrome::*;
pub use clip_editor_panel::*;
pub use dock::*;
pub use file_browser::*;
pub use importer_panel::*;
pub use inspector::*;
pub use modals::*;
pub use palette_panel::*;
pub use project_settings_panel::*;
