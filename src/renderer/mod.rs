// renderer/mod.rs — The Fake Terminal Renderer (now backed by a real OS window).

pub mod buffer;
pub mod color;

use std::io;
use minifb::{Window, WindowOptions, Scale};
use font8x8::legacy::BASIC_LEGACY;

use buffer::{Buffer, Cell};
pub use color::{Color, DEFAULT_FG, DEFAULT_BG};

const CELL_W: usize = 8;
const CELL_H: usize = 16;
pub const SCALE: usize = 2;

pub struct Renderer {
    window: Window,
    pixel_buffer: Vec<u32>,
    back:  Buffer,
    front: Buffer,
    pub width: usize,
    pub height: usize,
    pixel_width: usize,
    pixel_height: usize,
}

impl Renderer {
    pub fn new(width: usize, height: usize, title: &str) -> io::Result<Self> {
        let pixel_width  = width  * CELL_W;
        let pixel_height = height * CELL_H;

        let window = Window::new(
            title,
            pixel_width,
            pixel_height,
            WindowOptions {
                scale:  Scale::X2,
                resize: true,
                ..Default::default()
            },
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(Renderer {
            window,
            pixel_buffer: vec![Color::Black.to_rgb(DEFAULT_BG); pixel_width * pixel_height],
            back:  Buffer::new(width, height),
            front: Buffer::new(width, height),
            width,
            height,
            pixel_width,
            pixel_height,
        })
    }

    pub fn is_open(&self) -> bool { self.window.is_open() }
    pub fn current_keys(&self) -> Vec<minifb::Key> { self.window.get_keys() }
    pub fn current_mouse_pos(&self) -> Option<(f32, f32)> {
        self.window.get_mouse_pos(minifb::MouseMode::Pass).map(|(x, y)| {
            let max_x = (self.pixel_width as f32 - 1.0).max(0.0);
            let max_y = (self.pixel_height as f32 - 1.0).max(0.0);
            (x.clamp(0.0, max_x), y.clamp(0.0, max_y))
        })
    }
    pub fn is_mouse_button_down(&self, button: minifb::MouseButton) -> bool { self.window.get_mouse_down(button) }
    pub fn get_scroll_wheel(&self) -> (f32, f32) { self.window.get_scroll_wheel().unwrap_or((0.0, 0.0)) }

    pub fn clear(&mut self) {
        self.back.clear();
        self.pixel_buffer.fill(Color::Black.to_rgb(DEFAULT_BG));
    }

    /// Draw a 1:1 UI character (buffered)
    pub fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.back.set(x, y, Cell::new_scaled(ch, fg, bg, 1.0));
    }

    /// Draw a scaled character directly to pixels (immediate)
    pub fn draw_char_scaled_pixels(&mut self, px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32) {
        let fg_pixel = fg.to_rgb(DEFAULT_FG);
        let bg_pixel = bg.to_rgb(DEFAULT_BG);

        let char_idx = ch as usize;
        let bitmap: [u8; 8] = if char_idx < 128 { BASIC_LEGACY[char_idx] } else { BASIC_LEGACY['?' as usize] };

        let sw = (CELL_W as f32 * scale).round() as i32;
        let sh = (CELL_H as f32 * scale).round() as i32;
        if sw <= 0 || sh <= 0 { return; }

        for sy in 0..sh {
            let cur_py = py + sy;
            if cur_py < 0 || cur_py >= self.pixel_height as i32 { continue; }
            let font_row = (sy * 8) / sh;
            let row_byte = bitmap[font_row as usize];

            for sx in 0..sw {
                let cur_px = px + sx;
                if cur_px < 0 || cur_px >= self.pixel_width as i32 { continue; }
                let font_col = (sx * 8) / sw;
                let pixel_on = (row_byte >> font_col) & 1 != 0;
                let pixel_color = if pixel_on { fg_pixel } else { bg_pixel };

                self.pixel_buffer[cur_py as usize * self.pixel_width + cur_px as usize] = pixel_color;
            }
        }
    }

