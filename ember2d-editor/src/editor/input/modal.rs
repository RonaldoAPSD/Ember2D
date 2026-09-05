// editor/input/modal.rs — Button interaction logic for interactive modals.

use ember2d::input::Key;
use super::super::EditorState;

impl EditorState {
    pub(super) fn handle_modal_input(&mut self, input: &ember2d::input::InputManager, mouse: &ember2d::mouse::MouseState) {
        if self.modal.is_some() {
            let mw = 40usize;
            let mh = 8usize;
            let mx = (self.layout.screen_w.saturating_sub(mw)) / 2;
            let my = (self.layout.screen_h.saturating_sub(mh)) / 2;
            let btn_y = my + 5;
            let yes_x = mx + 8;
            let no_x  = mx + mw - 15;

            // Keyboard shortcuts
            if input.just_pressed(Key::Y) {
                self.confirm_modal();
                return;
            }
            if input.just_pressed(Key::N) || input.just_pressed(Key::Escape) {
                self.modal = None;
                return;
            }

            // Mouse interaction
            if mouse.left_just_pressed() && mouse.in_bounds {
                if mouse.cell_y == btn_y {
                    // YES button
                    if mouse.cell_x >= yes_x && mouse.cell_x < yes_x + 9 {
                        self.confirm_modal();
                        return;
                    }
                    // NO button
                    if mouse.cell_x >= no_x && mouse.cell_x < no_x + 9 {
                        self.modal = None;
                        return;
                    }
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
                        ns.project_name   = self.project_name.clone();
                        ns.panels         = self.panels.clone();
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
