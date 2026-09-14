// play/pause_menu.rs — PauseMenuState: the Esc menu pushed over a running
// PlayState. Split out of play.rs (R86, docs/ember2d-master-plan.md §3.2)
// via the same sibling-directory convention `mod render;`/`mod animation;`/
// `mod tests;` already use — play.rs was 751 real lines when R86's own fix
// and its test needed a home, over CLAUDE.md's 750-line hard limit.
//
// This is the engine's one OVERLAY state (`is_overlay` below, R51): it draws
// a small centered panel and relies on the play screen staying visible
// around it, so `Engine::run` keeps rendering the `PlayState` beneath it —
// the only case where a state under the top of the stack is drawn at all.

use crate::engine::{GameState, RenderContext, Transition, UpdateContext};
use crate::input::Key;
use crate::renderer::color::Color;

/// The width/height of the pause panel, in cells.
const PANEL_W: usize = 30;
const PANEL_H: usize = 8;

pub struct PauseMenuState {
    options: Vec<String>,
    selected: usize,
    pending_transition: Option<Transition>,
}

impl PauseMenuState {
    pub fn new() -> Self {
        Self {
            options: vec![
                "Resume".to_string(),
                "Back to Editor".to_string(),
                "Quit Game".to_string(),
            ],
            selected: 0,
            pending_transition: None,
        }
    }
}

/// Where a `size`-cell panel starts so it's centered on a `screen`-cell axis
/// — clamped to `0` when the screen is SMALLER than the panel (R86, master
/// plan §3.2): this used to be a bare `(screen - size) / 2` in `usize`, and
/// `compute_layout` floors the cell grid at 20×6 (`renderer/geometry.rs`),
/// so a window narrower than the 30-cell panel (under ~480 physical px at
/// the default scale) or shorter than its 8 rows underflowed — a debug
/// panic, or in release a wrapped-around origin feeding a gigantic
/// `draw_rect_filled` loop — the moment Esc was pressed in play mode.
pub(super) fn centered_origin(screen: usize, size: usize) -> usize {
    screen.saturating_sub(size) / 2
}

impl GameState for PauseMenuState {
    fn update(&mut self, ctx: UpdateContext) {
        if ctx.input.just_pressed(Key::Up) {
            self.selected = self.selected.saturating_sub(1);
        }
        if ctx.input.just_pressed(Key::Down) {
            if self.selected + 1 < self.options.len() {
                self.selected += 1;
            }
        }

        if ctx.input.just_pressed(Key::Enter) {
            match self.selected {
                0 => self.pending_transition = Some(Transition::Pop),
                1 => self.pending_transition = Some(Transition::ToEditor),
                2 => self.pending_transition = Some(Transition::Quit),
                _ => {}
            }
        }

        if ctx.input.just_pressed(Key::Escape) {
            self.pending_transition = Some(Transition::Pop);
        }
    }

    fn render(&mut self, ctx: RenderContext) {
        let x = centered_origin(ctx.renderer.width, PANEL_W);
        let y = centered_origin(ctx.renderer.height, PANEL_H);

        crate::ui::Panel::new(x, y, PANEL_W, PANEL_H)
            .with_title(" PAUSED ")
            .with_colors(Color::White, Color::DarkBlue)
            .draw(ctx.renderer);

        for (i, opt) in self.options.iter().enumerate() {
            let fg = if i == self.selected { Color::Yellow } else { Color::Grey };
            let bg = if i == self.selected { Color::DarkGrey } else { Color::DarkBlue };
            let prefix = if i == self.selected { "> " } else { "  " };
            ctx.renderer.draw_str(x + 2, y + 2 + i, &format!("{}{}", prefix, opt), fg, bg);
        }
    }

    fn take_transition(&mut self) -> Option<Transition> {
        self.pending_transition.take()
    }

    /// A small centered panel over the still-visible play screen (R51,
    /// docs/ember2d-master-plan.md §3.2) — the one state that needs what's
    /// beneath it drawn.
    fn is_overlay(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r86_the_pause_menu_origin_never_underflows_on_a_window_smaller_than_the_panel() {
        // `compute_layout`'s own minimum grid is 20×6 cells — narrower than
        // the 30-cell panel and shorter than its 8 rows.
        assert_eq!(centered_origin(20, PANEL_W), 0);
        assert_eq!(centered_origin(6, PANEL_H), 0);
        // Exactly the panel's own size: flush with the origin, not off by one.
        assert_eq!(centered_origin(PANEL_W, PANEL_W), 0);
        // The ordinary case is unchanged: a 90×25 grid centers as before.
        assert_eq!(centered_origin(90, PANEL_W), 30);
        assert_eq!(centered_origin(25, PANEL_H), 8);
    }
}
