// editor/input/modal.rs — Button interaction logic for interactive modals.

use super::super::ui::{ToolKind, WidgetId};
use super::super::{EditorMode, EditorState, Modal};
use ember2d::input::Key;

impl EditorState {
    /// 7C-4 (master plan §5.3): `modal` comes in owned (moved out of `mode`
    /// by `handle_update`'s `mem::take`) instead of being read from the
    /// deleted `modal: Option<Modal>` field — needed as an owned value
    /// here specifically because `confirm_modal` may do `*self = ns` on a
    /// successful level switch, which can't happen while `self.mode` (or
    /// any other part of `self`) is still borrowed.
    pub(super) fn handle_modal_input(
        &mut self,
        modal: Modal,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        // Keyboard shortcuts
        if input.just_pressed(Key::Y) {
            self.confirm_modal(modal);
            return;
        }
        if input.just_pressed(Key::N) || input.just_pressed(Key::Escape) {
            self.mode = EditorMode::Paint(ToolKind::Paint);
            return;
        }

        // Mouse interaction — 7C-1 (master plan §5.3): reads
        // `UiFrame::hit` (populated by `draw_confirm_modal` via
        // `draw_button`) instead of recomputing `mx`/`btn_y`/
        // `yes_x`/`no_x` independently here (E5).
        if mouse.left_just_pressed() && mouse.in_bounds {
            match self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                Some(WidgetId::ConfirmYes) => {
                    self.confirm_modal(modal);
                    return;
                }
                Some(WidgetId::ConfirmNo) => {
                    self.mode = EditorMode::Paint(ToolKind::Paint);
                    return;
                }
                _ => {}
            }
        }

        // Nothing happened this frame — stay open.
        self.mode = EditorMode::Modal(modal);
    }

    /// `modal` closes regardless of outcome — on load failure, `mode` is
    /// simply left at whatever the caller already set it to (`Paint`,
    /// matching every path above) since there's nothing else to restore to.
    fn confirm_modal(&mut self, modal: Modal) {
        match modal.purpose {
            crate::editor::ModalPurpose::ConfirmSwitchLevel { path } => {
                if let Ok(new_state) = crate::editor::EditorState::load(&path) {
                    let mut ns = new_state;
                    ns.project_folder = self.project_folder.clone();
                    ns.project_name = self.project_name.clone();
                    ns.panels = self.panels.clone();
                    ns.current_folder = self.current_folder.clone();
                    ns.refresh_project_files();

                    // Destructure and replace our own state
                    *self = ns;
                }
            }
        }
    }
}
