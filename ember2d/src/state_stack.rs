// state_stack.rs — which states in `Engine`'s stack get drawn each frame
// (R51, docs/ember2d-master-plan.md §3.2). Its own file, not a method on
// `Engine`, for two reasons: `engine.rs` is already past CLAUDE.md's 750-line
// limit (R76's own list), and this rule needs unit tests that build a stack of
// `Box<dyn GameState>` WITHOUT a window — `Engine::new` can't be constructed
// headlessly, but a bare `Vec<Box<dyn GameState>>` can.
//
// The rule is the ordinary state-stack one: an opaque state fully covers
// everything beneath it, so nothing beneath it is drawn at all; only a state
// that declares itself an overlay (`GameState::is_overlay`) keeps the states
// under it rendering. `Engine::run` used to draw the ENTIRE stack bottom-to-top
// regardless — harmless while `PlayState::render` began with an opaque
// full-screen fill (the paused editor beneath it was drawn, then buried), and
// broken the moment 7B-3 replaced that fill with the once-per-frame GPU clear:
// every F5 preview from then on showed the editor's chrome through every cell
// play didn't draw. See R51 for the full chain (and for why it sat misfiled as
// a Windows compositor artifact).

use crate::engine::GameState;

/// The index of the first state `Engine::run` should render this frame: the
/// topmost state that is NOT an overlay, so that state and everything above
/// it draw, and everything below it is skipped. `0` (draw the whole stack)
/// when every state is an overlay, or the stack is empty.
pub fn render_start_index(states: &[Box<dyn GameState>]) -> usize {
    states.iter().rposition(|s| !s.is_overlay()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{RenderContext, UpdateContext};

    struct Opaque;
    struct Overlay;

    impl GameState for Opaque {
        fn update(&mut self, _ctx: UpdateContext) {}
        fn render(&mut self, _ctx: RenderContext) {}
    }

    impl GameState for Overlay {
        fn update(&mut self, _ctx: UpdateContext) {}
        fn render(&mut self, _ctx: RenderContext) {}
        fn is_overlay(&self) -> bool {
            true
        }
    }

    #[test]
    fn an_opaque_top_state_hides_everything_beneath_it() {
        // EditorState -> PlayState: the exact F5 stack. Only play draws.
        let stack: Vec<Box<dyn GameState>> = vec![Box::new(Opaque), Box::new(Opaque)];
        assert_eq!(render_start_index(&stack), 1);
    }

    #[test]
    fn an_overlay_on_top_still_renders_the_opaque_state_under_it() {
        // PlayState -> PauseMenuState: the pause panel needs the play screen
        // visible around it.
        let stack: Vec<Box<dyn GameState>> = vec![Box::new(Opaque), Box::new(Overlay)];
        assert_eq!(render_start_index(&stack), 0);
    }

    #[test]
    fn two_stacked_overlays_render_from_the_opaque_state_beneath_both() {
        // EditorState -> PlayState -> PauseMenuState (the deepest legitimate
        // stack, per `Engine::push_state`'s own assertion) plus a hypothetical
        // second overlay: play and both overlays draw, the editor does not.
        let stack: Vec<Box<dyn GameState>> =
            vec![Box::new(Opaque), Box::new(Opaque), Box::new(Overlay), Box::new(Overlay)];
        assert_eq!(render_start_index(&stack), 1);
    }

    #[test]
    fn an_all_overlay_stack_renders_from_the_bottom() {
        let stack: Vec<Box<dyn GameState>> = vec![Box::new(Overlay), Box::new(Overlay)];
        assert_eq!(render_start_index(&stack), 0);
        let empty: Vec<Box<dyn GameState>> = Vec::new();
        assert_eq!(render_start_index(&empty), 0);
    }
}
