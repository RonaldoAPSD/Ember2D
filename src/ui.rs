// ui.rs — Generic UI component system for Ember2D.
//
// These components are designed to be reusable across the editor and in-game
// scripts. They focus on layout and drawing rather than state management.

use crate::renderer::{color::Color, Renderer};

// ── Panel ────────────────────────────────────────────────────────────────────

/// A bordered container with an optional title.
pub struct Panel {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
    pub title: String,
    pub fg: Color,
    pub bg: Color,
}

impl Panel {
    pub fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Panel {
            x, y, w, h,
            title: String::new(),
            fg: Color::White,
            bg: Color::Reset,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    pub fn with_colors(mut self, fg: Color, bg: Color) -> Self {
        self.fg = fg;
        self.bg = bg;
        self
    }

    pub fn draw(&self, renderer: &mut Renderer) {
        // Draw background
        renderer.draw_rect_filled(self.x, self.y, self.w, self.h, ' ', self.fg, self.bg);
        // Draw border
        renderer.draw_rect_outline(self.x, self.y, self.w, self.h, self.fg, self.bg);

        // Draw title
        if !self.title.is_empty() {
            let title_str = format!(" {} ", self.title);
            let tx = self.x + (self.w.saturating_sub(title_str.len())) / 2;
            renderer.draw_str(tx, self.y, &title_str, self.bg, self.fg);
        }
    }
}

// ── Menu ─────────────────────────────────────────────────────────────────────

/// A selectable vertical list of options.
pub struct Menu {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub options: Vec<String>,
    pub selected: usize,
    pub fg: Color,
    pub bg: Color,
    pub sel_fg: Color,
    pub sel_bg: Color,
}

impl Menu {
    pub fn new(x: usize, y: usize, w: usize, options: Vec<String>, selected: usize) -> Self {
        Menu {
            x, y, w, options, selected,
            fg: Color::White,
            bg: Color::Reset,
            sel_fg: Color::Black,
            sel_bg: Color::Cyan,
        }
    }

    pub fn with_colors(mut self, fg: Color, bg: Color, sel_fg: Color, sel_bg: Color) -> Self {
        self.fg = fg; self.bg = bg; self.sel_fg = sel_fg; self.sel_bg = sel_bg;
        self
    }

    pub fn draw(&self, renderer: &mut Renderer) {
        for (i, opt) in self.options.iter().enumerate() {
            let row = self.y + i;
            let is_sel = i == self.selected;
            
            let (fg, bg) = if is_sel {
                (self.sel_fg, self.sel_bg)
            } else {
                (self.fg, self.bg)
            };

            let prefix = if is_sel { "> " } else { "  " };
            let text = format!("{}{}", prefix, opt);
            let padded = format!("{:<width$}", text, width = self.w);
            renderer.draw_str(self.x, row, &padded, fg, bg);
        }
    }
}

// ── ProgressBar ──────────────────────────────────────────────────────────────

/// A horizontal bar representing a value from 0.0 to 1.0.
pub struct ProgressBar {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub progress: f32,
    pub fg: Color,
    pub bg: Color,
}

impl ProgressBar {
    pub fn new(x: usize, y: usize, w: usize, progress: f32) -> Self {
        ProgressBar { x, y, w, progress, fg: Color::Cyan, bg: Color::DarkGrey }
    }

    pub fn draw(&self, renderer: &mut Renderer) {
        let fill_w = (self.w as f32 * self.progress.clamp(0.0, 1.0)).round() as usize;
        renderer.draw_rect_filled(self.x, self.y, self.w, 1, ' ', self.fg, self.bg);
        if fill_w > 0 {
            renderer.draw_rect_filled(self.x, self.y, fill_w, 1, '=', self.bg, self.fg);
        }
    }
}
