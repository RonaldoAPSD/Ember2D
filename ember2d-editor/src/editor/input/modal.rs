// editor/input/modal.rs — Button interaction logic for interactive modals.

use super::super::ui::WidgetId;
use super::super::EditorState;
use ember2d::input::Key;

impl EditorState {
    pub(super) fn handle_modal_input(
        &mut self,
        input: &ember2d::input::InputManager,
        mouse: &ember2d::mouse::MouseState,
    ) {
        if self.modal.is_some() {
            // Keyboard shortcuts
            if input.just_pressed(Key::Y) {
                self.confirm_modal();
                return;
            }
            if input.just_pressed(Key::N) || input.just_pressed(Key::Escape) {
                self.modal = None;
                return;
            }

            // Mouse interaction — 7C-1 (master plan §5.3): reads
            // `UiFrame::hit` (populated by `draw_confirm_modal` via
            // `draw_button`) instead of recomputing `mx`/`btn_y`/
            // `yes_x`/`no_x` independently here (E5).
            if mouse.left_just_pressed() && mouse.in_bounds {
                match self.ui_frame.hit(mouse.pixel_x, mouse.pixel_y) {
                    Some(WidgetId::ConfirmYes) => self.confirm_modal(),
                    Some(WidgetId::ConfirmNo) => self.modal = None,
                    _ => {}
                }
            }
        }
    }

    fn confirm_modal(&mut self) {
        if let Some(m) = self.modal.take() {
            match m.purpose {
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
}
