// renderer/backend.rs — Abstracted rendering backends.

use std::io;
use crate::renderer::color::{Color, DEFAULT_BG, DEFAULT_FG};
use crate::renderer::buffer::{Buffer, Cell};
use crate::renderer::texture::Texture;
use font8x8::legacy::BASIC_LEGACY;

const CELL_W: usize = 8;
const CELL_H: usize = 16;

pub trait RenderBackend {
    fn name(&self) -> &str;
    
    fn clear(&mut self, pixel_buffer: &mut [u32]);
    
    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color);
    
    fn draw_char_scaled_pixels(&mut self, pixel_buffer: &mut [u32], pw: usize, ph: usize, 
                               px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32);

    fn draw_texture(&mut self, pixel_buffer: &mut [u32], pw: usize, ph: usize,
                    px: i32, py: i32, texture: &Texture, scale: f32);

    fn present(&mut self, pixel_buffer: &mut [u32], pw: usize, ph: usize) -> io::Result<()>;
    
    fn resize(&mut self, width: usize, height: usize);
    
    fn width(&self) -> usize;
    fn height(&self) -> usize;
}

// ────────────────────────── AsciiBackend ─────────────────────────────────────

pub struct AsciiBackend {
    back:  Buffer,
    width: usize,
    height: usize,
}

impl AsciiBackend {
    pub fn new(width: usize, height: usize) -> Self {
        AsciiBackend {
            back: Buffer::new(width, height),
            width,
            height,
        }
    }

    fn rasterize_ui_cell(&self, pixel_buffer: &mut [u32], pw: usize, ph: usize, 
                         cell_x: usize, cell_y: usize, cell: Cell) {
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
            if py >= ph { break; }
            for sx in 0..CELL_W {
                let pixel_on = (row_byte >> sx) & 1 != 0;
                let pixel_color = if pixel_on { fg_pixel } else { bg_pixel };
                let px = base_px + sx;
                if px < pw {
                    pixel_buffer[py * pw + px] = pixel_color;
                }
            }
        }
    }
}

impl RenderBackend for AsciiBackend {
    fn name(&self) -> &str { "Classic ASCII" }

    fn clear(&mut self, pixel_buffer: &mut [u32]) {
        self.back.clear();
        pixel_buffer.fill(Color::Black.to_rgb(DEFAULT_BG));
    }

    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.back.set(x, y, Cell::new(ch, fg, bg));
    }

    fn draw_char_scaled_pixels(&mut self, pixel_buffer: &mut [u32], pw: usize, ph: usize, 
                               px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32) {
        let fg_pixel = fg.to_rgb(DEFAULT_FG);
        let bg_pixel = bg.to_rgb(DEFAULT_BG);

        let char_idx = ch as usize;
        let bitmap: [u8; 8] = if char_idx < 128 { BASIC_LEGACY[char_idx] } else { BASIC_LEGACY['?' as usize] };

        let sw = (CELL_W as f32 * scale).round() as i32;
        let sh = (CELL_H as f32 * scale).round() as i32;
        if sw <= 0 || sh <= 0 { return; }

        for sy in 0..sh {
            let cur_py = py + sy;
            if cur_py < 0 || cur_py >= ph as i32 { continue; }
            let font_row = (sy * 8) / sh;
            let row_byte = bitmap[font_row as usize];

            for sx in 0..sw {
                let cur_px = px + sx;
                if cur_px < 0 || cur_px >= pw as i32 { continue; }
                let font_col = (sx * 8) / sw;
                let pixel_on = (row_byte >> font_col) & 1 != 0;
                let pixel_color = if pixel_on { fg_pixel } else { bg_pixel };

                pixel_buffer[cur_py as usize * pw + cur_px as usize] = pixel_color;
            }
        }
    }

    fn draw_texture(&mut self, pixel_buffer: &mut [u32], pw: usize, ph: usize,
                    px: i32, py: i32, texture: &Texture, scale: f32) {
        let sw = (texture.width as f32 * scale).round() as i32;
        let sh = (texture.height as f32 * scale).round() as i32;
        if sw <= 0 || sh <= 0 { return; }

        for sy in 0..sh {
            let cur_py = py + sy;
            if cur_py < 0 || cur_py >= ph as i32 { continue; }
            let tex_y = (sy * texture.height as i32) / sh;

            for sx in 0..sw {
                let cur_px = px + sx;
                if cur_px < 0 || cur_px >= pw as i32 { continue; }
                let tex_x = (sx * texture.width as i32) / sw;

                let tex_color = texture.pixels[tex_y as usize * texture.width as usize + tex_x as usize];
                
                // Simple alpha blending (top 8 bits)
                let alpha = (tex_color >> 24) & 0xFF;
                if alpha == 0 { continue; }
                if alpha == 255 {
                    pixel_buffer[cur_py as usize * pw + cur_px as usize] = tex_color;
                } else {
                    let bg = pixel_buffer[cur_py as usize * pw + cur_px as usize];
                    let r = (((tex_color >> 16) & 0xFF) * alpha + ((bg >> 16) & 0xFF) * (255 - alpha)) / 255;
                    let g = (((tex_color >> 8) & 0xFF) * alpha + ((bg >> 8) & 0xFF) * (255 - alpha)) / 255;
                    let b = ((tex_color & 0xFF) * alpha + (bg & 0xFF) * (255 - alpha)) / 255;
                    pixel_buffer[cur_py as usize * pw + cur_px as usize] = (r << 16) | (g << 8) | b;
                }
            }
        }
    }

    fn present(&mut self, pixel_buffer: &mut [u32], pw: usize, ph: usize) -> io::Result<()> {
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(cell) = self.back.get(x, y) {
                    if cell.ch != ' ' || cell.bg != Color::Reset {
                        self.rasterize_ui_cell(pixel_buffer, pw, ph, x, y, cell);
                    }
                }
            }
        }
        self.back.clear();
        Ok(())
    }

    fn resize(&mut self, width: usize, height: usize) {
        self.width = width;
        self.height = height;
        self.back = Buffer::new(width, height);
    }

    fn width(&self) -> usize { self.width }
    fn height(&self) -> usize { self.height }
}

// ────────────────────────── SpriteBackend ────────────────────────────────────

/// Delegates all rendering to AsciiBackend for now.
/// Override individual methods here as sprite-specific rendering diverges.
pub struct SpriteBackend(AsciiBackend);

impl SpriteBackend {
    pub fn new(width: usize, height: usize) -> Self {
        SpriteBackend(AsciiBackend::new(width, height))
    }
}

impl RenderBackend for SpriteBackend {
    fn name(&self) -> &str { "2D Sprites" }
    fn clear(&mut self, pb: &mut [u32]) { self.0.clear(pb) }
    fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) { self.0.draw_char(x, y, ch, fg, bg) }
    fn draw_char_scaled_pixels(&mut self, pb: &mut [u32], pw: usize, ph: usize,
                               px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32) {
        self.0.draw_char_scaled_pixels(pb, pw, ph, px, py, ch, fg, bg, scale)
    }
    fn draw_texture(&mut self, pb: &mut [u32], pw: usize, ph: usize,
                    px: i32, py: i32, texture: &Texture, scale: f32) {
        self.0.draw_texture(pb, pw, ph, px, py, texture, scale)
    }
    fn present(&mut self, pb: &mut [u32], pw: usize, ph: usize) -> io::Result<()> { self.0.present(pb, pw, ph) }
    fn resize(&mut self, width: usize, height: usize) { self.0.resize(width, height) }
    fn width(&self) -> usize { self.0.width() }
    fn height(&self) -> usize { self.0.height() }
}
