// editor/input/modal.rs — Button interaction logic for interactive modals.

use super::super::ui::{ToolKind, WidgetId};
use super::super::{EditorMode, EditorState, Modal, TextInputPurpose};
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
        // Keyboard shortcuts. Enter confirms too (R107's Enter half, fixed
        // in the Phase 9 gate pass): `[ YES ]` is drawn highlighted as the
        // default button, but only Y used to answer it.
        if input.just_pressed(Key::Y) || input.just_pressed(Key::Enter) {
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
            // 7D-3 checkpoint 7 (master plan §5.4): logical -> points, the
            // input choke point every chrome hit-test now goes through.
            let (px, py) = self.ui_space.logical_to_pt(mouse.pixel_x, mouse.pixel_y);
            match self.ui_frame.hit(px, py) {
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
                self.switch_to_level(&path);
            }
            // 7C-6 (master plan §5.3): confirming proceeds exactly to
            // where the un-confirmed "New Level" menu click used to go
            // directly — see `handle_menu_dropdown_click`'s own comment
            // on why this got a confirm step in front of it at all.
            crate::editor::ModalPurpose::ConfirmNewLevel => {
                self.prompt_buffer.clear();
                self.mode = EditorMode::Prompt(TextInputPurpose::NewLevelName);
            }
            // 7C-6 (master plan §5.3): irreversible — no undo entry for
            // this, unlike everything else this step added undo for.
            crate::editor::ModalPurpose::ConfirmDeleteFile { path } => {
                let _ = std::fs::remove_file(&path);
                self.refresh_project_files();
            }
        }
    }
}