    pub fn draw_str(&mut self, x: usize, y: usize, s: &str, fg: Color, bg: Color) {
        for (i, ch) in s.chars().enumerate() {
            self.draw_char(x.saturating_add(i), y, ch, fg, bg);
        }
    }

    pub fn draw_lines(&mut self, x: usize, y: usize, lines: &[&str], fg: Color, bg: Color) {
        for (i, line) in lines.iter().enumerate() {
            self.draw_str(x, y.saturating_add(i), line, fg, bg);
        }
    }

    pub fn draw_rect_outline(&mut self, x: usize, y: usize, w: usize, h: usize, fg: Color, bg: Color) {
        if w < 2 || h < 2 { return; }
        for col in (x + 1)..(x + w - 1) {
            self.draw_char(col, y, '-', fg, bg);
            self.draw_char(col, y + h - 1, '-', fg, bg);
        }
        for row in (y + 1)..(y + h - 1) {
            self.draw_char(x, row, '|', fg, bg);
            self.draw_char(x + w - 1, row, '|', fg, bg);
        }
        self.draw_char(x, y, '+', fg, bg);
        self.draw_char(x + w - 1, y, '+', fg, bg);
        self.draw_char(x, y + h - 1, '+', fg, bg);
        self.draw_char(x + w - 1, y + h - 1, '+', fg, bg);
    }

    pub fn draw_rect_filled(&mut self, x: usize, y: usize, w: usize, h: usize, ch: char, fg: Color, bg: Color) {
        for row in y..(y + h) {
            for col in x..(x + w) {
                self.draw_char(col, row, ch, fg, bg);
            }
        }
    }

    fn rasterize_ui_cell(&mut self, cell_x: usize, cell_y: usize, cell: Cell) {
        let fg_pixel = cell.fg.to_rgb(DEFAULT_FG);
        let bg_pixel = cell.bg.to_rgb(DEFAULT_BG);
        let char_idx = cell.ch as usize;
        let bitmap: [u8; 8] = if char_idx < 128 { BASIC_LEGACY[char_idx] } else { BASIC_LEGACY['?' as usize] };

        let base_px = cell_x * CELL_W;
        let base_py = cell_y * CELL_H;

        for sy in 0..CELL_H {
            let font_row = sy / 2; // UI is always doubled (8->16)
            let row_byte = bitmap[font_row];
            let py = base_py + sy;
            if py >= self.pixel_height { break; }
            for sx in 0..CELL_W {
                let pixel_on = (row_byte >> sx) & 1 != 0;
                let pixel_color = if pixel_on { fg_pixel } else { bg_pixel };
                let px = base_px + sx;
                if px < self.pixel_width {
                    self.pixel_buffer[py * self.pixel_width + px] = pixel_color;
                }
            }
        }
    }

    pub fn present(&mut self) -> io::Result<()> {
        // UI is redrawn every frame since we cleared pixel_buffer in clear()
        // We don't use front/back diffing here for simplicity since UI is small
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(cell) = self.back.get(x, y) {
                    if cell.ch != ' ' || cell.bg != Color::Reset {
                        self.rasterize_ui_cell(x, y, cell);
                    }
                }
            }
        }

        self.window
            .update_with_buffer(&self.pixel_buffer, self.pixel_width, self.pixel_height)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        self.back.clear(); // Ready for next frame
        Ok(())
    }

    pub fn try_handle_resize(&mut self) -> bool {
        let (screen_w, screen_h) = self.window.get_size();
        let new_w = (screen_w / SCALE / CELL_W).max(20);
        let new_h = (screen_h / SCALE / CELL_H).max(6);
        if new_w == self.width && new_h == self.height { return false; }
        
        self.width = new_w;
        self.height = new_h;
        self.pixel_width = new_w * CELL_W;
        self.pixel_height = new_h * CELL_H;
        self.pixel_buffer = vec![Color::Black.to_rgb(DEFAULT_BG); self.pixel_width * self.pixel_height];
        self.back = Buffer::new(new_w, new_h);
        self.front = Buffer::new(new_w, new_h);
        true
    }
}
