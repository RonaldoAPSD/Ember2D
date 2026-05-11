// renderer/mod.rs — The pixel-buffer renderer.

pub mod buffer;
pub mod color;
pub mod backend;
pub mod texture;
pub mod assets;

use std::io;
use minifb::{Window, WindowOptions, Scale, ScaleMode};

pub use color::{Color, DEFAULT_FG, DEFAULT_BG};
pub use texture::Texture;
pub use backend::{RenderBackend, AsciiBackend, SpriteBackend};
pub use assets::AssetManager;

const CELL_W: usize = 8;
const CELL_H: usize = 16;
pub const SCALE: usize = 2;

pub struct Renderer {
    window: Window,
    pixel_buffer: Vec<u32>,
    pub width: usize,
    pub height: usize,
    pixel_width: usize,
    pixel_height: usize,
    
    backend: Box<dyn RenderBackend>,
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
                scale_mode: ScaleMode::Stretch,
                ..Default::default()
            },
        )
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        let backend = Box::new(AsciiBackend::new(width, height));

        Ok(Renderer {
            window,
            pixel_buffer: vec![Color::Black.to_rgb(DEFAULT_BG); pixel_width * pixel_height],
            width,
            height,
            pixel_width,
            pixel_height,
            backend,
        })
    }

    // Proxy methods to the backend
    pub fn backend_name(&self) -> &str { self.backend.name() }
    pub fn set_backend(&mut self, backend: Box<dyn RenderBackend>) {
        self.backend = backend;
        self.width = self.backend.width();
        self.height = self.backend.height();
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

    #[cfg(target_os = "windows")]
    pub fn maximize(&self) {
        use std::ffi::c_void;
        extern "system" {
            fn ShowWindow(hWnd: *mut c_void, nCmdShow: i32) -> i32;
            fn SetWindowPos(hWnd: *mut c_void, hWndInsertAfter: *mut c_void, x: i32, y: i32, cx: i32, cy: i32, uFlags: u32) -> i32;
        }
        let handle = self.window.get_window_handle();
        unsafe {
            ShowWindow(handle, 3); // SW_MAXIMIZE = 3
            // Trigger a frame change to ensure the window correctly occupies the screen without artifacts.
            SetWindowPos(handle, std::ptr::null_mut(), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0020 | 0x0040); 
        }
    }

    pub fn clear(&mut self) {
        self.backend.clear(&mut self.pixel_buffer);
    }

    /// Draw a 1:1 UI character (buffered)
    pub fn draw_char(&mut self, x: usize, y: usize, ch: char, fg: Color, bg: Color) {
        self.backend.draw_char(x, y, ch, fg, bg);
    }

    /// Draw a scaled character directly to pixels (immediate)
    pub fn draw_char_scaled_pixels(&mut self, px: i32, py: i32, ch: char, fg: Color, bg: Color, scale: f32) {
        self.backend.draw_char_scaled_pixels(&mut self.pixel_buffer, self.pixel_width, self.pixel_height, px, py, ch, fg, bg, scale);
    }

    pub fn draw_texture(&mut self, px: i32, py: i32, texture: &Texture, scale: f32) {
        self.backend.draw_texture(&mut self.pixel_buffer, self.pixel_width, self.pixel_height, px, py, texture, scale);
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

    pub fn present(&mut self) -> io::Result<()> {
        self.backend.present(&mut self.pixel_buffer, self.pixel_width, self.pixel_height)?;

        self.window
            .update_with_buffer(&self.pixel_buffer, self.pixel_width, self.pixel_height)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;

        Ok(())
    }

    pub fn try_handle_resize(&mut self) -> bool {
        let (screen_w, screen_h) = self.window.get_size();
        
        // We ROUND UP the number of cells to ensure the buffer slightly overflows the window.
        // This causes minifb to stretch DOWN (downscale), which looks much smoother for text/ASCII.
        let new_w = ((screen_w + (SCALE * CELL_W - 1)) / SCALE / CELL_W).max(20);
        let new_h = ((screen_h + (SCALE * CELL_H - 1)) / SCALE / CELL_H).max(6);
        
        if new_w == self.width && new_h == self.height { return false; }
        
        self.width = new_w;
        self.height = new_h;
        self.pixel_width = new_w * CELL_W;
        self.pixel_height = new_h * CELL_H;
        self.pixel_buffer = vec![Color::Black.to_rgb(DEFAULT_BG); self.pixel_width * self.pixel_height];
        
        self.backend.resize(new_w, new_h);
        true
    }
}
